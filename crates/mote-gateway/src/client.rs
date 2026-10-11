use crate::{ClientCredential, GatewayConfig, NoiseSession};
use anyhow::Result;
use mote_protocol::{ClientFrame, HostFrame};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
};

pub struct GatewayClient {
    stream: TcpStream,
    session: NoiseSession,
}

pub(crate) fn write_packet(stream: &mut TcpStream, bytes: &[u8]) -> Result<()> {
    stream.write_all(&(u32::try_from(bytes.len())?).to_be_bytes())?;
    stream.write_all(bytes)?;
    Ok(())
}

pub(crate) fn read_packet(stream: &mut TcpStream, max: usize) -> Result<Vec<u8>> {
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    anyhow::ensure!(size > 0 && size <= max, "invalid packet length");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    Ok(bytes)
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
        let mut stream = TcpStream::connect_timeout(&address, config.handshake_timeout)?;
        stream.set_read_timeout(Some(config.confirm_timeout + config.handshake_timeout))?;
        stream.set_write_timeout(Some(config.handshake_timeout))?;
        stream.write_all(&[u8::from(pairing)])?;
        write_packet(&mut stream, first)?;
        let second = read_packet(&mut stream, 4096)?;
        let session = handshake.finish(&second)?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
        Ok(Self { stream, session })
    }

    pub fn host_id(&self) -> &mote_protocol::HostId {
        self.session.host_id()
    }
    pub fn send(&mut self, frame: &ClientFrame) -> Result<()> {
        for bytes in self.session.write_client(frame)? {
            write_packet(&mut self.stream, &bytes)?;
        }
        Ok(())
    }
    pub fn receive(&mut self) -> Result<HostFrame> {
        loop {
            let bytes = read_packet(&mut self.stream, 65_535)?;
            if let Some(frame) = self.session.read_host(&bytes)? {
                return Ok(frame);
            }
        }
    }
    pub fn exchange(&mut self, frame: &ClientFrame) -> Result<HostFrame> {
        self.send(frame)?;
        self.receive()
    }
}
