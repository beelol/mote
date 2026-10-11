use crate::{HostError, HostErrorCode};
use serde::{de::Error, Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

pub const PROTOCOL_MAJOR: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedVersionRange")]
pub struct VersionRange {
    pub min: u16,
    pub max: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedVersionRange {
    min: u16,
    max: u16,
}

impl TryFrom<UncheckedVersionRange> for VersionRange {
    type Error = HostError;

    fn try_from(value: UncheckedVersionRange) -> Result<Self, Self::Error> {
        Self::new(value.min, value.max)
    }
}

impl VersionRange {
    pub fn new(min: u16, max: u16) -> Result<Self, HostError> {
        if min > max {
            return Err(HostError::new(
                HostErrorCode::InvalidRequest,
                "invalid protocol version range",
            ));
        }
        Ok(Self { min, max })
    }

    pub fn current() -> Self {
        Self {
            min: PROTOCOL_MAJOR,
            max: PROTOCOL_MAJOR,
        }
    }

    pub fn negotiate(self, offered: Self) -> Result<u16, HostError> {
        if self.min <= PROTOCOL_MAJOR
            && PROTOCOL_MAJOR <= self.max
            && offered.min <= PROTOCOL_MAJOR
            && PROTOCOL_MAJOR <= offered.max
        {
            return Ok(PROTOCOL_MAJOR);
        }
        Err(HostError {
            code: HostErrorCode::IncompatibleVersion,
            message: "no supported protocol major in common".into(),
            supported: Some(self),
            offered: Some(offered),
        })
    }
}

pub(crate) fn supported_versions<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<VersionRange, D::Error> {
    let versions = VersionRange::deserialize(deserializer)?;
    VersionRange::current()
        .negotiate(versions)
        .map_err(D::Error::custom)?;
    Ok(versions)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HostId(String);

impl HostId {
    pub fn new(value: impl Into<String>) -> Result<Self, HostError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(HostError::new(
                HostErrorCode::InvalidRequest,
                "host ID must be a lowercase SHA-256 digest",
            ));
        }
        Ok(Self(value))
    }

    pub fn from_public_key(public_key: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(public_key)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for HostId {
    type Error = HostError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<HostId> for String {
    fn from(value: HostId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RequestId(String);

impl RequestId {
    pub fn new(value: impl Into<String>) -> Result<Self, HostError> {
        let value = value.into();
        if value.is_empty() {
            return Err(HostError::new(
                HostErrorCode::InvalidRequest,
                "request ID must be nonempty",
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RequestId {
    type Error = HostError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<RequestId> for String {
    fn from(value: RequestId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub host_id: HostId,
    #[serde(deserialize_with = "crate::nonnegative")]
    pub seq: i64,
}

impl Cursor {
    pub fn new(host_id: HostId, seq: i64) -> Result<Self, HostError> {
        if seq < 0 {
            return Err(HostError::new(
                HostErrorCode::InvalidCursor,
                "cursor sequence must be nonnegative",
            ));
        }
        Ok(Self { host_id, seq })
    }
}
