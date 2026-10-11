use mote_protocol::{ClientFrame, Cursor, HostError, HostFrame, HostId, VersionRange};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConnectionState {
    Connecting,
    Online,
    #[default]
    Offline,
    Incompatible {
        supported: VersionRange,
        offered: VersionRange,
    },
    IdentityChanged,
}

pub trait HostEventStream {
    fn next(&mut self, host_id: &HostId) -> Result<Option<HostFrame>, HostError>;
}

pub trait HostTransport {
    type EventStream: HostEventStream;

    fn send(&mut self, host_id: &HostId, frame: &ClientFrame) -> Result<(), HostError>;

    fn subscribe(
        &mut self,
        host_id: &HostId,
        cursor: &Cursor,
    ) -> Result<Self::EventStream, HostError>;
}
