use mote_eventlog::EventLog;
use mote_hrns::*;
use serde_json::{json, Map};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "mote-actions-{}-{suffix}-{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self {
            path: directory.join("tasks.db"),
        }
    }

    fn open(&self) -> HrnsStore {
        let events = EventLog::open(&self.path).unwrap();
        HrnsStore::open(&self.path, events).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.path.parent().unwrap());
    }
}

fn scope() -> Scope {
    Scope::new("owner", "personal").unwrap()
}

fn running_task(store: &HrnsStore, name: &str) -> Task {
    let task = store
        .enqueue(
            TaskId::new(name).unwrap(),
            scope(),
            format!("finish {name}"),
            HarnessKind::Mote,
        )
        .unwrap();
    store
        .transition(&scope(), &task.id, 0, TaskState::Running)
        .unwrap()
}

fn proposed_action(store: &HrnsStore, task: &Task) -> Action {
    let action = Action {
        id: ActionId::new(format!("{}:1", task.id.as_str())).unwrap(),
        task_id: task.id.clone(),
        state: ActionState::Proposed,
        name: "file.write".into(),
        arguments: serde_json::from_value(json!({
            "content": "hello",
            "path": "/tmp/mote-action-test"
        }))
        .unwrap(),
    };
    store.propose_action(&scope(), action.clone()).unwrap();
    action
}

fn receipt(action: &Action) -> ActionReceipt {
    ActionReceipt {
        action_id: action.id.clone(),
        request_hash: action_request_hash(action),
        status: ReceiptStatus::Committed,
        committed_at: 42,
        result_ref: Some("result:1".into()),
    }
}

#[test]
fn canonical_request_hash_ignores_object_key_order_but_binds_identity() {
    let task_id = TaskId::new("task").unwrap();
    let mut first_arguments = Map::new();
    first_arguments.insert("z".into(), json!({"b": 2, "a": 1}));
    first_arguments.insert("a".into(), json!(true));
    let mut second_arguments = Map::new();
    second_arguments.insert("a".into(), json!(true));
    second_arguments.insert("z".into(), json!({"a": 1, "b": 2}));
    let first = Action {
        id: ActionId::new("task:1").unwrap(),
        task_id: task_id.clone(),
        state: ActionState::Proposed,
        name: "tool".into(),
        arguments: first_arguments,
    };
    let mut second = first.clone();
    second.arguments = second_arguments;
    assert_eq!(action_request_hash(&first), action_request_hash(&second));
    second.id = ActionId::new("task:2").unwrap();
    assert_ne!(action_request_hash(&first), action_request_hash(&second));
}

#[test]
fn guarded_action_transitions_reject_stale_and_wrong_prior_states() {
    let fixture = Fixture::new();
    let store = fixture.open();
    let task = running_task(&store, "guards");
    let action = proposed_action(&store, &task);

    assert!(store.claim_action(&scope(), &action.id, 0).is_err());
    let awaiting = store
        .transition_action(&scope(), &action.id, 0, ActionState::AwaitingApproval)
        .unwrap();
    assert!(store.claim_action(&scope(), &action.id, 1).is_err());
    assert!(store.deny_action(&scope(), &action.id, 0).is_err());
    let ready = store
        .transition_action(&scope(), &awaiting.id, 1, ActionState::Ready)
        .unwrap();
    assert!(store.deny_action(&scope(), &ready.id, 2).is_err());
    let running = store.claim_action(&scope(), &ready.id, 2).unwrap();
    assert_eq!(running.state, ActionState::Running);
    assert!(store.claim_action(&scope(), &running.id, 3).is_err());
}

#[test]
fn receipt_and_committed_state_survive_restart_without_duplicate_commit() {
    let fixture = Fixture::new();
    let store = fixture.open();
    let task = running_task(&store, "restart");
    let action = proposed_action(&store, &task);
    let action = store
        .transition_action(&scope(), &action.id, 0, ActionState::Ready)
        .unwrap();
    let action = store.claim_action(&scope(), &action.id, 1).unwrap();
    let expected = receipt(&action);
    let (committed, stored) = store
        .commit_action(&scope(), &action.id, 2, expected.clone())
        .unwrap();
    assert_eq!(committed.state, ActionState::Committed);
    assert_eq!(stored, expected);
    drop(store);

    let reopened = fixture.open();
    assert_eq!(
        reopened.receipt(&scope(), &action.id).unwrap(),
        Some(expected.clone())
    );
    assert_eq!(
        reopened
            .commit_action(&scope(), &action.id, 3, expected.clone())
            .unwrap()
            .1,
        expected
    );
    let altered = ActionReceipt {
        result_ref: Some("altered".into()),
        ..receipt(&action)
    };
    assert!(reopened
        .commit_action(&scope(), &action.id, 3, altered)
        .is_err());
}

#[test]
fn altered_receipt_rolls_back_state_and_receipt() {
    let fixture = Fixture::new();
    let store = fixture.open();
    let task = running_task(&store, "altered");
    let action = proposed_action(&store, &task);
    let action = store
        .transition_action(&scope(), &action.id, 0, ActionState::Ready)
        .unwrap();
    let action = store.claim_action(&scope(), &action.id, 1).unwrap();
    let mut altered = receipt(&action);
    altered.request_hash = "not-the-request".into();

    assert!(store
        .commit_action(&scope(), &action.id, 2, altered)
        .is_err());
    assert_eq!(
        store.action(&scope(), &action.id).unwrap().unwrap().0.state,
        ActionState::Running
    );
    assert!(store.receipt(&scope(), &action.id).unwrap().is_none());
}

#[test]
fn failed_commit_event_rolls_back_receipt_and_action_state() {
    let fixture = Fixture::new();
    let store = fixture.open();
    let task = running_task(&store, "event-failure");
    let action = proposed_action(&store, &task);
    let action = store
        .transition_action(&scope(), &action.id, 0, ActionState::Ready)
        .unwrap();
    let action = store.claim_action(&scope(), &action.id, 1).unwrap();
    let event_log = EventLog::open(&fixture.path).unwrap();
    let before = event_log.max_seq().unwrap();
    let connection = rusqlite::Connection::open(&fixture.path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_action_commit BEFORE INSERT ON events
             WHEN NEW.kind='action.committed'
             BEGIN SELECT RAISE(FAIL,'injected'); END;",
        )
        .unwrap();

    assert!(store
        .commit_action(&scope(), &action.id, 2, receipt(&action))
        .is_err());
    assert_eq!(
        store.action(&scope(), &action.id).unwrap().unwrap().0.state,
        ActionState::Running
    );
    assert!(store.receipt(&scope(), &action.id).unwrap().is_none());
    assert_eq!(event_log.max_seq().unwrap(), before);
}
