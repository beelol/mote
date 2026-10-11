use crate::{base32, noise, DeviceStore, GatewayConfig, NoiseSession};
use anyhow::{anyhow, ensure, Result};
use mote_protocol::HostId;
use serde::{Deserialize, Serialize};
use snow::HandshakeState;
use std::{
    fs::{self, DirBuilder, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::Path,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub(crate) fn secure_directory(path: &Path) -> Result<()> {
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_dir()
            && !meta.file_type().is_symlink()
            && meta.mode() & 0o777 == 0o700
            && meta.uid() == unsafe { libc::geteuid() },
        "unsafe identity directory"
    );
    Ok(())
}

pub(crate) fn validate_private_file(file: &File) -> Result<()> {
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.mode() & 0o777 == 0o600 && meta.uid() == unsafe { libc::geteuid() },
        "unsafe private file"
    );
    Ok(())
}

pub struct HostIdentity {
    private: Zeroizing<[u8; 32]>,
    public: [u8; 32],
    host_id: HostId,
}

impl HostIdentity {
    pub fn load_or_create(data_dir: &Path) -> Result<Self> {
        secure_directory(data_dir)?;
        let path = data_dir.join("host.key");
        if fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
            let mut secret = Zeroizing::new([0; 32]);
            getrandom::fill(secret.as_mut()).map_err(|e| anyhow!("random source failed: {e}"))?;
            let mut suffix = [0; 16];
            getrandom::fill(&mut suffix).map_err(|e| anyhow!("random source failed: {e}"))?;
            let temporary = data_dir.join(format!(".host-key-{}", noise::hex(&suffix)));
            let creation = (|| -> Result<()> {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&temporary)?;
                file.write_all(secret.as_ref())?;
                file.sync_all()?;
                match fs::hard_link(&temporary, &path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
                File::open(data_dir)?.sync_all()?;
                Ok(())
            })();
            if temporary.exists() {
                fs::remove_file(&temporary)?;
            }
            creation?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)?;
        validate_private_file(&file)?;
        ensure!(file.metadata()?.len() == 32, "invalid host key length");
        let mut private = Zeroizing::new([0; 32]);
        file.read_exact(private.as_mut())?;
        let public = noise::public_of(&private)?;
        let host_id = HostId::from_public_key(&public);
        Ok(Self {
            private,
            public,
            host_id,
        })
    }

    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }
    pub fn public_key(&self) -> &[u8; 32] {
        &self.public
    }
}

pub struct PairingOffer {
    secret: Zeroizing<[u8; 16]>,
    host_public: [u8; 32],
    prefix: String,
    port: u16,
    addresses: Vec<String>,
    opened: Instant,
    window: Duration,
    used: bool,
    failures: u32,
}

impl PairingOffer {
    pub fn open(
        config: &GatewayConfig,
        host: &HostIdentity,
        now: Instant,
        addresses: &[String],
    ) -> Result<Self> {
        config.validate()?;
        ensure!(
            addresses.len() <= 8
                && addresses
                    .iter()
                    .all(|a| !a.is_empty() && a.len() <= 255 && a.is_ascii()),
            "invalid pairing addresses"
        );
        let mut secret = Zeroizing::new([0; 16]);
        getrandom::fill(secret.as_mut()).map_err(|e| anyhow!("random source failed: {e}"))?;
        Ok(Self {
            secret,
            host_public: host.public,
            prefix: config.pairing_prefix.clone(),
            port: config.bind.port(),
            addresses: addresses.to_vec(),
            opened: now,
            window: config.pairing_window,
            used: false,
            failures: 0,
        })
    }

    pub fn code(&self) -> String {
        let mut bytes = Zeroizing::new(Vec::new());
        bytes.push(1);
        bytes.extend_from_slice(&self.host_public);
        bytes.extend_from_slice(self.secret.as_ref());
        bytes.extend_from_slice(&self.port.to_be_bytes());
        bytes.push(self.addresses.len() as u8);
        for address in &self.addresses {
            bytes.push(address.len() as u8);
            bytes.extend_from_slice(address.as_bytes());
        }
        format!("{}-{}", self.prefix, base32::encode(&bytes))
    }

    pub fn accepts_attempts(&self, now: Instant) -> bool {
        !self.used
            && self.failures < 5
            && now
                .checked_duration_since(self.opened)
                .is_some_and(|elapsed| elapsed < self.window)
    }
}

