//! Watchers and approvals (docs/rfcs/v0.md section 15).
//!
//! `schedules(id, cron, prompt, enabled)` fires a prompt into a thread at its cron time, as if
//! the owner typed it. `approvals(id, thread_id, kind, payload, status, expires_at)` holds every
//! outward action; in v0 the only kind is `file_write` outside the data directory. Nothing with
//! status `pending` executes. Approvals expire after 24 hours.

use chrono::{TimeZone, Utc};
use mote_eventlog::EventLog;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::ffi::{CString, OsString};
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{collections::HashMap, sync::Mutex};
use mote_protocol::{ActionDigest, ApprovalDecision, ApprovalVerdict, HostId};
#[cfg(test)]
use mote_protocol::{PendingApproval, RequestId};
use sha2::{Digest, Sha256};

/// Approvals expire after 24 hours.
pub const APPROVAL_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// The thread a scheduled prompt fires into.
pub const SCHEDULE_THREAD: &str = "scheduled";

/// Outcome of a requested file write.
#[derive(Debug, PartialEq)]
pub enum WriteDecision {
    /// Written immediately (inside the data directory).
    Executed,
    /// Held for approval (outside the data directory).
    Pending(i64),
}

#[derive(Debug)]
pub enum ApprovalError {
    Database(rusqlite::Error),
    Filesystem(std::io::Error),
    Payload(serde_json::Error),
}

impl fmt::Display for ApprovalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "approval database: {error}"),
            Self::Filesystem(error) => write!(formatter, "approval filesystem: {error}"),
            Self::Payload(error) => write!(formatter, "approval payload: {error}"),
        }
    }
}

impl std::error::Error for ApprovalError {}

impl From<rusqlite::Error> for ApprovalError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<std::io::Error> for ApprovalError {
    fn from(error: std::io::Error) -> Self {
        Self::Filesystem(error)
    }
}

impl From<serde_json::Error> for ApprovalError {
    fn from(error: serde_json::Error) -> Self {
        Self::Payload(error)
    }
}

#[derive(Deserialize, Serialize)]
struct FileWritePayload {
    path: PathBuf,
    resolved_path: PathBuf,
    expectation: TargetExpectation,
    content: String,
}

