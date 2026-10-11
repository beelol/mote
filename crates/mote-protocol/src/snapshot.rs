use crate::{Cursor, HostError, HostErrorCode, HostId, RequestId};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedSurface {
    #[serde(deserialize_with = "crate::nonempty")]
    pub thread: String,
    #[serde(deserialize_with = "crate::nonnegative")]
    pub surface_seq: i64,
    #[serde(deserialize_with = "crate::surface_contract")]
    pub surface_contract: u32,
    pub tree: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Sending,
    Sent,
    Delivered,
    Read,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatMessage {
    #[serde(deserialize_with = "crate::nonempty")]
    pub id: String,
    pub role: MessageRole,
    pub text: String,
    pub status: MessageStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThreadSnapshot {
    #[serde(deserialize_with = "crate::nonempty")]
    pub thread: String,
    pub messages: Vec<ChatMessage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingApproval {
    pub approval_id: RequestId,
    #[serde(deserialize_with = "crate::nonempty")]
    pub thread: String,
    #[serde(deserialize_with = "crate::nonempty")]
    pub kind: String,
    pub payload: Map<String, Value>,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedHostSnapshot")]
pub struct HostSnapshot {
    pub host_id: HostId,
    pub cursor: Cursor,
    pub threads: Vec<ThreadSnapshot>,
    pub surfaces: Vec<CommittedSurface>,
    pub pending_approvals: Vec<PendingApproval>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedHostSnapshot {
    host_id: HostId,
    cursor: Cursor,
    threads: Vec<ThreadSnapshot>,
    surfaces: Vec<CommittedSurface>,
    pending_approvals: Vec<PendingApproval>,
}

impl TryFrom<UncheckedHostSnapshot> for HostSnapshot {
    type Error = HostError;

    fn try_from(value: UncheckedHostSnapshot) -> Result<Self, Self::Error> {
        if value.host_id != value.cursor.host_id {
            return Err(HostError::new(
                HostErrorCode::InvalidCursor,
                "snapshot cursor belongs to another host",
            ));
        }
        Ok(Self {
            host_id: value.host_id,
            cursor: value.cursor,
            threads: value.threads,
            surfaces: value.surfaces,
            pending_approvals: value.pending_approvals,
        })
    }
}
