use crate::{Action, ActionId};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Committed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionReceipt {
    pub action_id: ActionId,
    pub request_hash: String,
    pub status: ReceiptStatus,
    pub committed_at: u64,
    pub result_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionError {
    Unavailable,
    Failed,
    Indeterminate,
}

impl std::fmt::Display for ActionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "action executor unavailable",
            Self::Failed => "action execution failed",
            Self::Indeterminate => "action outcome is indeterminate",
        })
    }
}

impl std::error::Error for ActionError {}

pub trait ActionExecutor: Send {
    fn execute(&mut self, action: &Action) -> Result<ActionReceipt, ActionError>;
}

pub trait ActionEffect: Send {
    fn apply_once(&mut self, action: &Action) -> Result<Option<String>, ActionError>;
}

pub struct MoteActionExecutor<E> {
    effect: E,
    clock: fn() -> u64,
}

impl<E> MoteActionExecutor<E> {
    pub fn new(effect: E) -> Self {
        Self {
            effect,
            clock: unix_time,
        }
    }

    pub fn with_clock(effect: E, clock: fn() -> u64) -> Self {
        Self { effect, clock }
    }
}

impl<E: ActionEffect> ActionExecutor for MoteActionExecutor<E> {
    fn execute(&mut self, action: &Action) -> Result<ActionReceipt, ActionError> {
        let result_ref = self.effect.apply_once(action)?;
        Ok(ActionReceipt {
            action_id: action.id.clone(),
            request_hash: action_request_hash(action),
            status: ReceiptStatus::Committed,
            committed_at: (self.clock)(),
            result_ref,
        })
    }
}

pub fn action_request_hash(action: &Action) -> String {
    let request = serde_json::json!({
        "action_id": action.id,
        "task_id": action.task_id,
        "name": action.name,
        "arguments": action.arguments,
    });
    let canonical = canonicalize(request);
    format!("{:x}", Sha256::digest(serde_json::to_vec(&canonical).expect("action request JSON")))
}

fn canonicalize(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize).collect()),
        Value::Object(values) => {
            let mut keys: Vec<_> = values.into_iter().collect();
            keys.sort_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                keys.into_iter()
                    .map(|(key, value)| (key, canonicalize(value)))
                    .collect::<Map<_, _>>(),
            )
        }
        scalar => scalar,
    }
}

fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
