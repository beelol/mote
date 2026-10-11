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

impl crate::HostDirectory {
    pub fn receive_frame(&mut self, host: &HostId, frame: HostFrame) -> Result<(), HostError> {
        use mote_protocol::HostErrorCode;
        self.state(host)?;
        let frame = match self.snapshot_frame(host, frame) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Ok(()),
            Err(error) => {
                if error.code == HostErrorCode::IdentityChanged {
                    self.set_connection(host, ConnectionState::IdentityChanged)?;
                }
                return Err(error);
            }
        };
        let result = match frame {
            HostFrame::SnapshotChunk { .. } => unreachable!("snapshot assembler consumes chunks"),
            HostFrame::Hello {
                host_id,
                protocol_major,
                versions,
            } => {
                if host_id != *host {
                    Err(HostError::new(
                        HostErrorCode::IdentityChanged,
                        "host identity changed",
                    ))
                } else if protocol_major != mote_protocol::PROTOCOL_MAJOR
                    || VersionRange::current().negotiate(versions).is_err()
                {
                    self.set_connection(
                        host,
                        ConnectionState::Incompatible {
                            supported: VersionRange::current(),
                            offered: versions,
                        },
                    )?;
                    return Err(HostError::new(
                        HostErrorCode::IncompatibleVersion,
                        "host version incompatible",
                    ));
                } else {
                    self.set_connection(host, ConnectionState::Online)
                }
            }
            HostFrame::Snapshot { snapshot } => self
                .install_snapshot(host, snapshot)
                .and_then(|_| self.set_connection(host, ConnectionState::Online)),
            HostFrame::Event { host_id, event } if event.kind == "resync" => {
                crate::require_host(host, &host_id)
                    .and_then(|_| self.set_connection(host, ConnectionState::Connecting))
            }
            HostFrame::Event { host_id, event } => {
                self.apply_event(host, &host_id, event).map(|_| ())
            }
            HostFrame::Acknowledgement {
                host_id,
                request_id,
                cursor,
            } => crate::require_host(host, &host_id)
                .and_then(|_| crate::require_host(host, &cursor.host_id))
                .and_then(|_| self.acknowledge(host, &request_id).map(|_| ())),
            HostFrame::Error { error, .. } => Err(error),
            HostFrame::CommittedSurface { .. } => Err(HostError::new(
                HostErrorCode::InvalidRequest,
                "surface requires a sequenced event or snapshot",
            )),
            HostFrame::Result { .. } | HostFrame::RequestState { .. } => Ok(()),
        };
        if let Err(error) = &result {
            match error.code {
                HostErrorCode::IdentityChanged => {
                    self.set_connection(host, ConnectionState::IdentityChanged)?
                }
                HostErrorCode::IncompatibleVersion => self.set_connection(
                    host,
                    ConnectionState::Incompatible {
                        supported: error.supported.unwrap_or(VersionRange::current()),
                        offered: error.offered.unwrap_or(VersionRange::current()),
                    },
                )?,
                _ => {}
            }
        }
        result
    }
}
