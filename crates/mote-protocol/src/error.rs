use crate::VersionRange;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostErrorCode {
    ParseError,
    InvalidRequest,
    InvalidParams,
    IncompatibleVersion,
    IdentityChanged,
    Unauthorized,
    InvalidCursor,
    StaleSurface,
    InvalidInteraction,
    InvalidApproval,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostError {
    pub code: HostErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported: Option<VersionRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offered: Option<VersionRange>,
}

impl HostError {
    pub fn new(code: HostErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            supported: None,
            offered: None,
        }
    }
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HostError {}
