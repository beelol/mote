use crate::Event;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestEnvelope {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SuccessEnvelope {
    pub id: Value,
    pub result: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub id: Value,
    pub error: Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event: Event,
}
