use crate::{
    action_request_hash, Action, ActionId, ActionReceipt, ActionState, Checkpoint, HarnessKind,
    ReceiptStatus, Scope, Task, TaskId, TaskState,
};
use mote_eventlog::EventLog;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::Path;
use std::sync::Mutex;

pub struct HrnsStore {
    reader: Mutex<Connection>,
    events: EventLog,
}

fn decode<T: serde::de::DeserializeOwned>(text: String) -> rusqlite::Result<T> {
    serde_json::from_str(&text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn encode(value: &impl serde::Serialize) -> rusqlite::Result<String> {
    serde_json::to_string(value).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
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

fn action_state_key(state: ActionState) -> &'static str {
    match state {
        ActionState::Proposed => "proposed",
        ActionState::AwaitingApproval => "awaiting_approval",
        ActionState::Ready => "ready",
        ActionState::Running => "running",
        ActionState::Committed => "committed",
        ActionState::Denied => "denied",
        ActionState::Failed => "failed",
        ActionState::Cancelled => "cancelled",
    }
}

fn write_checkpoint_tx(
    tx: &Transaction<'_>,
    checkpoint: &Checkpoint,
    task_state: TaskState,
) -> rusqlite::Result<()> {
    if task_state != checkpoint.state {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let prior = tx
        .query_row(
            "SELECT revision FROM hrns_checkpoint WHERE task_id=?1",
            [checkpoint.task_id.as_str()],
            |r| r.get::<_, u64>(0),
        )
        .optional()?;
    let expected = prior.map_or(0, |revision| revision + 1);
    if checkpoint.revision != expected {
        return Err(rusqlite::Error::InvalidQuery);
    }
    if let Some(action_id) = checkpoint.last_action.as_ref() {
        let belongs: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM hrns_action WHERE id=?1 AND task_id=?2)",
            params![action_id.as_str(), checkpoint.task_id.as_str()],
            |r| r.get(0),
        )?;
        if !belongs {
            return Err(rusqlite::Error::InvalidQuery);
        }
    }
    tx.execute(
        "INSERT INTO hrns_checkpoint(task_id,revision,checkpoint) VALUES(?1,?2,?3) ON CONFLICT(task_id) DO UPDATE SET revision=excluded.revision,checkpoint=excluded.checkpoint",
        params![checkpoint.task_id.as_str(),checkpoint.revision,encode(checkpoint)?],
    )?;
    Ok(())
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
                WHERE state IN ('running','waiting_for_approval','waiting_for_input','blocked');
             CREATE TABLE IF NOT EXISTS hrns_action(
                position INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT NOT NULL UNIQUE,
                task_id TEXT NOT NULL,
                state TEXT NOT NULL,
                revision INTEGER NOT NULL DEFAULT 0,
                action TEXT NOT NULL,
                FOREIGN KEY(task_id) REFERENCES hrns_task(id));
             CREATE INDEX IF NOT EXISTS hrns_action_task ON hrns_action(task_id,position);
             CREATE TABLE IF NOT EXISTS hrns_checkpoint(
                task_id TEXT PRIMARY KEY,
                revision INTEGER NOT NULL,
                checkpoint TEXT NOT NULL,
                FOREIGN KEY(task_id) REFERENCES hrns_task(id));
             CREATE TABLE IF NOT EXISTS hrns_action_receipt(
                action_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                request_hash TEXT NOT NULL,
                receipt TEXT NOT NULL,
                FOREIGN KEY(action_id) REFERENCES hrns_action(id),
                FOREIGN KEY(task_id) REFERENCES hrns_task(id));",
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

    pub fn active_record(&self) -> rusqlite::Result<Option<(Task, u64)>> {
        let conn = self.reader.lock().unwrap();
        conn.query_row(
            "SELECT task,revision FROM hrns_task WHERE state IN ('running','waiting_for_approval','waiting_for_input','blocked')",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)),
        )
        .optional()?
        .map(|(task, revision)| Ok((decode(task)?, revision)))
        .transpose()
    }

    pub fn next_queued_record(&self) -> rusqlite::Result<Option<(Task, u64)>> {
        let conn = self.reader.lock().unwrap();
        conn.query_row(
            "SELECT task,revision FROM hrns_task WHERE state='queued' ORDER BY position LIMIT 1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)),
        )
        .optional()?
        .map(|(task, revision)| Ok((decode(task)?, revision)))
        .transpose()
    }

    pub fn task_records(&self) -> rusqlite::Result<Vec<(Task, u64)>> {
        let conn = self.reader.lock().unwrap();
        let mut statement =
            conn.prepare("SELECT task,revision FROM hrns_task ORDER BY position")?;
        let records = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)))?
            .map(|row| {
                let (task, revision) = row?;
                Ok((decode(task)?, revision))
            })
            .collect();
        records
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
            let mut task: Task = decode(text)?;
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

    pub fn propose_action(&self, scope: &Scope, action: Action) -> rusqlite::Result<Action> {
        if action.state != ActionState::Proposed || action.name.trim().is_empty() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let event_scope = serde_json::to_string(scope)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let payload =
            serde_json::json!({"task_id":action.task_id,"action_id":action.id,"state":"proposed"});
        self.events
            .append_with(&event_scope, "action.proposed", &payload, |tx| {
                let task_state: String = tx.query_row(
                    "SELECT state FROM hrns_task WHERE id=?1 AND account=?2 AND realm=?3",
                    params![
                        action.task_id.as_str(),
                        scope.account.as_str(),
                        scope.realm.as_str()
                    ],
                    |r| r.get(0),
                )?;
                if !matches!(
                    task_state.as_str(),
                    "running" | "waiting_for_approval" | "waiting_for_input" | "blocked"
                ) {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                tx.execute(
                    "INSERT INTO hrns_action(id,task_id,state,action) VALUES(?1,?2,'proposed',?3)",
                    params![
                        action.id.as_str(),
                        action.task_id.as_str(),
                        encode(&action)?
                    ],
                )?;
                Ok(action)
            })
            .map(|(action, _)| action)
    }

    pub fn propose_action_with_checkpoint(
        &self,
        scope: &Scope,
        action: Action,
        task_revision: u64,
        checkpoint: Checkpoint,
    ) -> rusqlite::Result<(Action, Checkpoint)> {
        if action.state != ActionState::Proposed
            || action.name.trim().is_empty()
            || checkpoint.task_id != action.task_id
            || checkpoint.last_action.as_ref() != Some(&action.id)
            || checkpoint.state != TaskState::Running
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        match checkpoint.step.as_ref() {
            Some(crate::ModelStep::Tool { name, arguments })
                if *name == action.name && *arguments == action.arguments => {}
            _ => return Err(rusqlite::Error::InvalidQuery),
        }
        let event_scope = serde_json::to_string(scope)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let payload = serde_json::json!({"task_id":action.task_id,"action_id":action.id,"state":"proposed","step":checkpoint.step});
        self.events
            .append_with(&event_scope, "action.proposed", &payload, |tx| {
                let (task_text, stored_revision): (String, u64) = tx.query_row(
                    "SELECT task,revision FROM hrns_task WHERE id=?1 AND account=?2 AND realm=?3",
                    params![
                        action.task_id.as_str(),
                        scope.account.as_str(),
                        scope.realm.as_str()
                    ],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                let task: Task = decode(task_text)?;
                if task.state != TaskState::Running || stored_revision != task_revision {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                tx.execute(
                    "INSERT INTO hrns_action(id,task_id,state,action) VALUES(?1,?2,'proposed',?3)",
                    params![
                        action.id.as_str(),
                        action.task_id.as_str(),
                        encode(&action)?
                    ],
                )?;
                write_checkpoint_tx(tx, &checkpoint, task.state)?;
                Ok((action, checkpoint))
            })
            .map(|(value, _)| value)
    }

    pub fn action(&self, scope: &Scope, id: &ActionId) -> rusqlite::Result<Option<(Action, u64)>> {
        let conn = self.reader.lock().unwrap();
        conn.query_row(
            "SELECT a.action,a.revision FROM hrns_action a JOIN hrns_task t ON t.id=a.task_id WHERE a.id=?1 AND t.account=?2 AND t.realm=?3",
            params![id.as_str(),scope.account.as_str(),scope.realm.as_str()],
            |r| Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?)),
        ).optional()?.map(|(action,revision)| Ok((decode(action)?,revision))).transpose()
    }

    pub fn scoped_action(&self, id: &ActionId) -> rusqlite::Result<Option<(Scope, Action, u64)>> {
        let conn = self.reader.lock().unwrap();
        conn.query_row(
            "SELECT t.account,t.realm,a.action,a.revision FROM hrns_action a JOIN hrns_task t ON t.id=a.task_id WHERE a.id=?1",
            [id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, u64>(3)?,
                ))
            },
        )
        .optional()?
        .map(|(account, realm, action, revision)| {
            Ok((
                Scope::new(account, realm).map_err(|_| rusqlite::Error::InvalidQuery)?,
                decode(action)?,
                revision,
            ))
        })
        .transpose()
    }

    pub fn actions(&self, scope: &Scope, task_id: &TaskId) -> rusqlite::Result<Vec<Action>> {
        let conn = self.reader.lock().unwrap();
        let mut statement = conn.prepare(
            "SELECT a.action FROM hrns_action a JOIN hrns_task t ON t.id=a.task_id WHERE a.task_id=?1 AND t.account=?2 AND t.realm=?3 ORDER BY a.position",
        )?;
        let actions = statement
            .query_map(
                params![
                    task_id.as_str(),
                    scope.account.as_str(),
                    scope.realm.as_str()
                ],
                |r| r.get::<_, String>(0),
            )?
            .map(|row| decode(row?))
            .collect();
        actions
    }

    pub fn latest_action(
        &self,
        scope: &Scope,
        task_id: &TaskId,
    ) -> rusqlite::Result<Option<(Action, u64)>> {
        let conn = self.reader.lock().unwrap();
        conn.query_row(
            "SELECT a.action,a.revision FROM hrns_action a JOIN hrns_task t ON t.id=a.task_id WHERE a.task_id=?1 AND t.account=?2 AND t.realm=?3 ORDER BY a.position DESC LIMIT 1",
            params![task_id.as_str(),scope.account.as_str(),scope.realm.as_str()],
            |r| Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?)),
        ).optional()?.map(|(action,revision)| Ok((decode(action)?,revision))).transpose()
    }

    pub fn transition_action(
        &self,
        scope: &Scope,
        id: &ActionId,
        revision: u64,
        next: ActionState,
    ) -> rusqlite::Result<Action> {
        let event_scope = serde_json::to_string(scope)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let payload = serde_json::json!({"action_id":id,"state":next});
        self.events.append_with(&event_scope, "action.changed", &payload, |tx| {
            let (text,stored_revision): (String,u64) = tx.query_row(
                "SELECT a.action,a.revision FROM hrns_action a JOIN hrns_task t ON t.id=a.task_id WHERE a.id=?1 AND t.account=?2 AND t.realm=?3",
                params![id.as_str(),scope.account.as_str(),scope.realm.as_str()],
                |r| Ok((r.get(0)?,r.get(1)?)),
            )?;
            if stored_revision != revision { return Err(rusqlite::Error::InvalidQuery); }
            let mut action: Action = decode(text)?;
            action.transition(action.state,next).map_err(|_| rusqlite::Error::InvalidQuery)?;
            tx.execute(
                "UPDATE hrns_action SET state=?1,action=?2,revision=revision+1 WHERE id=?3",
                params![action_state_key(next),encode(&action)?,id.as_str()],
            )?;
            Ok(action)
        }).map(|(action,_)| action)
    }

    pub fn claim_action(
        &self,
        scope: &Scope,
        id: &ActionId,
        revision: u64,
    ) -> rusqlite::Result<Action> {
        let current = self
            .action(scope, id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        if current.0.state != ActionState::Ready || current.1 != revision {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.transition_action(scope, id, revision, ActionState::Running)
    }

    pub fn commit_action(
        &self,
        scope: &Scope,
        id: &ActionId,
        revision: u64,
        receipt: ActionReceipt,
    ) -> rusqlite::Result<(Action, ActionReceipt)> {
        if receipt.action_id != *id || receipt.status != ReceiptStatus::Committed {
            return Err(rusqlite::Error::InvalidQuery);
        }
        if let Some(existing) = self.receipt(scope, id)? {
            let (action, _) = self
                .action(scope, id)?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            if action.state == ActionState::Committed && existing == receipt {
                return Ok((action, existing));
            }
            return Err(rusqlite::Error::InvalidQuery);
        }
        let event_scope = serde_json::to_string(scope)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        let payload = serde_json::json!({
            "action_id": id,
            "state": "committed",
            "request_hash": receipt.request_hash,
            "result_ref": receipt.result_ref,
        });
        self.events
            .append_with(&event_scope, "action.committed", &payload, |tx| {
                let (text, stored_revision): (String, u64) = tx.query_row(
                    "SELECT a.action,a.revision FROM hrns_action a JOIN hrns_task t ON t.id=a.task_id WHERE a.id=?1 AND t.account=?2 AND t.realm=?3",
                    params![id.as_str(), scope.account.as_str(), scope.realm.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                if stored_revision != revision {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                let mut action: Action = decode(text)?;
                if action.state != ActionState::Running
                    || receipt.request_hash != action_request_hash(&action)
                {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                action
                    .transition(ActionState::Running, ActionState::Committed)
                    .map_err(|_| rusqlite::Error::InvalidQuery)?;
                tx.execute(
                    "INSERT INTO hrns_action_receipt(action_id,task_id,request_hash,receipt) VALUES(?1,?2,?3,?4)",
                    params![
                        id.as_str(),
                        action.task_id.as_str(),
                        receipt.request_hash.as_str(),
                        encode(&receipt)?
                    ],
                )?;
                tx.execute(
                    "UPDATE hrns_action SET state='committed',action=?1,revision=revision+1 WHERE id=?2 AND revision=?3 AND state='running'",
                    params![encode(&action)?, id.as_str(), revision],
                )?;
                Ok((action, receipt))
            })
            .map(|(value, _)| value)
    }

    pub fn receipt(
        &self,
        scope: &Scope,
        id: &ActionId,
    ) -> rusqlite::Result<Option<ActionReceipt>> {
        let connection = self.reader.lock().unwrap();
        connection
            .query_row(
                "SELECT r.receipt FROM hrns_action_receipt r JOIN hrns_task t ON t.id=r.task_id WHERE r.action_id=?1 AND t.account=?2 AND t.realm=?3",
                params![id.as_str(), scope.account.as_str(), scope.realm.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(decode)
            .transpose()
    }

    pub fn deny_action(
        &self,
        scope: &Scope,
        id: &ActionId,
        revision: u64,
    ) -> rusqlite::Result<Action> {
        let current = self
            .action(scope, id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        if current.0.state != ActionState::AwaitingApproval || current.1 != revision {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.transition_action(scope, id, revision, ActionState::Denied)
    }

    pub fn fail_action(
        &self,
        scope: &Scope,
        id: &ActionId,
        revision: u64,
    ) -> rusqlite::Result<Action> {
        let current = self
            .action(scope, id)?
            .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        if !matches!(
            current.0.state,
            ActionState::Proposed
                | ActionState::AwaitingApproval
                | ActionState::Ready
                | ActionState::Running
        ) || current.1 != revision
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.transition_action(scope, id, revision, ActionState::Failed)
    }

    pub fn write_checkpoint(
        &self,
        scope: &Scope,
        checkpoint: Checkpoint,
    ) -> rusqlite::Result<Checkpoint> {
        let event_scope = serde_json::to_string(scope)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let payload = serde_json::json!({"task_id":checkpoint.task_id,"revision":checkpoint.revision,"state":checkpoint.state});
        self.events
            .append_with(&event_scope, "task.checkpointed", &payload, |tx| {
                let task_text: String = tx.query_row(
                    "SELECT task FROM hrns_task WHERE id=?1 AND account=?2 AND realm=?3",
                    params![
                        checkpoint.task_id.as_str(),
                        scope.account.as_str(),
                        scope.realm.as_str()
                    ],
                    |r| r.get(0),
                )?;
                let task: Task = decode(task_text)?;
                write_checkpoint_tx(tx, &checkpoint, task.state)?;
                Ok(checkpoint)
            })
            .map(|(checkpoint, _)| checkpoint)
    }

    pub fn record_step(
        &self,
        scope: &Scope,
        id: &TaskId,
        task_revision: u64,
        next: Option<TaskState>,
        checkpoint: Checkpoint,
    ) -> rusqlite::Result<(Task, Checkpoint)> {
        if checkpoint.task_id != *id || checkpoint.step.is_none() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let event_scope = serde_json::to_string(scope)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let payload =
            serde_json::json!({"task_id":id,"state":checkpoint.state,"step":checkpoint.step});
        self.events
            .append_with(&event_scope, "task.stepped", &payload, |tx| {
                let (task_text, stored_revision): (String, u64) = tx.query_row(
                    "SELECT task,revision FROM hrns_task WHERE id=?1 AND account=?2 AND realm=?3",
                    params![id.as_str(), scope.account.as_str(), scope.realm.as_str()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?;
                if stored_revision != task_revision {
                    return Err(rusqlite::Error::InvalidQuery);
                }
                let mut task: Task = decode(task_text)?;
                if let Some(next) = next {
                    task.transition(task.state, next)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?;
                    tx.execute(
                        "UPDATE hrns_task SET state=?1,task=?2,revision=revision+1 WHERE id=?3",
                        params![state_key(next), encode(&task)?, id.as_str()],
                    )?;
                }
                write_checkpoint_tx(tx, &checkpoint, task.state)?;
                Ok((task, checkpoint))
            })
            .map(|(value, _)| value)
    }

    pub fn checkpoint(
        &self,
        scope: &Scope,
        task_id: &TaskId,
    ) -> rusqlite::Result<Option<Checkpoint>> {
        let conn = self.reader.lock().unwrap();
        conn.query_row(
            "SELECT c.checkpoint FROM hrns_checkpoint c JOIN hrns_task t ON t.id=c.task_id WHERE c.task_id=?1 AND t.account=?2 AND t.realm=?3",
            params![task_id.as_str(),scope.account.as_str(),scope.realm.as_str()],
            |r| r.get::<_,String>(0),
        ).optional()?.map(decode).transpose()
    }
}
