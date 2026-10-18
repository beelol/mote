use mote_hrns::*;
use serde_json::json;

#[test]
fn identities_reject_empty_values_even_when_deserialized() {
    for value in ["", " ", "\n"] {
        assert!(TaskId::new(value).is_err());
        assert!(serde_json::from_value::<TaskId>(json!(value)).is_err());
        assert!(serde_json::from_value::<ActionId>(json!(value)).is_err());
        assert!(Scope::new(value, "personal").is_err());
        assert!(
            serde_json::from_value::<Scope>(json!({"account": "owner", "realm": value})).is_err()
        );
    }
}

#[test]
fn steps_roundtrip_and_refuse_unrecognized_or_missing_fields() {
    for value in [
        json!({"type":"message", "text":"Working"}),
        json!({"type":"tool", "name":"memory_search", "arguments":{"query":"trip"}}),
        json!({"type":"complete", "text":"Done", "sources":["source-1"]}),
        json!({"type":"wait_for_input", "question":"Which city?"}),
    ] {
        let step: ModelStep = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(step).unwrap(), value);
        let mut extra = value;
        extra["execute"] = json!(true);
        assert!(serde_json::from_value::<ModelStep>(extra).is_err());
    }
    for value in [
        json!({"type":"message"}),
        json!({"type":"execute"}),
        json!({"type":"tool", "name":"x", "arguments":[]}),
    ] {
        assert!(serde_json::from_value::<ModelStep>(value).is_err());
    }
}

struct FakeHarness;
impl Harness for FakeHarness {
    fn step(&mut self, request: HarnessRequest) -> Result<ModelStep, HarnessError> {
        assert_eq!(request.task.scope, Scope::new("owner", "personal").unwrap());
        Ok(ModelStep::Complete {
            text: request.task.outcome,
            sources: vec![],
        })
    }
}

#[test]
fn adapters_share_task_and_checkpoint_contracts() {
    for harness in [HarnessKind::Opencode, HarnessKind::Mote] {
        let mut task = Task {
            id: TaskId::new("task-1").unwrap(),
            scope: Scope::new("owner", "personal").unwrap(),
            outcome: "Compare options".into(),
            state: TaskState::Queued,
            harness,
        };
        for state in [
            TaskState::Queued,
            TaskState::Running,
            TaskState::WaitingForApproval,
            TaskState::WaitingForInput,
            TaskState::Blocked,
            TaskState::Complete,
            TaskState::Failed,
            TaskState::Cancelled,
        ] {
            task.state = state;
            let encoded = serde_json::to_value(&task).unwrap();
            assert_eq!(serde_json::from_value::<Task>(encoded).unwrap(), task);
        }
        let request = HarnessRequest {
            task,
            context: "bounded context".into(),
            checkpoint: None,
        };
        let adapter: &mut dyn Harness = &mut FakeHarness;
        assert!(matches!(
            adapter.step(request),
            Ok(ModelStep::Complete { .. })
        ));
    }
}

#[test]
fn actions_and_checkpoints_roundtrip_without_adapter_state() {
    for state in [
        ActionState::Proposed,
        ActionState::AwaitingApproval,
        ActionState::Ready,
        ActionState::Running,
        ActionState::Committed,
        ActionState::Denied,
        ActionState::Failed,
        ActionState::Cancelled,
    ] {
        let action = Action {
            id: ActionId::new("action-1").unwrap(),
            task_id: TaskId::new("task-1").unwrap(),
            state,
            name: "browser_read".into(),
            arguments: Default::default(),
        };
        assert_eq!(
            serde_json::from_value::<Action>(serde_json::to_value(&action).unwrap()).unwrap(),
            action
        );
    }
    let checkpoint = Checkpoint {
        task_id: TaskId::new("task-1").unwrap(),
        revision: 1,
        state: TaskState::WaitingForInput,
        last_action: None,
        step: Some(ModelStep::WaitForInput {
            question: "Which city?".into(),
        }),
    };
    let mut value = serde_json::to_value(&checkpoint).unwrap();
    assert_eq!(
        serde_json::from_value::<Checkpoint>(value.clone()).unwrap(),
        checkpoint
    );
    value["opencode_session"] = json!("foreign");
    assert!(serde_json::from_value::<Checkpoint>(value).is_err());
}
