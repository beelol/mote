//! Watchers and approvals (docs/rfcs/v0.md section 15).
//!
//! `schedules(id, cron, prompt, enabled)` fires a prompt into a thread at its cron time, as if
//! the owner typed it. `approvals(id, thread_id, kind, payload, status, expires_at)` holds every
//! outward action; in v0 the only kind is `file_write` outside the data directory. Nothing with
//! status `pending` executes. Approvals expire after 24 hours.

use chrono::{TimeZone, Utc};
use mote_eventlog::EventLog;
use rusqlite::Connection;
use serde_json::json;
use std::path::Path;
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
    ) -> rusqlite::Result<WriteDecision> {
        if is_inside(data_dir, path) {
            std::fs::write(path, content).ok();
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
    pub fn approve(&self, id: i64, now: i64) -> rusqlite::Result<bool> {
        let (status, expires, payload): (String, i64, String) = self.conn.query_row(
            "SELECT status, expires_at, payload FROM approvals WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if status != "pending" {
            return Ok(false);
        }
        if now > expires {
            self.set_status(id, "expired")?;
            return Ok(false);
        }
        let v: serde_json::Value = serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null);
        if let (Some(p), Some(c)) = (v["path"].as_str(), v["content"].as_str()) {
            std::fs::write(p, c).ok();
        }
        self.set_status(id, "approved")?;
        Ok(true)
    }

    pub fn deny(&self, id: i64) -> rusqlite::Result<()> {
        self.set_status(id, "denied")
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

    fn set_status(&self, id: i64, status: &str) -> rusqlite::Result<()> {
        self.conn
            .execute("UPDATE approvals SET status=?1 WHERE id=?2", rusqlite::params![status, id])?;
        Ok(())
    }
}

fn is_inside(dir: &Path, path: &Path) -> bool {
    match (dir.canonicalize(), path.canonicalize()) {
        (Ok(d), Ok(p)) => p.starts_with(d),
        // If the target does not exist yet, compare against the lexical parent.
        _ => path.starts_with(dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
        w.deny(id).unwrap();
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
}
