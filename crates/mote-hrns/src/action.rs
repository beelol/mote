use crate::{ActionId, InvalidTransition, TaskId};
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

impl ActionState {
    pub fn permits(self, next: Self) -> bool {
        use ActionState::*;
        matches!(
            (self, next),
            (Proposed, AwaitingApproval | Ready | Cancelled | Failed)
                | (AwaitingApproval, Ready | Denied | Cancelled | Failed)
                | (Ready, Running | Cancelled | Failed)
                | (Running, Committed | Failed)
        )
    }
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

impl Action {
    pub fn transition(
        &mut self,
        expected: ActionState,
        next: ActionState,
    ) -> Result<(), InvalidTransition> {
        if self.state != expected || !self.state.permits(next) {
            return Err(InvalidTransition);
        }
        self.state = next;
        Ok(())
    }
}
