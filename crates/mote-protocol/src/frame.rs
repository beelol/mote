use crate::{
    CommittedSurface, Cursor, HostError, HostErrorCode, HostId, HostSnapshot, RequestId,
    VersionRange, PROTOCOL_MAJOR,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub seq: i64,
    pub ts: i64,
    pub scope_id: String,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TypedInteraction {
    pub host_id: HostId,
    #[serde(deserialize_with = "crate::nonempty")]
    pub thread: String,
    #[serde(deserialize_with = "crate::nonnegative")]
    pub surface_seq: i64,
    #[serde(deserialize_with = "crate::surface_contract")]
    pub surface_contract: u32,
    pub node_path: Vec<u32>,
    #[serde(deserialize_with = "crate::nonempty")]
    pub event: String,
    pub args: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalVerdict {
    Approve,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalDecision {
    pub host_id: HostId,
    #[serde(deserialize_with = "crate::nonempty")]
    pub thread: String,
    pub approval_id: RequestId,
    pub decision: ApprovalVerdict,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientFrame {
    Hello {
        #[serde(deserialize_with = "crate::version::supported_versions")]
        versions: VersionRange,
    },
    Call {
        request_id: RequestId,
        #[serde(deserialize_with = "crate::nonempty")]
        method: String,
        params: Value,
    },
    Subscribe {
        request_id: RequestId,
        #[serde(deserialize_with = "crate::nonempty")]
        scope_id: String,
        cursor: Cursor,
    },
    Interaction {
        request_id: RequestId,
        interaction: TypedInteraction,
    },
    ApprovalDecision {
        request_id: RequestId,
        decision: ApprovalDecision,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    try_from = "UncheckedHostFrame"
)]
pub enum HostFrame {
    Hello {
        host_id: HostId,
        protocol_major: u16,
        versions: VersionRange,
    },
    Result {
        request_id: RequestId,
        result: Value,
    },
    Event {
        host_id: HostId,
        event: Event,
    },
    CommittedSurface {
        host_id: HostId,
        surface: CommittedSurface,
    },
    Snapshot {
        snapshot: HostSnapshot,
    },
    Acknowledgement {
        host_id: HostId,
        request_id: RequestId,
        cursor: Cursor,
    },
    Error {
        request_id: Option<RequestId>,
        error: HostError,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum UncheckedHostFrame {
    Hello {
        host_id: HostId,
        protocol_major: u16,
        #[serde(deserialize_with = "crate::version::supported_versions")]
        versions: VersionRange,
    },
    Result {
        request_id: RequestId,
        result: Value,
    },
    Event {
        host_id: HostId,
        event: Event,
    },
    CommittedSurface {
        host_id: HostId,
        surface: CommittedSurface,
    },
    Snapshot {
        snapshot: HostSnapshot,
    },
    Acknowledgement {
        host_id: HostId,
        request_id: RequestId,
        cursor: Cursor,
    },
    Error {
        request_id: Option<RequestId>,
        error: HostError,
    },
}

impl TryFrom<UncheckedHostFrame> for HostFrame {
    type Error = HostError;

    fn try_from(value: UncheckedHostFrame) -> Result<Self, HostError> {
        Ok(match value {
            UncheckedHostFrame::Hello {
                host_id,
                protocol_major,
                versions,
            } => {
                if protocol_major != PROTOCOL_MAJOR {
                    return Err(HostError::new(
                        HostErrorCode::IncompatibleVersion,
                        "unsupported protocol major",
                    ));
                }
                Self::Hello {
                    host_id,
                    protocol_major,
                    versions,
                }
            }
            UncheckedHostFrame::Result { request_id, result } => {
                Self::Result { request_id, result }
            }
            UncheckedHostFrame::Event { host_id, event } => {
                if event.seq < 0 {
                    return Err(HostError::new(
                        HostErrorCode::InvalidCursor,
                        "event sequence must be nonnegative",
                    ));
                }
                Self::Event { host_id, event }
            }
            UncheckedHostFrame::CommittedSurface { host_id, surface } => {
                Self::CommittedSurface { host_id, surface }
            }
            UncheckedHostFrame::Snapshot { snapshot } => Self::Snapshot { snapshot },
            UncheckedHostFrame::Acknowledgement {
                host_id,
                request_id,
                cursor,
            } => {
                if host_id != cursor.host_id {
                    return Err(HostError::new(
                        HostErrorCode::InvalidCursor,
                        "acknowledgement cursor belongs to another host",
                    ));
                }
                Self::Acknowledgement {
                    host_id,
                    request_id,
                    cursor,
                }
            }
            UncheckedHostFrame::Error { request_id, error } => Self::Error { request_id, error },
        })
    }
}
