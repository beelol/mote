//! Transport-independent host frames and the private local JSON envelopes.

pub mod envelope;
mod error;
mod frame;
mod snapshot;
mod version;

pub use error::{HostError, HostErrorCode};
pub use frame::{
    ApprovalDecision, ApprovalVerdict, ClientFrame, Event, HostFrame, TypedInteraction,
};
pub use snapshot::{
    ChatMessage, CommittedSurface, HostSnapshot, MessageRole, MessageStatus, PendingApproval,
    ThreadSnapshot,
};
pub use version::{Cursor, HostId, RequestId, VersionRange, PROTOCOL_MAJOR};

use serde::{de::Error, Deserialize, Deserializer};

fn nonempty<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.is_empty() {
        return Err(D::Error::custom("string must be nonempty"));
    }
    Ok(value)
}

fn nonnegative<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    let value = i64::deserialize(deserializer)?;
    if value < 0 {
        return Err(D::Error::custom("sequence must be nonnegative"));
    }
    Ok(value)
}

fn surface_contract<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    match u32::deserialize(deserializer)? {
        1 => Ok(1),
        _ => Err(D::Error::custom("unsupported Matrix surface contract")),
    }
}
