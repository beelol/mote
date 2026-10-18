use mote_harness::{OpenCodeSession, ServerFactory, ThinkEvent};
use mote_hrns::adapters::OpenCodeAdapter;
use mote_hrns::{
    Checkpoint, Harness, HarnessError, HarnessKind, HarnessRequest, ModelStep, Scope, Task, TaskId,
    TaskState,
};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TurnCall {
    session: String,
    model: String,
    system: Option<String>,
    prompt: String,
}

struct ScriptedSession {
    session_id: String,
    state: Arc<ScriptedState>,
}

struct ScriptedState {
    create_calls: Mutex<usize>,
    turns: Mutex<VecDeque<Result<(Vec<ThinkEvent>, Duration), String>>>,
    calls: Mutex<Vec<TurnCall>>,
}

impl ScriptedState {
    fn new(turns: Vec<Result<(Vec<ThinkEvent>, Duration), String>>) -> Self {
        Self {
            create_calls: Mutex::new(0),
            turns: Mutex::new(turns.into()),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl OpenCodeSession for ScriptedSession {
    fn create_session(&self) -> Result<String, String> {
        *self.state.create_calls.lock().unwrap() += 1;
        Ok(self.session_id.clone())
    }

    fn run_step(
        &self,
        session: &str,
        model: &str,
        system: Option<&str>,
        prompt: &str,
        timeout: Duration,
    ) -> Result<(Vec<ThinkEvent>, Duration), String> {
        self.run_turn(session, model, system, prompt, timeout)
    }

    fn run_turn(
        &self,
        session: &str,
        model: &str,
        system: Option<&str>,
        prompt: &str,
        _timeout: Duration,
    ) -> Result<(Vec<ThinkEvent>, Duration), String> {
        self.state.calls.lock().unwrap().push(TurnCall {
            session: session.into(),
            model: model.into(),
            system: system.map(str::to_string),
            prompt: prompt.into(),
        });
        self.state
            .turns
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted turn")
    }
}

struct ScriptedFactory {
    starts: Mutex<usize>,
    state: Arc<ScriptedState>,
}

impl ServerFactory for ScriptedFactory {
    fn start(&self) -> Result<Arc<dyn OpenCodeSession>, String> {
        let mut starts = self.starts.lock().unwrap();
        *starts += 1;
        Ok(Arc::new(ScriptedSession {
            session_id: format!("session-{starts}"),
            state: self.state.clone(),
        }))
    }
}

fn task() -> Task {
    Task {
        id: TaskId::new("task-1").unwrap(),
        scope: Scope::new("owner", "personal").unwrap(),
        outcome: "Find the saved project color".into(),
        state: TaskState::Running,
        harness: HarnessKind::Opencode,
    }
}

fn request(checkpoint: Option<Checkpoint>) -> HarnessRequest {
    HarnessRequest {
        task: task(),
        context: "Only use the scoped Mote context.".into(),
        checkpoint,
    }
}

fn scripted_factory(
    turns: Vec<Result<(Vec<ThinkEvent>, Duration), String>>,
) -> (Arc<ScriptedFactory>, Arc<ScriptedState>) {
    let state = Arc::new(ScriptedState::new(turns));
    let factory = Arc::new(ScriptedFactory {
        starts: Mutex::new(0),
        state: state.clone(),
    });
    (factory, state)
}

#[test]
fn resumes_an_opencode_session_from_motes_opaque_checkpoint() {
    let (factory, session) = scripted_factory(vec![
        Ok((
            vec![ThinkEvent::ToolCall {
                name: "memory_search".into(),
                id: Some("call-1".into()),
                input: json!({"query":"project color"}),
            }],
            Duration::from_millis(8),
        )),
        Ok((
            vec![
                ThinkEvent::Text {
                    text: "The saved color is red.".into(),
                },
                ThinkEvent::Done { ok: true },
            ],
            Duration::from_millis(5),
        )),
    ]);

    let mut first = OpenCodeAdapter::new(factory.clone(), "ollama/qwen3-coder:30b-64k")
        .unwrap();
    let proposed = first.step(request(None)).unwrap();
    assert_eq!(
        proposed.step,
        ModelStep::Tool {
            name: "memory_search".into(),
            arguments: serde_json::from_value(json!({"query":"project color"})).unwrap(),
        }
    );
    let adapter = proposed.adapter_checkpoint.expect("opaque session checkpoint");
    assert_eq!(
        adapter,
        json!({"session_id":"session-1","parser_version":1})
    );

    let checkpoint = Checkpoint {
        task_id: task().id,
        revision: 1,
        state: TaskState::WaitingForApproval,
        last_action: None,
        step: Some(proposed.step),
        adapter: Some(adapter),
    };
    drop(first);

    let mut restarted =
        OpenCodeAdapter::new(factory.clone(), "ollama/qwen3-coder:30b-64k").unwrap();
    let completed = restarted.step(request(Some(checkpoint))).unwrap();
    assert_eq!(
        completed.step,
        ModelStep::Complete {
            text: "The saved color is red.".into(),
            sources: Vec::new(),
        }
    );
    assert_eq!(*factory.starts.lock().unwrap(), 2);
    assert_eq!(*session.create_calls.lock().unwrap(), 2);
    let calls = session.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].prompt, "Find the saved project color");
    assert!(calls[1].prompt.contains("Mote-owned durable checkpoint"));
    assert!(calls[1].prompt.contains("\"revision\":1"));
    assert_eq!(calls[0].session, "session-1");
    assert_eq!(calls[1].session, "session-2");
    assert!(calls.iter().all(|call| {
        call.model == "ollama/qwen3-coder:30b-64k"
            && call.system.as_deref() == Some("Only use the scoped Mote context.")
    }));
}

#[test]
fn rejects_invalid_checkpoint_before_starting_opencode() {
    let (factory, _) = scripted_factory(Vec::new());
    let checkpoint = Checkpoint {
        task_id: task().id,
        revision: 2,
        state: TaskState::Running,
        last_action: None,
        step: None,
        adapter: Some(json!({"session_id":"session-7","parser_version":99})),
    };
    let mut adapter = OpenCodeAdapter::new(factory.clone(), "ollama/qwen3-coder:30b-64k")
        .unwrap();
    assert_eq!(
        adapter.step(request(Some(checkpoint))),
        Err(HarnessError::InvalidOutput)
    );
    assert_eq!(*factory.starts.lock().unwrap(), 0);
}

#[test]
fn rejects_a_checkpoint_owned_by_another_task() {
    let (factory, _) = scripted_factory(Vec::new());
    let checkpoint = Checkpoint {
        task_id: TaskId::new("another-task").unwrap(),
        revision: 2,
        state: TaskState::Running,
        last_action: None,
        step: None,
        adapter: Some(json!({"session_id":"session-7","parser_version":1})),
    };
    let mut adapter = OpenCodeAdapter::new(factory.clone(), "ollama/qwen3-coder:30b-64k")
        .unwrap();
    assert_eq!(
        adapter.step(request(Some(checkpoint))),
        Err(HarnessError::InvalidOutput)
    );
    assert_eq!(*factory.starts.lock().unwrap(), 0);
}

#[test]
fn initial_mote_checkpoint_still_sends_the_task_outcome() {
    let (factory, state) = scripted_factory(vec![Ok((
        vec![
            ThinkEvent::Text { text: "done".into() },
            ThinkEvent::Done { ok: true },
        ],
        Duration::ZERO,
    ))]);
    let checkpoint = Checkpoint {
        task_id: task().id,
        revision: 0,
        state: TaskState::Running,
        last_action: None,
        step: None,
        adapter: None,
    };
    let mut adapter = OpenCodeAdapter::new(factory, "ollama/qwen3-coder:30b-64k").unwrap();
    adapter.step(request(Some(checkpoint))).unwrap();
    assert_eq!(
        state.calls.lock().unwrap()[0].prompt,
        "Find the saved project color"
    );
}

#[test]
fn fails_closed_on_malformed_or_failed_events() {
    let cases = vec![
        vec![ThinkEvent::ToolCall {
            name: "memory_search".into(),
            id: None,
            input: json!("not an object"),
        }],
        vec![ThinkEvent::Done { ok: false }],
        vec![ThinkEvent::Text {
            text: "unfinished".into(),
        }],
        vec![
            ThinkEvent::ToolCall {
                name: "memory_search".into(),
                id: None,
                input: json!({"query":"project color"}),
            },
            ThinkEvent::ToolResult {
                name: "memory_search".into(),
                id: None,
                output: "red".into(),
                is_error: false,
            },
            ThinkEvent::Done { ok: true },
        ],
    ];
    for events in cases {
        let (factory, _) = scripted_factory(vec![Ok((events, Duration::ZERO))]);
        let mut adapter = OpenCodeAdapter::new(factory, "ollama/qwen3-coder:30b-64k")
            .unwrap();
        assert_eq!(adapter.step(request(None)), Err(HarnessError::InvalidOutput));
    }
}

#[test]
fn classifies_timeouts_without_exposing_server_errors() {
    let (factory, _) = scripted_factory(vec![Err("turn timed out with secret details".into())]);
    let mut adapter = OpenCodeAdapter::new(factory, "ollama/qwen3-coder:30b-64k").unwrap();
    assert_eq!(adapter.step(request(None)), Err(HarnessError::Timeout));
}
