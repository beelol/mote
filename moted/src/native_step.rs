use crate::config::ModelGate;
use crate::model_call::{ledger_entry, CallTiming, ModelRequest};
use mote_context::{ContextStore, Pack, PartKind};
use mote_eventlog::EventLog;
use mote_hrns::adapters::{
    model_step_schema, LocalStepClient, LocalStepError, RawStep, StepPrompt,
};
use mote_hrns::ModelStep;
use mote_localmodel::Ollama;
use mote_redact::redact;
use schemars::schema::RootSchema;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

pub(crate) const NATIVE_THINK_MODEL: &str = "qwen3-coder:30b-64k";
pub(crate) const NATIVE_THINK_CONTEXT: u32 = 65_536;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StepTransportError {
    Timeout,
    Unavailable,
    Malformed(String),
}

pub(crate) trait StructuredStepTransport: Send {
    fn complete(
        &mut self,
        model: &str,
        messages: Value,
        schema: Value,
        num_ctx: u32,
    ) -> Result<String, StepTransportError>;
}

pub(crate) struct OllamaStepTransport {
    client: Ollama,
}

impl OllamaStepTransport {
    pub(crate) fn new(client: Ollama) -> Self {
        Self { client }
    }
}

impl StructuredStepTransport for OllamaStepTransport {
    fn complete(
        &mut self,
        model: &str,
        messages: Value,
        schema: Value,
        num_ctx: u32,
    ) -> Result<String, StepTransportError> {
        self.client
            .chat_structured_with_raw::<ModelStep>(model, messages, schema, num_ctx)
            .map(|reply| reply.content)
            .map_err(|error| match error.content {
                Some(content) => StepTransportError::Malformed(content),
                None => StepTransportError::Unavailable,
            })
    }
}

pub(crate) struct LedgeredLocalStepClient<T> {
    log: EventLog,
    store: Arc<Mutex<ContextStore>>,
    transport: T,
    gate: Arc<dyn ModelGate>,
    model: String,
    num_ctx: u32,
}

impl<T> LedgeredLocalStepClient<T>
where
    T: StructuredStepTransport,
{
    pub(crate) fn new(
        log: EventLog,
        store: Arc<Mutex<ContextStore>>,
        transport: T,
        gate: Arc<dyn ModelGate>,
        model: impl Into<String>,
        num_ctx: u32,
    ) -> Result<Self, String> {
        let model = model.into();
        if model != NATIVE_THINK_MODEL {
            return Err(format!(
                "native harness requires approved local think model {NATIVE_THINK_MODEL}"
            ));
        }
        if num_ctx == 0 {
            return Err("native harness context must be greater than zero".into());
        }
        Ok(Self {
            log,
            store,
            transport,
            gate,
            model,
            num_ctx,
        })
    }

    fn pack(system: &str, input: &str) -> Pack {
        Pack::from_texts(
            "think",
            vec![
                (PartKind::Charter, "native_system".into(), system.into()),
                (PartKind::Profile, "native_profile".into(), String::new()),
                (PartKind::Memory, "native_memory".into(), String::new()),
                (
                    PartKind::WorkingSet,
                    "native_working_set".into(),
                    String::new(),
                ),
                (PartKind::Turn, "native_request".into(), input.into()),
            ],
        )
    }

    fn record(
        &self,
        pack: &Pack,
        completion: &str,
        timing: CallTiming,
        status: &str,
    ) -> Result<(), LocalStepError> {
        let entry = ledger_entry(
            "think",
            &self.model,
            pack,
            completion,
            timing,
            false,
            status,
        );
        self.store
            .lock()
            .map_err(|_| LocalStepError::Unavailable)?
            .record(pack, &entry)
            .map(|_| ())
            .map_err(|_| LocalStepError::Unavailable)
    }

    fn reject(
        &self,
        request: ModelRequest<'_>,
        pack: &Pack,
        status: &str,
        error: LocalStepError,
    ) -> Result<RawStep, LocalStepError> {
        let timing = request.finish(status, None);
        self.record(pack, "", timing, status)?;
        Err(error)
    }
}

