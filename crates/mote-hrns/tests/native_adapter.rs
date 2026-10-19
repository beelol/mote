use mote_hrns::adapters::{LocalStepClient, LocalStepError, NativeAdapter, RawStep, StepPrompt};
use mote_hrns::{
    Harness, HarnessError, HarnessKind, HarnessRequest, ModelStep, Scope, Task, TaskId, TaskState,
};
use schemars::schema::RootSchema;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Observed {
    prompts: Vec<StepPrompt>,
    schemas: Vec<Value>,
}

struct ScriptedClient {
    replies: VecDeque<Result<RawStep, LocalStepError>>,
    observed: Arc<Mutex<Observed>>,
}

impl LocalStepClient for ScriptedClient {
    fn complete(
        &mut self,
        prompt: &StepPrompt,
        schema: &RootSchema,
    ) -> Result<RawStep, LocalStepError> {
        let mut observed = self.observed.lock().unwrap();
        observed.prompts.push(prompt.clone());
        observed.schemas.push(serde_json::to_value(schema).unwrap());
        drop(observed);
        self.replies.pop_front().expect("scripted native step")
    }
}

fn request() -> HarnessRequest {
    HarnessRequest {
        task: Task {
            id: TaskId::new("task-native").unwrap(),
            scope: Scope::new("owner", "personal").unwrap(),
            outcome: "Research the supplied notes".into(),
            state: TaskState::Running,
            harness: HarnessKind::Mote,
        },
        context: "Scoped and bounded context".into(),
        checkpoint: None,
    }
}

fn adapter(replies: Vec<Result<RawStep, LocalStepError>>) -> (NativeAdapter, Arc<Mutex<Observed>>) {
    let observed = Arc::new(Mutex::new(Observed::default()));
    let client = ScriptedClient {
        replies: replies.into(),
        observed: observed.clone(),
    };
    let adapter = NativeAdapter::new(client, "qwen3-coder:30b")
        .unwrap()
        .with_allowed_tools(["memory_search", "file_read"])
        .unwrap();
    (adapter, observed)
}

#[test]
fn accepts_each_strict_model_step_and_supplies_schema() {
    let values = [
        json!({"type":"message","text":"Working"}),
        json!({"type":"tool","name":"memory_search","arguments":{"query":"notes"}}),
        json!({"type":"wait_for_input","question":"Which note set?"}),
        json!({"type":"complete","text":"Done","sources":["memory:sqlite:1:1"]}),
    ];
    let replies = values
        .iter()
        .map(|value| Ok(RawStep::new(value.to_string())))
        .collect();
    let (mut adapter, observed) = adapter(replies);
    for expected in values {
        let reply = adapter.step(request()).unwrap();
        assert_eq!(serde_json::to_value(reply.step).unwrap(), expected);
        assert_eq!(reply.adapter_checkpoint, None);
    }

    let observed = observed.lock().unwrap();
    assert_eq!(observed.prompts.len(), 4);
    assert!(observed
        .prompts
        .iter()
        .all(|prompt| prompt.model() == "qwen3-coder:30b"));
    assert!(observed
        .prompts
        .iter()
        .all(|prompt| prompt.system().contains("never execute")));
    let prompt_input: Value = serde_json::from_str(observed.prompts[0].input()).unwrap();
    assert_eq!(prompt_input["task"]["id"], "task-native");
    assert_eq!(prompt_input["context"], "Scoped and bounded context");
    let schema_text = observed.schemas[0].to_string();
    assert!(schema_text.contains("\"additionalProperties\":false"));
    assert!(schema_text.contains("wait_for_input"));
}

#[test]
fn rejects_malformed_extra_empty_and_unknown_tool_outputs() {
    let replies = vec![
        Ok(RawStep::new("not json")),
        Ok(RawStep::new(
            json!({"type":"message","text":"valid","execute":true}).to_string(),
        )),
        Ok(RawStep::new(
            json!({"type":"message","text":" "}).to_string(),
        )),
        Ok(RawStep::new(
            json!({"type":"tool","name":"shell","arguments":{}}).to_string(),
        )),
        Ok(RawStep::new(
            json!({"type":"complete","text":"done","sources":[""]}).to_string(),
        )),
    ];
    let (mut adapter, _) = adapter(replies);
    for _ in 0..5 {
        assert_eq!(adapter.step(request()), Err(HarnessError::InvalidOutput));
    }
}

#[test]
fn maps_local_failures_without_retaining_adapter_state() {
    let replies = vec![
        Err(LocalStepError::Timeout),
        Err(LocalStepError::Unavailable),
        Err(LocalStepError::Malformed),
        Ok(RawStep::new(
            json!({"type":"message","text":"Recovered"}).to_string(),
        )),
    ];
    let (mut adapter, observed) = adapter(replies);
    assert_eq!(adapter.step(request()), Err(HarnessError::Timeout));
    assert_eq!(adapter.step(request()), Err(HarnessError::Unavailable));
    assert_eq!(adapter.step(request()), Err(HarnessError::InvalidOutput));
    assert_eq!(
        adapter.step(request()).unwrap().step,
        ModelStep::Message {
            text: "Recovered".into()
        }
    );
    assert_eq!(observed.lock().unwrap().prompts.len(), 4);
}
