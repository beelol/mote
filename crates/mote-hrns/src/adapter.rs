use crate::{Checkpoint, ModelStep, Task};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessRequest {
    pub task: Task,
    pub context: String,
    pub checkpoint: Option<Checkpoint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HarnessError {
    Unavailable,
    InvalidOutput,
    Timeout,
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "harness unavailable",
            Self::InvalidOutput => "invalid harness output",
            Self::Timeout => "harness timed out",
        })
    }
}

impl std::error::Error for HarnessError {}

pub trait Harness: Send {
    fn step(&mut self, request: HarnessRequest) -> Result<ModelStep, HarnessError>;
}
