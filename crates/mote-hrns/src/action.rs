use crate::{ActionId, TaskId};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionState {
    Proposed,
    AwaitingApproval,
    Ready,
    Running,
    Committed,
    Denied,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub id: ActionId,
    pub task_id: TaskId,
    pub state: ActionState,
    pub name: String,
    pub arguments: Map<String, Value>,
}
