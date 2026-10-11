use crate::{
    client::{read_packet, write_packet},
    DeviceStore, GatewayConfig, HostIdentity, NoiseSession, PairingOffer, PendingHandshake,
};
use anyhow::{ensure, Result};
use mote_ipc::{Actor, Dispatch};
use mote_protocol::{
    ClientFrame, HostError, HostErrorCode, HostFrame, HostId, RequestId, VersionRange,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct PendingDevice {
    pub client_id: String,
    pub name: String,
}
struct Confirmation {
    device: PendingDevice,
    answer: Mutex<Option<bool>>,
    changed: Condvar,
}
struct Shared {
    config: GatewayConfig,
    host: HostIdentity,
    store: Mutex<DeviceStore>,
    offer: Mutex<Option<PairingOffer>>,
    pending: Mutex<BTreeMap<String, Arc<Confirmation>>>,
    running: AtomicBool,
}
pub struct GatewayServer {
    shared: Arc<Shared>,
    address: SocketAddr,
    accept: Option<JoinHandle<()>>,
}
impl GatewayServer {
    pub fn bind_loopback<D: Dispatch>(mut config: GatewayConfig, dispatch: Arc<D>) -> Result<Self> {
        config.validate()?;
        ensure!(
            config.bind.ip() == "127.0.0.1".parse::<std::net::IpAddr>()?,
            "gateway must bind IPv4 loopback"
        );
        let host = HostIdentity::load_or_create(&config.data_dir)?;
        let store = DeviceStore::open(&config.data_dir.join("mote.db"), host.host_id())?;
        let listener = TcpListener::bind(config.bind)?;
        let address = listener.local_addr()?;
        config.bind = address;
        let shared = Arc::new(Shared {
            config,
            host,
            store: Mutex::new(store),
            offer: Mutex::new(None),
            pending: Mutex::new(BTreeMap::new()),
            running: AtomicBool::new(true),
        });
        let state = shared.clone();
        let accept = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if !state.running.load(Ordering::Acquire) {
                    break;
                }
                if let Ok(stream) = stream {
                    let state = state.clone();
                    let dispatch = dispatch.clone();
                    std::thread::spawn(move || {
                        let _ = serve(stream, &state, dispatch.as_ref());
                    });
                }
            }
        });
        Ok(Self {
            shared,
            address,
            accept: Some(accept),
        })
    }
    pub fn host_id(&self) -> &HostId {
        self.shared.host.host_id()
    }
    pub fn public_key(&self) -> &[u8; 32] {
        self.shared.host.public_key()
    }
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
    pub fn open_pairing(&self) -> Result<String> {
        let offer = PairingOffer::open(
            &self.shared.config,
            &self.shared.host,
            Instant::now(),
            &["127.0.0.1".into()],
        )?;
        let code = offer.code();
        *self.shared.offer.lock().unwrap() = Some(offer);
        Ok(code)
    }
    pub fn pending_devices(&self) -> Vec<PendingDevice> {
        self.shared
            .pending
            .lock()
            .unwrap()
            .values()
            .map(|p| p.device.clone())
            .collect()
    }
    pub fn confirm_pairing(&self, client_id: &str, approved: bool) -> Result<()> {
        let pending = self
            .shared
            .pending
            .lock()
            .unwrap()
            .get(client_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no pending device"))?;
        let mut answer = pending.answer.lock().unwrap();
        ensure!(answer.is_none(), "device already answered");
        *answer = Some(approved);
        pending.changed.notify_all();
        Ok(())
    }
    pub fn revoke(&self, public_key: &[u8; 32]) -> Result<bool> {
        self.shared.store.lock().unwrap().revoke(public_key)
    }
}
impl Drop for GatewayServer {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::Release);
        for pending in self.shared.pending.lock().unwrap().values() {
            pending.changed.notify_all();
        }
        let _ = TcpStream::connect(self.address);
        if let Some(handle) = self.accept.take() {
            let _ = handle.join();
        }
    }
}
fn serve<D: Dispatch>(mut stream: TcpStream, shared: &Shared, dispatch: &D) -> Result<()> {
    stream.set_read_timeout(Some(shared.config.handshake_timeout))?;
    stream.set_write_timeout(Some(shared.config.handshake_timeout))?;
    let mut mode = [0];
    stream.read_exact(&mut mode)?;
    ensure!(mode[0] <= 1, "invalid handshake mode");
    let first = read_packet(&mut stream, 4096)?;
    let (mut session, second) = if mode[0] == 1 {
        let pending = {
            let mut offer = shared.offer.lock().unwrap();
            PendingHandshake::pairing(
                &shared.config,
                &shared.host,
                offer
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("pairing closed"))?,
                &first,
                Instant::now(),
            )?
        };
        let id = pending.client_id().to_string();
        let confirmation = Arc::new(Confirmation {
            device: PendingDevice {
                client_id: id.clone(),
                name: pending.client_name().into(),
            },
            answer: Mutex::new(None),
            changed: Condvar::new(),
        });
        shared
            .pending
            .lock()
            .unwrap()
            .insert(id.clone(), confirmation.clone());
        let answer = confirmation.answer.lock().unwrap();
        let (answer, _) = confirmation
            .changed
            .wait_timeout_while(answer, shared.config.confirm_timeout, |a| {
                a.is_none() && shared.running.load(Ordering::Acquire)
            })
            .unwrap();
        let approved = *answer == Some(true);
        shared.pending.lock().unwrap().remove(&id);
        pending.confirm(&shared.store.lock().unwrap(), approved, Instant::now())?
    } else {
        PendingHandshake::session(
            &shared.config,
            &shared.host,
            &shared.store.lock().unwrap(),
            &first,
        )?
    };
    write_packet(&mut stream, &second)?;
    stream.set_read_timeout(None)?;
    let actor = Actor::PairedDevice {
        host_id: session.host_id().clone(),
        device_id: session.client_id().into(),
    };
    while shared.running.load(Ordering::Acquire) {
        let bytes = read_packet(&mut stream, 65_535)?;
        let Some(frame) = session.read_client(&bytes)? else {
            continue;
        };
        ensure!(
            shared
                .store
                .lock()
                .unwrap()
                .active_device(session.client_id())?,
            "device revoked"
        );
        if let ClientFrame::Hello { versions } = frame {
            VersionRange::current().negotiate(versions)?;
            send(
                &mut stream,
                &mut session,
                &HostFrame::Hello {
                    host_id: shared.host.host_id().clone(),
                    protocol_major: shared.config.protocol_major,
                    versions: VersionRange::current(),
                },
            )?;
            continue;
        }
        if let ClientFrame::Subscribe {
            scope_id,
            cursor,
            request_id,
        } = frame
        {
            ensure!(cursor.host_id == *session.host_id(), "wrong host cursor");
            if let Some(mut subscription) = dispatch.subscribe(&scope_id, cursor.seq) {
                while shared.running.load(Ordering::Acquire)
                    && shared
                        .store
                        .lock()
                        .unwrap()
                        .active_device(session.client_id())?
                {
                    if let Some(event) = subscription.recv_timeout(Duration::from_millis(100)) {
                        let event = mote_protocol::Event {
                            seq: event.seq,
                            ts: event.ts,
                            scope_id: event.scope_id,
                            kind: event.kind,
                            payload: event.payload,
                        };
                        send(
                            &mut stream,
                            &mut session,
                            &HostFrame::Event {
                                host_id: shared.host.host_id().clone(),
                                event,
                            },
                        )?;
                    }
                }
            } else {
                send(
                    &mut stream,
                    &mut session,
                    &error(
                        Some(request_id),
                        HostErrorCode::InvalidRequest,
                        "subscription unavailable",
                    ),
                )?;
            }
            return Ok(());
        }
        let (id, method, params, allowed) = match &frame {
            ClientFrame::Call {
                request_id,
                method,
                params,
            } => (
                request_id,
                method.as_str(),
                params.clone(),
                matches!(
                    method.as_str(),
                    "hello" | "ping" | "status" | "turn" | "snapshot"
                ),
            ),
            ClientFrame::Interaction {
                request_id,
                interaction,
            } => (
                request_id,
                "interaction",
                serde_json::to_value(interaction)?,
                true,
            ),
            ClientFrame::ApprovalDecision {
                request_id,
                decision,
            } => (
                request_id,
                "approval.decision",
                serde_json::to_value(decision)?,
                true,
            ),
            _ => unreachable!(),
        };
        let outcome = if !allowed {
            error(
                Some(id.clone()),
                HostErrorCode::Unauthorized,
                "method is local-only",
            )
        } else {
            let hash = crate::noise::hex(&Sha256::digest(serde_json::to_vec(&frame)?));
            let claim = shared
                .store
                .lock()
                .unwrap()
                .claim(session.client_id(), id.as_str(), &hash);
            match claim {
                Ok(Some(outcome)) => outcome,
                Err(_) => error(
                    Some(id.clone()),
                    HostErrorCode::InvalidRequest,
                    "request already claimed or changed",
                ),
                Ok(None) => {
                    let outcome = match dispatch.call_with_context(&actor, method, params) {
                        Ok(result) => HostFrame::Result {
                            request_id: id.clone(),
                            result,
                        },
                        Err(err) => error(
                            Some(id.clone()),
                            match err.code.as_str() {
                                "unauthorized" => HostErrorCode::Unauthorized,
                                "invalid_params" => HostErrorCode::InvalidParams,
                                "invalid_request" => HostErrorCode::InvalidRequest,
                                _ => HostErrorCode::Internal,
                            },
                            "host refused request",
                        ),
                    };
                    let outcome = redacted(&outcome)?;
                    shared.store.lock().unwrap().finish_request(
                        session.client_id(),
                        id.as_str(),
                        &outcome,
                    )?;
                    outcome
                }
            }
        };
        send(&mut stream, &mut session, &outcome)?;
    }
    Ok(())
}
fn error(request_id: Option<RequestId>, code: HostErrorCode, message: &str) -> HostFrame {
    HostFrame::Error {
        request_id,
        error: HostError::new(code, message),
    }
}
fn redacted(frame: &HostFrame) -> Result<HostFrame> {
    let mut frame = frame.clone();
    match &mut frame {
        HostFrame::Result { result, .. } => private_payload(result),
        HostFrame::Event { event, .. } => private_payload(&mut event.payload),
        HostFrame::CommittedSurface { surface, .. } => private_map(&mut surface.tree),
        HostFrame::Snapshot { snapshot } => {
            for thread in &mut snapshot.threads {
                for message in &mut thread.messages {
                    message.text = private_text(&message.text);
                }
            }
            for surface in &mut snapshot.surfaces {
                private_map(&mut surface.tree);
            }
            for approval in &mut snapshot.pending_approvals {
                private_map(&mut approval.payload);
            }
        }
        _ => {}
    }
    Ok(serde_json::from_str(&mote_redact::redact(
        &serde_json::to_string(&frame)?,
    ))?)
}

