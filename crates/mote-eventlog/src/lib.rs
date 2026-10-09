//! Append-only event log with cursor replay and live streaming.
//!
//! One SQLite table `events(seq, ts, scope_id, kind, payload)`. `payload` is JSON that has
//! already passed through [`mote_redact::redact`] before storage. A subscriber supplies a
//! cursor and receives every event with `seq > cursor` in order, then live events, with no
//! duplicate. Retention keeps [`RETENTION_LIMIT`] real events per scope behind one `retention`
//! marker. A subscriber that falls more than [`LAG_LIMIT`] events behind receives a single
//! `resync` event and recovers by replaying from the log.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Real events kept per scope before older ones are pruned.
pub const RETENTION_LIMIT: i64 = 5000;
/// A subscriber more than this many events behind is told to resync.
pub const LAG_LIMIT: usize = 1000;

/// A stored event. `payload` is already redacted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub seq: i64,
    pub ts: i64,
    pub scope_id: String,
    pub kind: String,
    pub payload: serde_json::Value,
}

type Queue = Arc<(Mutex<VecDeque<Event>>, Condvar)>;

struct SubHandle {
    scope: String,
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

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl EventLog {
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
             CREATE INDEX IF NOT EXISTS events_scope_seq ON events(scope_id, seq);",
        )?;
        Ok(Self { inner: Arc::new(Mutex::new(Inner { conn, subs: Vec::new() })) })
    }

    /// Append an event. The payload is redacted before storage. Returns the stored event with
    /// its assigned `seq`.
    pub fn append(
        &self,
        scope_id: &str,
        kind: &str,
        payload: &serde_json::Value,
    ) -> rusqlite::Result<Event> {
        let redacted_text = mote_redact::redact(&payload.to_string());
        let mut guard = self.inner.lock().unwrap();
        let ev = guard.insert(scope_id, kind, &redacted_text)?;
        guard.broadcast(&ev);
        guard.enforce_retention(scope_id)?;
        Ok(ev)
    }

    /// Subscribe to a scope from `cursor`. The returned subscription first yields every stored
    /// event with `seq > cursor`, then live events.
    pub fn subscribe(&self, scope_id: &str, cursor: i64) -> rusqlite::Result<Subscription> {
        let mut guard = self.inner.lock().unwrap();
        // Read the replay snapshot while holding the lock so no live event can slip in below
        // the snapshot's max seq.
        let replay = guard.read_after(scope_id, cursor)?;
        let queue: Queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let alive = Arc::new(AtomicBool::new(true));
        guard.subs.push(SubHandle {
            scope: scope_id.to_string(),
            queue: queue.clone(),
            alive: alive.clone(),
        });
        Ok(Subscription { replay: replay.into(), queue, alive })
    }

    /// Current highest seq, or 0 if empty.
    pub fn max_seq(&self) -> rusqlite::Result<i64> {
        self.inner.lock().unwrap().max_seq()
    }

    /// Fetch a single event by seq, if it still exists.
    pub fn get(&self, seq: i64) -> rusqlite::Result<Option<Event>> {
        let guard = self.inner.lock().unwrap();
        let mut stmt = guard.conn.prepare(
            "SELECT seq, ts, scope_id, kind, payload FROM events WHERE seq=?1",
        )?;
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
}

impl Inner {
    fn insert(&mut self, scope_id: &str, kind: &str, payload_text: &str) -> rusqlite::Result<Event> {
        let ts = now_millis();
        self.conn.execute(
            "INSERT INTO events(ts, scope_id, kind, payload) VALUES(?1, ?2, ?3, ?4)",
            rusqlite::params![ts, scope_id, kind, payload_text],
        )?;
        let seq = self.conn.last_insert_rowid();
        let payload = serde_json::from_str(payload_text)
            .unwrap_or_else(|_| serde_json::Value::String(payload_text.to_string()));
        Ok(Event { seq, ts, scope_id: scope_id.to_string(), kind: kind.to_string(), payload })
    }

    fn max_seq(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get(0))
    }

    fn read_after(&self, scope_id: &str, cursor: i64) -> rusqlite::Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, ts, scope_id, kind, payload FROM events
             WHERE scope_id=?1 AND seq>?2 ORDER BY seq ASC",
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
            if s.scope != ev.scope_id {
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

    fn enforce_retention(&mut self, scope_id: &str) -> rusqlite::Result<()> {
        let real: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE scope_id=?1 AND kind!='retention'",
            [scope_id],
            |r| r.get(0),
        )?;
        if real <= RETENTION_LIMIT {
            return Ok(());
        }
        let excess = real - RETENTION_LIMIT;
        // Delete the oldest real events for this scope.
        self.conn.execute(
            "DELETE FROM events WHERE seq IN (
                 SELECT seq FROM events WHERE scope_id=?1 AND kind!='retention'
                 ORDER BY seq ASC LIMIT ?2)",
            rusqlite::params![scope_id, excess],
        )?;
        // Ensure exactly one retention marker exists for the scope, as the oldest row.
        let markers: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM events WHERE scope_id=?1 AND kind='retention'",
            [scope_id],
            |r| r.get(0),
        )?;
        if markers == 0 {
            let ts = now_millis();
            self.conn.execute(
                "INSERT INTO events(ts, scope_id, kind, payload) VALUES(?1, ?2, 'retention', ?3)",
                rusqlite::params![ts, scope_id, "{\"pruned\":true}"],
            )?;
        }
        Ok(())
    }
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
            .append("s", "test", &serde_json::json!({ "token": "sk-abcdefghijklmnopqrstuv" }))
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
}
