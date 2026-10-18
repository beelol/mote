use crate::{HarnessKind, Scope, Task, TaskId, TaskState};
use mote_eventlog::EventLog;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

pub struct HrnsStore {
    reader: Mutex<Connection>,
    events: EventLog,
}

fn decode(text: String) -> rusqlite::Result<Task> {
    serde_json::from_str(&text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn encode(task: &Task) -> rusqlite::Result<String> {
    serde_json::to_string(task).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
}

fn state_key(state: TaskState) -> &'static str {
    match state {
        TaskState::Queued => "queued",
        TaskState::Running => "running",
        TaskState::WaitingForApproval => "waiting_for_approval",
        TaskState::WaitingForInput => "waiting_for_input",
        TaskState::Blocked => "blocked",
        TaskState::Complete => "complete",
        TaskState::Failed => "failed",
        TaskState::Cancelled => "cancelled",
    }
}

impl HrnsStore {
    pub fn open(path: &Path, events: EventLog) -> rusqlite::Result<Self> {
        let expected = std::fs::canonicalize(path)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let actual = events
            .database_path()
            .and_then(|p| std::fs::canonicalize(p).ok());
        if actual.as_ref() != Some(&expected) {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let reader = Connection::open(expected)?;
        reader.execute_batch(
            "CREATE TABLE IF NOT EXISTS hrns_task(
                position INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                account TEXT NOT NULL,
                realm TEXT NOT NULL,
                state TEXT NOT NULL,
                revision INTEGER NOT NULL DEFAULT 0,
                task TEXT NOT NULL);
             CREATE UNIQUE INDEX IF NOT EXISTS hrns_one_active ON hrns_task((1))
                WHERE state IN ('running','waiting_for_approval','waiting_for_input','blocked');",
        )?;
        Ok(Self {
            reader: Mutex::new(reader),
            events,
        })
    }

    pub fn enqueue(
        &self,
        id: TaskId,
        scope: Scope,
        outcome: String,
        harness: HarnessKind,
    ) -> rusqlite::Result<Task> {
        if outcome.trim().is_empty() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let task = Task {
            id,
            scope,
            outcome,
            state: TaskState::Queued,
            harness,
        };
        let payload = serde_json::json!({"task_id":task.id, "state":"queued"});
        let event_scope = serde_json::to_string(&task.scope)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        self.events.append_with(&event_scope, "task.queued", &payload, |tx| {
            tx.execute("INSERT INTO hrns_task(id,account,realm,state,task) VALUES(?1,?2,?3,'queued',?4)",
                params![task.id.as_str(), task.scope.account.as_str(), task.scope.realm.as_str(), encode(&task)?])?;
            Ok(())
        })?;
        Ok(task)
    }

    pub fn get(&self, scope: &Scope, id: &TaskId) -> rusqlite::Result<Option<(Task, u64)>> {
        let conn = self.reader.lock().unwrap();
        conn.query_row(
            "SELECT task,revision FROM hrns_task WHERE id=?1 AND account=?2 AND realm=?3",
            params![id.as_str(), scope.account.as_str(), scope.realm.as_str()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)),
        )
        .optional()?
        .map(|(task, revision)| Ok((decode(task)?, revision)))
        .transpose()
    }

    pub fn queued(&self, scope: &Scope) -> rusqlite::Result<Vec<Task>> {
        let conn = self.reader.lock().unwrap();
        let mut statement = conn.prepare("SELECT task FROM hrns_task WHERE account=?1 AND realm=?2 AND state='queued' ORDER BY position")?;
        let rows = statement
            .query_map(params![scope.account.as_str(), scope.realm.as_str()], |r| {
                r.get::<_, String>(0)
            })?;
        rows.map(|r| decode(r?)).collect()
    }

    pub fn active(&self, scope: &Scope) -> rusqlite::Result<Option<Task>> {
        let conn = self.reader.lock().unwrap();
        let text = conn.query_row("SELECT task FROM hrns_task WHERE account=?1 AND realm=?2 AND state IN ('running','waiting_for_approval','waiting_for_input','blocked')",
            params![scope.account.as_str(),scope.realm.as_str()], |r| r.get::<_,String>(0)).optional()?;
        text.map(decode).transpose()
    }

    pub fn transition(
        &self,
        scope: &Scope,
        id: &TaskId,
        revision: u64,
        next: TaskState,
    ) -> rusqlite::Result<Task> {
        let event_scope = serde_json::to_string(scope)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let payload = serde_json::json!({"task_id":id, "state":next});
        self.events.append_with(&event_scope, "task.changed", &payload, |tx| {
            let (text, stored_revision, position): (String,u64,i64) = tx.query_row(
                "SELECT task,revision,position FROM hrns_task WHERE id=?1 AND account=?2 AND realm=?3",
                params![id.as_str(),scope.account.as_str(),scope.realm.as_str()],
                |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            if stored_revision != revision { return Err(rusqlite::Error::InvalidQuery); }
            let mut task = decode(text)?;
            if task.state == TaskState::Queued && next == TaskState::Running {
                let earlier: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM hrns_task WHERE state='queued' AND position<?1)", [position], |r| r.get(0))?;
                if earlier { return Err(rusqlite::Error::InvalidQuery); }
            }
            task.transition(task.state,next).map_err(|_| rusqlite::Error::InvalidQuery)?;
            tx.execute("UPDATE hrns_task SET state=?1,task=?2,revision=revision+1 WHERE id=?3",
                params![state_key(next),encode(&task)?,id.as_str()])?;
            Ok(task)
        }).map(|(task,_)| task)
    }
}