pub struct ClientCredential {
    private: Zeroizing<[u8; 32]>,
    public: [u8; 32],
    host_public: [u8; 32],
    host_id: HostId,
    pairing_secret: Option<Zeroizing<[u8; 16]>>,
}

impl ClientCredential {
    pub fn from_pairing_code(config: &GatewayConfig, code: &str) -> Result<Self> {
        config.validate()?;
        ensure!(code.len() <= 4096, "pairing code too large");
        let encoded = code
            .strip_prefix(&format!("{}-", config.pairing_prefix))
            .ok_or_else(|| anyhow!("invalid pairing prefix"))?;
        let bytes =
            Zeroizing::new(base32::decode(encoded).ok_or_else(|| anyhow!("invalid base32"))?);
        ensure!(
            bytes.len() >= 52 && bytes[0] == 1 && bytes[51] <= 8,
            "invalid pairing code"
        );
        let mut offset = 52;
        for _ in 0..bytes[51] {
            let len = *bytes
                .get(offset)
                .ok_or_else(|| anyhow!("missing address"))? as usize;
            offset += 1;
            let address = bytes
                .get(offset..offset + len)
                .ok_or_else(|| anyhow!("truncated address"))?;
            ensure!(len > 0 && address.is_ascii(), "invalid address");
            offset += len;
        }
        ensure!(offset == bytes.len(), "trailing pairing bytes");
        let host_public = bytes[1..33].try_into()?;
        let mut private = Zeroizing::new([0; 32]);
        getrandom::fill(private.as_mut()).map_err(|e| anyhow!("random source failed: {e}"))?;
        let public = noise::public_of(&private)?;
        Ok(Self {
            private,
            public,
            host_public,
            host_id: HostId::from_public_key(&host_public),
            pairing_secret: Some(Zeroizing::new(bytes[33..49].try_into()?)),
        })
    }

    pub fn restore(host_public: [u8; 32], private: [u8; 32]) -> Result<Self> {
        let private = Zeroizing::new(private);
        let public = noise::public_of(&private)?;
        Ok(Self {
            private,
            public,
            host_public,
            host_id: HostId::from_public_key(&host_public),
            pairing_secret: None,
        })
    }

    pub fn private_key_for_secure_storage(&self) -> &[u8; 32] {
        &self.private
    }
    pub fn host_public_key(&self) -> &[u8; 32] {
        &self.host_public
    }
    pub fn public_key(&self) -> &[u8; 32] {
        &self.public
    }
    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }

    pub fn begin_pairing(
        &self,
        config: &GatewayConfig,
        name: &str,
        counter: i64,
    ) -> Result<(ClientHandshake, Vec<u8>)> {
        let secret = self
            .pairing_secret
            .as_ref()
            .ok_or_else(|| anyhow!("no pairing secret"))?;
        self.begin(
            config,
            name,
            counter,
            Some(&noise::psk_from_secret(secret.as_ref())),
        )
    }

    pub fn begin_session(
        &self,
        config: &GatewayConfig,
        counter: i64,
    ) -> Result<(ClientHandshake, Vec<u8>)> {
        self.begin(config, "paired", counter, None)
    }

    fn begin(
        &self,
        config: &GatewayConfig,
        name: &str,
        counter: i64,
        psk: Option<&[u8; 32]>,
    ) -> Result<(ClientHandshake, Vec<u8>)> {
        config.validate()?;
        ensure!(
            counter > 0 && !name.is_empty() && name.len() <= 256,
            "invalid client hello"
        );
        let client_id = noise::hex(&sha2::Sha256::digest(self.public));
        let hello = Hello {
            host_id: self.host_id.clone(),
            client_id: client_id.clone(),
            protocol_major: config.protocol_major,
            counter,
            name: name.to_string(),
        };
        let mut state = noise::handshake(
            &config.prologue,
            &self.private,
            Some(&self.host_public),
            psk,
        )?;
        let mut first = vec![0; 4096];
        let n = state.write_message(&serde_json::to_vec(&hello)?, &mut first)?;
        first.truncate(n);
        Ok((
            ClientHandshake {
                state,
                host_id: self.host_id.clone(),
                client_id,
                config: config.clone(),
            },
            first,
        ))
    }
}