fn private_map(map: &mut serde_json::Map<String, serde_json::Value>) {
    for (key, value) in map {
        if matches!(
            key.as_str(),
            "path" | "resolved_path" | "socket" | "data_dir" | "profile_path"
        ) {
            *value = serde_json::Value::String("[local path]".into());
        } else {
            private_payload(value);
        }
    }
}
fn private_payload(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => private_map(map),
        serde_json::Value::Array(values) => values.iter_mut().for_each(private_payload),
        serde_json::Value::String(text) => *text = private_text(text),
        _ => {}
    }
}
fn private_text(text: &str) -> String {
    text.split_inclusive(char::is_whitespace)
        .map(|word| {
            let token = word.trim_start_matches(['(', '"', '\'', '=']);
            if (token.starts_with('/') && token.len() > 1)
                || [
                    "/Users/",
                    "/private/",
                    "/tmp/",
                    "/var/",
                    "/Volumes/",
                    "/home/",
                ]
                .iter()
                .any(|prefix| token.contains(prefix))
            {
                let whitespace = &word[word.trim_end().len()..];
                format!("[local path]{whitespace}")
            } else {
                word.to_string()
            }
        })
        .collect()
}
fn send(stream: &mut TcpStream, session: &mut NoiseSession, frame: &HostFrame) -> Result<()> {
    for bytes in session.write_host(&redacted(frame)?)? {
        write_packet(stream, &bytes)?;
    }
    Ok(())
}
