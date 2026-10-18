use crate::ModelStep;
use serde::{Deserialize, Serialize};

macro_rules! identity {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err("identity must not be empty".into());
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = String;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

identity!(TaskId);
identity!(ActionId);
identity!(AccountId);
identity!(RealmId);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub account: AccountId,
    pub realm: RealmId,
}

impl Scope {
    pub fn new(account: impl Into<String>, realm: impl Into<String>) -> Result<Self, String> {
        Ok(Self {
            account: AccountId::new(account)?,
            realm: RealmId::new(realm)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessKind {
    Opencode,
    Mote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    Queued,
    Running,
    WaitingForApproval,
    WaitingForInput,
    Blocked,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTransition;

impl std::fmt::Display for InvalidTransition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("stale or invalid state transition")
    }
}

impl std::error::Error for InvalidTransition {}

impl TaskState {
    pub fn permits(self, next: Self) -> bool {
        use TaskState::*;
        matches!(
            (self, next),
            (Queued, Running | Cancelled)
                | (
                    Running,
                    WaitingForApproval | WaitingForInput | Blocked | Complete | Failed | Cancelled
                )
                | (
                    WaitingForApproval | WaitingForInput | Blocked,
                    Running | Failed | Cancelled
                )
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Failed | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: TaskId,
    pub scope: Scope,
    pub outcome: String,
    pub state: TaskState,
    pub harness: HarnessKind,
}

impl Task {
    pub fn transition(
        &mut self,
        expected: TaskState,
        next: TaskState,
    ) -> Result<(), InvalidTransition> {
        if self.state != expected || !self.state.permits(next) {
            return Err(InvalidTransition);
        }
        self.state = next;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub task_id: TaskId,
    pub revision: u64,
    pub state: TaskState,
    pub last_action: Option<ActionId>,
    pub step: Option<ModelStep>,
    #[serde(default)]
    pub adapter: Option<serde_json::Value>,
}