impl<T> LocalStepClient for LedgeredLocalStepClient<T>
where
    T: StructuredStepTransport,
{
    fn complete(
        &mut self,
        prompt: &StepPrompt,
        schema: &RootSchema,
    ) -> Result<RawStep, LocalStepError> {
        let system = redact(prompt.system());
        let input = redact(prompt.input());
        let pack = Self::pack(&system, &input);
        let supplied_schema =
            serde_json::to_value(schema).map_err(|_| LocalStepError::Malformed)?;
        let expected_schema =
            serde_json::to_value(model_step_schema()).map_err(|_| LocalStepError::Malformed)?;
        let messages = json!([
            { "role": "system", "content": system },
            { "role": "user", "content": input }
        ]);
        let request = match ModelRequest::begin(
            &self.log,
            "think",
            &self.model,
            json!({
                "num_ctx": self.num_ctx,
                "format": supplied_schema,
                "messages": messages,
            }),
        ) {
            Ok(request) => request,
            Err(_) => {
                self.record(
                    &pack,
                    "",
                    CallTiming {
                        total_ms: 0,
                        first_token_ms: None,
                    },
                    "unavailable",
                )?;
                return Err(LocalStepError::Unavailable);
            }
        };

        if supplied_schema != expected_schema {
            return self.reject(request, &pack, "malformed", LocalStepError::Malformed);
        }
        if prompt.model() != self.model {
            return self.reject(request, &pack, "malformed", LocalStepError::Malformed);
        }
        if self.gate.check(&self.model).is_err() {
            return self.reject(request, &pack, "unavailable", LocalStepError::Unavailable);
        }

        let result = self
            .transport
            .complete(&self.model, messages, supplied_schema, self.num_ctx);
        let (reply, completion, status) = match result {
            Ok(content) => (Ok(RawStep::new(content.clone())), content, "ok"),
            Err(StepTransportError::Timeout) => {
                (Err(LocalStepError::Timeout), String::new(), "timeout")
            }
            Err(StepTransportError::Unavailable) => (
                Err(LocalStepError::Unavailable),
                String::new(),
                "unavailable",
            ),
            Err(StepTransportError::Malformed(content)) => {
                (Err(LocalStepError::Malformed), content, "malformed")
            }
        };
        let timing = request.finish(status, None);
        self.record(&pack, &completion, timing, status)?;
        reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelGate;
    use mote_context::{count_tokens, hash_text};
    use mote_hrns::adapters::NativeAdapter;
    use mote_hrns::{
        Harness, HarnessError, HarnessKind, HarnessRequest, Scope, Task, TaskId, TaskState,
    };
    use std::collections::VecDeque;

    const SECRET: &str = "sk-abcdefghijklmnopqrstuvwxyz";

    #[derive(Default)]
    struct Observed {
        calls: Vec<(String, Value, Value, u32)>,
    }

    struct ScriptedTransport {
        replies: VecDeque<Result<String, StepTransportError>>,
        observed: Arc<Mutex<Observed>>,
    }

    impl StructuredStepTransport for ScriptedTransport {
        fn complete(
            &mut self,
            model: &str,
            messages: Value,
            schema: Value,
            num_ctx: u32,
        ) -> Result<String, StepTransportError> {
            self.observed
                .lock()
                .unwrap()
                .calls
                .push((model.to_owned(), messages, schema, num_ctx));
            self.replies.pop_front().expect("scripted native step")
        }
    }

    struct FixedGate(Result<(), String>);

    impl ModelGate for FixedGate {
        fn check(&self, _model: &str) -> Result<(), String> {
            self.0.clone()
        }
    }

    fn request() -> HarnessRequest {
        HarnessRequest {
            task: Task {
                id: TaskId::new("native-ledger").unwrap(),
                scope: Scope::new("owner", "personal").unwrap(),
                outcome: format!("Keep {SECRET} private"),
                state: TaskState::Running,
                harness: HarnessKind::Mote,
            },
            context: "bounded context".into(),
            checkpoint: None,
        }
    }

    type TestClient = (
        LedgeredLocalStepClient<ScriptedTransport>,
        Arc<Mutex<ContextStore>>,
        EventLog,
        Arc<Mutex<Observed>>,
    );

    fn client(
        replies: Vec<Result<String, StepTransportError>>,
        gate: Result<(), String>,
    ) -> TestClient {
        let log = EventLog::open_in_memory().unwrap();
        let store = Arc::new(Mutex::new(ContextStore::open_in_memory().unwrap()));
        let observed = Arc::new(Mutex::new(Observed::default()));
        let transport = ScriptedTransport {
            replies: replies.into(),
            observed: observed.clone(),
        };
        let client = LedgeredLocalStepClient::new(
            log.clone(),
            store.clone(),
            transport,
            Arc::new(FixedGate(gate)),
            NATIVE_THINK_MODEL,
            NATIVE_THINK_CONTEXT,
        )
        .unwrap();
        (client, store, log, observed)
    }

    #[test]
    fn sends_only_the_approved_model_exact_schema_and_redacted_prompt() {
        assert!(LedgeredLocalStepClient::new(
            EventLog::open_in_memory().unwrap(),
            Arc::new(Mutex::new(ContextStore::open_in_memory().unwrap())),
            ScriptedTransport {
                replies: VecDeque::new(),
                observed: Arc::new(Mutex::new(Observed::default())),
            },
            Arc::new(FixedGate(Ok(()))),
            "qwen3:4b",
            NATIVE_THINK_CONTEXT,
        )
        .is_err());

        let output = r#"{"type":"message","text":"working"}"#.to_string();
        let (client, store, log, observed) = client(vec![Ok(output)], Ok(()));
        let mut adapter = NativeAdapter::new(client, NATIVE_THINK_MODEL).unwrap();
        adapter.step(request()).unwrap();

        let observed = observed.lock().unwrap();
        assert_eq!(observed.calls.len(), 1);
        assert_eq!(observed.calls[0].0, NATIVE_THINK_MODEL);
        assert_eq!(
            observed.calls[0].2,
            serde_json::to_value(model_step_schema()).unwrap()
        );
        assert_eq!(observed.calls[0].3, NATIVE_THINK_CONTEXT);
        assert!(!observed.calls[0].1.to_string().contains(SECRET));
        assert!(observed.calls[0].1.to_string().contains("[redacted]"));
        let sent_messages = observed.calls[0].1.clone();
        drop(observed);

        let store = store.lock().unwrap();
        assert_eq!(store.count_packs().unwrap(), 1);
        assert_eq!(store.count_ledger().unwrap(), 1);
        let parts_json: String = store
            .conn()
            .query_row("SELECT parts FROM packs", [], |row| row.get(0))
            .unwrap();
        assert!(!parts_json.contains(SECRET));
        let parts: Vec<mote_context::Part> = serde_json::from_str(&parts_json).unwrap();
        assert_eq!(
            parts[0].hash,
            hash_text(sent_messages[0]["content"].as_str().unwrap())
        );
        assert_eq!(
            parts[4].hash,
            hash_text(sent_messages[1]["content"].as_str().unwrap())
        );
        let events = log.recent_kind_since("model.request", 0, 10).unwrap();
        assert_eq!(events.len(), 1);
        assert!(!events[0].payload.to_string().contains(SECRET));
    }

    #[test]
    fn every_attempt_records_one_pack_and_ledger_row_on_all_failures() {
        let replies = vec![
            Err(StepTransportError::Malformed("not json".into())),
            Err(StepTransportError::Timeout),
            Err(StepTransportError::Unavailable),
        ];
        let (step_client, store, log, observed) = client(replies, Ok(()));
        let mut adapter = NativeAdapter::new(step_client, NATIVE_THINK_MODEL).unwrap();
        assert_eq!(adapter.step(request()), Err(HarnessError::InvalidOutput));
        assert_eq!(adapter.step(request()), Err(HarnessError::Timeout));
        assert_eq!(adapter.step(request()), Err(HarnessError::Unavailable));

        let (refused, refused_store, refused_log, refused_observed) =
            client(Vec::new(), Err("memory gate refused".into()));
        let mut refused = NativeAdapter::new(refused, NATIVE_THINK_MODEL).unwrap();
        assert_eq!(refused.step(request()), Err(HarnessError::Unavailable));

        assert_eq!(observed.lock().unwrap().calls.len(), 3);
        assert!(refused_observed.lock().unwrap().calls.is_empty());
        let store = store.lock().unwrap();
        assert_eq!(store.count_packs().unwrap(), 3);
        assert_eq!(store.count_ledger().unwrap(), 3);
        let rows = store.ledger_since(0).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.status.as_str())
                .collect::<Vec<_>>(),
            vec!["unavailable", "timeout", "malformed"]
        );
        assert_eq!(rows[2].completion_tokens, count_tokens("not json") as i64);
        drop(store);

        let refused_store = refused_store.lock().unwrap();
        assert_eq!(refused_store.count_packs().unwrap(), 1);
        assert_eq!(refused_store.count_ledger().unwrap(), 1);
        assert_eq!(
            refused_store.ledger_since(0).unwrap()[0].status,
            "unavailable"
        );
        assert_eq!(
            log.recent_kind_since("model.response", 0, 10)
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            refused_log
                .recent_kind_since("model.response", 0, 10)
                .unwrap()
                .len(),
            1
        );
    }
}
