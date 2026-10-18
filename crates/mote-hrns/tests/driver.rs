use mote_eventlog::EventLog;
use mote_hrns::*;
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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
            "mote-driver-{}-{suffix}-{}",
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

#[derive(Clone)]
struct Script {
    steps: Arc<Mutex<VecDeque<Result<HarnessReply, HarnessError>>>>,
    requests: Arc<Mutex<Vec<HarnessRequest>>>,
}

impl Script {
    fn new(steps: impl IntoIterator<Item = Result<HarnessReply, HarnessError>>) -> Self {
        Self {
            steps: Arc::new(Mutex::new(steps.into_iter().collect())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

struct FakeHarness(Script);

impl Harness for FakeHarness {
    fn step(&mut self, request: HarnessRequest) -> Result<HarnessReply, HarnessError> {
        self.0.requests.lock().unwrap().push(request);
        self.0.steps.lock().unwrap().pop_front().unwrap()
    }
}

#[derive(Clone, Default)]
struct ExecutorState {
    receipts: Arc<Mutex<HashMap<ActionId, ActionReceipt>>>,
    attempts: Arc<AtomicU64>,
    effects: Arc<AtomicU64>,
    fail_after_first_effect: Arc<AtomicBool>,
}

struct FakeExecutor(ExecutorState);

impl ActionExecutor for FakeExecutor {
    fn execute(&mut self, action: &Action) -> Result<ActionReceipt, ActionError> {
        self.0.attempts.fetch_add(1, Ordering::Relaxed);
        if let Some(receipt) = self.0.receipts.lock().unwrap().get(&action.id).cloned() {
            return Ok(receipt);
        }
        self.0.effects.fetch_add(1, Ordering::Relaxed);
        let receipt = ActionReceipt {
            action_id: action.id.clone(),
            request_hash: action_request_hash(action),
            status: ReceiptStatus::Committed,
            committed_at: 1,
            result_ref: Some("result-1".into()),
        };
        self.0
            .receipts
            .lock()
            .unwrap()
            .insert(action.id.clone(), receipt.clone());
        if self
            .0
            .fail_after_first_effect
            .swap(false, Ordering::Relaxed)
        {
            return Err(ActionError::Failed);
        }
        Ok(receipt)
    }
}

fn scope() -> Scope {
    Scope::new("owner", "personal").unwrap()
}

fn complete(text: &str) -> Result<HarnessReply, HarnessError> {
    Ok(HarnessReply {
        step: ModelStep::Complete {
            text: text.into(),
            sources: vec![],
        },
        adapter_checkpoint: None,
    })
}

fn make_driver(
    store: HrnsStore,
    opencode: Script,
    mote: Script,
    executor: ExecutorState,
) -> Driver {
    Driver::new(
        store,
        HarnessAdapters {
            opencode: Box::new(FakeHarness(opencode)),
            mote: Box::new(FakeHarness(mote)),
        },
        Box::new(FakeExecutor(executor)),
    )
}

fn task(name: &str, harness: HarnessKind) -> NewTask {
    NewTask {
        id: TaskId::new(name).unwrap(),
        scope: scope(),
        outcome: format!("finish {name}"),
        harness,
    }
}

#[test]
fn fifo_uses_only_the_selected_harness_and_never_starts_two_tasks() {
    let fixture = Fixture::new();
    let opencode = Script::new([complete("first")]);
    let mote = Script::new([complete("second")]);
    let mut driver = make_driver(
        fixture.open(),
        opencode.clone(),
        mote.clone(),
        ExecutorState::default(),
    );
    driver
        .enqueue(task("first", HarnessKind::Opencode))
        .unwrap();
    driver.enqueue(task("second", HarnessKind::Mote)).unwrap();

    assert!(
        matches!(driver.step().unwrap(), DriverOutcome::Started(task) if task.id.as_str()=="first")
    );
    assert!(
        matches!(driver.step().unwrap(), DriverOutcome::Completed { task, .. } if task.id.as_str()=="first")
    );
    let first_id = TaskId::new("first").unwrap();
    assert!(matches!(
        driver
            .store()
            .checkpoint(&scope(), &first_id)
            .unwrap()
            .unwrap()
            .step,
        Some(ModelStep::Complete { text, .. }) if text == "first"
    ));
    assert!(
        matches!(driver.step().unwrap(), DriverOutcome::Started(task) if task.id.as_str()=="second")
    );
    assert!(
        matches!(driver.step().unwrap(), DriverOutcome::Completed { task, .. } if task.id.as_str()=="second")
    );
    assert!(matches!(driver.step().unwrap(), DriverOutcome::Idle));
    assert_eq!(opencode.requests.lock().unwrap().len(), 1);
    assert_eq!(mote.requests.lock().unwrap().len(), 1);
}

#[test]
fn failed_action_attempt_is_terminal_and_never_retried_after_restart() {
    let fixture = Fixture::new();
    let script = Script::new([
        Ok(HarnessReply {
            step: ModelStep::Tool {
                name: "browser.read".into(),
                arguments: serde_json::from_value(json!({"url":"https://example.test"})).unwrap(),
            },
            adapter_checkpoint: Some(json!({"session_id":"session-1"})),
        }),
        complete("done"),
    ]);
    let executor = ExecutorState::default();
    executor
        .fail_after_first_effect
        .store(true, Ordering::Relaxed);
    let empty = Script::new([]);
    let mut driver = make_driver(
        fixture.open(),
        script.clone(),
        empty.clone(),
        executor.clone(),
    );
    driver.enqueue(task("tool", HarnessKind::Opencode)).unwrap();
    assert!(matches!(driver.step().unwrap(), DriverOutcome::Started(_)));
    let action = match driver.step().unwrap() {
        DriverOutcome::Proposed(action) => action,
        other => panic!("unexpected {other:?}"),
    };
    assert!(matches!(
        driver.step().unwrap(),
        DriverOutcome::AwaitingApproval { .. }
    ));
    driver
        .store()
        .transition_action(&scope(), &action.id, 1, ActionState::Ready)
        .unwrap();
    assert!(matches!(driver.step().unwrap(), DriverOutcome::Resumed(_)));
    assert!(matches!(driver.step().unwrap(), DriverOutcome::Claimed(_)));
    assert!(matches!(
        driver.cancel(&TaskId::new("tool").unwrap()),
        Err(DriverError::ActionInFlight)
    ));
    assert!(matches!(
        driver.step(),
        Err(DriverError::Action(ActionError::Failed))
    ));
    drop(driver);

    let mut driver = make_driver(fixture.open(), script.clone(), empty, executor.clone());
    assert!(matches!(
        driver.recover().unwrap(),
        DriverOutcome::Recovered { checkpoints: 0 }
    ));
    assert!(matches!(
        driver.step().unwrap(),
        DriverOutcome::Completed { .. }
    ));
    assert_eq!(executor.effects.load(Ordering::Relaxed), 1);
    assert_eq!(executor.attempts.load(Ordering::Relaxed), 1);
    assert_eq!(script.requests.lock().unwrap().len(), 2);
}

#[test]
fn restart_after_claim_marks_action_indeterminate_without_dispatch() {
    let fixture = Fixture::new();
    let script = Script::new([
        Ok(HarnessReply {
            step: ModelStep::Tool {
                name: "browser.submit".into(),
                arguments: serde_json::from_value(json!({"node":"submit"})).unwrap(),
            },
            adapter_checkpoint: None,
        }),
        complete("not dispatched"),
    ]);
    let executor = ExecutorState::default();
    let empty = Script::new([]);
    let mut driver = make_driver(
        fixture.open(),
        script.clone(),
        empty.clone(),
        executor.clone(),
    );
    driver
        .enqueue(task("claimed", HarnessKind::Opencode))
        .unwrap();
    assert!(matches!(driver.step().unwrap(), DriverOutcome::Started(_)));
    let action = match driver.step().unwrap() {
        DriverOutcome::Proposed(action) => action,
        other => panic!("unexpected {other:?}"),
    };
    assert!(matches!(
        driver.step().unwrap(),
        DriverOutcome::AwaitingApproval { .. }
    ));
    driver
        .store()
        .transition_action(&scope(), &action.id, 1, ActionState::Ready)
        .unwrap();
    assert!(matches!(driver.step().unwrap(), DriverOutcome::Resumed(_)));
    assert!(matches!(driver.step().unwrap(), DriverOutcome::Claimed(_)));
    drop(driver);

    let mut restarted = make_driver(fixture.open(), script, empty, executor.clone());
    assert!(matches!(
        restarted.step(),
        Err(DriverError::Action(ActionError::Indeterminate))
    ));
    assert_eq!(executor.attempts.load(Ordering::Relaxed), 0);
    assert_eq!(executor.effects.load(Ordering::Relaxed), 0);
    assert_eq!(
        restarted
            .store()
            .action(&scope(), &action.id)
            .unwrap()
            .unwrap()
            .0
            .state,
        ActionState::Failed
    );
}

#[test]
fn recovery_repairs_interrupted_action_and_terminal_checkpoints_once() {
    let fixture = Fixture::new();
    let store = fixture.open();
    let first = store
        .enqueue(
            TaskId::new("first").unwrap(),
            scope(),
            "first".into(),
            HarnessKind::Mote,
        )
        .unwrap();
    let first = store
        .transition(&scope(), &first.id, 0, TaskState::Running)
        .unwrap();
    store
        .write_checkpoint(
            &scope(),
            Checkpoint {
                task_id: first.id.clone(),
                revision: 0,
                state: TaskState::Running,
                last_action: None,
                step: None,
                adapter: None,
            },
        )
        .unwrap();
    store
        .transition(&scope(), &first.id, 1, TaskState::Complete)
        .unwrap();

    let second = store
        .enqueue(
            TaskId::new("second").unwrap(),
            scope(),
            "second".into(),
            HarnessKind::Mote,
        )
        .unwrap();
    let second = store
        .transition(&scope(), &second.id, 0, TaskState::Running)
        .unwrap();
    let action = Action {
        id: ActionId::new("second:1").unwrap(),
        task_id: second.id.clone(),
        state: ActionState::Proposed,
        name: "browser.read".into(),
        arguments: Default::default(),
    };
    store.propose_action(&scope(), action.clone()).unwrap();

    let empty = Script::new([]);
    let driver = make_driver(store, empty.clone(), empty, ExecutorState::default());
    assert!(matches!(
        driver.recover().unwrap(),
        DriverOutcome::Recovered { checkpoints: 2 }
    ));
    assert!(matches!(
        driver.recover().unwrap(),
        DriverOutcome::Recovered { checkpoints: 0 }
    ));
    assert_eq!(
        driver
            .store()
            .checkpoint(&scope(), &first.id)
            .unwrap()
            .unwrap()
            .state,
        TaskState::Complete
    );
    assert_eq!(
        driver
            .store()
            .checkpoint(&scope(), &second.id)
            .unwrap()
            .unwrap()
            .last_action,
        Some(action.id)
    );
}

#[test]
fn every_persisted_task_state_recovers_idempotently() {
    for state in [
        TaskState::Running,
        TaskState::WaitingForApproval,
        TaskState::WaitingForInput,
        TaskState::Blocked,
        TaskState::Complete,
        TaskState::Failed,
        TaskState::Cancelled,
    ] {
        let fixture = Fixture::new();
        let store = fixture.open();
        let task = store
            .enqueue(
                TaskId::new(format!("task-{state:?}")).unwrap(),
                scope(),
                "recover".into(),
                HarnessKind::Mote,
            )
            .unwrap();
        let task = store
            .transition(&scope(), &task.id, 0, TaskState::Running)
            .unwrap();
        let task = if state == TaskState::Running {
            task
        } else {
            store.transition(&scope(), &task.id, 1, state).unwrap()
        };
        store
            .write_checkpoint(
                &scope(),
                Checkpoint {
                    task_id: task.id.clone(),
                    revision: 0,
                    state,
                    last_action: None,
                    step: None,
                    adapter: None,
                },
            )
            .unwrap();
        drop(store);
        let empty = Script::new([]);
        let driver = make_driver(
            fixture.open(),
            empty.clone(),
            empty,
            ExecutorState::default(),
        );
        assert!(matches!(
            driver.recover().unwrap(),
            DriverOutcome::Recovered { checkpoints: 0 }
        ));
    }
}

#[test]
fn cancellation_and_harness_failure_are_durable() {
    let fixture = Fixture::new();
    let failed = Script::new([Err(HarnessError::Unavailable)]);
    let empty = Script::new([]);
    let mut driver = make_driver(fixture.open(), failed, empty, ExecutorState::default());
    let first = driver
        .enqueue(task("first", HarnessKind::Opencode))
        .unwrap();
    let second = driver.enqueue(task("second", HarnessKind::Mote)).unwrap();
    assert!(matches!(
        driver.cancel(&second.id).unwrap(),
        DriverOutcome::Cancelled(_)
    ));
    assert!(matches!(driver.step().unwrap(), DriverOutcome::Started(_)));
    assert!(matches!(
        driver.step(),
        Err(DriverError::Harness(HarnessError::Unavailable))
    ));
    assert_eq!(
        driver
            .store()
            .get(&scope(), &first.id)
            .unwrap()
            .unwrap()
            .0
            .state,
        TaskState::Failed
    );
    assert_eq!(
        driver
            .store()
            .get(&scope(), &second.id)
            .unwrap()
            .unwrap()
            .0
            .state,
        TaskState::Cancelled
    );
}

#[test]
fn input_question_survives_restart_without_another_model_call() {
    let fixture = Fixture::new();
    let script = Script::new([Ok(HarnessReply {
        step: ModelStep::WaitForInput {
            question: "Which city?".into(),
        },
        adapter_checkpoint: None,
    })]);
    let empty = Script::new([]);
    let mut driver = make_driver(
        fixture.open(),
        script.clone(),
        empty.clone(),
        ExecutorState::default(),
    );
    driver
        .enqueue(task("input", HarnessKind::Opencode))
        .unwrap();
    assert!(matches!(driver.step().unwrap(), DriverOutcome::Started(_)));
    assert!(matches!(
        driver.step().unwrap(),
        DriverOutcome::WaitingForInput { question, .. } if question == "Which city?"
    ));
    drop(driver);

    let mut driver = make_driver(
        fixture.open(),
        script.clone(),
        empty,
        ExecutorState::default(),
    );
    assert!(matches!(
        driver.step().unwrap(),
        DriverOutcome::WaitingForInput { question, .. } if question == "Which city?"
    ));
    assert_eq!(script.requests.lock().unwrap().len(), 1);
}
