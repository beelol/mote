use anyhow::{ensure, Result};
use mote_protocol::{ClientFrame, HostFrame, HostId};
use serde::{Deserialize, Serialize};
use snow::{HandshakeState, TransportState};

// Handshake patterns, chunked framing and key helpers come from `agent-link-core`, which
// keeps the same byte format; the Mote and Overseer vectors in `tests/vectors` prove it.
pub(crate) use agent_link_core::noise::{handshake, public_of};
pub use agent_link_core::noise::{
    hex, psk_from_secret, seal, unhex, Opener, CHUNK, PATTERN_PAIRING, PATTERN_SESSION,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundFrame<T> {
    host_id: HostId,
    client_id: String,
    protocol_major: u16,
    frame: T,
}

pub struct NoiseSession {
    transport: TransportState,
    opener: Opener,
    host_id: HostId,
    client_id: String,
    protocol_major: u16,
    limit: usize,
    initiator: bool,
    failed: bool,
}

impl NoiseSession {
    pub(crate) fn new(
        handshake: HandshakeState,
        host_id: HostId,
        client_id: String,
        config: &super::GatewayConfig,
        initiator: bool,
    ) -> Result<Self> {
        Ok(Self {
            transport: handshake.into_transport_mode()?,
            opener: Opener::new(config.max_message),
            host_id,
            client_id,
            protocol_major: config.protocol_major,
            limit: config.max_message,
            initiator,
            failed: false,
        })
    }

    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    fn write<T: Serialize>(&mut self, frame: &T) -> Result<Vec<Vec<u8>>> {
        ensure!(!self.failed, "closed session");
        let result = (|| {
            let bytes = serde_json::to_vec(&BoundFrame {
                host_id: self.host_id.clone(),
                client_id: self.client_id.clone(),
                protocol_major: self.protocol_major,
                frame,
            })?;
            ensure!(bytes.len() <= self.limit, "message too large");
            seal(&mut self.transport, &bytes)
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn read<T: serde::de::DeserializeOwned>(&mut self, frame: &[u8]) -> Result<Option<T>> {
        ensure!(!self.failed, "closed session");
        let result = (|| {
            let Some(bytes) = self.opener.open(&mut self.transport, frame)? else {
                return Ok(None);
            };
            let bound: BoundFrame<T> = serde_json::from_slice(&bytes)?;
            ensure!(
                bound.host_id == self.host_id
                    && bound.client_id == self.client_id
                    && bound.protocol_major == self.protocol_major,
                "frame identity changed"
            );
            Ok(Some(bound.frame))
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub fn write_client(&mut self, frame: &ClientFrame) -> Result<Vec<Vec<u8>>> {
        self.check_direction(true)?;
        self.check_client(frame)?;
        self.write(frame)
    }

    pub fn read_client(&mut self, frame: &[u8]) -> Result<Option<ClientFrame>> {
        self.check_direction(false)?;
        let result = self.read(frame)?;
        if let Some(frame) = &result {
            self.check_client(frame)?;
        }
        Ok(result)
    }

    pub fn write_host(&mut self, frame: &HostFrame) -> Result<Vec<Vec<u8>>> {
        self.check_direction(false)?;
        self.check_host(frame)?;
        self.write(frame)
    }

    pub(crate) fn host_frame_fits(&self, frame: &HostFrame) -> Result<bool> {
        Ok(serde_json::to_vec(&BoundFrame {
            host_id: self.host_id.clone(),
            client_id: self.client_id.clone(),
            protocol_major: self.protocol_major,
            frame,
        })?
        .len()
            <= self.limit)
    }

    pub fn read_host(&mut self, frame: &[u8]) -> Result<Option<HostFrame>> {
        self.check_direction(true)?;
        let result = self.read(frame)?;
        if let Some(frame) = &result {
            self.check_host(frame)?;
        }
        Ok(result)
    }

    fn check_client(&mut self, frame: &ClientFrame) -> Result<()> {
        let id = match frame {
            ClientFrame::Subscribe { cursor, .. } => Some(&cursor.host_id),
            ClientFrame::Interaction { interaction, .. } => Some(&interaction.host_id),
            ClientFrame::ApprovalDecision { decision, .. } => Some(&decision.host_id),
            _ => None,
        };
        self.check_id(id)
    }

    fn check_host(&mut self, frame: &HostFrame) -> Result<()> {
        let id = match frame {
            HostFrame::Hello {
                host_id,
                protocol_major,
                ..
            } => {
                if *protocol_major != self.protocol_major {
                    self.failed = true;
                    anyhow::bail!("protocol changed");
                }
                Some(host_id)
            }
            HostFrame::Event { host_id, .. }
            | HostFrame::CommittedSurface { host_id, .. }
            | HostFrame::Acknowledgement { host_id, .. } => Some(host_id),
            HostFrame::Snapshot { snapshot } => Some(&snapshot.host_id),
            HostFrame::SnapshotChunk { chunk } => Some(&chunk.cursor.host_id),
            _ => None,
        };
        self.check_id(id)
    }

    fn check_id(&mut self, id: Option<&HostId>) -> Result<()> {
        if id.is_some_and(|id| *id != self.host_id) {
            self.failed = true;
            anyhow::bail!("frame belongs to another host");
        }
        Ok(())
    }

    fn check_direction(&mut self, initiator: bool) -> Result<()> {
        ensure!(!self.failed, "closed session");
        if self.initiator != initiator {
            self.failed = true;
            anyhow::bail!("wrong direction");
        }
        Ok(())
    }
}
