use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelStep {
    Message {
        text: String,
    },
    Tool {
        name: String,
        arguments: Map<String, Value>,
    },
    Complete {
        text: String,
        sources: Vec<String>,
    },
    WaitForInput {
        question: String,
    },
}
