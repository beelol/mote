use crate::config::ModelGate;
use super::native_step::{
    LedgeredLocalStepClient, OllamaStepTransport, NATIVE_THINK_CONTEXT, NATIVE_THINK_MODEL,
};
use mote_context::ContextStore;
use mote_eventlog::EventLog;
use mote_hrns::adapters::{LocalStepClient, NativeAdapter};
use mote_hrns::{
    Action, ActionError, ActionExecutor, ActionReceipt, Driver, DriverError, DriverOutcome,
    Harness, HarnessAdapters, HarnessError, HarnessKind, HarnessReply, HarnessRequest, HrnsStore,
    NewTask, Scope, TaskContextProvider, TaskId,
};
use mote_localmodel::Ollama;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

const MAX_STEPS_PER_TURN: usize = 16;
const NATIVE_TOOLS: [&str; 15] = [
    "memory_search",
    "memory_write",
    "file_read",
    "file_list",
    "browser.open",
    "browser.snapshot",
    "browser.text",
    "browser.find",
    "browser.click",
    "browser.type",
    "browser.select",
    "browser.back",
    "browser.scroll",
    "browser.screenshot",
    "browser.fetch",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeTurnOutcome {
    Complete {
        task_id: TaskId,
        text: String,
        sources: Vec<String>,
    },
    WaitingForApproval {
        task_id: TaskId,
        action_id: String,
    },
    WaitingForInput {
        task_id: TaskId,
        question: String,
    },
    Blocked {
        task_id: TaskId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceSource {
    pub session_id: i64,
    pub utterance_id: u64,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceTurnOutcome {
    Enqueued(NativeTurnOutcome),
    Duplicate { task_id: TaskId },
}

struct UnavailableHarness;

impl Harness for UnavailableHarness {
    fn step(&mut self, _request: HarnessRequest) -> Result<HarnessReply, HarnessError> {
        Err(HarnessError::Unavailable)
    }
}

struct HeldActionExecutor;

impl ActionExecutor for HeldActionExecutor {
    fn execute(&mut self, _action: &Action) -> Result<ActionReceipt, ActionError> {
        Err(ActionError::Unavailable)
    }
}

#[derive(Clone, Default)]
struct NativeContexts(Arc<Mutex<HashMap<TaskId, String>>>);

impl NativeContexts {
    fn insert(&self, task_id: TaskId, context: String) {
        self.0.lock().unwrap().insert(task_id, context);
    }
}

impl TaskContextProvider for NativeContexts {
    fn build(
        &mut self,
        task: &mote_hrns::Task,
        _checkpoint: Option<&mote_hrns::Checkpoint>,
    ) -> Result<String, mote_hrns::ContextError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&task.id)
            .cloned()
            .unwrap_or_else(|| task.outcome.clone()))
    }
}

pub struct NativeTaskRuntime {
    driver: Driver,
    contexts: NativeContexts,
}

impl NativeTaskRuntime {
    pub fn production(
        database: &Path,
        log: EventLog,
        client: Ollama,
        gate: Arc<dyn ModelGate>,
    ) -> Result<Self, String> {
        let context_store = ContextStore::open(database).map_err(|error| error.to_string())?;
        let client = LedgeredLocalStepClient::new(
            log.clone(),
            Arc::new(Mutex::new(context_store)),
            OllamaStepTransport::new(client),
            gate,
            NATIVE_THINK_MODEL,
            NATIVE_THINK_CONTEXT,
        )?;
        Self::with_client(database, log, client)
    }

    pub fn with_client(
        database: &Path,
        log: EventLog,
        client: impl LocalStepClient + 'static,
    ) -> Result<Self, String> {
        let native =
            NativeAdapter::new(client, NATIVE_THINK_MODEL)?.with_allowed_tools(NATIVE_TOOLS)?;
        let store = HrnsStore::open(database, log).map_err(|error| error.to_string())?;
        let contexts = NativeContexts::default();
        let driver = Driver::new(
            store,
            HarnessAdapters {
                opencode: Box::new(UnavailableHarness),
                mote: Box::new(native),
            },
            Box::new(HeldActionExecutor),
        )
        .with_context_provider(contexts.clone());
        Ok(Self { driver, contexts })
    }

    pub fn submit(
        &mut self,
        task_id: TaskId,
        scope: Scope,
        outcome: String,
        context: String,
    ) -> Result<NativeTurnOutcome, DriverError> {
        self.contexts.insert(task_id.clone(), context);
        self.driver.enqueue(NewTask {
            id: task_id,
            scope,
            outcome,
            harness: HarnessKind::Mote,
        })?;
        for _ in 0..MAX_STEPS_PER_TURN {
            match self.driver.step()? {
                DriverOutcome::Started(_)
                | DriverOutcome::Message { .. }
                | DriverOutcome::Proposed(_)
                | DriverOutcome::Resumed(_)
                | DriverOutcome::Claimed(_)
                | DriverOutcome::Committed { .. }
                | DriverOutcome::Recovered { .. } => {}
                DriverOutcome::Completed {
                    task,
                    text,
                    sources,
                } => {
                    return Ok(NativeTurnOutcome::Complete {
                        task_id: task.id,
                        text,
                        sources,
                    });
                }
                DriverOutcome::AwaitingApproval { task, action_id } => {
                    return Ok(NativeTurnOutcome::WaitingForApproval {
                        task_id: task.id,
                        action_id: action_id.as_str().to_owned(),
                    });
                }
                DriverOutcome::WaitingForInput { task, question } => {
                    return Ok(NativeTurnOutcome::WaitingForInput {
                        task_id: task.id,
                        question,
                    });
                }
                DriverOutcome::Blocked(task) => {
                    return Ok(NativeTurnOutcome::Blocked { task_id: task.id });
                }
                DriverOutcome::Idle | DriverOutcome::Cancelled(_) => {
                    return Err(DriverError::CorruptState);
                }
            }
        }
        Err(DriverError::CorruptState)
    }

    pub fn enqueue_owner_turn(
        &mut self,
        scope: Scope,
        text: String,
        source: VoiceSource,
    ) -> Result<VoiceTurnOutcome, DriverError> {
        let task_id = TaskId::new(format!(
            "voice-{}-{}",
            source.session_id, source.utterance_id
        ))
        .map_err(|_| DriverError::CorruptState)?;
        if self
            .driver
            .store()
            .get(&scope, &task_id)?
            .is_some()
        {
            return Ok(VoiceTurnOutcome::Duplicate { task_id });
        }
        let context = format!(
            "Owner voice request. Session {} utterance {} from {}ms through {}ms.\n{}",
            source.session_id, source.utterance_id, source.start_ms, source.end_ms, text
        );
        self.submit(task_id, scope, text, context)
            .map(VoiceTurnOutcome::Enqueued)
    }

    #[cfg(test)]
    fn driver(&self) -> &Driver {
        &self.driver
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_hrns::adapters::{LocalStepError, RawStep, StepPrompt};
    use schemars::schema::RootSchema;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct ScriptedClient(VecDeque<Result<RawStep, LocalStepError>>);

    impl LocalStepClient for ScriptedClient {
        fn complete(
            &mut self,
            _prompt: &StepPrompt,
            _schema: &RootSchema,
        ) -> Result<RawStep, LocalStepError> {
            self.0.pop_front().expect("scripted native response")
        }
    }

    fn fixture(
        replies: impl IntoIterator<Item = Result<RawStep, LocalStepError>>,
    ) -> (std::path::PathBuf, NativeTaskRuntime, EventLog) {
        let root = std::env::temp_dir().join(format!(
            "mote-native-runtime-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let database = root.join("mote.db");
        let log = EventLog::open(&database).unwrap();
        let runtime = NativeTaskRuntime::with_client(
            &database,
            log.clone(),
            ScriptedClient(replies.into_iter().collect()),
        )
        .unwrap();
        (root, runtime, log)
    }

    fn task_id(value: &str) -> TaskId {
        TaskId::new(value).unwrap()
    }

    fn scope() -> Scope {
        Scope::new("owner", "personal").unwrap()
    }

    fn voice_source() -> VoiceSource {
        VoiceSource {
            session_id: 41,
            utterance_id: 7,
            start_ms: 100,
            end_ms: 900,
        }
    }

    #[test]
    fn selected_native_turn_runs_through_the_durable_driver() {
        let (root, mut runtime, log) = fixture([Ok(RawStep::new(
            r#"{"type":"complete","text":"finished","sources":["local:1"]}"#,
        ))]);
        let outcome = runtime
            .submit(
                task_id("turn-1"),
                scope(),
                "owner request".into(),
                "bounded context".into(),
            )
            .unwrap();
        assert_eq!(
            outcome,
            NativeTurnOutcome::Complete {
                task_id: task_id("turn-1"),
                text: "finished".into(),
                sources: vec!["local:1".into()],
            }
        );
        let (task, _) = runtime
            .driver()
            .store()
            .get(&scope(), &task_id("turn-1"))
            .unwrap()
            .unwrap();
        assert_eq!(task.state, mote_hrns::TaskState::Complete);
        assert!(log
            .recent_kind_since("task.stepped", i64::MIN, 10)
            .unwrap()
            .iter()
            .any(|event| event.payload["step"]["text"] == "finished"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_voice_final_creates_one_task_and_one_model_step() {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        server.set_chat_chunks(vec![
            serde_json::json!({
                "message":{"role":"assistant","content":"{\"type\":\"complete\",\"text\":\"finished\",\"sources\":[]}"},
                "done":true
            })
            .to_string(),
        ]);
        let root = std::env::temp_dir().join(format!(
            "mote-native-voice-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let database = root.join("mote.db");
        let log = EventLog::open(&database).unwrap();
        let mut runtime = NativeTaskRuntime::production(
            &database,
            log,
            Ollama::new(server.base_url()),
            Arc::new(crate::config::AllowAll),
        )
        .unwrap();
        let first = runtime
            .enqueue_owner_turn(scope(), "hello mote".into(), voice_source())
            .unwrap();
        assert!(matches!(first, VoiceTurnOutcome::Enqueued(_)));
        assert_eq!(
            runtime
                .enqueue_owner_turn(scope(), "hello mote".into(), voice_source())
                .unwrap(),
            VoiceTurnOutcome::Duplicate {
                task_id: task_id("voice-41-7")
            }
        );
        assert_eq!(
            ContextStore::open(&database)
                .unwrap()
                .count_ledger()
                .unwrap(),
            1
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tool_proposal_is_durable_and_held_for_approval() {
        let (root, mut runtime, _) = fixture([Ok(RawStep::new(
            r#"{"type":"tool","name":"file_read","arguments":{"path":"notes.txt"}}"#,
        ))]);
        let outcome = runtime
            .submit(
                task_id("turn-2"),
                scope(),
                "read notes".into(),
                "bounded context".into(),
            )
            .unwrap();
        assert_eq!(
            outcome,
            NativeTurnOutcome::WaitingForApproval {
                task_id: task_id("turn-2"),
                action_id: "turn-2:1".into(),
            }
        );
        let action = runtime
            .driver()
            .store()
            .latest_action(&scope(), &task_id("turn-2"))
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(action.state, mote_hrns::ActionState::AwaitingApproval);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_native_output_fails_the_durable_task_closed() {
        let (root, mut runtime, _) = fixture([Err(LocalStepError::Malformed)]);
        assert!(matches!(
            runtime.submit(
                task_id("turn-3"),
                scope(),
                "request".into(),
                "bounded context".into(),
            ),
            Err(DriverError::Harness(HarnessError::InvalidOutput))
        ));
        let (task, _) = runtime
            .driver()
            .store()
            .get(&scope(), &task_id("turn-3"))
            .unwrap()
            .unwrap();
        assert_eq!(task.state, mote_hrns::TaskState::Failed);
        std::fs::remove_dir_all(root).unwrap();
    }
}