struct ResolvedWriteTarget {
    path: PathBuf,
    parent: PathBuf,
    filename: OsString,
    expectation: TargetExpectation,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum TargetExpectation {
    Missing,
    Existing { device: u64, inode: u64 },
}

pub struct Watchers {
    conn: Connection,
    intents: Mutex<HashMap<i64, FileWritePayload>>,
}

fn to_dt(ms: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap_or_else(|| Utc.timestamp_opt(0, 0).unwrap())
}

impl Watchers {
    pub fn open(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schedules(
                 id INTEGER PRIMARY KEY AUTOINCREMENT, cron TEXT NOT NULL, prompt TEXT NOT NULL,
                 enabled INTEGER NOT NULL DEFAULT 1, last_fired INTEGER);
             CREATE TABLE IF NOT EXISTS approvals(
                 id INTEGER PRIMARY KEY AUTOINCREMENT, thread_id TEXT NOT NULL, kind TEXT NOT NULL,
                 payload TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
                 expires_at INTEGER NOT NULL, created_at INTEGER NOT NULL);",
        )?;
        super::approvals::migrate(&conn)?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS approval_owner(id INTEGER PRIMARY KEY CHECK(id=1),host_id TEXT NOT NULL)")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS approval_devices(host_id TEXT NOT NULL, device_id TEXT NOT NULL, approval_id INTEGER NOT NULL, action_hash TEXT NOT NULL, expires_at INTEGER NOT NULL, PRIMARY KEY(host_id,device_id,approval_id));")?;
        let mut intents = HashMap::new();
        let legacy: Vec<(i64, String)> = conn.prepare("SELECT id,payload FROM approvals WHERE action_hash IS NULL AND kind='file_write'")?.query_map([], |row| Ok((row.get(0)?,row.get(1)?)))?.collect::<Result<_,_>>()?;
        for (id, payload) in legacy {
            if let Ok(intent) = serde_json::from_str::<FileWritePayload>(&payload) {
                let digest = intent_digest(&intent).map_err(|_| rusqlite::Error::InvalidQuery)?;
                let display = display_payload(&intent);
                conn.execute("UPDATE approvals SET payload=?2,action_hash=?3 WHERE id=?1", rusqlite::params![id, display.to_string(), digest.as_str()])?;
                intents.insert(id, intent);
            }
        }
        Ok(Self { conn, intents: Mutex::new(intents) })
    }

    pub(crate) fn bind_local_host(&self, host: &HostId) -> rusqlite::Result<()> {
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS approval_owner(id INTEGER PRIMARY KEY CHECK(id=1),host_id TEXT NOT NULL)")?;
        self.conn.execute("INSERT OR IGNORE INTO approval_owner(id,host_id) VALUES(1,?1)",[host.as_str()])?;
        let owner: String = self.conn.query_row("SELECT host_id FROM approval_owner WHERE id=1", [], |row| row.get(0))?;
        if owner != host.as_str() { return Err(rusqlite::Error::InvalidQuery); }
        self.conn.execute("UPDATE approvals SET host_id=?1 WHERE host_id IS NULL AND kind='file_write'",[host.as_str()])?;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn approval_store(&self) -> rusqlite::Result<super::approvals::ApprovalStore<'_>> {
        let owner: String = self.conn.query_row("SELECT host_id FROM approval_owner WHERE id=1", [], |row| row.get(0))?;
        let host = HostId::new(owner).map_err(|_| rusqlite::Error::InvalidQuery)?;
        super::approvals::ApprovalStore::new(&self.conn,host)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::open(Connection::open_in_memory()?)
    }

    /// Add a schedule. `now` seeds `last_fired` so it does not fire for past occurrences.
    pub fn add_schedule(&self, cron: &str, prompt: &str, enabled: bool, now: i64) -> rusqlite::Result<i64> {
        self.conn.execute(
            "INSERT INTO schedules(cron, prompt, enabled, last_fired) VALUES(?1, ?2, ?3, ?4)",
            rusqlite::params![cron, prompt, enabled as i64, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Fire every schedule whose next cron occurrence after `last_fired` is at or before `now`.
    /// Returns the ids fired. A fired prompt is appended to the schedule thread as a user turn.
    pub fn tick(&self, now: i64, log: &EventLog) -> rusqlite::Result<Vec<i64>> {
        let mut fired = Vec::new();
        let mut stmt = self
            .conn
            .prepare("SELECT id, cron, prompt, last_fired FROM schedules WHERE enabled=1")?;
        let rows: Vec<(i64, String, String, Option<i64>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<_, _>>()?;
        for (id, cron, prompt, last_fired) in rows {
            let Ok(schedule) = cron::Schedule::from_str(&cron) else {
                continue;
            };
            let base = to_dt(last_fired.unwrap_or(0));
            if let Some(next) = schedule.after(&base).next() {
                if next.timestamp_millis() <= now {
                    log.append(
                        SCHEDULE_THREAD,
                        "turn.user",
                        &json!({ "text": prompt, "source": "schedule", "schedule_id": id }),
                    )
                    .ok();
                    self.conn.execute(
                        "UPDATE schedules SET last_fired=?1 WHERE id=?2",
                        rusqlite::params![next.timestamp_millis(), id],
                    )?;
                    fired.push(id);
                }
            }
        }
        Ok(fired)
    }

    /// Request a file write. Inside the data directory it executes immediately; outside it is
    /// held as a pending approval.
    pub fn request_file_write(
        &self,
        data_dir: &Path,
        thread_id: &str,
        path: &Path,
        content: &str,
        now: i64,
    ) -> Result<WriteDecision, ApprovalError> {
        self.request_file_write_impl(data_dir, thread_id, path, content, now, || Ok(()))
    }

    #[cfg(test)]
    fn request_file_write_with_hook(
        &self,
        data_dir: &Path,
        thread_id: &str,
        path: &Path,
        content: &str,
        now: i64,
        before_target_open: impl FnOnce() -> std::io::Result<()>,
    ) -> Result<WriteDecision, ApprovalError> {
        self.request_file_write_impl(
            data_dir,
            thread_id,
            path,
            content,
            now,
            before_target_open,
        )
    }

    fn request_file_write_impl(
        &self,
        data_dir: &Path,
        thread_id: &str,
        path: &Path,
        content: &str,
        now: i64,
        before_target_open: impl FnOnce() -> std::io::Result<()>,
    ) -> Result<WriteDecision, ApprovalError> {
        let resolved = resolve_write_target(path)?;
        if resolved.path.starts_with(data_dir.canonicalize()?) {
            write_resolved(&resolved, content, before_target_open)?;
            return Ok(WriteDecision::Executed);
        }
        let intent = FileWritePayload {
            path: path.to_path_buf(),
            resolved_path: resolved.path,
            expectation: resolved.expectation,
            content: content.to_string(),
        };
        let hash = intent_digest(&intent)?;
        let payload = display_payload(&intent).to_string();
        self.conn.execute(
            "INSERT INTO approvals(thread_id, kind, payload, status, expires_at, created_at, action_hash,host_id)
             VALUES(?1, 'file_write', ?2, 'pending', ?3, ?4, ?5,(SELECT host_id FROM approval_owner WHERE id=1))",
            rusqlite::params![thread_id, payload, now + APPROVAL_TTL_MS, now, hash.as_str()],
        )?;
        let id = self.conn.last_insert_rowid();
        self.intents.lock().unwrap().insert(id, intent);
        Ok(WriteDecision::Pending(id))
    }

    pub fn status(&self, id: i64) -> rusqlite::Result<String> {
        self.conn.query_row("SELECT status FROM approvals WHERE id=?1", [id], |r| r.get(0))
    }

    /// Approve an approval, executing its action. Returns whether it executed.
    pub fn approve(&self, id: i64, now: i64) -> Result<bool, ApprovalError> {
        let Some(thread_id) = self.approval_thread(id)? else {
            return Ok(false);
        };
        self.approve_for_thread(id, &thread_id, now)
    }

    pub fn approve_for_thread(
        &self,
        id: i64,
        thread_id: &str,
        now: i64,
    ) -> Result<bool, ApprovalError> {
        self.approve_for_thread_impl(id, thread_id, now, || Ok(()))
    }

    #[cfg(test)]
    fn approve_for_thread_with_hook(
        &self,
        id: i64,
        thread_id: &str,
        now: i64,
        before_target_open: impl FnOnce() -> std::io::Result<()>,
    ) -> Result<bool, ApprovalError> {
        self.approve_for_thread_impl(id, thread_id, now, before_target_open)
    }

    fn approve_for_thread_impl(
        &self,
        id: i64,
        thread_id: &str,
        now: i64,
        before_target_open: impl FnOnce() -> std::io::Result<()>,
    ) -> Result<bool, ApprovalError> {
        self.expire_due(now)?;
        let payload = match self.conn.query_row(
            "SELECT payload FROM approvals
             WHERE id=?1 AND thread_id=?2 AND kind='file_write' AND status='pending' AND expires_at>=?3",
            rusqlite::params![id, thread_id, now],
            |row| row.get::<_, String>(0),
        ) {
            Ok(payload) => payload,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let _: serde_json::Value = serde_json::from_str(&payload)?;
        let intents = self.intents.lock().unwrap();
        let write = intents.get(&id).ok_or_else(|| std::io::Error::other("exact approval intent unavailable after restart"))?;
        let digest = intent_digest(write)?;
        let stored_hash: Option<String> = self.conn.query_row("SELECT action_hash FROM approvals WHERE id=?1", [id], |row| row.get(0))?;
        if stored_hash.as_deref() != Some(digest.as_str()) {
            return Err(std::io::Error::other("approval intent changed").into());
        }
        let resolved = resolve_write_target(&write.path)?;
        if resolved.path != write.resolved_path || resolved.expectation != write.expectation {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "approved write target changed while pending",
            )
            .into());
        }
        let claimed = self.conn.execute("UPDATE approvals SET status='claimed',claimed_at=?3 WHERE id=?1 AND thread_id=?2 AND kind='file_write' AND status='pending' AND expires_at>=?3",rusqlite::params![id,thread_id,now])?;
        if claimed == 0 { return Ok(false); }
        if let Err(error) = write_resolved(&resolved, &write.content, before_target_open) {
            self.conn.execute("UPDATE approvals SET status='pending',claimed_at=NULL WHERE id=?1 AND status='claimed'",[id])?;
            return Err(error.into());
        }
        let changed = self.conn.execute(
            "UPDATE approvals SET status='approved',completed_at=?3
             WHERE id=?1 AND thread_id=?2 AND status='claimed'",
            rusqlite::params![id, thread_id, now],
        )?;
        Ok(changed == 1)
    }

    pub fn deny(&self, id: i64, now: i64) -> rusqlite::Result<bool> {
        let Some(thread_id) = self.approval_thread(id)? else {
            return Ok(false);
        };
        self.deny_for_thread(id, &thread_id, now)
    }

    pub fn deny_for_thread(&self, id: i64, thread_id: &str, now: i64) -> rusqlite::Result<bool> {
        self.expire_due(now)?;
        let changed = self.conn.execute(
            "UPDATE approvals SET status='denied'
             WHERE id=?1 AND thread_id=?2 AND status='pending' AND expires_at>=?3",
            rusqlite::params![id, thread_id, now],
        )?;
        Ok(changed == 1)
    }

    /// Mark pending approvals past their expiry as expired.
    pub fn expire_due(&self, now: i64) -> rusqlite::Result<usize> {
        let n = self.conn.execute(
            "UPDATE approvals SET status='expired' WHERE status='pending' AND expires_at < ?1",
            [now],
        )?;
        Ok(n)
    }

    /// The surface tree (as JSON data) for an approval card. The client renders it.
    pub fn approval_card(&self, id: i64) -> rusqlite::Result<serde_json::Value> {
        let payload: String =
            self.conn.query_row("SELECT payload FROM approvals WHERE id=?1", [id], |r| r.get(0))?;
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);
        let path = v["filename"].as_str().unwrap_or("file");
        Ok(json!({
            "type": "stack",
            "children": [
                { "type": "text", "value": format!("Approve file write to {path}?") },
                { "type": "row", "children": [
                    { "type": "action", "label": "Approve", "event": format!("approve:{id}") },
                    { "type": "action", "label": "Deny", "event": format!("deny:{id}") }
                ]}
            ]
        }))
    }

    fn approval_thread(&self, id: i64) -> rusqlite::Result<Option<String>> {
        match self.conn.query_row(
            "SELECT thread_id FROM approvals WHERE id=?1",
            [id],
            |row| row.get(0),
        ) {
            Ok(thread_id) => Ok(Some(thread_id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error),
        }
    }

    #[cfg(test)]
    pub fn remote_approvals(&self, host: &HostId, device: &str, now: i64) -> Result<Vec<PendingApproval>, ApprovalError> {
        self.expire_due(now)?;
        let mut stmt = self.conn.prepare("SELECT id,thread_id,kind,payload,action_hash,expires_at FROM approvals WHERE status='pending' AND action_hash IS NOT NULL AND (host_id IS NULL OR host_id=?1) ORDER BY id")?;
        let rows = stmt.query_map([host.as_str()], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,String>(1)?, r.get::<_,String>(2)?, r.get::<_,String>(3)?, r.get::<_,String>(4)?, r.get::<_,i64>(5)?)))?;
        let mut result = Vec::new();
        for row in rows {
            let (id, thread, kind, payload, hash, expires_at) = row?;
            let payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&payload)?;
            let action_hash = ActionDigest::new(hash).map_err(|_| std::io::Error::other("invalid action hash"))?;
            self.conn.execute("INSERT OR IGNORE INTO approval_devices(host_id,device_id,approval_id,action_hash,expires_at) VALUES(?1,?2,?3,?4,?5)", rusqlite::params![host.as_str(), device, id, action_hash.as_str(), expires_at])?;
            result.push(PendingApproval { approval_id: RequestId::new(id.to_string()).expect("positive database ID"), thread, kind, payload, action_hash, expires_at });
        }
        Ok(result)
    }

    pub fn remote_decide(&self, host: &HostId, device: &str, decision: &ApprovalDecision, now: i64) -> Result<bool, ApprovalError> {
        if decision.host_id != *host { return Ok(false); }
        let Ok(id) = decision.approval_id.as_str().parse::<i64>() else { return Ok(false); };
        // NULL hosts exist only before local legacy migration. Startup binds them before serving.
        let matches: bool = self.conn.query_row("SELECT EXISTS(SELECT 1 FROM approvals a JOIN approval_devices d ON d.approval_id=a.id WHERE a.id=?1 AND a.thread_id=?2 AND a.status='pending' AND a.action_hash=?3 AND a.expires_at=?4 AND a.expires_at>=?5 AND (a.host_id IS NULL OR a.host_id=?6) AND d.host_id=?6 AND d.device_id=?7 AND d.action_hash=a.action_hash AND d.expires_at=a.expires_at)", rusqlite::params![id, decision.thread, decision.action_hash.as_str(), decision.expires_at, now, host.as_str(), device], |row| row.get(0))?;
        if !matches { self.expire_due(now)?; return Ok(false); }
        match decision.decision {
            ApprovalVerdict::Approve => self.approve_for_thread(id, &decision.thread, now),
            ApprovalVerdict::Deny => Ok(self.deny_for_thread(id, &decision.thread, now)?),
        }
    }

}

