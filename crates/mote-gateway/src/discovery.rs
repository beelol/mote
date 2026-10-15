use crate::{noise, ClientCredential, GatewayClient, GatewayConfig};
use anyhow::{anyhow, ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{ErrorKind, Read},
    net::{IpAddr, SocketAddr},
    os::fd::AsRawFd,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub const SERVICE_TYPE: &str = "_mote._tcp";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryRecord {
    pub endpoint: SocketAddr,
    pub protocol_major: u16,
    pub fingerprint: String,
}

impl DiscoveryRecord {
    pub fn new(endpoint: SocketAddr, protocol_major: u16, public_key: &[u8; 32]) -> Result<Self> {
        Self::from_txt(
            endpoint,
            &[
                format!("protocol_major={protocol_major}"),
                format!("fingerprint={}", fingerprint(public_key)),
            ],
        )
    }

    pub fn txt(&self) -> Vec<String> {
        vec![
            format!("protocol_major={}", self.protocol_major),
            format!("fingerprint={}", self.fingerprint),
        ]
    }

    pub fn from_txt(endpoint: SocketAddr, txt: &[String]) -> Result<Self> {
        ensure!(
            endpoint.port() != 0 && txt.len() == 2,
            "invalid discovery record"
        );
        let mut fields = BTreeMap::new();
        for item in txt {
            let (key, value) = item
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid TXT field"))?;
            ensure!(fields.insert(key, value).is_none(), "duplicate TXT field");
        }
        let protocol_major: u16 = fields
            .get("protocol_major")
            .ok_or_else(|| anyhow!("missing protocol"))?
            .parse()?;
        let fingerprint = fields
            .get("fingerprint")
            .ok_or_else(|| anyhow!("missing fingerprint"))?;
        ensure!(
            protocol_major > 0
                && fingerprint.len() == 64
                && fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "invalid discovery metadata"
        );
        Ok(Self {
            endpoint,
            protocol_major,
            fingerprint: (*fingerprint).into(),
        })
    }
}

fn fingerprint(public_key: &[u8; 32]) -> String {
    noise::hex(&Sha256::digest(public_key))
}

pub trait Advertisement: Send + Sync {}
pub trait Advertiser {
    fn start(&self, record: &DiscoveryRecord) -> Result<Box<dyn Advertisement>>;
}
pub trait DiscoveryBrowser {
    fn browse(&self, timeout: Duration) -> Result<Vec<DiscoveryRecord>>;
}

pub fn advertise(
    enabled: bool,
    advertiser: &dyn Advertiser,
    record: &DiscoveryRecord,
) -> Result<Option<Box<dyn Advertisement>>> {
    if enabled {
        Ok(Some(advertiser.start(record)?))
    } else {
        Ok(None)
    }
}

pub fn discover(
    enabled: bool,
    browser: &dyn DiscoveryBrowser,
    timeout: Duration,
) -> Result<Vec<DiscoveryRecord>> {
    if enabled {
        browser.browse(timeout)
    } else {
        Ok(Vec::new())
    }
}

pub struct EndpointCandidate {
    record: DiscoveryRecord,
    pinned_public_key: [u8; 32],
}

impl EndpointCandidate {
    pub fn new(record: DiscoveryRecord, credential: &ClientCredential) -> Result<Self> {
        DiscoveryRecord::from_txt(record.endpoint, &record.txt())?;
        ensure!(
            record.protocol_major == mote_protocol::PROTOCOL_MAJOR,
            "incompatible discovery protocol"
        );
        ensure!(
            record.fingerprint == fingerprint(credential.host_public_key()),
            "unknown discovery identity"
        );
        Ok(Self {
            record,
            pinned_public_key: *credential.host_public_key(),
        })
    }

    pub fn connect(
        &self,
        config: &GatewayConfig,
        credential: &ClientCredential,
        counter: i64,
        saved_endpoint: &mut SocketAddr,
    ) -> Result<GatewayClient> {
        ensure!(
            credential.host_public_key() == &self.pinned_public_key,
            "candidate belongs to another host"
        );
        let client = GatewayClient::connect(self.record.endpoint, config, credential, counter)?;
        *saved_endpoint = self.record.endpoint;
        Ok(client)
    }
}

struct DnsSdChild(Child);
impl Advertisement for DnsSdChild {}
impl Drop for DnsSdChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn(args: &[String], capture: bool) -> Result<DnsSdChild> {
    Ok(DnsSdChild(
        Command::new("/usr/bin/dns-sd")
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(if capture {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .spawn()?,
    ))
}

pub struct DnsSdAdvertiser;
impl Advertiser for DnsSdAdvertiser {
    fn start(&self, record: &DiscoveryRecord) -> Result<Box<dyn Advertisement>> {
        DiscoveryRecord::from_txt(record.endpoint, &record.txt())?;
        ensure!(
            record.endpoint.ip().is_unspecified()
                || crate::net::is_local_range(record.endpoint.ip()),
            "discovery requires a local endpoint"
        );
        let name = format!("mote-{}", &record.fingerprint[..24]);
        let mut args = vec![
            "-R".into(),
            name.clone(),
            SERVICE_TYPE.into(),
            "local".into(),
            record.endpoint.port().to_string(),
        ];
        args.extend(record.txt());
        let mut child = spawn(&args, false)?;
        std::thread::sleep(Duration::from_millis(50));
        ensure!(child.0.try_wait()?.is_none(), "Bonjour registration failed");
        Ok(Box::new(child))
    }
}

fn capture(args: &[String], timeout: Duration) -> Result<String> {
    let mut child = spawn(args, true)?;
    let mut stdout = child
        .0
        .stdout
        .take()
        .ok_or_else(|| anyhow!("missing Bonjour output"))?;
    let descriptor = stdout.as_raw_fd();
    let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
    ensure!(
        flags >= 0
            && unsafe { libc::fcntl(descriptor, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0,
        "cannot read Bonjour output"
    );
    let deadline = Instant::now() + timeout;
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        match stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                output.extend_from_slice(&buffer[..count]);
                ensure!(output.len() <= 65_536, "Bonjour output too large");
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error.into()),
        }
        if Instant::now() >= deadline {
            break;
        }
    }
    if let Some(status) = child.0.try_wait()? {
        ensure!(status.success(), "Bonjour query failed");
    }
    Ok(String::from_utf8(output)?)
}

pub struct DnsSdBrowser;
fn parse_zone(zone: &str) -> Vec<(String, u16, Vec<String>)> {
    let mut services = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    for line in zone.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.len() < 3 || !words[0].ends_with("._mote._tcp") {
            continue;
        }
        if words[1] == "SRV"
            && words.len() >= 6
            && words[5].ends_with(".local.")
            && !words[5].starts_with('-')
        {
            if let Ok(port) = words[4].parse::<u16>() {
                services.insert(words[0], (words[5], port));
            }
        } else if words[1] == "TXT" {
            let txt: Option<Vec<_>> = words[2..]
                .iter()
                .map(|word| Some(word.strip_prefix('"')?.strip_suffix('"')?.to_owned()))
                .collect();
            metadata.insert(words[0], txt);
        }
    }
    services
        .into_iter()
        .filter_map(|(name, (hostname, port))| {
            let txt = metadata.get(name)?.as_ref()?;
            DiscoveryRecord::from_txt(SocketAddr::from(([127, 0, 0, 1], port)), txt).ok()?;
            Some((hostname.to_owned(), port, txt.clone()))
        })
        .take(16)
        .collect()
}

impl DiscoveryBrowser for DnsSdBrowser {
    fn browse(&self, timeout: Duration) -> Result<Vec<DiscoveryRecord>> {
        ensure!(
            !timeout.is_zero() && timeout <= Duration::from_secs(30),
            "invalid discovery timeout"
        );
        let deadline = Instant::now() + timeout;
        let zone = capture(
            &["-Z".into(), SERVICE_TYPE.into(), "local".into()],
            timeout / 2,
        )?;
        let services = parse_zone(&zone);
        let mut records = Vec::new();
        let count = services.len().clamp(1, 16) as u32;
        for (hostname, port, txt) in services {
            let remaining = deadline.saturating_duration_since(Instant::now()) / count;
            if remaining.is_zero() {
                break;
            }
            let addresses = capture(&["-G".into(), "v4".into(), hostname.clone()], remaining)?;
            for line in addresses.lines() {
                let words: Vec<_> = line.split_whitespace().collect();
                if words.len() >= 6 && words[1] == "Add" && words[4] == hostname {
                    if let Ok(ip) = words[5].parse::<IpAddr>() {
                        if let Ok(record) =
                            DiscoveryRecord::from_txt(SocketAddr::new(ip, port), &txt)
                        {
                            if !records.contains(&record) {
                                records.push(record);
                            }
                        }
                    }
                }
            }
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_zone_parser_rejects_extra_or_malformed_txt() {
        let fingerprint = "a".repeat(64);
        for tail in ["", " \"host_id=private\"", " unquoted", " \"broken"] {
            let zone = format!("fixture._mote._tcp SRV 0 0 47810 fixture.local. ; target\nfixture._mote._tcp TXT \"protocol_major=1\" \"fingerprint={fingerprint}\"{tail}\n");
            let parsed = parse_zone(&zone);
            if tail.is_empty() {
                assert_eq!(
                    parsed,
                    vec![(
                        "fixture.local.".into(),
                        47810,
                        vec![
                            "protocol_major=1".into(),
                            format!("fingerprint={fingerprint}")
                        ]
                    )]
                );
            } else {
                assert!(parsed.is_empty(), "accepted unexpected TXT: {tail}");
            }
        }
    }
}
