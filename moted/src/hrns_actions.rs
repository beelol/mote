use mote_eventlog::EventLog;
use mote_hrns::{
    action_request_hash, Action, ActionEffect, ActionError, ActionId, ActionState, HrnsStore,
};
use mote_protocol::HostId;
use rusqlite::{Connection, OptionalExtension};
use std::path::PathBuf;

pub struct ApprovalEffect {
    database: PathBuf,
    host_id: HostId,
}

impl ApprovalEffect {
    pub fn new(database: PathBuf, host_id: HostId) -> Self {
        Self { database, host_id }
    }
}

impl ActionEffect for ApprovalEffect {
    fn apply_once(&mut self, action: &Action) -> Result<Option<String>, ActionError> {
        let connection = Connection::open(&self.database).map_err(|_| ActionError::Unavailable)?;
        let record: Option<(i64, String, String)> = connection
            .query_row(
                "SELECT id,status,hrns_request_hash FROM approvals
                 WHERE host_id=?1 AND hrns_action_id=?2",
                rusqlite::params![self.host_id.as_str(), action.id.as_str()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|_| ActionError::Unavailable)?;
        let Some((approval_id, status, request_hash)) = record else {
            return Err(ActionError::Unavailable);
        };
        if request_hash != action_request_hash(action) {
            return Err(ActionError::Failed);
        }
        match status.as_str() {
            "approved" => Ok(Some(format!("approval:{approval_id}"))),
            "pending" | "claimed" => Err(ActionError::Unavailable),
            "failed" | "denied" | "expired" => Err(ActionError::Failed),
            _ => Err(ActionError::Failed),
        }
    }
}

pub fn reconcile_approval(
    database: &std::path::Path,
    events: EventLog,
    host_id: &HostId,
    action_id: &ActionId,
) -> rusqlite::Result<Option<Action>> {
    let connection = Connection::open(database)?;
    let approval: Option<(String, String)> = connection
        .query_row(
            "SELECT status,hrns_request_hash FROM approvals
             WHERE host_id=?1 AND hrns_action_id=?2",
            rusqlite::params![host_id.as_str(), action_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((status, request_hash)) = approval else {
        return Ok(None);
    };
    let store = HrnsStore::open(database, events)?;
    let Some((scope, action, revision)) = store.scoped_action(action_id)? else {
        return Err(rusqlite::Error::InvalidQuery);
    };
    if request_hash != action_request_hash(&action) {
        return Err(rusqlite::Error::InvalidQuery);
    }
    match (status.as_str(), action.state) {
        ("approved", ActionState::AwaitingApproval) => store
            .transition_action(&scope, action_id, revision, ActionState::Ready)
            .map(Some),
        ("denied" | "expired", ActionState::AwaitingApproval) => {
            store.deny_action(&scope, action_id, revision).map(Some)
        }
        (
            "failed",
            ActionState::Proposed
            | ActionState::AwaitingApproval
            | ActionState::Ready
            | ActionState::Running,
        ) => store.fail_action(&scope, action_id, revision).map(Some),
        ("pending" | "claimed", _) => Ok(None),
        ("approved", ActionState::Ready | ActionState::Running | ActionState::Committed)
        | ("denied" | "expired", ActionState::Denied)
        | ("failed", ActionState::Failed) => Ok(Some(action)),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watchers::{Watchers, WriteDecision};
    use mote_hrns::{HarnessKind, Scope, TaskId, TaskState};
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn fixture() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "mote-hrns-effect-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        (root, data)
    }

    fn action(path: &std::path::Path) -> Action {
        Action {
            id: ActionId::new("task:1").unwrap(),
            task_id: TaskId::new("task").unwrap(),
            state: ActionState::Running,
            name: "file.write".into(),
            arguments: serde_json::from_value(json!({
                "path": path,
                "content": "exact content"
            }))
            .unwrap(),
        }
    }

    fn awaiting_action(database: &std::path::Path, action: &Action) -> EventLog {
        let events = EventLog::open(database).unwrap();
        let store = HrnsStore::open(database, events.clone()).unwrap();
        let scope = Scope::new("owner", "personal").unwrap();
        let task = store
            .enqueue(
                action.task_id.clone(),
                scope.clone(),
                "write the file".into(),
                HarnessKind::Opencode,
            )
            .unwrap();
        store
            .transition(&scope, &task.id, 0, TaskState::Running)
            .unwrap();
        let mut proposed = action.clone();
        proposed.state = ActionState::Proposed;
        store.propose_action(&scope, proposed).unwrap();
        store
            .transition_action(&scope, &action.id, 0, ActionState::AwaitingApproval)
            .unwrap();
        events
    }

    #[test]
    fn approved_file_effect_is_observed_without_redispatch() {
        let (root, data) = fixture();
        let database = data.join("mote.db");
        let target = root.join("held.txt");
        let host = HostId::from_public_key(&[8; 32]);
        let action = action(&target);
        let watchers = Watchers::open(Connection::open(&database).unwrap()).unwrap();
        watchers.bind_local_host(&host).unwrap();
        let WriteDecision::Pending(approval_id) = watchers
            .request_file_write_for_action(
                &data,
                "main",
                &target,
                "exact content",
                1000,
                (&action.id, &action_request_hash(&action)),
            )
            .unwrap()
        else {
            panic!("write must wait for approval");
        };
        let mut effect = ApprovalEffect::new(database, host);
        assert_eq!(effect.apply_once(&action), Err(ActionError::Unavailable));
        assert!(watchers.approve(approval_id, 1001).unwrap());
        assert_eq!(
            effect.apply_once(&action).unwrap(),
            Some(format!("approval:{approval_id}"))
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "exact content");
        assert!(!watchers.approve(approval_id, 1002).unwrap());
        assert_eq!(
            effect.apply_once(&action).unwrap(),
            Some(format!("approval:{approval_id}"))
        );
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "exact content");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_action_cannot_reuse_an_approved_effect() {
        let (root, data) = fixture();
        let database = data.join("mote.db");
        let target = root.join("held.txt");
        let host = HostId::from_public_key(&[7; 32]);
        let action = action(&target);
        let watchers = Watchers::open(Connection::open(&database).unwrap()).unwrap();
        watchers.bind_local_host(&host).unwrap();
        let WriteDecision::Pending(approval_id) = watchers
            .request_file_write_for_action(
                &data,
                "main",
                &target,
                "exact content",
                1000,
                (&action.id, &action_request_hash(&action)),
            )
            .unwrap()
        else {
            panic!("write must wait for approval");
        };
        assert!(watchers.approve(approval_id, 1001).unwrap());
        let mut changed = action.clone();
        changed.arguments.insert("content".into(), json!("changed"));
        let mut effect = ApprovalEffect::new(database, host);
        assert_eq!(effect.apply_once(&changed), Err(ActionError::Failed));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "exact content");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn terminal_approval_reconciles_the_matching_hrns_action_once() {
        let (root, data) = fixture();
        let database = data.join("mote.db");
        let target = root.join("held.txt");
        let host = HostId::from_public_key(&[6; 32]);
        let action = action(&target);
        let events = awaiting_action(&database, &action);
        let watchers = Watchers::open(Connection::open(&database).unwrap()).unwrap();
        watchers.bind_local_host(&host).unwrap();
        let WriteDecision::Pending(approval_id) = watchers
            .request_file_write_for_action(
                &data,
                "main",
                &target,
                "exact content",
                1000,
                (&action.id, &action_request_hash(&action)),
            )
            .unwrap()
        else {
            panic!("write must wait for approval");
        };
        assert!(watchers.approve(approval_id, 1001).unwrap());

        let ready = reconcile_approval(&database, events.clone(), &host, &action.id)
            .unwrap()
            .unwrap();
        assert_eq!(ready.state, ActionState::Ready);
        let same = reconcile_approval(&database, events, &host, &action.id)
            .unwrap()
            .unwrap();
        assert_eq!(same.state, ActionState::Ready);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "exact content");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn terminal_approval_preserves_the_action_realm() {
        let (root, data) = fixture();
        let database = data.join("mote.db");
        let target = root.join("held.txt");
        let host = HostId::from_public_key(&[5; 32]);
        let action = action(&target);
        let events = EventLog::open(&database).unwrap();
        let store = HrnsStore::open(&database, events.clone()).unwrap();
        let scope = Scope::new("owner", "work").unwrap();
        let task = store
            .enqueue(
                action.task_id.clone(),
                scope.clone(),
                "write the work file".into(),
                HarnessKind::Opencode,
            )
            .unwrap();
        store
            .transition(&scope, &task.id, 0, TaskState::Running)
            .unwrap();
        let mut proposed = action.clone();
        proposed.state = ActionState::Proposed;
        store.propose_action(&scope, proposed).unwrap();
        store
            .transition_action(&scope, &action.id, 0, ActionState::AwaitingApproval)
            .unwrap();
        let watchers = Watchers::open(Connection::open(&database).unwrap()).unwrap();
        watchers.bind_local_host(&host).unwrap();
        let WriteDecision::Pending(approval_id) = watchers
            .request_file_write_for_action(
                &data,
                "main",
                &target,
                "exact content",
                1000,
                (&action.id, &action_request_hash(&action)),
            )
            .unwrap()
        else {
            panic!("write must wait for approval");
        };
        assert!(watchers.approve(approval_id, 1001).unwrap());

        let ready = reconcile_approval(&database, events, &host, &action.id)
            .unwrap()
            .unwrap();
        assert_eq!(ready.state, ActionState::Ready);
        assert_eq!(store.action(&scope, &action.id).unwrap().unwrap().0, ready);
        std::fs::remove_dir_all(root).unwrap();
    }
}