fn intent_digest(intent: &FileWritePayload) -> Result<ActionDigest, serde_json::Error> {
    let canonical = serde_json::to_value(intent)?;
    Ok(ActionDigest::new(format!("{:x}", Sha256::digest(serde_json::to_vec(&canonical)?))).expect("SHA-256 digest"))
}

fn display_payload(intent: &FileWritePayload) -> serde_json::Value {
    let filename = intent.resolved_path.file_name().unwrap_or_default().to_string_lossy();
    let display = json!({"filename":filename,"bytes":intent.content.len()});
    serde_json::from_str(&mote_redact::redact(&display.to_string())).expect("redacted JSON")
}

fn resolve_write_target(path: &Path) -> std::io::Result<ResolvedWriteTarget> {
    let (resolved, expectation) = match path.canonicalize() {
        Ok(path) => {
            let metadata = path.metadata()?;
            (
                path,
                TargetExpectation::Existing {
                    device: metadata.dev(),
                    inode: metadata.ino(),
                },
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match path.symlink_metadata() {
                Ok(_) => return Err(error),
                Err(metadata_error)
                    if metadata_error.kind() != std::io::ErrorKind::NotFound =>
                {
                    return Err(metadata_error);
                }
                Err(_) => {}
            }
            let parent = path.parent().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "write target has no parent")
            })?;
            let filename = path.file_name().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "write target has no filename")
            })?;
            (parent.canonicalize()?.join(filename), TargetExpectation::Missing)
        }
        Err(error) => return Err(error),
    };
    let parent = resolved.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "write target has no parent")
    })?;
    let filename = resolved.file_name().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "write target has no filename")
    })?;
    Ok(ResolvedWriteTarget {
        path: resolved.clone(),
        parent: parent.to_path_buf(),
        filename: filename.to_os_string(),
        expectation,
    })
}

