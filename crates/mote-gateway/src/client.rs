use crate::carrier::Socket;
use crate::{ClientCredential, GatewayConfig, NoiseSession};
use anyhow::Result;
use mote_protocol::{ClientFrame, HostFrame};
use std::net::SocketAddr;

pub struct GatewayClient {
    stream: Socket,
    session: NoiseSession,
    snapshot: mote_protocol::SnapshotAssembler,
}

impl GatewayClient {
    pub fn pair(
        address: SocketAddr,
        config: &GatewayConfig,
        credential: &ClientCredential,
        name: &str,
        counter: i64,
    ) -> Result<Self> {
        let (handshake, first) = credential.begin_pairing(config, name, counter)?;
        Self::handshake(address, config, handshake, &first, true)
    }

    pub fn connect(
        address: SocketAddr,
        config: &GatewayConfig,
        credential: &ClientCredential,
        counter: i64,
    ) -> Result<Self> {
        let (handshake, first) = credential.begin_session(config, counter)?;
        Self::handshake(address, config, handshake, &first, false)
    }

    fn handshake(
        address: SocketAddr,
        config: &GatewayConfig,
        handshake: crate::ClientHandshake,
        first: &[u8],
        pairing: bool,
    ) -> Result<Self> {
        let mut stream = Socket::connect(address, pairing, config.handshake_timeout)?;
        stream.write(first)?;
        let second = stream.read(
            4096,
            Some(config.confirm_timeout + config.handshake_timeout),
        )?;
        let session = handshake.finish(&second)?;
        Ok(Self {
            stream,
            session,
            snapshot: Default::default(),
        })
    }

    pub fn host_id(&self) -> &mote_protocol::HostId {
        self.session.host_id()
    }
    pub fn send(&mut self, frame: &ClientFrame) -> Result<()> {
        for bytes in self.session.write_client(frame)? {
            self.stream.write(&bytes)?;
        }
        Ok(())
    }
    pub fn receive(&mut self) -> Result<HostFrame> {
        loop {
            let bytes = self.stream.read(65_535, None)?;
            if let Some(frame) = self.session.read_host(&bytes)? {
                if let Some(frame) = self.snapshot.accept(self.session.host_id(), frame)? {
                    return Ok(frame);
                }
            }
        }
    }
    /// Waits for an authoritative reply without an application deadline.
    /// RequestState replies are nonterminal. A transport error does not prove action failure.
    pub fn exchange(&mut self, frame: &ClientFrame) -> Result<HostFrame> {
        self.send(frame)?;
        let outcome = self.receive()?;
        if let HostFrame::Result { request_id, .. }
        | HostFrame::Error {
            request_id: Some(request_id),
            ..
        } = &outcome
        {
            let acknowledgement = self.receive()?;
            anyhow::ensure!(
                matches!(acknowledgement, HostFrame::Acknowledgement { host_id, request_id: id, .. } if host_id == *self.host_id() && id == *request_id),
                "missing matching acknowledgement"
            );
        }
        Ok(outcome)
    }
}
