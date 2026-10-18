//! Append-only event log with cursor replay and live streaming.
//!
//! One SQLite table `events(seq, ts, scope_id, kind, payload)`. `payload` is JSON that has
//! already passed through [`mote_redact::redact`] before storage. A subscriber supplies a
//! cursor and receives every event with `seq > cursor` in order, then live events, with no
//! duplicate. Retention keeps [`RETENTION_LIMIT`] real events per scope behind one `retention`
//! marker. A subscriber that falls more than [`LAG_LIMIT`] events behind receives a single
//! `resync` event and recovers by replaying from the log.

use mote_protocol::{Cursor, HostId, HostSnapshot, ThreadProjection};
use rusqlite::{Connection, OptionalExtension};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Real events kept per scope before older ones are pruned.
pub const RETENTION_LIMIT: i64 = 5000;
/// A subscriber more than this many events behind is told to resync.
pub const LAG_LIMIT: usize = 1000;

pub use mote_protocol::Event;

type Queue = Arc<(Mutex<VecDeque<Event>>, Condvar)>;

struct SubHandle {
    scope: Option<String>,
    queue: Queue,
    alive: Arc<AtomicBool>,
}

struct Inner {
    conn: Connection,
    subs: Vec<SubHandle>,
}

/// Handle to the event log. Cheap to clone; all clones share one connection and subscriber set.
#[derive(Clone)]
pub struct EventLog {
    inner: Arc<Mutex<Inner>>,
}

pub enum Replay {
    Events(Subscription),
    ResyncRequired { oldest_retained: i64 },
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl EventLog {
    pub fn database_path(&self) -> Option<std::path::PathBuf> {
        self.inner
            .lock()
            .unwrap()
            .conn
            .path()
            .filter(|path| !path.is_empty())
            .map(std::path::PathBuf::from)
    }

    /// Open the log at `path`, creating the table if needed.
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        Self::from_conn(conn)
    }