fn write_resolved(
    target: &ResolvedWriteTarget,
    content: &str,
    before_target_open: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let parent = open_directory(&target.parent)?;
    let identity = parent.metadata()?;
    let mut existing = match target.expectation {
        TargetExpectation::Missing => None,
        TargetExpectation::Existing { device, inode } => {
            let file = open_existing_target(&parent, &target.filename)?;
            require_identity(&file, device, inode)?;
            Some(file)
        }
    };
    before_target_open()?;
    let current = open_directory(&target.parent)?;
    let current_identity = current.metadata()?;
    if identity.dev() != current_identity.dev() || identity.ino() != current_identity.ino() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "write target parent changed during validation",
        ));
    }
    match target.expectation {
        TargetExpectation::Missing => {
            let mut file = create_target(&parent, &target.filename)?;
            file.write_all(content.as_bytes())
        }
        TargetExpectation::Existing { device, inode } => {
            let current = open_existing_target(&parent, &target.filename)?;
            require_identity(&current, device, inode)?;
            let file = existing.as_mut().expect("existing target descriptor");
            require_same_identity(file, &current)?;
            file.set_len(0)?;
            file.write_all(content.as_bytes())
        }
    }
}

fn open_existing_target(parent: &File, filename: &std::ffi::OsStr) -> std::io::Result<File> {
    open_target(
        parent,
        filename,
        libc::O_WRONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        0,
    )
}

fn create_target(parent: &File, filename: &std::ffi::OsStr) -> std::io::Result<File> {
    open_target(
        parent,
        filename,
        libc::O_WRONLY
            | libc::O_CREAT
            | libc::O_EXCL
            | libc::O_NOFOLLOW
            | libc::O_CLOEXEC,
        0o666,
    )
}

fn open_target(
    parent: &File,
    filename: &std::ffi::OsStr,
    flags: libc::c_int,
    mode: libc::c_uint,
) -> std::io::Result<File> {
    let name = c_string(filename)?;
    let descriptor = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags, mode) };
    if descriptor < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn require_identity(file: &File, device: u64, inode: u64) -> std::io::Result<()> {
    let metadata = file.metadata()?;
    if metadata.dev() != device || metadata.ino() != inode {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "write target identity changed during validation",
        ));
    }
    Ok(())
}

fn require_same_identity(first: &File, second: &File) -> std::io::Result<()> {
    let first = first.metadata()?;
    let second = second.metadata()?;
    if first.dev() != second.dev() || first.ino() != second.ino() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "write target identity changed during validation",
        ));
    }
    Ok(())
}

