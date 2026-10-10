//! Watchers and approvals (docs/rfcs/v0.md section 15).
//!
//! `schedules(id, cron, prompt, enabled)` fires a prompt into a thread at its cron time, as if
//! the owner typed it. `approvals(id, thread_id, kind, payload, status, expires_at)` holds every
//! outward action; in v0 the only kind is `file_write` outside the data directory. Nothing with
//! status `pending` executes. Approvals expire after 24 hours.

use chrono::{TimeZone, Utc};
use mote_eventlog::EventLog;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::json;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

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

#[derive(Deserialize)]
struct FileWritePayload {
    path: PathBuf,
    content: String,
}

pub struct Watchers {
    conn: Connection,
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
        Ok(Self { conn })
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
        if is_inside(data_dir, path)? {
            std::fs::write(path, content)?;
            return Ok(WriteDecision::Executed);
        }
        let payload = json!({ "path": path.to_string_lossy(), "content": content }).to_string();
        self.conn.execute(
            "INSERT INTO approvals(thread_id, kind, payload, status, expires_at, created_at)
             VALUES(?1, 'file_write', ?2, 'pending', ?3, ?4)",
            rusqlite::params![thread_id, payload, now + APPROVAL_TTL_MS, now],
        )?;
        Ok(WriteDecision::Pending(self.conn.last_insert_rowid()))
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
        self.expire_due(now)?;
        let payload = match self.conn.query_row(
            "SELECT payload FROM approvals
             WHERE id=?1 AND thread_id=?2 AND status='pending' AND expires_at>=?3",
            rusqlite::params![id, thread_id, now],
            |row| row.get::<_, String>(0),
        ) {
            Ok(payload) => payload,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        let write: FileWritePayload = serde_json::from_str(&payload)?;
        std::fs::write(write.path, write.content)?;
        let changed = self.conn.execute(
            "UPDATE approvals SET status='approved'
             WHERE id=?1 AND thread_id=?2 AND status='pending' AND expires_at>=?3",
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
        let path = v["path"].as_str().unwrap_or("");
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

}

fn is_inside(dir: &Path, path: &Path) -> std::io::Result<bool> {
    let resolved_dir = dir.canonicalize()?;
    let resolved_path = match path.canonicalize() {
        Ok(path) => path,
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
            parent.canonicalize()?.join(filename)
        }
        Err(error) => return Err(error),
    };
    Ok(resolved_path.starts_with(resolved_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
