//! Encrypted loopback acceptance fixtures. The caller supplies the real daemon dispatcher.

use anyhow::{anyhow, ensure, Result};
use mote_eventlog::{Event, EventLog};
use mote_gateway::{ClientCredential, GatewayClient, GatewayConfig, GatewayServer};
use mote_ipc::Dispatch;
use mote_protocol::{ClientFrame, HostFrame, HostId};
use serde_json::json;
use std::{
    net::SocketAddr,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

/// Owns temporary daemon storage and a real Noise gateway on an ephemeral loopback port.
/// The event log returned by the factory must be the daemon's own log so broadcasts stay live.
pub struct LoopbackHost<D: Dispatch> {
    server: Option<GatewayServer>,
    daemon: Arc<D>,
    log: EventLog,
    config: GatewayConfig,
    host_id: HostId,
    public_key: [u8; 32],
    root: tempfile::TempDir,
}

impl<D: Dispatch> LoopbackHost<D> {
    pub fn start(factory: impl FnOnce(&Path) -> Result<(Arc<D>, EventLog)>) -> Result<Self> {
        let root = tempfile::Builder::new()
            .prefix("mote-loopback-")
            .tempdir()?;
        let data = root.path().join("data");
        mote_gateway::HostIdentity::load_or_create(&data)?;
        let (daemon, log) = factory(&data)?;
        let mut config = GatewayConfig::new(data);
        config.bind = "127.0.0.1:0".parse()?;
        config.confirm_timeout = Duration::from_secs(3);
        config.handshake_timeout = Duration::from_secs(3);
        let server = GatewayServer::bind_loopback(config.clone(), daemon.clone())?;
        let host_id = server.host_id().clone();
        let public_key = *server.public_key();
        Ok(Self {
            server: Some(server),
            daemon,
            log,
            config,
            host_id,
            public_key,
            root,
        })
    }

    pub fn daemon(&self) -> &D {
        &self.daemon
    }
    pub fn root(&self) -> &Path {
        self.root.path()
    }
    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }
    pub fn public_key(&self) -> &[u8; 32] {
        &self.public_key
    }
    pub fn endpoint(&self) -> Result<SocketAddr> {
        Ok(self.server()?.local_addr())
    }

    fn server(&self) -> Result<&GatewayServer> {
        self.server
            .as_ref()
            .ok_or_else(|| anyhow!("fixture gateway is suspended"))
    }

    /// Returns the real, single-use MOTE1 offer for an external Simulator or fixture client.
    pub fn open_pairing(&self) -> Result<String> {
        self.server()?.open_pairing()
    }

    /// Completes owner confirmation without replacing the gateway's authentication exchange.
    pub fn confirm_pending(&self, approved: bool) -> Result<usize> {
        let server = self.server()?;
        let pending = server.pending_devices();
        for device in &pending {
            server.confirm_pairing(&device.client_id, approved)?;
        }
        Ok(pending.len())
    }

    pub fn pair(&self, offer: &str, name: &str) -> Result<PairedClient> {
        let credential = ClientCredential::from_pairing_code(&self.config, offer)?;
        let server = self.server()?;
        let connection = std::thread::scope(|scope| -> Result<GatewayClient> {
            let attempt = scope.spawn(|| {
                GatewayClient::pair(server.local_addr(), &self.config, &credential, name, 1)
            });
            let deadline = Instant::now() + self.config.confirm_timeout;
            while !attempt.is_finished() {
                if self.confirm_pending(true)? > 0 {
                    break;
                }
                ensure!(Instant::now() < deadline, "pairing confirmation timed out");
                std::thread::sleep(Duration::from_millis(5));
            }
            attempt
                .join()
                .map_err(|_| anyhow!("pairing worker panicked"))?
        })?;
        Ok(PairedClient {
            connection: Some(connection),
            credential,
            counter: 1,
            host_id: self.host_id.clone(),
        })
    }

    pub fn reconnect(&self, client: &mut PairedClient) -> Result<()> {
        ensure!(
            client.host_id == self.host_id,
            "client belongs to another fixture host"
        );
        client.force_disconnect();
        client.counter = client
            .counter
            .checked_add(1)
            .ok_or_else(|| anyhow!("session counter exhausted"))?;
        client.connection = Some(GatewayClient::connect(
            self.endpoint()?,
            &self.config,
            &client.credential,
            client.counter,
        )?);
        Ok(())
    }

    /// Stops the gateway. This simulates transport suspension, not operating-system sleep.
    pub fn suspend(&mut self) {
        self.server.take();
    }

    pub fn resume(&mut self) -> Result<()> {
        ensure!(self.server.is_none(), "fixture gateway is already running");
        self.server = Some(GatewayServer::bind_loopback(
            self.config.clone(),
            self.daemon.clone(),
        )?);
        Ok(())
    }

    /// Binds the replacement while the old endpoint is held so its port must differ.
    pub fn rotate_endpoint(&mut self) -> Result<()> {
        self.server()?;
        let replacement = GatewayServer::bind_loopback(self.config.clone(), self.daemon.clone())?;
        self.server = Some(replacement);
        Ok(())
    }

    pub fn fill_retention(&self, scope: &str, count: usize) -> Result<()> {
        for _ in 0..count {
            self.log.append(scope, "heartbeat", &json!({}))?;
        }
        Ok(())
    }

    /// Reads retained events across every scope in sequence order.
    pub fn events_after(&self, cursor: i64) -> Result<Vec<Event>> {
        ensure!(cursor >= 0, "negative inspection cursor");
        let mut events = Vec::new();
        for seq in cursor.saturating_add(1)..=self.log.max_seq()? {
            if let Some(event) = self.log.get(seq)? {
                events.push(event);
            }
        }
        Ok(events)
    }
}

/// A real paired credential with a monotonic reconnect counter and an optional live transport.
pub struct PairedClient {
    connection: Option<GatewayClient>,
    credential: ClientCredential,
    counter: i64,
    host_id: HostId,
}

impl PairedClient {
    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }
    pub fn force_disconnect(&mut self) {
        self.connection.take();
    }
    fn connection(&mut self) -> Result<&mut GatewayClient> {
        self.connection
            .as_mut()
            .ok_or_else(|| anyhow!("fixture client is disconnected"))
    }
    pub fn send(&mut self, frame: &ClientFrame) -> Result<()> {
        self.connection()?.send(frame)
    }
    pub fn receive(&mut self) -> Result<HostFrame> {
        self.connection()?.receive()
    }
    pub fn exchange(&mut self, frame: &ClientFrame) -> Result<HostFrame> {
        self.connection()?.exchange(frame)
    }
}
