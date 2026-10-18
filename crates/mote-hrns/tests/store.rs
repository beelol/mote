use mote_eventlog::EventLog;
use mote_hrns::*;
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
            "mote-hrns-{}-{suffix}-{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self {
            path: directory.join("tasks.db"),
        }
    }
    fn open(&self) -> (HrnsStore, EventLog) {
        let log = EventLog::open(&self.path).unwrap();
        (HrnsStore::open(&self.path, log.clone()).unwrap(), log)
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
fn enqueue(store: &HrnsStore, name: &str) -> Task {
    store
        .enqueue(
            TaskId::new(name).unwrap(),
            scope(),
            "Research".into(),
            HarnessKind::Mote,
        )
        .unwrap()
}

fn action(task: &Task, name: &str) -> Action {
    Action {
        id: ActionId::new(name).unwrap(),
        task_id: task.id.clone(),
        state: ActionState::Proposed,
        name: "browser.read".into(),
        arguments: serde_json::Map::new(),
    }
}

#[test]
fn fifo_and_single_active_survive_reopening() {
    let fixture = Fixture::new();
    let (store, log) = fixture.open();
    let first = enqueue(&store, "first");
    let second = enqueue(&store, "second");
    assert!(store
        .transition(&scope(), &second.id, 0, TaskState::Running)
        .is_err());
    store
        .transition(&scope(), &first.id, 0, TaskState::Running)
        .unwrap();
    assert!(store
        .transition(&scope(), &second.id, 0, TaskState::Running)
        .is_err());
    drop(store);
    drop(log);
    let (store, _) = fixture.open();
    assert_eq!(store.active(&scope()).unwrap().unwrap().id, first.id);
    assert_eq!(store.queued(&scope()).unwrap(), vec![second.clone()]);
    store
        .transition(&scope(), &first.id, 1, TaskState::Complete)
        .unwrap();
    store
        .transition(&scope(), &second.id, 0, TaskState::Running)
        .unwrap();
}

#[test]
fn scope_stale_revision_and_duplicate_ids_fail_without_events() {
    let fixture = Fixture::new();
    let (store, log) = fixture.open();
    let task = enqueue(&store, "first");
    let before = log.max_seq().unwrap();
    let wrong = Scope::new("owner", "work").unwrap();
    assert!(store.get(&wrong, &task.id).unwrap().is_none());
    assert!(store.queued(&wrong).unwrap().is_empty());
    assert!(store
        .transition(&wrong, &task.id, 0, TaskState::Running)
        .is_err());
    assert!(store
        .transition(&scope(), &task.id, 9, TaskState::Running)
        .is_err());
    assert!(store
        .enqueue(
            task.id.clone(),
            scope(),
            "Duplicate".into(),
            HarnessKind::Mote
        )
        .is_err());
    assert_eq!(log.max_seq().unwrap(), before);
    assert_eq!(store.get(&scope(), &task.id).unwrap().unwrap(), (task, 0));
}

#[test]
fn database_mismatch_is_refused() {
    let first = Fixture::new();
    let second = Fixture::new();
    let (_, first_log) = first.open();
    let (_, second_log) = second.open();
    assert!(HrnsStore::open(&first.path, second_log).is_err());
    drop(first_log);
}

#[test]
fn projection_failure_rolls_back_the_task_change() {
    let fixture = Fixture::new();
    let (store, log) = fixture.open();
    let task = enqueue(&store, "first");
    let before = log.max_seq().unwrap();
    let connection = rusqlite::Connection::open(&fixture.path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_task_event BEFORE INSERT ON events WHEN NEW.kind='task.changed' BEGIN SELECT RAISE(FAIL,'injected'); END;").unwrap();
    assert!(store
        .transition(&scope(), &task.id, 0, TaskState::Running)
        .is_err());
    assert_eq!(store.get(&scope(), &task.id).unwrap().unwrap(), (task, 0));
    assert_eq!(log.max_seq().unwrap(), before);
}

#[test]
fn concurrent_enqueue_preserves_committed_order_and_one_active_task() {
    let fixture = Fixture::new();
    let (store, log) = fixture.open();
    let store = std::sync::Arc::new(store);
    let threads: Vec<_> = (0..12)
        .map(|index| {
            let store = store.clone();
            std::thread::spawn(move || enqueue(&store, &format!("task-{index}")))
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let queued = store.queued(&scope()).unwrap();
    assert_eq!(queued.len(), 12);
    let events = log.recent_since(0, 100).unwrap();
    let committed: Vec<_> = events
        .iter()
        .rev()
        .map(|event| event.payload["task_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        queued
            .iter()
            .map(|task| task.id.as_str())
            .collect::<Vec<_>>(),
        committed
    );
    let attempts: Vec<_> = queued
        .into_iter()
        .map(|task| {
            let store = store.clone();
            std::thread::spawn(move || {
                store
                    .transition(&scope(), &task.id, 0, TaskState::Running)
                    .is_ok()
            })
        })
        .collect();
    let successes = attempts
        .into_iter()
        .map(|handle| usize::from(handle.join().unwrap()))
        .sum::<usize>();
    assert_eq!(successes, 1);
}

#[test]
fn actions_and_checkpoints_are_scoped_guarded_and_durable() {
    let fixture = Fixture::new();
    let (store, log) = fixture.open();
    let task = enqueue(&store, "first");
    let task = store
        .transition(&scope(), &task.id, 0, TaskState::Running)
        .unwrap();
    let proposed = action(&task, "action-1");
    store.propose_action(&scope(), proposed.clone()).unwrap();
    let actions = store.actions(&scope(), &task.id).unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0], proposed);
    let wrong = Scope::new("owner", "work").unwrap();
    assert!(store.action(&wrong, &proposed.id).unwrap().is_none());
    assert!(store.propose_action(&scope(), proposed.clone()).is_err());

    let ready = store
        .transition_action(&scope(), &proposed.id, 0, ActionState::Ready)
        .unwrap();
    assert_eq!(ready.state, ActionState::Ready);
    let before = log.max_seq().unwrap();
    assert!(store
        .transition_action(&scope(), &proposed.id, 0, ActionState::Running)
        .is_err());
    assert_eq!(log.max_seq().unwrap(), before);

    let checkpoint = Checkpoint {
        task_id: task.id.clone(),
        revision: 0,
        state: TaskState::Running,
        last_action: Some(proposed.id.clone()),
        step: None,
        adapter: None,
    };
    store
        .write_checkpoint(&scope(), checkpoint.clone())
        .unwrap();
    assert!(store
        .write_checkpoint(&scope(), checkpoint.clone())
        .is_err());
    drop(store);
    drop(log);

    let (store, _) = fixture.open();
    assert_eq!(
        store.checkpoint(&scope(), &task.id).unwrap(),
        Some(checkpoint)
    );
    assert_eq!(
        store.action(&scope(), &proposed.id).unwrap(),
        Some((ready, 1))
    );
}

#[test]
fn invalid_checkpoint_and_failed_action_event_leave_no_partial_state() {
    let fixture = Fixture::new();
    let (store, log) = fixture.open();
    let task = enqueue(&store, "first");
    let task = store
        .transition(&scope(), &task.id, 0, TaskState::Running)
        .unwrap();
    let proposed = action(&task, "action-1");
    store.propose_action(&scope(), proposed.clone()).unwrap();
    let foreign = ActionId::new("missing").unwrap();
    let before = log.max_seq().unwrap();
    assert!(store
        .write_checkpoint(
            &scope(),
            Checkpoint {
                task_id: task.id.clone(),
                revision: 0,
                state: TaskState::Running,
                last_action: Some(foreign),
                step: None,
                adapter: None,
            },
        )
        .is_err());
    assert_eq!(log.max_seq().unwrap(), before);

    let connection = rusqlite::Connection::open(&fixture.path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_action_event BEFORE INSERT ON events WHEN NEW.kind='action.changed' BEGIN SELECT RAISE(FAIL,'injected'); END;").unwrap();
    assert!(store
        .transition_action(&scope(), &proposed.id, 0, ActionState::Ready)
        .is_err());
    assert_eq!(
        store.action(&scope(), &proposed.id).unwrap(),
        Some((proposed, 0))
    );
    assert_eq!(log.max_seq().unwrap(), before);
}

#[test]
fn model_step_and_checkpoint_roll_back_as_one_transition() {
    let fixture = Fixture::new();
    let (store, log) = fixture.open();
    let task = enqueue(&store, "first");
    let task = store
        .transition(&scope(), &task.id, 0, TaskState::Running)
        .unwrap();
    store
        .write_checkpoint(
            &scope(),
            Checkpoint {
                task_id: task.id.clone(),
                revision: 0,
                state: TaskState::Running,
                last_action: None,
                step: None,
                adapter: None,
            },
        )
        .unwrap();
    let before = log.max_seq().unwrap();
    let connection = rusqlite::Connection::open(&fixture.path).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_step_event BEFORE INSERT ON events WHEN NEW.kind='task.stepped' BEGIN SELECT RAISE(FAIL,'injected'); END;").unwrap();
    let checkpoint = Checkpoint {
        task_id: task.id.clone(),
        revision: 1,
        state: TaskState::Complete,
        last_action: None,
        step: Some(ModelStep::Complete {
            text: "done".into(),
            sources: vec![],
        }),
        adapter: None,
    };
    assert!(store
        .record_step(&scope(), &task.id, 1, Some(TaskState::Complete), checkpoint,)
        .is_err());
    assert_eq!(store.get(&scope(), &task.id).unwrap().unwrap(), (task, 1));
    assert_eq!(
        store
            .checkpoint(&scope(), &TaskId::new("first").unwrap())
            .unwrap()
            .unwrap()
            .revision,
        0
    );
    assert_eq!(log.max_seq().unwrap(), before);
}

#[test]
fn stale_model_revision_cannot_publish_a_tool_proposal() {
    let fixture = Fixture::new();
    let (store, log) = fixture.open();
    let task = enqueue(&store, "first");
    let task = store
        .transition(&scope(), &task.id, 0, TaskState::Running)
        .unwrap();
    let task = store
        .transition(&scope(), &task.id, 1, TaskState::Blocked)
        .unwrap();
    let task = store
        .transition(&scope(), &task.id, 2, TaskState::Running)
        .unwrap();
    let action = action(&task, "stale-action");
    let checkpoint = Checkpoint {
        task_id: task.id.clone(),
        revision: 0,
        state: TaskState::Running,
        last_action: Some(action.id.clone()),
        step: Some(ModelStep::Tool {
            name: action.name.clone(),
            arguments: action.arguments.clone(),
        }),
        adapter: None,
    };
    let before = log.max_seq().unwrap();
    assert!(store
        .propose_action_with_checkpoint(&scope(), action.clone(), 1, checkpoint)
        .is_err());
    assert!(store.action(&scope(), &action.id).unwrap().is_none());
    assert!(store.checkpoint(&scope(), &task.id).unwrap().is_none());
    assert_eq!(log.max_seq().unwrap(), before);
}

#[test]
fn durable_model_step_event_is_redacted() {
    let fixture = Fixture::new();
    let (store, log) = fixture.open();
    let task = enqueue(&store, "first");
    let task = store
        .transition(&scope(), &task.id, 0, TaskState::Running)
        .unwrap();
    let secret = "sk-abcdefghijklmnopqrstuv";
    let checkpoint = Checkpoint {
        task_id: task.id.clone(),
        revision: 0,
        state: TaskState::Running,
        last_action: None,
        step: Some(ModelStep::Message {
            text: format!("result {secret}"),
        }),
        adapter: None,
    };
    store
        .record_step(&scope(), &task.id, 1, None, checkpoint.clone())
        .unwrap();
    let event = log
        .recent_since(0, 20)
        .unwrap()
        .into_iter()
        .find(|event| event.kind == "task.stepped")
        .unwrap();
    assert!(!event.payload.to_string().contains(secret));
    assert!(event.payload.to_string().contains("[redacted]"));
    assert_eq!(
        store.checkpoint(&scope(), &task.id).unwrap(),
        Some(checkpoint)
    );
}