    /// Open an in-memory log, for tests.
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::from_conn(Connection::open_in_memory()?)
    }

    fn from_conn(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS events(
                 seq INTEGER PRIMARY KEY AUTOINCREMENT,
                 ts INTEGER NOT NULL,
                 scope_id TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 payload TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS events_scope_seq ON events(scope_id, seq);
             CREATE TABLE IF NOT EXISTS client_threads(scope_id TEXT PRIMARY KEY, projection TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS replay_floor(scope_id TEXT PRIMARY KEY, seq INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS projection_version(version INTEGER PRIMARY KEY);",
        )?;
        let initialized: bool =
            conn.query_row("SELECT EXISTS(SELECT 1 FROM projection_version)", [], |r| {
                r.get(0)
            })?;
        if !initialized {
            let tx = conn.unchecked_transaction()?;
            let events: Vec<Event> = tx
                .prepare("SELECT seq, ts, scope_id, kind, payload FROM events ORDER BY seq")?
                .query_map([], event_row)?
                .collect::<rusqlite::Result<_>>()?;
            for event in events {
                Inner::project(&tx, &event)?;
            }
            tx.execute("INSERT INTO replay_floor SELECT scope_id, MAX(0, MIN(seq)-1) FROM events WHERE kind!='retention' AND scope_id IN (SELECT scope_id FROM events WHERE kind='retention') GROUP BY scope_id", [])?;
            tx.execute("INSERT INTO projection_version VALUES(1)", [])?;
            tx.commit()?;
        }
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                conn,
                subs: Vec::new(),
            })),
        })
    }

    /// Append an event. The payload is redacted before storage. Returns the stored event with
    /// its assigned `seq`.
    pub fn append(
        &self,
        scope_id: &str,
        kind: &str,
        payload: &serde_json::Value,
    ) -> rusqlite::Result<Event> {
        self.append_with(scope_id, kind, payload, |_| Ok(()))
            .map(|(_, event)| event)
    }

    /// The mutation and redacted event commit together before subscribers are notified.
    /// The callback must not reenter this log or perform external effects.
    pub fn append_with<T>(
        &self,
        scope_id: &str,
        kind: &str,
        payload: &serde_json::Value,
        apply: impl FnOnce(&rusqlite::Transaction<'_>) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<(T, Event)> {
        let redacted_text = mote_redact::redact(&payload.to_string());
        let mut guard = self.inner.lock().unwrap();
        let (value, ev) = {
            let transaction = guard.conn.transaction()?;
            let value = apply(&transaction)?;
            let ev = Inner::insert(&transaction, scope_id, kind, &redacted_text)?;
            Inner::project(&transaction, &ev)?;
            Inner::enforce_retention(&transaction, scope_id)?;
            transaction.commit()?;
            (value, ev)
        };
        guard.broadcast(&ev);
        Ok((value, ev))
    }

    /// Subscribe to a scope from `cursor`. The returned subscription first yields every stored
    /// event with `seq > cursor`, then live events.
    pub fn subscribe(&self, scope_id: &str, cursor: i64) -> rusqlite::Result<Subscription> {
        let mut guard = self.inner.lock().unwrap();
        guard.subscribe(scope_id, cursor)
    }

    pub fn oldest_retained(&self, scope_id: &str) -> rusqlite::Result<Option<i64>> {
        self.inner.lock().unwrap().conn.query_row(
            "SELECT MIN(seq) FROM events WHERE scope_id=?1 AND kind!='retention'",
            [scope_id],
            |r| r.get(0),
        )
    }

    pub fn subscribe_checked(&self, scope_id: &str, cursor: i64) -> rusqlite::Result<Replay> {
        let mut guard = self.inner.lock().unwrap();
        let floor: i64 = guard
            .conn
            .query_row(
                "SELECT seq FROM replay_floor WHERE scope_id=?1",
                [scope_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        if cursor < floor || cursor < 0 || cursor > guard.max_seq()? {
            let oldest_retained = guard.conn.query_row(
                "SELECT COALESCE(MIN(seq),0) FROM events WHERE scope_id=?1 AND kind!='retention'",
                [scope_id],
                |r| r.get(0),
            )?;
            return Ok(Replay::ResyncRequired { oldest_retained });
        }
        guard.subscribe(scope_id, cursor).map(Replay::Events)
    }

    pub fn subscribe_host(&self, cursor: i64) -> rusqlite::Result<Replay> {
        let mut guard = self.inner.lock().unwrap();
        let floor: i64 =
            guard
                .conn
                .query_row("SELECT COALESCE(MAX(seq),0) FROM replay_floor", [], |r| {
                    r.get(0)
                })?;
        if cursor < floor || cursor < 0 || cursor > guard.max_seq()? {
            let oldest_retained = guard.conn.query_row(
                "SELECT COALESCE(MIN(seq),0) FROM events WHERE kind!='retention'",
                [],
                |r| r.get(0),
            )?;
            return Ok(Replay::ResyncRequired { oldest_retained });
        }
        guard.subscribe_to(None, cursor).map(Replay::Events)
    }

    pub fn host_snapshot(&self, host_id: &HostId) -> rusqlite::Result<HostSnapshot> {
        self.snapshot(host_id, None)
    }

    pub fn host_snapshot_for_device(
        &self,
        host_id: &HostId,
        device: &str,
        now: i64,
    ) -> rusqlite::Result<HostSnapshot> {
        self.snapshot(host_id, Some((device, now)))
    }

    fn snapshot(
        &self,
        host_id: &HostId,
        device: Option<(&str, i64)>,
    ) -> rusqlite::Result<HostSnapshot> {
        let guard = self.inner.lock().unwrap();
        let tx = guard.conn.unchecked_transaction()?;
        let seq = tx.query_row("SELECT COALESCE(MAX(seq),0) FROM events", [], |r| r.get(0))?;
        let mut threads = Vec::new();
        let mut surfaces = Vec::new();
        let projections: Vec<String> = tx
            .prepare("SELECT projection FROM client_threads ORDER BY scope_id")?
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for encoded in projections {
            let projection: ThreadProjection =
                serde_json::from_str(&encoded).map_err(|_| rusqlite::Error::InvalidQuery)?;
            threads.push(projection.thread);
            if let Some(surface) = projection.surface {
                surfaces.push(surface);
            }
        }
        let mut pending_approvals = Vec::new();
        if let Some((device, now)) = device {
            let has_approvals: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='approvals')", [], |r| r.get(0))?;
            if has_approvals {
                let columns: Vec<String> = tx
                    .prepare("PRAGMA table_info(approvals)")?
                    .query_map([], |row| row.get(1))?
                    .collect::<rusqlite::Result<_>>()?;
                if !columns.iter().any(|column| column == "host_id") {
                    tx.execute_batch("ALTER TABLE approvals ADD COLUMN host_id TEXT")?;
                }
                tx.execute(
                    "UPDATE approvals SET host_id=?1 WHERE host_id IS NULL AND kind='file_write'",
                    [host_id.as_str()],
                )?;
                let approvals: Vec<(i64, String, String, String, String, i64)> = tx.prepare("SELECT id,thread_id,kind,payload,action_hash,expires_at FROM approvals WHERE host_id=?2 AND status='pending' AND action_hash IS NOT NULL AND expires_at>=?1 ORDER BY id")?.query_map(rusqlite::params![now, host_id.as_str()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?.collect::<rusqlite::Result<_>>()?;
                for (id, thread, kind, payload, hash, expires_at) in approvals {
                    let approval = mote_protocol::PendingApproval {
                        approval_id: mote_protocol::RequestId::new(id.to_string())
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        thread,
                        kind,
                        payload: serde_json::from_str(&mote_redact::redact(&payload))
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        action_hash: mote_protocol::ActionDigest::new(hash)
                            .map_err(|_| rusqlite::Error::InvalidQuery)?,
                        expires_at,
                    };
                    if approval.kind == "file_write" {
                        tx.execute("INSERT OR IGNORE INTO approval_devices(host_id,device_id,approval_id,action_hash,expires_at) VALUES(?1,?2,?3,?4,?5)", rusqlite::params![host_id.as_str(),device,id,approval.action_hash.as_str(),expires_at])?;
                    }
                    pending_approvals.push(approval);
                }
            }
        }
        tx.commit()?;
        Ok(HostSnapshot {
            host_id: host_id.clone(),
            cursor: Cursor::new(host_id.clone(), seq).map_err(|_| rusqlite::Error::InvalidQuery)?,
            threads,
            surfaces,
            pending_approvals,
            status: mote_protocol::HostStatus::Online,
        })
    }

    /// Current highest seq, or 0 if empty.
    pub fn max_seq(&self) -> rusqlite::Result<i64> {
        self.inner.lock().unwrap().max_seq()
    }

    /// Fetch a single event by seq, if it still exists.
    pub fn get(&self, seq: i64) -> rusqlite::Result<Option<Event>> {
        let guard = self.inner.lock().unwrap();
        let mut stmt = guard
            .conn
            .prepare("SELECT seq, ts, scope_id, kind, payload FROM events WHERE seq=?1")?;
        let mut rows = stmt.query_map([seq], |r| {
            let payload_text: String = r.get(4)?;
            let payload = serde_json::from_str(&payload_text)
                .unwrap_or(serde_json::Value::String(payload_text));
            Ok(Event {
                seq: r.get(0)?,
                ts: r.get(1)?,
                scope_id: r.get(2)?,
                kind: r.get(3)?,
                payload,
            })
        })?;
        match rows.next() {
            Some(ev) => Ok(Some(ev?)),
            None => Ok(None),
        }
    }

    /// Count of real (non-retention) events in a scope.
    pub fn count(&self, scope_id: &str) -> rusqlite::Result<i64> {
        let guard = self.inner.lock().unwrap();
        guard.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE scope_id=?1 AND kind!='retention'",
            [scope_id],
            |r| r.get(0),
        )
    }

    /// Read at most `limit` events at or after `since_ts`, newest first.
    pub fn recent_since(&self, since_ts: i64, limit: usize) -> rusqlite::Result<Vec<Event>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let guard = self.inner.lock().unwrap();
        let mut stmt = guard.conn.prepare(
            "SELECT seq, ts, scope_id, kind, payload FROM events
             WHERE ts>=?1 ORDER BY seq DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(rusqlite::params![since_ts, limit as i64], |row| {
            let payload_text: String = row.get(4)?;
            let payload = serde_json::from_str(&payload_text)
                .unwrap_or(serde_json::Value::String(payload_text));
            Ok(Event {
                seq: row.get(0)?,
                ts: row.get(1)?,
                scope_id: row.get(2)?,
                kind: row.get(3)?,
                payload,
            })
        })?;
        rows.collect()
    }

    /// Read at most `limit` events of `kind` at or after `since_ts`, newest first.
    pub fn recent_kind_since(
        &self,
        kind: &str,
        since_ts: i64,
        limit: usize,
    ) -> rusqlite::Result<Vec<Event>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let guard = self.inner.lock().unwrap();
        let mut stmt = guard.conn.prepare(
            "SELECT seq, ts, scope_id, kind, payload FROM events
             WHERE kind=?1 AND ts>=?2 ORDER BY seq DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(rusqlite::params![kind, since_ts, limit as i64], |row| {
            let payload_text: String = row.get(4)?;
            let payload = serde_json::from_str(&payload_text)
                .unwrap_or(serde_json::Value::String(payload_text));
            Ok(Event {
                seq: row.get(0)?,
                ts: row.get(1)?,
                scope_id: row.get(2)?,
                kind: row.get(3)?,
                payload,
            })
        })?;
        rows.collect()
    }
}

impl Inner {
    fn subscribe(&mut self, scope_id: &str, cursor: i64) -> rusqlite::Result<Subscription> {
        self.subscribe_to(Some(scope_id), cursor)
    }

    fn subscribe_to(
        &mut self,
        scope_id: Option<&str>,
        cursor: i64,
    ) -> rusqlite::Result<Subscription> {
        let replay = self.read_after(scope_id, cursor)?;
        let queue: Queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let alive = Arc::new(AtomicBool::new(true));
        self.subs.push(SubHandle {
            scope: scope_id.map(str::to_owned),
            queue: queue.clone(),
            alive: alive.clone(),
        });
        Ok(Subscription {
            replay: replay.into(),
            queue,
            alive,
        })
    }

    fn project(conn: &Connection, event: &Event) -> rusqlite::Result<()> {
        if !ThreadProjection::handles(&event.kind) {
            return Ok(());
        }
        let encoded: Option<String> = conn
            .query_row(
                "SELECT projection FROM client_threads WHERE scope_id=?1",
                [&event.scope_id],
                |r| r.get(0),
            )
            .optional()?;
        let mut projection = match encoded {
            Some(encoded) => {
                serde_json::from_str(&encoded).map_err(|_| rusqlite::Error::InvalidQuery)?
            }
            None => ThreadProjection::new(event.scope_id.clone()),
        };
        projection.apply(&mote_protocol::Event {
            seq: event.seq,
            ts: event.ts,
            scope_id: event.scope_id.clone(),
            kind: event.kind.clone(),
            payload: event.payload.clone(),
        });
        let encoded =
            serde_json::to_string(&projection).map_err(|_| rusqlite::Error::InvalidQuery)?;
        conn.execute("INSERT INTO client_threads(scope_id,projection) VALUES(?1,?2) ON CONFLICT(scope_id) DO UPDATE SET projection=excluded.projection", rusqlite::params![event.scope_id, encoded])?;
        Ok(())
    }
    fn insert(
        conn: &Connection,
        scope_id: &str,
        kind: &str,
        payload_text: &str,
    ) -> rusqlite::Result<Event> {
        let ts = now_millis();
        conn.execute(
            "INSERT INTO events(ts, scope_id, kind, payload) VALUES(?1, ?2, ?3, ?4)",
            rusqlite::params![ts, scope_id, kind, payload_text],
        )?;
        let seq = conn.last_insert_rowid();
        let payload = serde_json::from_str(payload_text)
            .unwrap_or_else(|_| serde_json::Value::String(payload_text.to_string()));
        Ok(Event {
            seq,
            ts,
            scope_id: scope_id.to_string(),
            kind: kind.to_string(),
            payload,
        })
    }

    fn max_seq(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get(0))
    }

    fn read_after(&self, scope_id: Option<&str>, cursor: i64) -> rusqlite::Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, ts, scope_id, kind, payload FROM events
             WHERE (?1 IS NULL OR scope_id=?1) AND seq>?2 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(rusqlite::params![scope_id, cursor], |r| {
            let payload_text: String = r.get(4)?;
            let payload = serde_json::from_str(&payload_text)
                .unwrap_or(serde_json::Value::String(payload_text));
            Ok(Event {
                seq: r.get(0)?,
                ts: r.get(1)?,
                scope_id: r.get(2)?,
                kind: r.get(3)?,
                payload,
            })
        })?;
        rows.collect()
    }

    fn broadcast(&mut self, ev: &Event) {
        self.subs.retain(|s| s.alive.load(Ordering::Acquire));
        for s in &self.subs {
            if s.scope.as_ref().is_some_and(|scope| scope != &ev.scope_id) {
                continue;
            }
            let (lock, cv) = &*s.queue;
            let mut q = lock.lock().unwrap();
            q.push_back(ev.clone());
            if q.len() > LAG_LIMIT {
                // Subscriber is too far behind; collapse the backlog into one resync signal.
                q.clear();
                q.push_back(Event {
                    seq: ev.seq,
                    ts: ev.ts,
                    scope_id: ev.scope_id.clone(),
                    kind: "resync".to_string(),
                    payload: serde_json::Value::Null,
                });
            }
            cv.notify_one();
        }
    }

    fn enforce_retention(conn: &Connection, scope_id: &str) -> rusqlite::Result<()> {
        let real: i64 = conn.query_row(
            "SELECT COUNT(*) FROM events WHERE scope_id=?1 AND kind!='retention'",
            [scope_id],
            |r| r.get(0),
        )?;
        if real <= RETENTION_LIMIT {
            return Ok(());
        }
        let excess = real - RETENTION_LIMIT;
        let floor: i64 = conn.query_row("SELECT MAX(seq) FROM (SELECT seq FROM events WHERE scope_id=?1 AND kind!='retention' ORDER BY seq LIMIT ?2)", rusqlite::params![scope_id, excess], |r| r.get(0))?;
        conn.execute("INSERT INTO replay_floor(scope_id,seq) VALUES(?1,?2) ON CONFLICT(scope_id) DO UPDATE SET seq=excluded.seq", rusqlite::params![scope_id, floor])?;
        // Delete the oldest real events for this scope.
        conn.execute(
            "DELETE FROM events WHERE seq IN (
                 SELECT seq FROM events WHERE scope_id=?1 AND kind!='retention'
                 ORDER BY seq ASC LIMIT ?2)",
            rusqlite::params![scope_id, excess],
        )?;
        // Ensure exactly one retention marker exists for the scope, as the oldest row.
        let markers: i64 = conn.query_row(
            "SELECT COUNT(*) FROM events WHERE scope_id=?1 AND kind='retention'",
            [scope_id],
            |r| r.get(0),
        )?;
        if markers == 0 {
            let ts = now_millis();
            conn.execute(
                "INSERT INTO events(ts, scope_id, kind, payload) VALUES(?1, ?2, 'retention', ?3)",
                rusqlite::params![ts, scope_id, "{\"pruned\":true}"],
            )?;
        }
        Ok(())
    }
}

