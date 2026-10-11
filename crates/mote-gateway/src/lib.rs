pub mod base32;
pub mod classes;
pub mod devices;
pub mod net;
pub mod noise;
pub mod pairing;
pub mod server;
pub mod client;
mod carrier;

pub use client::GatewayClient;
pub use server::GatewayServer;

pub use devices::DeviceStore;
pub use noise::NoiseSession;
pub use pairing::{
    ClientCredential, ClientHandshake, HostIdentity, PairingOffer, PendingHandshake,
};

use std::{net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Clone, Debug)]
pub struct GatewayConfig {
    pub data_dir: PathBuf,
    pub bind: SocketAddr,
    pub prologue: Vec<u8>,
    pub pairing_prefix: String,
    pub protocol_major: u16,
    pub pairing_window: Duration,
    pub confirm_timeout: Duration,
    pub handshake_timeout: Duration,
    pub max_message: usize,
}

impl GatewayConfig {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            bind: "127.0.0.1:47810".parse().expect("literal socket address"),
            prologue: b"mote-gateway-v1".to_vec(),
            pairing_prefix: "MOTE1".into(),
            protocol_major: mote_protocol::PROTOCOL_MAJOR,
            pairing_window: Duration::from_secs(120),
            confirm_timeout: Duration::from_secs(60),
            handshake_timeout: Duration::from_secs(10),
            max_message: 1 << 20,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.protocol_major == mote_protocol::PROTOCOL_MAJOR,
            "unsupported protocol"
        );
        anyhow::ensure!(
            !self.prologue.is_empty() && !self.pairing_prefix.is_empty(),
            "empty security domain"
        );
        anyhow::ensure!(
            !self.pairing_window.is_zero()
                && !self.confirm_timeout.is_zero()
                && !self.handshake_timeout.is_zero(),
            "zero timeout"
        );
        anyhow::ensure!(
            self.max_message > 0 && self.max_message <= 16 << 20,
            "invalid message limit"
        );
        Ok(())
    }
}
