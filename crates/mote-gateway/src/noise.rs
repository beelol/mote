use anyhow::{anyhow, ensure, Result};
use mote_protocol::{ClientFrame, HostFrame, HostId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snow::{Builder, HandshakeState, TransportState};

pub const PATTERN_SESSION: &str = "Noise_IK_25519_ChaChaPoly_SHA256";
pub const PATTERN_PAIRING: &str = "Noise_IKpsk1_25519_ChaChaPoly_SHA256";
pub const CHUNK: usize = 65_000;

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(text: &str) -> Result<Vec<u8>> {
    ensure!(text.len() % 2 == 0 && text.is_ascii(), "invalid hex");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).map_err(|_| anyhow!("invalid hex")))
        .collect()
}

pub fn psk_from_secret(secret: &[u8]) -> [u8; 32] {
    Sha256::digest(secret).into()
}

pub(crate) fn public_of(private: &[u8; 32]) -> Result<[u8; 32]> {
    use snow::resolvers::{CryptoResolver, DefaultResolver};
    let mut dh = DefaultResolver
        .resolve_dh(&snow::params::DHChoice::Curve25519)
        .ok_or_else(|| anyhow!("missing curve"))?;
    dh.set(private);
    Ok(dh.pubkey().try_into()?)
}

pub(crate) fn handshake(
    prologue: &[u8],
    private: &[u8; 32],
    remote: Option<&[u8; 32]>,
    psk: Option<&[u8; 32]>,
) -> Result<HandshakeState> {
    let mut builder = Builder::new(
        if psk.is_some() {
            PATTERN_PAIRING
        } else {
            PATTERN_SESSION
        }
        .parse()?,
    )
    .prologue(prologue)?
    .local_private_key(private)?;
    if let Some(psk) = psk {
        builder = builder.psk(1, psk)?;
    }
    Ok(if let Some(remote) = remote {
        builder.remote_public_key(remote)?.build_initiator()?
    } else {
        builder.build_responder()?
    })
}

pub fn seal(transport: &mut TransportState, message: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut frames = Vec::new();
    let mut chunks = message.chunks(CHUNK).peekable();
    if chunks.peek().is_none() {
        let mut out = vec![0; 17];
        let n = transport.write_message(&[1], &mut out)?;
        out.truncate(n);
        return Ok(vec![out]);
    }
    while let Some(chunk) = chunks.next() {
        let mut plain = Vec::with_capacity(chunk.len() + 1);
        plain.push(if chunks.peek().is_some() { 0 } else { 1 });
        plain.extend_from_slice(chunk);
        let mut out = vec![0; plain.len() + 16];
        let n = transport.write_message(&plain, &mut out)?;
        out.truncate(n);
        frames.push(out);
    }
    Ok(frames)
}

pub struct Opener {
    pending: Vec<u8>,
    limit: usize,
    failed: bool,
}

impl Opener {
    pub fn new(limit: usize) -> Self {
        Self {
            pending: Vec::new(),
            limit,
            failed: false,
        }
    }

    pub fn open(
        &mut self,
        transport: &mut TransportState,
        frame: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        ensure!(!self.failed, "closed reader");
        let result = self.open_inner(transport, frame);
        if result.is_err() {
            self.failed = true;
            self.pending.clear();
        }
        result
    }

    fn open_inner(
        &mut self,
        transport: &mut TransportState,
        frame: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        ensure!((17..=65_535).contains(&frame.len()), "bad frame length");
        let mut plain = vec![0; frame.len()];
        let n = transport.read_message(frame, &mut plain)?;
        plain.truncate(n);
        let (flag, chunk) = plain.split_first().ok_or_else(|| anyhow!("empty frame"))?;
        ensure!(*flag <= 1, "invalid chunk flag");
        ensure!(
            chunk.len() <= CHUNK && self.pending.len() + chunk.len() <= self.limit,
            "message too large"
        );
        self.pending.extend_from_slice(chunk);
        Ok(if *flag == 1 {
            Some(std::mem::take(&mut self.pending))
        } else {
            None
        })
    }
}

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