fn open_directory(path: &Path) -> std::io::Result<File> {
    if !path.is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "resolved write parent is not absolute",
        ));
    }
    let root = c_string(std::ffi::OsStr::new("/"))?;
    let descriptor = unsafe {
        libc::open(
            root.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut directory = unsafe { File::from_raw_fd(descriptor) };
    for component in path.components() {
        use std::path::Component;
        let Component::Normal(name) = component else {
            if matches!(component, Component::RootDir) {
                continue;
            }
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "resolved write parent contains a non-normal component",
            ));
        };
        let name = c_string(name)?;
        let descriptor = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if descriptor < 0 {
            return Err(std::io::Error::last_os_error());
        }
        directory = unsafe { File::from_raw_fd(descriptor) };
    }
    Ok(directory)
}

fn c_string(value: &std::ffi::OsStr) -> std::io::Result<CString> {
    CString::new(value.as_bytes()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "write path contains a NUL byte")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_hash_covers_canonical_exact_intent() {
        let intent = FileWritePayload { path: "/selected".into(), resolved_path: "/resolved".into(), expectation: TargetExpectation::Missing, content: "secret".into() };
        assert_eq!(intent_digest(&intent).unwrap().as_str(), "e11c1aa5f1bfb0f02d6fe781cb5366229208a1a971db55f0c0dd6a8666c82e1c");
    }

    #[test]
    fn restarted_pending_intent_is_visible_but_cannot_execute() {
        let root = temp_root("intent-restart");
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let target = root.join("held.txt");
        let path = data.join("mote.db");
        let host = HostId::from_public_key(&[4;32]);
        let w = Watchers::open(Connection::open(&path).unwrap()).unwrap();
        let WriteDecision::Pending(id) = w.request_file_write(&data, "main", &target, "private exact intent", 1000).unwrap() else { panic!("pending"); };
        let approval = w.remote_approvals(&host, "device", 1001).unwrap().remove(0);
        drop(w);
        let w = Watchers::open(Connection::open(&path).unwrap()).unwrap();
        assert_eq!(w.remote_approvals(&host, "device", 1002).unwrap(), vec![approval.clone()]);
        let mut decision = ApprovalDecision { host_id: host.clone(), thread: "main".into(), approval_id: approval.approval_id, action_hash: approval.action_hash, expires_at: approval.expires_at, decision: ApprovalVerdict::Approve };
        assert!(w.remote_decide(&host, "device", &decision, 1003).is_err());
        assert!(!target.exists());
        assert_eq!(w.status(id).unwrap(), "pending");
        decision.decision = ApprovalVerdict::Deny;
        assert!(w.remote_decide(&host, "device", &decision, 1004).unwrap());
        assert_eq!(w.status(id).unwrap(), "denied");
        drop(w);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remote_approval_binds_exact_intent_device_host_thread_and_expiry() {
        use mote_protocol::{ApprovalDecision, ApprovalVerdict, HostId};
        let root = temp_root("remote-binding");
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let target = root.join("held.txt");
        let w = Watchers::open_in_memory().unwrap();
        let WriteDecision::Pending(id) = w.request_file_write(&data, "main", &target, "sk-private-secret", 1000).unwrap() else { panic!("pending"); };
        let host = HostId::from_public_key(&[1;32]);
        let other = HostId::from_public_key(&[2;32]);
        let pending = w.remote_approvals(&host, "device", 1001).unwrap().remove(0);
        let decision = ApprovalDecision { host_id: host.clone(), thread: "main".into(), approval_id: pending.approval_id, action_hash: pending.action_hash, expires_at: pending.expires_at, decision: ApprovalVerdict::Approve };
        for altered in 0..5 {
            let mut bad = decision.clone();
            match altered {
                0 => bad.host_id = other.clone(),
                1 => bad.thread = "other".into(),
                2 => bad.action_hash = mote_protocol::ActionDigest::new("0".repeat(64)).unwrap(),
                3 => bad.expires_at += 1,
                _ => bad.approval_id = mote_protocol::RequestId::new("999").unwrap(),
            }
            assert!(!w.remote_decide(&host, "device", &bad, 1002).unwrap());
            assert!(!target.exists());
        }
        assert!(!w.remote_decide(&host, "unbound-device", &decision, 1002).unwrap());
        let payload: String = w.conn.query_row("SELECT payload FROM approvals WHERE id=?1", [id], |r| r.get(0)).unwrap();
        assert!(!payload.contains("sk-private-secret"));
        assert!(!payload.contains(root.to_str().unwrap()));
        assert!(w.remote_decide(&host, "device", &decision, 1002).unwrap());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "sk-private-secret");
        assert!(!w.remote_decide(&host, "device", &decision, 1003).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remote_approval_expires_and_exact_content_changes_digest() {
        let root = temp_root("remote-expiry");
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let w = Watchers::open_in_memory().unwrap();
        let host = mote_protocol::HostId::from_public_key(&[3;32]);
        w.request_file_write(&data, "main", &root.join("one.txt"), "one", 1000).unwrap();
        w.request_file_write(&data, "main", &root.join("one.txt"), "two", 1000).unwrap();
        let pending = w.remote_approvals(&host, "device", 1001).unwrap();
        assert_ne!(pending[0].action_hash, pending[1].action_hash);
        let first = &pending[0];
        let decision = mote_protocol::ApprovalDecision { host_id: host.clone(), thread: "main".into(), approval_id: first.approval_id.clone(), action_hash: first.action_hash.clone(), expires_at: first.expires_at, decision: mote_protocol::ApprovalVerdict::Approve };
        assert!(!w.remote_decide(&host, "device", &decision, first.expires_at + 1).unwrap());
        assert!(!root.join("one.txt").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    use std::time::Duration;

    fn temp_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mote-watchers-{name}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ))
    }

    #[test]
    fn schedule_fires_at_cron_time() {
        let w = Watchers::open_in_memory().unwrap();
        let log = EventLog::open_in_memory().unwrap();
        // Every minute at second 0. Seed last_fired at epoch.
        w.add_schedule("0 * * * * *", "stand up and stretch", true, 0).unwrap();

        // Before the first minute boundary: no fire.
        assert!(w.tick(30_000, &log).unwrap().is_empty());
        // Past the 00:01:00 boundary: fires once.
        let fired = w.tick(61_000, &log).unwrap();
        assert_eq!(fired.len(), 1);
        // The prompt landed in the schedule thread as a user turn.
        let sub = log.subscribe(SCHEDULE_THREAD, 0).unwrap();
        assert_eq!(sub.replay_remaining(), 1);
        // Ticking again at the same time does not refire.
        assert!(w.tick(61_000, &log).unwrap().is_empty());
        let _ = Duration::from_millis(1);
    }

    #[test]
    fn file_write_outside_data_dir_needs_approval() {
        let data = std::env::temp_dir().join(format!("mote-wa-data-{}", std::process::id()));
        let out = std::env::temp_dir().join(format!("mote-wa-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        let target = out.join("note.txt");

        let w = Watchers::open_in_memory().unwrap();

        // Pending: nothing executes.
        let decision = w.request_file_write(&data, "t1", &target, "hello", 1000).unwrap();
        let id = match decision {
            WriteDecision::Pending(id) => id,
            other => panic!("expected pending, got {other:?}"),
        };
        assert!(!target.exists(), "pending approval must not write");
        assert_eq!(w.status(id).unwrap(), "pending");

        // Deny leaves it unexecuted.
        assert!(w.deny(id, 1001).unwrap());
        assert!(!target.exists());
        assert_eq!(w.status(id).unwrap(), "denied");

        // A fresh approval, approved, executes.
        let id2 = match w.request_file_write(&data, "t1", &target, "world", 2000).unwrap() {
            WriteDecision::Pending(id) => id,
            other => panic!("{other:?}"),
        };
        assert!(w.approve(id2, 3000).unwrap());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "world");

        // Expiry leaves it unexecuted.
        let _ = std::fs::remove_file(&target);
        let id3 = match w.request_file_write(&data, "t1", &target, "late", 10_000).unwrap() {
            WriteDecision::Pending(id) => id,
            other => panic!("{other:?}"),
        };
        // Approve after the 24h window.
        assert!(!w.approve(id3, 10_000 + APPROVAL_TTL_MS + 1).unwrap());
        assert!(!target.exists());
        assert_eq!(w.status(id3).unwrap(), "expired");

        // A write inside the data dir executes immediately.
        let inside = data.join("ok.txt");
        assert_eq!(
            w.request_file_write(&data, "t1", &inside, "direct", 5000).unwrap(),
            WriteDecision::Executed
        );
        assert_eq!(std::fs::read_to_string(&inside).unwrap(), "direct");

        std::fs::remove_dir_all(&data).ok();
        std::fs::remove_dir_all(&out).ok();
    }

    #[test]
    fn nonexistent_traversal_target_is_held_for_approval() {
        let root = temp_root("traversal");
        let data = root.join("data");
        let escape = root.join("escape");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&escape).unwrap();
        let target = data.join("..").join("escape").join("note.txt");
        let w = Watchers::open_in_memory().unwrap();

        let decision = w.request_file_write(&data, "t1", &target, "held", 1_000).unwrap();

        let WriteDecision::Pending(id) = decision else {
            panic!("traversal target must be pending");
        };
        assert_eq!(w.status(id).unwrap(), "pending");
        assert!(!escape.join("note.txt").exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn nonexistent_child_below_symlinked_outside_parent_is_held_for_approval() {
        use std::os::unix::fs::symlink;

        let root = temp_root("symlink-parent");
        let data = root.join("data");
        let outside = root.join("outside");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        symlink(&outside, data.join("linked")).unwrap();
        let target = data.join("linked").join("note.txt");
        let w = Watchers::open_in_memory().unwrap();

        let decision = w.request_file_write(&data, "t1", &target, "held", 1_000).unwrap();

        let WriteDecision::Pending(id) = decision else {
            panic!("symlinked outside target must be pending");
        };
        assert_eq!(w.status(id).unwrap(), "pending");
        assert!(!outside.join("note.txt").exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn unresolved_existing_symlink_target_fails_closed() {
        use std::os::unix::fs::symlink;

        let root = temp_root("unresolved-symlink");
        let data = root.join("data");
        let outside = root.join("outside");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let outside_target = outside.join("note.txt");
        let target = data.join("linked-file");
        symlink(&outside_target, &target).unwrap();
        let w = Watchers::open_in_memory().unwrap();

        assert!(matches!(
            w.request_file_write(&data, "t1", &target, "blocked", 1_000),
            Err(ApprovalError::Filesystem(_))
        ));
        assert!(!outside_target.exists());
        let approvals: i64 =
            w.conn.query_row("SELECT COUNT(*) FROM approvals", [], |row| row.get(0)).unwrap();
        assert_eq!(approvals, 0);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn failed_direct_inside_write_returns_an_error_without_an_approval() {
        let root = temp_root("inside-failure");
        let data = root.join("data");
        let target = data.join("directory-target");
        std::fs::create_dir_all(&target).unwrap();
        let w = Watchers::open_in_memory().unwrap();

        assert!(matches!(
            w.request_file_write(&data, "t1", &target, "nope", 1_000),
            Err(ApprovalError::Filesystem(_))
        ));
        let approvals: i64 =
            w.conn.query_row("SELECT COUNT(*) FROM approvals", [], |row| row.get(0)).unwrap();
        assert_eq!(approvals, 0);
        assert!(target.is_dir());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn failed_approved_outside_write_stays_pending_and_can_be_denied() {
        let root = temp_root("approved-failure");
        let data = root.join("data");
        let outside = root.join("outside");
        let target = outside.join("directory-target");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        let w = Watchers::open_in_memory().unwrap();
        let WriteDecision::Pending(id) =
            w.request_file_write(&data, "t1", &target, "nope", 1_000).unwrap()
        else {
            panic!("outside target must be pending");
        };

        assert!(matches!(w.approve(id, 1_001), Err(ApprovalError::Filesystem(_))));
        assert_eq!(w.status(id).unwrap(), "pending");
        assert!(w.deny(id, 1_002).unwrap());
        assert_eq!(w.status(id).unwrap(), "denied");
        assert!(target.is_dir());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn malformed_pending_payload_stays_pending_and_can_be_denied() {
        let w = Watchers::open_in_memory().unwrap();
        w.conn
            .execute(
                "INSERT INTO approvals(thread_id, kind, payload, status, expires_at, created_at)
                 VALUES('t1', 'file_write', '{not-json', 'pending', 10000, 1000)",
                [],
            )
            .unwrap();
        let id = w.conn.last_insert_rowid();

        assert!(matches!(w.approve(id, 1_001), Err(ApprovalError::Payload(_))));
        assert_eq!(w.status(id).unwrap(), "pending");
        assert!(w.deny(id, 1_002).unwrap());
        assert_eq!(w.status(id).unwrap(), "denied");
    }

    #[cfg(unix)]
    #[test]
    fn parent_swap_after_validation_cannot_redirect_an_inside_write() {
        use std::os::unix::fs::symlink;

        let root = temp_root("inside-parent-swap");
        let data = root.join("data");
        let parent = data.join("safe");
        let parked = data.join("parked");
        let outside = root.join("outside");
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let target = parent.join("note.txt");
        let redirected = outside.join("note.txt");
        let w = Watchers::open_in_memory().unwrap();

        let result = w.request_file_write_with_hook(
            &data,
            "t1",
            &target,
            "blocked",
            1_000,
            || {
                std::fs::rename(&parent, &parked)?;
                symlink(&outside, &parent)
            },
        );

        assert!(matches!(result, Err(ApprovalError::Filesystem(_))));
        assert!(!redirected.exists());
        assert!(!parked.join("note.txt").exists());
        let approvals: i64 =
            w.conn.query_row("SELECT COUNT(*) FROM approvals", [], |row| row.get(0)).unwrap();
        assert_eq!(approvals, 0);
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn parent_swap_after_validation_keeps_approved_write_pending() {
        use std::os::unix::fs::symlink;

        let root = temp_root("approved-parent-swap");
        let data = root.join("data");
        let parent = root.join("approved");
        let parked = root.join("parked");
        let outside = root.join("outside");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&parent).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let target = parent.join("note.txt");
        let redirected = outside.join("note.txt");
        let w = Watchers::open_in_memory().unwrap();
        let WriteDecision::Pending(id) =
            w.request_file_write(&data, "t1", &target, "blocked", 1_000).unwrap()
        else {
            panic!("outside target must be pending");
        };

        let result = w.approve_for_thread_with_hook(id, "t1", 1_001, || {
            std::fs::rename(&parent, &parked)?;
            symlink(&outside, &parent)
        });

        assert!(matches!(result, Err(ApprovalError::Filesystem(_))));
        assert!(!redirected.exists());
        assert!(!parked.join("note.txt").exists());
        assert_eq!(w.status(id).unwrap(), "pending");
        assert!(w.deny(id, 1_002).unwrap());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn target_swap_after_validation_cannot_replace_an_inside_file() {
        let root = temp_root("inside-target-swap");
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let target = data.join("note.txt");
        let parked = data.join("parked.txt");
        let replacement = data.join("replacement.txt");
        std::fs::write(&target, "original").unwrap();
        std::fs::write(&replacement, "replacement").unwrap();
        let w = Watchers::open_in_memory().unwrap();

        let result = w.request_file_write_with_hook(
            &data,
            "t1",
            &target,
            "blocked",
            1_000,
            || {
                std::fs::rename(&target, &parked)?;
                std::fs::rename(&replacement, &target)
            },
        );

        assert!(matches!(result, Err(ApprovalError::Filesystem(_))));
        assert_eq!(std::fs::read_to_string(&parked).unwrap(), "original");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "replacement");
        let approvals: i64 =
            w.conn.query_row("SELECT COUNT(*) FROM approvals", [], |row| row.get(0)).unwrap();
        assert_eq!(approvals, 0);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn target_hardlink_swap_after_validation_keeps_approval_pending() {
        let root = temp_root("approved-target-swap");
        let data = root.join("data");
        let outside = root.join("outside");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let target = outside.join("note.txt");
        let parked = outside.join("parked.txt");
        let bait = outside.join("bait.txt");
        std::fs::write(&target, "original").unwrap();
        std::fs::write(&bait, "replacement").unwrap();
        let w = Watchers::open_in_memory().unwrap();
        let WriteDecision::Pending(id) =
            w.request_file_write(&data, "t1", &target, "blocked", 1_000).unwrap()
        else {
            panic!("outside target must be pending");
        };

        let result = w.approve_for_thread_with_hook(id, "t1", 1_001, || {
            std::fs::rename(&target, &parked)?;
            std::fs::hard_link(&bait, &target)
        });

        assert!(matches!(result, Err(ApprovalError::Filesystem(_))));
        assert_eq!(std::fs::read_to_string(&parked).unwrap(), "original");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "replacement");
        assert_eq!(std::fs::read_to_string(&bait).unwrap(), "replacement");
        assert_eq!(w.status(id).unwrap(), "pending");
        assert!(w.deny(id, 1_002).unwrap());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn missing_inside_target_created_after_validation_is_not_truncated() {
        let root = temp_root("missing-target-swap");
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let target = data.join("note.txt");
        let w = Watchers::open_in_memory().unwrap();

        let result = w.request_file_write_with_hook(
            &data,
            "t1",
            &target,
            "blocked",
            1_000,
            || std::fs::write(&target, "replacement"),
        );

        assert!(matches!(result, Err(ApprovalError::Filesystem(_))));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "replacement");
        let approvals: i64 =
            w.conn.query_row("SELECT COUNT(*) FROM approvals", [], |row| row.get(0)).unwrap();
        assert_eq!(approvals, 0);
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn pending_symlink_retarget_cannot_change_the_approved_destination() {
        use std::os::unix::fs::symlink;

        let root = temp_root("pending-symlink-retarget");
        let data = root.join("data");
        let outside = root.join("outside");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let first = outside.join("first.txt");
        let second = outside.join("second.txt");
        let link = outside.join("selected.txt");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        symlink(&first, &link).unwrap();
        let w = Watchers::open_in_memory().unwrap();
        let WriteDecision::Pending(id) =
            w.request_file_write(&data, "t1", &link, "blocked", 1_000).unwrap()
        else {
            panic!("outside target must be pending");
        };
        let card = w.approval_card(id).unwrap().to_string();
        assert!(card.contains("first.txt"));
        assert!(!card.contains("selected.txt"));
        assert!(!card.contains(outside.to_str().unwrap()));
        std::fs::remove_file(&link).unwrap();
        symlink(&second, &link).unwrap();

        assert!(matches!(w.approve(id, 1_001), Err(ApprovalError::Filesystem(_))));
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second");
        assert_eq!(w.status(id).unwrap(), "pending");
        assert!(w.deny(id, 1_002).unwrap());
        std::fs::remove_dir_all(root).ok();
    }
    #[test]
    fn legacy_migration_preserves_decisions_and_binds_only_local_host() {
        let root = temp_root("legacy-migration");
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let root = root.canonicalize().unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE approvals(id INTEGER PRIMARY KEY AUTOINCREMENT,thread_id TEXT NOT NULL,kind TEXT NOT NULL,payload TEXT NOT NULL,status TEXT NOT NULL,expires_at INTEGER NOT NULL,created_at INTEGER NOT NULL)").unwrap();
        for (name, expiry) in [("approve.txt",2000),("deny.txt",2000),("expire.txt",1000),("failure.txt",2000)] {
            let path = root.join(name);
            let intent = FileWritePayload { path:path.clone(),resolved_path:path,expectation:TargetExpectation::Missing,content:"sk-abcdefghijklmnopqrstuv".into() };
            conn.execute("INSERT INTO approvals(thread_id,kind,payload,status,expires_at,created_at) VALUES('thread','file_write',?1,'pending',?2,1000)",rusqlite::params![serde_json::to_string(&intent).unwrap(),expiry]).unwrap();
        }
        let watchers = Watchers::open(conn).unwrap();
        let home = HostId::from_public_key(b"home");
        let office = HostId::from_public_key(b"office");
        assert!(watchers.approval_store().is_err());
        watchers.bind_local_host(&home).unwrap();
        assert!(watchers.approval_store().is_ok());
        assert!(watchers.bind_local_host(&office).is_err());
        assert!(watchers.remote_approvals(&office,"foreign-device",1000).unwrap().is_empty());
        let cards = watchers.remote_approvals(&home,"owner",1000).unwrap();
        assert_eq!(cards.len(),4);
        assert!(!serde_json::to_string(&cards).unwrap().contains("sk-abcdefghijklmnopqrstuv"));
        assert!(!serde_json::to_string(&cards).unwrap().contains(root.to_str().unwrap()));
        assert!(watchers.approve_for_thread(1,"thread",1001).unwrap());
        assert!(!watchers.approve_for_thread(1,"thread",1002).unwrap());
        assert_eq!(std::fs::read_to_string(root.join("approve.txt")).unwrap(),"sk-abcdefghijklmnopqrstuv");
        assert!(watchers.deny_for_thread(2,"thread",1001).unwrap());
        assert!(!root.join("deny.txt").exists());
        assert!(!watchers.approve_for_thread(3,"thread",1001).unwrap());
        assert_eq!(watchers.status(3).unwrap(),"expired");
        std::fs::create_dir(root.join("failure.txt")).unwrap();
        assert!(watchers.approve_for_thread(4,"thread",1001).is_err());
        assert_eq!(watchers.status(4).unwrap(),"pending");
        assert!(watchers.deny_for_thread(4,"thread",1002).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }
}