use sha2::Digest;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hello {
    host_id: HostId,
    client_id: String,
    protocol_major: u16,
    counter: i64,
    name: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Welcome {
    host_id: HostId,
    client_id: String,
    protocol_major: u16,
}

pub struct ClientHandshake {
    state: HandshakeState,
    host_id: HostId,
    client_id: String,
    config: GatewayConfig,
}

impl ClientHandshake {
    pub fn finish(mut self, second: &[u8]) -> Result<NoiseSession> {
        ensure!(second.len() <= 4096, "handshake too large");
        let mut plain = [0; 4096];
        let n = self.state.read_message(second, &mut plain)?;
        let welcome: Welcome = serde_json::from_slice(&plain[..n])?;
        ensure!(
            welcome.host_id == self.host_id
                && welcome.client_id == self.client_id
                && welcome.protocol_major == self.config.protocol_major,
            "host identity or protocol changed"
        );
        NoiseSession::new(self.state, self.host_id, self.client_id, &self.config, true)
    }
}

pub struct PendingHandshake {
    state: HandshakeState,
    hello: Hello,
    public: [u8; 32],
    config: GatewayConfig,
    received: Instant,
}

impl PendingHandshake {
    pub fn pairing(
        config: &GatewayConfig,
        host: &HostIdentity,
        offer: &mut PairingOffer,
        first: &[u8],
        now: Instant,
    ) -> Result<Self> {
        ensure!(
            offer.accepts_attempts(now)
                && offer.host_public == host.public
                && offer.prefix == config.pairing_prefix,
            "closed or wrong-host offer"
        );
        let psk = Zeroizing::new(noise::psk_from_secret(offer.secret.as_ref()));
        let result = Self::receive(config, host, Some(&psk), first, now);
        if result.is_ok() {
            offer.used = true;
        } else {
            offer.failures += 1;
        }
        result
    }

    pub fn confirm(
        self,
        store: &DeviceStore,
        approved: bool,
        now: Instant,
    ) -> Result<(NoiseSession, Vec<u8>)> {
        ensure!(
            approved
                && now
                    .checked_duration_since(self.received)
                    .is_some_and(|elapsed| elapsed < self.config.confirm_timeout),
            "owner denied or confirmation expired"
        );
        ensure!(store.host_id() == &self.hello.host_id, "wrong host store");
        store.register(&self.public, &self.hello.name, self.hello.counter)?;
        self.finish()
    }

    pub fn session(
        config: &GatewayConfig,
        host: &HostIdentity,
        store: &DeviceStore,
        first: &[u8],
    ) -> Result<(NoiseSession, Vec<u8>)> {
        ensure!(store.host_id() == host.host_id(), "wrong host store");
        let pending = Self::receive(config, host, None, first, Instant::now())?;
        store.accept_counter(&pending.public, pending.hello.counter)?;
        pending.finish()
    }

    pub fn client_id(&self) -> &str {
        &self.hello.client_id
    }
    pub fn client_name(&self) -> &str {
        &self.hello.name
    }
    pub fn client_public_key(&self) -> &[u8; 32] {
        &self.public
    }

    fn receive(
        config: &GatewayConfig,
        host: &HostIdentity,
        psk: Option<&[u8; 32]>,
        first: &[u8],
        now: Instant,
    ) -> Result<Self> {
        config.validate()?;
        ensure!(first.len() <= 4096, "handshake too large");
        let mut state = noise::handshake(&config.prologue, &host.private, None, psk)?;
        let mut plain = [0; 4096];
        let n = state.read_message(first, &mut plain)?;
        let hello: Hello = serde_json::from_slice(&plain[..n])?;
        let public: [u8; 32] = state
            .get_remote_static()
            .ok_or_else(|| anyhow!("missing client identity"))?
            .try_into()?;
        ensure!(
            hello.host_id == host.host_id
                && hello.protocol_major == config.protocol_major
                && hello.client_id == noise::hex(&sha2::Sha256::digest(public))
                && hello.counter > 0
                && !hello.name.is_empty()
                && hello.name.len() <= 256,
            "invalid authenticated hello"
        );
        Ok(Self {
            state,
            hello,
            public,
            config: config.clone(),
            received: now,
        })
    }

    fn finish(mut self) -> Result<(NoiseSession, Vec<u8>)> {
        let welcome = Welcome {
            host_id: self.hello.host_id.clone(),
            client_id: self.hello.client_id.clone(),
            protocol_major: self.config.protocol_major,
        };
        let mut second = vec![0; 4096];
        let n = self
            .state
            .write_message(&serde_json::to_vec(&welcome)?, &mut second)?;
        second.truncate(n);
        let session = NoiseSession::new(
            self.state,
            self.hello.host_id,
            self.hello.client_id,
            &self.config,
            false,
        )?;
        Ok((session, second))
    }
}
