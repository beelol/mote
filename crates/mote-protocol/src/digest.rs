use crate::{HostError, HostErrorCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ActionDigest(String);

impl ActionDigest {
    pub fn new(value: impl Into<String>) -> Result<Self, HostError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(HostError::new(
                HostErrorCode::InvalidApproval,
                "action hash must be a lowercase SHA-256 digest",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ActionDigest {
    type Error = HostError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ActionDigest> for String {
    fn from(value: ActionDigest) -> Self {
        value.0
    }
}