fn event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    let text: String = row.get(4)?;
    Ok(Event {
        seq: row.get(0)?,
        ts: row.get(1)?,
        scope_id: row.get(2)?,
        kind: row.get(3)?,
        payload: serde_json::from_str(&text).map_err(|_| rusqlite::Error::InvalidQuery)?,
    })
}

/// A live subscription. Call [`Subscription::recv_timeout`] to pull the next event.
pub struct Subscription {
    replay: VecDeque<Event>,
    queue: Queue,
    alive: Arc<AtomicBool>,
}

impl Subscription {
    /// Pull the next event, waiting up to `timeout` for a live one. Replay events are returned
    /// first and never block.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Option<Event> {
        if let Some(ev) = self.replay.pop_front() {
            return Some(ev);
        }
        let (lock, cv) = &*self.queue;
        let mut q = lock.lock().unwrap();
        if let Some(ev) = q.pop_front() {
            return Some(ev);
        }
        let (mut q, _) = cv.wait_timeout(q, timeout).unwrap();
        q.pop_front()
    }

    /// Number of replay events not yet consumed.
    pub fn replay_remaining(&self) -> usize {
        self.replay.len()
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(s: &str) -> serde_json::Value {
        serde_json::json!({ "msg": s })
    }

    #[test]
    fn approval_snapshot_grants_only_records_owned_by_this_host() {
        let log = EventLog::open_in_memory().unwrap();
        let host = HostId::from_public_key(&[4; 32]);
        let other = HostId::from_public_key(&[5; 32]);
        {
            let guard = log.inner.lock().unwrap();
            guard.conn.execute_batch("CREATE TABLE approvals(id INTEGER PRIMARY KEY,host_id TEXT,thread_id TEXT,kind TEXT,payload TEXT,action_hash TEXT,status TEXT,expires_at INTEGER);
                CREATE TABLE approval_devices(host_id TEXT,device_id TEXT,approval_id INTEGER,action_hash TEXT,expires_at INTEGER);").unwrap();
            for (id, owner) in [(1, host.as_str()), (2, other.as_str())] {
                guard.conn.execute("INSERT INTO approvals VALUES(?1,?2,'main','browser','{}',?3,'pending',1000)", rusqlite::params![id, owner, "a".repeat(64)]).unwrap();
            }
            guard
                .conn
                .execute(
                    "INSERT INTO approvals VALUES(3,NULL,'main','browser','{}',?1,'pending',1000)",
                    ["b".repeat(64)],
                )
                .unwrap();
        }
        let snapshot = log.host_snapshot_for_device(&host, "ipad", 100).unwrap();
        assert_eq!(snapshot.pending_approvals.len(), 1);
        assert_eq!(snapshot.pending_approvals[0].approval_id.as_str(), "1");
        let grants: i64 = log
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT count(*) FROM approval_devices", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(grants, 0);
    }

    #[test]
    fn failed_projection_rolls_back_event_and_live_delivery() {
        let log = EventLog::open_in_memory().unwrap();
        let host = HostId::from_public_key(&[4; 32]);
        log.append("main", "turn.user", &serde_json::json!({"text":"prior"}))
            .unwrap();
        let before = log.host_snapshot(&host).unwrap();
        let mut live = log.subscribe("main", 1).unwrap();
        log.inner.lock().unwrap().conn.execute_batch("CREATE TRIGGER fail_projection BEFORE UPDATE ON client_threads BEGIN SELECT RAISE(FAIL, 'projection failed'); END;").unwrap();
        assert!(log
            .append("main", "turn.user", &serde_json::json!({"text":"lost"}))
            .is_err());
        assert_eq!(log.host_snapshot(&host).unwrap(), before);
        assert!(live.recv_timeout(Duration::from_millis(1)).is_none());
    }

    #[test]
    fn appended_mutation_and_event_commit_together() {
        let log = EventLog::open_in_memory().unwrap();
        let mut live = log.subscribe("main", 0).unwrap();
        let (value, event) = log.append_with("main", "task.changed",
            &serde_json::json!({"api_key":"private-value"}), |tx| {
                tx.execute_batch("CREATE TABLE task_probe(value INTEGER); INSERT INTO task_probe VALUES(42);")?;
                Ok(42)
            }).unwrap();
        assert_eq!(value, 42);
        assert!(!event.payload.to_string().contains("private-value"));
        assert_eq!(
            live.recv_timeout(Duration::from_millis(1)).unwrap().seq,
            event.seq
        );
        let stored: i64 = log
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT value FROM task_probe", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, value);
    }

    #[test]
    fn failed_mutation_rolls_back_and_broadcasts_nothing() {
        let log = EventLog::open_in_memory().unwrap();
        let mut live = log.subscribe("main", 0).unwrap();
        let result = log.append_with("main", "task.changed", &serde_json::json!({}), |tx| {
            tx.execute_batch(
                "CREATE TABLE task_probe(value INTEGER); INSERT INTO task_probe VALUES(42);",
            )?;
            Err::<(), _>(rusqlite::Error::InvalidQuery)
        });
        assert!(result.is_err());
        assert_eq!(log.max_seq().unwrap(), 0);
        assert!(live.recv_timeout(Duration::from_millis(1)).is_none());
        let tables: i64 = log
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='task_probe'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
    }

    #[test]
    fn failed_event_projection_rolls_back_attached_mutation() {
        let log = EventLog::open_in_memory().unwrap();
        log.append("main", "turn.user", &serde_json::json!({"text":"prior"}))
            .unwrap();
        log.inner.lock().unwrap().conn.execute_batch(
            "CREATE TABLE task_probe(value INTEGER); INSERT INTO task_probe VALUES(1);
             CREATE TRIGGER fail_projection BEFORE UPDATE ON client_threads BEGIN SELECT RAISE(FAIL, 'projection failed'); END;"
        ).unwrap();
        let mut live = log.subscribe("main", 1).unwrap();
        assert!(log
            .append_with(
                "main",
                "turn.user",
                &serde_json::json!({"text":"lost"}),
                |tx| { tx.execute("UPDATE task_probe SET value=2", []) }
            )
            .is_err());
        assert_eq!(log.max_seq().unwrap(), 1);
        assert!(live.recv_timeout(Duration::from_millis(1)).is_none());
        let value: i64 = log
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT value FROM task_probe", [], |r| r.get(0))
            .unwrap();
        assert_eq!(value, 1);
    }

    #[test]
    fn checked_replay_uses_deleted_sequence_not_sparse_scope_minimum() {
        let log = EventLog::open_in_memory().unwrap();
        log.append("other", "test", &val("a")).unwrap();
        log.append("main", "test", &val("b")).unwrap();
        assert_eq!(log.oldest_retained("main").unwrap(), Some(2));
        assert!(matches!(
            log.subscribe_checked("main", 0).unwrap(),
            Replay::Events(_)
        ));
        for _ in 0..5000 {
            log.append("main", "test", &val("c")).unwrap();
        }
        assert!(matches!(
            log.subscribe_checked("main", 1).unwrap(),
            Replay::ResyncRequired { .. }
        ));
        assert!(matches!(
            log.subscribe_checked("main", 2).unwrap(),
            Replay::Events(_)
        ));
    }

    #[test]
    fn host_replay_detects_retention_holes_in_every_scope() {
        let log = EventLog::open_in_memory().unwrap();
        log.append("A", "test", &val("A1")).unwrap();
        log.append("B", "test", &val("B2")).unwrap();
        for _ in 0..5000 {
            log.append("B", "test", &val("later B")).unwrap();
        }
        assert_eq!(log.oldest_retained("A").unwrap(), Some(1));
        assert!(matches!(
            log.subscribe_checked("A", 1).unwrap(),
            Replay::Events(_)
        ));
        assert!(matches!(
            log.subscribe_host(1).unwrap(),
            Replay::ResyncRequired { .. }
        ));
        let Replay::Events(mut replay) = log.subscribe_host(2).unwrap() else {
            panic!("deleted event was already consumed")
        };
        assert_eq!(replay.recv_timeout(Duration::ZERO).unwrap().seq, 3);
    }

    #[test]
    fn append_is_strictly_increasing_and_redacts() {
        let log = EventLog::open_in_memory().unwrap();
        let mut last = 0;
        for i in 0..1000 {
            let ev = log.append("s", "test", &val(&format!("n{i}"))).unwrap();
            assert!(ev.seq > last, "seq must strictly increase");
            last = ev.seq;
        }
        // A planted token is redacted before storage.
        let ev = log
            .append(
                "s",
                "test",
                &serde_json::json!({ "token": "sk-abcdefghijklmnopqrstuv" }),
            )
            .unwrap();
        assert_eq!(ev.payload["token"], "[redacted]");
        let replay = log.subscribe("s", ev.seq - 1).unwrap();
        assert!(!format!("{:?}", replay.replay).contains("sk-abcdefghijklmnopqrstuv"));
    }

    #[test]
    fn two_subscribers_at_different_cursors() {
        let log = EventLog::open_in_memory().unwrap();
        for i in 0..10 {
            log.append("s", "test", &val(&format!("n{i}"))).unwrap();
        }
        let mut a = log.subscribe("s", 0).unwrap();
        let mut b = log.subscribe("s", 5).unwrap();
        assert_eq!(a.replay_remaining(), 10);
        assert_eq!(b.replay_remaining(), 5);
        // Live event reaches both, once each, after replay drains.
        let live = log.append("s", "test", &val("live")).unwrap();
        for _ in 0..10 {
            a.recv_timeout(Duration::from_millis(50));
        }
        for _ in 0..5 {
            b.recv_timeout(Duration::from_millis(50));
        }
        let ga = a.recv_timeout(Duration::from_millis(200)).unwrap();
        let gb = b.recv_timeout(Duration::from_millis(200)).unwrap();
        assert_eq!(ga.seq, live.seq);
        assert_eq!(gb.seq, live.seq);
        // No duplicate delivery.
        assert!(a.recv_timeout(Duration::from_millis(50)).is_none());
    }

    #[test]
    fn retention_failure_rolls_back_append_and_broadcast() {
        for trigger in [
            "CREATE TRIGGER reject_retention BEFORE DELETE ON events BEGIN SELECT RAISE(FAIL, 'retention delete failed'); END;",
            "CREATE TRIGGER reject_retention BEFORE INSERT ON events WHEN NEW.kind = 'retention' BEGIN SELECT RAISE(FAIL, 'retention marker failed'); END;",
        ] {
            let log = EventLog::open_in_memory().unwrap();
            log.inner.lock().unwrap().conn.execute_batch(
                "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 5000)
                 INSERT INTO events(ts, scope_id, kind, payload) SELECT 0, 's', 'seed', '{}' FROM n;",
            ).unwrap();
            let before = log.max_seq().unwrap();
            let first = log.get(1).unwrap();
            let mut live = log.subscribe("s", before).unwrap();
            log.inner.lock().unwrap().conn.execute_batch(trigger).unwrap();

            assert!(log.append("s", "ui.surface", &val("replacement")).is_err());
            assert_eq!(log.max_seq().unwrap(), before, "failed retention stored the replacement");
            assert_eq!(log.get(1).unwrap(), first, "failed retention removed prior data");
            assert_eq!(log.count("s").unwrap(), RETENTION_LIMIT);
            assert!(log.get(before + 1).unwrap().is_none());
            assert!(live.recv_timeout(Duration::from_millis(5)).is_none(), "failed append was broadcast");

            log.inner.lock().unwrap().conn.execute_batch("DROP TRIGGER reject_retention;").unwrap();
            let committed = log.append("s", "ui.surface", &val("committed")).unwrap();
            assert_eq!(live.recv_timeout(Duration::from_millis(5)), Some(committed.clone()));
            assert_eq!(log.get(committed.seq).unwrap(), Some(committed));
            assert!(live.recv_timeout(Duration::from_millis(5)).is_none());
            assert_eq!(log.count("s").unwrap(), RETENTION_LIMIT);
        }
    }

    #[test]
    fn retention_keeps_5000_behind_one_marker() {
        let log = EventLog::open_in_memory().unwrap();
        for i in 0..12_000 {
            log.append("s", "test", &val(&format!("n{i}"))).unwrap();
        }
        assert_eq!(log.count("s").unwrap(), RETENTION_LIMIT);
        let guard = log.inner.lock().unwrap();
        let markers: i64 = guard
            .conn
            .query_row(
                "SELECT COUNT(*) FROM events WHERE scope_id='s' AND kind='retention'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(markers, 1, "exactly one retention marker");
        // The marker is the oldest surviving row, so no cursor can replay a pruned event.
        let min_real: i64 = guard
            .conn
            .query_row(
                "SELECT MIN(seq) FROM events WHERE scope_id='s' AND kind!='retention'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let marker_seq: i64 = guard
            .conn
            .query_row(
                "SELECT seq FROM events WHERE scope_id='s' AND kind='retention'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(marker_seq < min_real, "marker is the oldest row");
    }

    #[test]
    fn stalled_subscriber_gets_resync() {
        let log = EventLog::open_in_memory().unwrap();
        let mut sub = log.subscribe("s", 0).unwrap();
        for i in 0..(LAG_LIMIT + 50) {
            log.append("s", "test", &val(&format!("n{i}"))).unwrap();
        }
        // Drain and look for the resync signal.
        let mut saw_resync = false;
        while let Some(ev) = sub.recv_timeout(Duration::from_millis(20)) {
            if ev.kind == "resync" {
                saw_resync = true;
                break;
            }
        }
        assert!(saw_resync, "a stalled subscriber must receive a resync");
        // Recover from the log without loss: everything is still replayable.
        let fresh = log.subscribe("s", 0).unwrap();
        assert_eq!(fresh.replay_remaining() as i64, log.count("s").unwrap());
    }

    #[test]
    fn recent_events_are_bounded_newest_first_and_redacted() {
        let log = EventLog::open_in_memory().unwrap();
        log.append("main", "talk.delta", &serde_json::json!({"text": "first"}))
            .unwrap();
        log.append(
            "pipeline",
            "model.request",
            &serde_json::json!({"prompt": "sk-abcdefghijklmnopqrstuvwxyz"}),
        )
        .unwrap();
        log.append("main", "talk.done", &serde_json::json!({"ledger_id": 1}))
            .unwrap();

        let events = log.recent_since(0, 2).unwrap();

        assert_eq!(
            events
                .iter()
                .map(|event| event.kind.as_str())
                .collect::<Vec<_>>(),
            ["talk.done", "model.request",]
        );
        assert_eq!(events[1].payload["prompt"], "[redacted]");
    }

    #[test]
    fn recent_events_by_kind_are_filtered_before_the_limit() {
        let log = EventLog::open_in_memory().unwrap();
        log.append("pipeline", "model.request", &serde_json::json!({"id": 1}))
            .unwrap();
        for index in 0..5 {
            log.append("main", "talk.delta", &serde_json::json!({"index": index}))
                .unwrap();
        }
        log.append("pipeline", "model.request", &serde_json::json!({"id": 2}))
            .unwrap();

        let events = log.recent_kind_since("model.request", 0, 2).unwrap();

        assert_eq!(
            events
                .iter()
                .map(|event| event.payload["id"].as_i64())
                .collect::<Vec<_>>(),
            [Some(2), Some(1),]
        );
    }
}
