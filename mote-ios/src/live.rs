use crate::keychain::StoredCredential;
use anyhow::{ensure, Result};
use mote_gateway::{ClientCredential, GatewayClient, GatewayConfig};
use mote_host_client::{
    CredentialStore, HostCommand, HostEventStream, HostRequestTransport, HostTransport,
};
use mote_protocol::{
    ClientFrame, Cursor, HostError, HostErrorCode, HostFrame, HostId, HostSnapshot, RequestId,
};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

pub struct NetworkHost<S> {
    host_id: HostId,
    endpoint: Arc<Mutex<SocketAddr>>,
    connected_endpoint: SocketAddr,
    generation: Arc<std::sync::atomic::AtomicU64>,
    connected_generation: u64,
    needs_reconnect: bool,
    config: GatewayConfig,
    store: Arc<Mutex<S>>,
    connection: GatewayClient,
}

fn failure(error: impl std::fmt::Display) -> HostError {
    HostError::new(HostErrorCode::Internal, error.to_string())
}

impl<S: CredentialStore<Credential = StoredCredential>> NetworkHost<S> {
    pub fn pair(
        endpoint: SocketAddr,
        config: GatewayConfig,
        offer: &str,
        name: &str,
        store: Arc<Mutex<S>>,
    ) -> Result<Self> {
        ensure!(
            mote_gateway::net::is_local_range(endpoint.ip()),
            "endpoint must be local"
        );
        let credential = ClientCredential::from_pairing_code(&config, offer)?;
        let connection = GatewayClient::pair(endpoint, &config, &credential, name, 1)?;
        let host_id = credential.host_id().clone();
        store
            .lock()
            .map_err(|_| anyhow::anyhow!("credential store unavailable"))?
            .save(&host_id, &StoredCredential::paired(&credential))?;
        Ok(Self {
            host_id,
            endpoint: Arc::new(Mutex::new(endpoint)),
            connected_endpoint: endpoint,
            generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            connected_generation: 0,
            needs_reconnect: false,
            config,
            store,
            connection,
        })
    }

    pub fn connect(
        host_id: HostId,
        endpoint: SocketAddr,
        config: GatewayConfig,
        store: Arc<Mutex<S>>,
    ) -> Result<Self> {
        ensure!(
            mote_gateway::net::is_local_range(endpoint.ip()),
            "endpoint must be local"
        );
        let (credential, counter) = {
            let mut store = store
                .lock()
                .map_err(|_| anyhow::anyhow!("credential store unavailable"))?;
            let mut stored = store
                .load(&host_id)?
                .ok_or_else(|| anyhow::anyhow!("host credential is missing"))?;
            let credential = stored.restore(&host_id)?;
            let counter = stored.reserve_counter()?;
            store.save(&host_id, &stored)?;
            (credential, counter)
        };
        let connection = GatewayClient::connect(endpoint, &config, &credential, counter)?;
        Ok(Self {
            host_id,
            endpoint: Arc::new(Mutex::new(endpoint)),
            connected_endpoint: endpoint,
            generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            connected_generation: 0,
            needs_reconnect: false,
            config,
            store,
            connection,
        })
    }

    pub fn host_id(&self) -> &HostId {
        &self.host_id
    }
    pub fn endpoint(&self) -> SocketAddr {
        *self.endpoint.lock().expect("endpoint lock")
    }
    pub fn rediscover(&mut self, record: &mote_gateway::discovery::DiscoveryRecord) -> Result<()> {
        let (candidate, credential, counter) = {
            let mut store = self
                .store
                .lock()
                .map_err(|_| anyhow::anyhow!("credential store unavailable"))?;
            let mut stored = store
                .load(&self.host_id)?
                .ok_or_else(|| anyhow::anyhow!("host credential is missing"))?;
            let credential = stored.restore(&self.host_id)?;
            let candidate =
                mote_gateway::discovery::EndpointCandidate::new(record.clone(), &credential)?;
            let counter = stored.reserve_counter()?;
            store.save(&self.host_id, &stored)?;
            (candidate, credential, counter)
        };
        let mut endpoint = self.endpoint();
        let connection = candidate.connect(&self.config, &credential, counter, &mut endpoint)?;
        *self
            .endpoint
            .lock()
            .map_err(|_| anyhow::anyhow!("endpoint unavailable"))? = endpoint;
        self.connected_endpoint = endpoint;
        self.connected_generation = self
            .generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        self.needs_reconnect = false;
        self.connection = connection;
        Ok(())
    }
    pub fn reopen(&self) -> Result<Self> {
        let mut host = Self::connect(
            self.host_id.clone(),
            self.endpoint(),
            self.config.clone(),
            self.store.clone(),
        )?;
        host.endpoint = self.endpoint.clone();
        host.generation = self.generation.clone();
        host.connected_generation = self.generation.load(std::sync::atomic::Ordering::Acquire);
        Ok(host)
    }

    pub fn request(&mut self, command: HostCommand) -> Result<Vec<HostFrame>, HostError> {
        if command.host_id != self.host_id {
            return Err(HostError::new(
                HostErrorCode::IdentityChanged,
                "command belongs to another host",
            ));
        }
        let mut outbox = mote_host_client::Outbox::new(&self.host_id);
        outbox.enqueue(&self.host_id, command.frame.clone())?;
        let id = match &command.frame {
            ClientFrame::Call { request_id, .. }
            | ClientFrame::Interaction { request_id, .. }
            | ClientFrame::ApprovalDecision { request_id, .. } => request_id,
            _ => {
                return Err(HostError::new(
                    HostErrorCode::InvalidRequest,
                    "command is not a unary request",
                ))
            }
        };
        if self.needs_reconnect
            || self.connected_endpoint != self.endpoint()
            || self.connected_generation
                != self.generation.load(std::sync::atomic::Ordering::Acquire)
        {
            let reopened = self.reopen().map_err(failure)?;
            self.connection = reopened.connection;
            self.connected_endpoint = reopened.connected_endpoint;
            self.connected_generation = reopened.connected_generation;
            self.needs_reconnect = false;
        }
        let outcome = (|| {
            self.connection.send(&command.frame).map_err(failure)?;
            let reply = self.connection.receive().map_err(failure)?;
            if matches!(reply, HostFrame::RequestState { .. }) {
                return Ok(vec![reply]);
            }
            let acknowledgement = self.connection.receive().map_err(failure)?;
            ensure_ack(&self.host_id, id, &reply, &acknowledgement)?;
            Ok(vec![reply, acknowledgement])
        })();
        if outcome.is_err() {
            self.needs_reconnect = true;
        }
        outcome
    }

    pub fn snapshot(&mut self) -> Result<HostSnapshot> {
        let request_id = RequestId::new(format!("snapshot-{}", unique_id()))?;
        let frames = self.request(HostCommand {
            host_id: self.host_id.clone(),
            frame: ClientFrame::Call {
                request_id,
                method: "snapshot".into(),
                params: serde_json::json!({}),
            },
        })?;
        match &frames[0] {
            HostFrame::Result { result, .. } => Ok(serde_json::from_value(result.clone())?),
            _ => anyhow::bail!("snapshot unavailable"),
        }
    }

    pub fn subscribe(&self, cursor: &Cursor) -> Result<NetworkStream> {
        ensure!(
            cursor.host_id == self.host_id,
            "cursor belongs to another host"
        );
        let mut network = self.reopen()?;
        network.connection.send(&ClientFrame::Subscribe {
            request_id: RequestId::new(format!("events-{}", unique_id()))?,
            scope_id: "main".into(),
            cursor: cursor.clone(),
        })?;
        Ok(NetworkStream {
            host_id: self.host_id.clone(),
            connection: network.connection,
        })
    }

    pub fn reconnect_events(&self, cursor: &Cursor) -> Result<NetworkStream> {
        let stream = self.subscribe(cursor)?;
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Ok(stream)
    }
}

fn ensure_ack(
    host: &HostId,
    id: &RequestId,
    reply: &HostFrame,
    ack: &HostFrame,
) -> Result<(), HostError> {
    if !matches!(reply, HostFrame::Result { request_id, .. } | HostFrame::Error { request_id: Some(request_id), .. } if request_id == id)
        || !matches!(ack, HostFrame::Acknowledgement { host_id, request_id, cursor } if host_id == host && request_id == id && &cursor.host_id == host)
    {
        return Err(HostError::new(
            HostErrorCode::IdentityChanged,
            "unmatched host acknowledgement",
        ));
    }
    Ok(())
}

pub fn unique_id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

pub struct NetworkStream {
    host_id: HostId,
    connection: GatewayClient,
}
impl NetworkStream {
    pub fn receive(&mut self) -> Result<HostFrame> {
        self.connection.receive()
    }
}
impl HostEventStream for NetworkStream {
    fn next(&mut self, host: &HostId) -> Result<Option<HostFrame>, HostError> {
        if host != &self.host_id {
            return Err(HostError::new(
                HostErrorCode::IdentityChanged,
                "stream belongs to another host",
            ));
        }
        self.receive().map(Some).map_err(failure)
    }
}
impl<S: CredentialStore<Credential = StoredCredential> + Send + 'static> HostRequestTransport
    for NetworkHost<S>
{
    fn request(&mut self, command: HostCommand) -> Result<Vec<HostFrame>, HostError> {
        NetworkHost::request(self, command)
    }
}
impl<S: CredentialStore<Credential = StoredCredential>> HostTransport for NetworkHost<S> {
    type EventStream = NetworkStream;
    fn send(&mut self, host: &HostId, frame: &ClientFrame) -> Result<(), HostError> {
        if host != &self.host_id {
            return Err(HostError::new(
                HostErrorCode::IdentityChanged,
                "command belongs to another host",
            ));
        }
        self.connection.send(frame).map_err(failure)
    }
    fn subscribe(&mut self, host: &HostId, cursor: &Cursor) -> Result<NetworkStream, HostError> {
        if host != &self.host_id {
            return Err(HostError::new(
                HostErrorCode::IdentityChanged,
                "stream belongs to another host",
            ));
        }
        NetworkHost::subscribe(self, cursor).map_err(failure)
    }
}

use mote_host_client::{ConnectionState, HostDirectory, HostUpdate, PairedHost};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    pub name: String,
    pub public_key: [u8; 32],
    pub endpoint: SocketAddr,
    #[serde(default, skip_serializing)]
    pub offer: Option<String>,
}

pub struct LiveRuntime {
    pub(crate) directory: HostDirectory,
    pub(crate) rx: Receiver<HostUpdate>,
    pub(crate) persist: Sender<HostUpdate>,
    pub(crate) commands: BTreeMap<HostId, Sender<HostCommand>>,
    pub(crate) status: BTreeMap<HostId, String>,
    pub(crate) approval_requests: BTreeMap<(HostId, RequestId), RequestId>,
    pub(crate) authenticated: bool,
}

fn live_label(ui: &mut egui::Ui, id: egui::Id, text: String) {
    let response = ui.label(&text);
    #[cfg(target_os = "ios")]
    mote_client_core::native_accessibility::record(
        ui,
        id,
        mote_client_core::native_accessibility::Role::Label,
        &text,
        None,
        &response,
    );
    #[cfg(not(target_os = "ios"))]
    let _ = (id, response);
}

impl LiveRuntime {
    pub fn from_environment(ctx: egui::Context) -> Result<Option<Self>> {
        if std::env::args().any(|arg| arg == "--mote-offline") {
            return Ok(None);
        }
        let root = documents()?;
        let config_path = root.join("live-hosts.json");
        let configs: Vec<HostConfig> = if let Ok(value) = std::env::var("MOTE_SIMULATOR_HOSTS") {
            serde_json::from_str(&value)?
        } else if config_path.exists() {
            serde_json::from_reader(std::fs::File::open(&config_path)?)?
        } else {
            return Ok(None);
        };
        Ok(Some(Self::from_configs(ctx, configs, false)?))
    }

    pub fn from_first_pairing(ctx: egui::Context, config: HostConfig) -> Result<Self> {
        ensure!(
            config.offer.is_some(),
            "first pairing requires a transient code"
        );
        Self::from_configs(ctx, vec![config], true)
    }

    fn from_configs(ctx: egui::Context, configs: Vec<HostConfig>, fresh: bool) -> Result<Self> {
        let root = documents()?;
        let config_path = root.join("live-hosts.json");
        ensure!(
            !configs.is_empty() && configs.len() <= 8,
            "invalid paired-host count"
        );
        let directory_path = root.join("live-state.json");
        let directory = if fresh {
            HostDirectory::default()
        } else if directory_path.exists() {
            HostDirectory::load(&directory_path)?
        } else {
            HostDirectory::default()
        };
        let mut ids = std::collections::BTreeSet::new();
        for config in &configs {
            ensure!(
                mote_gateway::net::is_local_range(config.endpoint.ip())
                    && !config.name.trim().is_empty(),
                "invalid local host"
            );
            ensure!(
                ids.insert(HostId::from_public_key(&config.public_key)),
                "duplicate configured host"
            );
        }
        let (persist, incoming) = mpsc::channel::<HostUpdate>();
        let (render, rx) = mpsc::channel();
        let mut durable = directory.clone();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            for update in incoming {
                let host_id = update.host_id().clone();
                let mut next = durable.clone();
                let reduced = next.reduce_update(update.clone());
                if let Err(error) = next.save(&directory_path) {
                    let _ = render.send(HostUpdate::Failed {
                        host_id,
                        error: failure(error),
                    });
                    repaint.request_repaint();
                    continue;
                }
                durable = next;
                if render.send(update).is_err() {
                    return;
                }
                if let Err(error) = reduced {
                    let _ = render.send(HostUpdate::Failed { host_id, error });
                }
                repaint.request_repaint();
            }
        });
        let saved_configs = Arc::new(Mutex::new(configs.clone()));
        let store = Arc::new(Mutex::new(crate::keychain::KeychainStore));
        let mut commands = BTreeMap::new();
        for config in configs {
            let id = HostId::from_public_key(&config.public_key);
            let cursor = directory
                .state(&id)
                .map(|state| state.cursor().clone())
                .unwrap_or(Cursor::new(id.clone(), 0)?);
            let (command_tx, command_rx) = mpsc::channel();
            commands.insert(id.clone(), command_tx);
            let updates = persist.clone();
            let store = store.clone();
            let repaint = ctx.clone();
            let config_path = config_path.clone();
            let saved_configs = saved_configs.clone();
            let select = directory.selected_host().is_none() && commands.len() == 1;
            std::thread::spawn(move || {
                HostWorker {
                    config,
                    cursor,
                    select,
                    commands: Some(command_rx),
                    updates,
                    store,
                    configs: saved_configs,
                    config_path,
                    ctx: repaint,
                }
                .run();
            });
        }
        Ok(Self {
            directory,
            rx,
            persist,
            commands,
            status: BTreeMap::new(),
            approval_requests: BTreeMap::new(),
            authenticated: false,
        })
    }

    pub fn drain(&mut self) {
        while let Ok(update) = self.rx.try_recv() {
            let host = update.host_id().clone();
            if matches!(
                &update,
                HostUpdate::Connection {
                    state: ConnectionState::Online,
                    ..
                }
            ) {
                self.authenticated = true;
            }
            match &update {
                HostUpdate::Frame {
                    frame: HostFrame::Result { .. },
                    ..
                } => {
                    self.status
                        .insert(host.clone(), "Host accepted interaction".into());
                }
                HostUpdate::Frame {
                    frame: HostFrame::Error { error, .. },
                    ..
                }
                | HostUpdate::Failed { error, .. } => {
                    self.status.insert(host.clone(), error.message.clone());
                }
                _ => {}
            }
            if let Err(error) = self.directory.reduce_update(update) {
                self.status.insert(host.clone(), error.message);
            }
            self.approval_requests.retain(|(host, approval), _| {
                !self.directory.state(host).is_ok_and(|state| {
                    state.approval_states().get(approval).is_some_and(|state| {
                        state.status != mote_host_client::ApprovalStatus::Pending
                    })
                })
            });
        }
    }

    fn send(&mut self, host: &HostId, frame: ClientFrame) -> bool {
        if !self
            .directory
            .state(host)
            .is_ok_and(|state| state.connection() == &ConnectionState::Online)
        {
            self.status.insert(host.clone(), "Host is offline".into());
            return false;
        }
        if let Some(sender) = self.commands.get(host) {
            if sender
                .send(HostCommand {
                    host_id: host.clone(),
                    frame,
                })
                .is_ok()
            {
                self.status.insert(host.clone(), "Waiting for host".into());
                return true;
            } else {
                self.status
                    .insert(host.clone(), "Host connection is unavailable".into());
            }
        }
        false
    }

    pub fn draw(&mut self, ctx: &egui::Context, safe: egui::Margin, composer: &mut String) {
        let mut header = egui::Frame::side_top_panel(&ctx.style());
        header.inner_margin.top = header.inner_margin.top.saturating_add(safe.top);
        let selected = self.directory.selected_host().cloned();
        egui::TopBottomPanel::top("live-hosts")
            .frame(header)
            .show(ctx, |ui| {
                ui.heading("Mote");
                let hosts: Vec<_> = self.directory.hosts().cloned().collect();
                ui.horizontal_wrapped(|ui| {
                    for host in hosts {
                        ui.push_id(host.host_id(), |ui| {
                            let response = ui.add_sized(
                                [128.0, 48.0],
                                egui::Button::new(host.name())
                                    .selected(selected.as_ref() == Some(host.host_id())),
                            );
                            #[cfg(target_os = "ios")]
                            mote_client_core::native_accessibility::record(
                                ui,
                                response.id,
                                mote_client_core::native_accessibility::Role::Button,
                                host.name(),
                                None,
                                &response,
                            );
                            if response.clicked() {
                                let _ = self.persist.send(HostUpdate::Selected {
                                    host_id: host.host_id().clone(),
                                });
                            }
                        });
                    }
                });
                if let Some(host) = &selected {
                    if let Ok(state) = self.directory.state(host) {
                        let name = self
                            .directory
                            .host(host)
                            .map_or("Host", |paired| paired.name());
                        let status = match state.connection() {
                            ConnectionState::Connecting => "connecting",
                            ConnectionState::Online => "online",
                            ConnectionState::Offline => "offline",
                            ConnectionState::Incompatible { .. } => "incompatible",
                            ConnectionState::IdentityChanged => "identity changed",
                        };
                        live_label(
                            ui,
                            ui.make_persistent_id(("host-status", host)),
                            format!("{name}: {status}"),
                        );
                        live_label(
                            ui,
                            ui.make_persistent_id(("host-cursor", host)),
                            format!("Cursor: {}", state.cursor().seq),
                        );
                    }
                } else {
                    live_label(
                        ui,
                        ui.make_persistent_id("no-host-status"),
                        "Connecting to paired hosts".into(),
                    );
                }
                if let Some(status) = selected.as_ref().and_then(|host| self.status.get(host)) {
                    live_label(
                        ui,
                        ui.make_persistent_id(("request-status", &selected)),
                        status.clone(),
                    );
                }
                ui.horizontal(|ui| {
                    let field = mote_client_core::native_text::singleline(
                        ui,
                        egui::Id::new("live-chat"),
                        composer,
                        egui::vec2(ui.available_width() - 112.0, 48.0),
                        "Message your Mote host",
                        false,
                    );
                    #[cfg(target_os = "ios")]
                    mote_client_core::native_accessibility::record(
                        ui,
                        field.id,
                        mote_client_core::native_accessibility::Role::TextField,
                        "Message your Mote host",
                        Some(composer.clone()),
                        &field,
                    );
                    let send_response = ui
                        .push_id(("live-send", &selected), |ui| {
                            ui.add_enabled_ui(
                                selected.is_some() && !composer.trim().is_empty(),
                                |ui| ui.add_sized([96.0, 48.0], egui::Button::new("Send")),
                            )
                            .inner
                        })
                        .inner;
                    #[cfg(target_os = "ios")]
                    mote_client_core::native_accessibility::record(
                        ui,
                        send_response.id,
                        mote_client_core::native_accessibility::Role::Button,
                        "Send",
                        None,
                        &send_response,
                    );
                    let send = send_response.clicked()
                        || (field.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                    if send && !composer.trim().is_empty() {
                        if let Some(host) = &selected {
                            self.send(
                                host,
                                ClientFrame::Call {
                                    request_id: RequestId::new(format!("chat-{}", unique_id()))
                                        .expect("generated ID"),
                                    method: "turn".into(),
                                    params: serde_json::json!({"thread":"main","message":composer}),
                                },
                            );
                            composer.clear();
                        }
                    }
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(host) = selected else {
                return;
            };
            let Ok(state) = self.directory.state(&host) else {
                return;
            };
            let online = state.connection() == &ConnectionState::Online;
            let thread = state.selected_thread().unwrap_or("main");
            let messages = state
                .threads()
                .get(thread)
                .map(|thread| thread.messages.clone())
                .unwrap_or_default();
            let surface = state
                .surfaces()
                .get(thread)
                .and_then(|surfaces| surfaces.last_key_value())
                .map(|(_, surface)| surface.clone());
            let snapshot_cursor = state.cursor().seq;
            let (mut approvals, mut terminal): (Vec<_>, Vec<_>) = state
                .approval_states()
                .values()
                .filter(|state| state.approval.thread == thread && state.approval.kind == "browser")
                .cloned()
                .partition(|state| state.status == mote_host_client::ApprovalStatus::Pending);
            terminal.sort_by(|left, right| {
                right
                    .approval
                    .expires_at
                    .cmp(&left.approval.expires_at)
                    .then_with(|| right.approval.approval_id.cmp(&left.approval.approval_id))
            });
            approvals.extend(terminal.into_iter().take(6));
            let host_name = self
                .directory
                .host(&host)
                .map_or("Host", |paired| paired.name())
                .to_owned();
            egui::ScrollArea::vertical()
                .id_salt(("live-chat", &host))
                .max_height(ui.available_height() * 0.32)
                .show(ui, |ui| {
                    for message in messages {
                        let id =
                            ui.make_persistent_id(("chat-message", &host, thread, &message.id));
                        live_label(ui, id, format!("{:?}: {}", message.role, message.text));
                    }
                });
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(i64::MAX as u128) as i64;
            let approval_area = egui::ScrollArea::vertical()
                .id_salt(("live-approvals", &host))
                .max_height(ui.available_height() * 0.45)
                .show(ui, |ui| {
                    for approval in approvals {
                        let key = (host.clone(), approval.approval.approval_id.clone());
                        if approval.status == mote_host_client::ApprovalStatus::Pending
                            && now_ms < approval.approval.expires_at
                        {
                            ctx.request_repaint_after(Duration::from_millis(
                                (approval.approval.expires_at - now_ms).min(1000) as u64,
                            ));
                        }
                        if let Some(decision) = mote_client_core::approval::draw(
                            ui,
                            mote_client_core::approval::Card {
                                host_id: &host,
                                host_name: &host_name,
                                approval: &approval.approval,
                                status: match approval.status {
                                    mote_host_client::ApprovalStatus::Pending => {
                                        mote_client_core::approval::ApprovalStatus::Pending
                                    }
                                    mote_host_client::ApprovalStatus::Approved => {
                                        mote_client_core::approval::ApprovalStatus::Approved
                                    }
                                    mote_host_client::ApprovalStatus::Denied => {
                                        mote_client_core::approval::ApprovalStatus::Denied
                                    }
                                    mote_host_client::ApprovalStatus::Expired => {
                                        mote_client_core::approval::ApprovalStatus::Expired
                                    }
                                    mote_host_client::ApprovalStatus::Failed => {
                                        mote_client_core::approval::ApprovalStatus::Failed
                                    }
                                },
                                online,
                                in_flight: self.approval_requests.contains_key(&key),
                                now_ms,
                            },
                        ) {
                            let request_id = RequestId::new(format!("approval-{}", unique_id()))
                                .expect("generated ID");
                            if self.send(
                                &host,
                                ClientFrame::ApprovalDecision {
                                    request_id: request_id.clone(),
                                    decision,
                                },
                            ) {
                                self.approval_requests.insert(key, request_id);
                                ctx.request_repaint();
                            }
                        }
                    }
                });
            #[cfg(test)]
            mote_client_core::observation::scroll("approvals", ui.clip_rect(), &approval_area);
            #[cfg(not(test))]
            let _ = approval_area;
            if let Some(surface) = surface {
                let bytes = serde_json::to_vec(&surface.tree).unwrap_or_default();
                match mote_ui::v1::validate(&bytes) {
                    Ok(surface_tree) => {
                        let mut events = Vec::new();
                        ui.push_id((&host, &surface.thread, surface.surface_seq), |ui| {
                            ui.add_enabled_ui(online, |ui| {
                                mote_client_core::surface_v1::draw_with_transition_namespace(
                                    ui,
                                    &surface_tree,
                                    &mut events,
                                    egui::Id::new((
                                        "live-surface-transition",
                                        &host,
                                        &surface.thread,
                                    )),
                                )
                            });
                        });
                        for event in events {
                            let (name, args, path) = match event {
                                mote_client_core::surface_v1::Event::Action { event, path } => {
                                    (event, serde_json::Map::new(), path)
                                }
                                mote_client_core::surface_v1::Event::Change {
                                    field,
                                    value,
                                    path,
                                } => (
                                    "change".into(),
                                    serde_json::json!({"field":field,"value":value})
                                        .as_object()
                                        .unwrap()
                                        .clone(),
                                    path,
                                ),
                            };
                            self.send(
                                &host,
                                ClientFrame::Interaction {
                                    request_id: RequestId::new(format!("ui-{}", unique_id()))
                                        .expect("generated ID"),
                                    interaction: mote_protocol::TypedInteraction {
                                        host_id: host.clone(),
                                        thread: surface.thread.clone(),
                                        surface_seq: surface.surface_seq,
                                        surface_contract: surface.surface_contract,
                                        node_path: path
                                            .into_iter()
                                            .map(|index| index as u32)
                                            .collect(),
                                        event: name,
                                        args,
                                    },
                                },
                            );
                        }
                    }
                    Err(_) => {
                        live_label(
                            ui,
                            ui.make_persistent_id("incompatible-surface"),
                            "This committed surface requires a compatible client".into(),
                        );
                    }
                }
            }
            live_label(
                ui,
                ui.make_persistent_id(("committed-cursor", &host)),
                format!("Committed through {snapshot_cursor}"),
            );
        });
    }
}

struct HostWorker<S> {
    config: HostConfig,
    cursor: Cursor,
    select: bool,
    commands: Option<Receiver<HostCommand>>,
    updates: Sender<HostUpdate>,
    store: Arc<Mutex<S>>,
    configs: Arc<Mutex<Vec<HostConfig>>>,
    config_path: PathBuf,
    ctx: egui::Context,
}

impl<S: CredentialStore<Credential = StoredCredential> + Send + 'static> HostWorker<S> {
    fn run(mut self) {
        let id = HostId::from_public_key(&self.config.public_key);
        let mut delay = Duration::from_millis(250);
        loop {
            let result = attach_host(
                self.config.clone(),
                self.cursor.clone(),
                self.select,
                &mut self.commands,
                &self.updates,
                self.store.clone(),
                self.configs.clone(),
                &self.config_path,
                self.ctx.clone(),
            );
            let Err(error) = result else { return };
            if self
                .updates
                .send(HostUpdate::Failed {
                    host_id: id.clone(),
                    error: failure(error),
                })
                .is_err()
                || self
                    .updates
                    .send(HostUpdate::Connection {
                        host_id: id.clone(),
                        state: ConnectionState::Offline,
                    })
                    .is_err()
            {
                return;
            }
            self.ctx.request_repaint();
            let saved = self
                .store
                .lock()
                .ok()
                .and_then(|store| store.load(&id).ok().flatten())
                .is_some();
            if !saved || self.commands.is_none() {
                return;
            }
            std::thread::sleep(delay);
            delay = delay.saturating_mul(2).min(Duration::from_secs(5));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn attach_host<S: CredentialStore<Credential = StoredCredential> + Send + 'static>(
    config: HostConfig,
    cursor: Cursor,
    select: bool,
    commands: &mut Option<Receiver<HostCommand>>,
    updates: &Sender<HostUpdate>,
    store: Arc<Mutex<S>>,
    configs: Arc<Mutex<Vec<HostConfig>>>,
    config_path: &Path,
    ctx: egui::Context,
) -> Result<()> {
    let id = HostId::from_public_key(&config.public_key);
    updates.send(HostUpdate::Registered {
        host: PairedHost::new(
            config.public_key,
            config.name.clone(),
            None,
            mote_protocol::VersionRange::current(),
        )?,
    })?;
    if select {
        updates.send(HostUpdate::Selected {
            host_id: id.clone(),
        })?;
    }
    updates.send(HostUpdate::Connection {
        host_id: id.clone(),
        state: ConnectionState::Connecting,
    })?;
    let gateway_config = GatewayConfig::new(config_path.parent().unwrap_or(Path::new(".")));
    use mote_gateway::discovery::DiscoveryBrowser;
    let records = crate::discovery::SimulatorBrowser
        .browse(Duration::from_secs(2))
        .unwrap_or_default();
    network_evidence(
        config_path,
        serde_json::json!({"kind":"browse","host_id":id,"records":records.iter().map(|record| serde_json::json!({"endpoint":record.endpoint,"txt":record.txt()})).collect::<Vec<_>>()}),
    )?;
    if std::env::var("MOTE_SIMULATOR_KEYCHAIN_DELETE")
        .ok()
        .as_deref()
        == Some(id.as_str())
    {
        let mut store = store
            .lock()
            .map_err(|_| anyhow::anyhow!("credential store unavailable"))?;
        store.delete(&id)?;
        ensure!(
            store.load(&id)?.is_none(),
            "deleted credential still exists"
        );
        network_evidence(
            config_path,
            serde_json::json!({"kind":"keychain_deleted","host_id":id,"missing":true}),
        )?;
        anyhow::bail!("Host requires a new owner-present pairing");
    }
    let credential_exists = store
        .lock()
        .map_err(|_| anyhow::anyhow!("credential store unavailable"))?
        .load(&id)?
        .is_some();
    let mut network = if credential_exists {
        let mut connected = NetworkHost::connect(
            id.clone(),
            config.endpoint,
            gateway_config.clone(),
            store.clone(),
        );
        if connected.is_err() {
            for record in &records {
                let expected = mote_gateway::discovery::DiscoveryRecord::new(
                    record.endpoint,
                    1,
                    &config.public_key,
                )?;
                if record.protocol_major != 1 || record.fingerprint != expected.fingerprint {
                    continue;
                }
                connected = NetworkHost::connect(
                    id.clone(),
                    record.endpoint,
                    gateway_config.clone(),
                    store.clone(),
                );
                if connected.is_ok() {
                    break;
                }
            }
        }
        connected?
    } else if let Some(offer) = &config.offer {
        ensure!(
            ClientCredential::from_pairing_code(&gateway_config, offer)?.host_id() == &id,
            "pairing offer has a different host identity"
        );
        NetworkHost::pair(
            config.endpoint,
            gateway_config.clone(),
            offer,
            "Mote iPad",
            store.clone(),
        )?
    } else {
        anyhow::bail!("Host requires a new owner-present pairing");
    };
    ensure!(
        network.host_id() == &id,
        "pairing offer has a different host identity"
    );
    network_evidence(
        config_path,
        serde_json::json!({"kind":if credential_exists {"keychain_restored"} else {"paired"},"host_id":id,"endpoint":network.endpoint(),"saved_cursor":cursor.seq}),
    )?;
    for record in records {
        if mote_gateway::discovery::DiscoveryRecord::new(record.endpoint, 1, &config.public_key)?
            .fingerprint
            != record.fingerprint
        {
            continue;
        }
        let prior = network.endpoint();
        let accepted = network.rediscover(&record).is_ok();
        network_evidence(
            config_path,
            serde_json::json!({"kind":"identity_proof","host_id":id,"candidate":record.endpoint,"accepted":accepted,"prior":prior,"current":network.endpoint()}),
        )?;
    }
    {
        let mut configs = configs
            .lock()
            .map_err(|_| anyhow::anyhow!("host metadata unavailable"))?;
        if let Some(config) = configs
            .iter_mut()
            .find(|config| HostId::from_public_key(&config.public_key) == id)
        {
            config.endpoint = network.endpoint();
        }
        save_configs(config_path, &configs)?;
    }
    let snapshot = network.snapshot()?;
    ensure!(
        snapshot.host_id == id,
        "snapshot has a different host identity"
    );
    updates.send(HostUpdate::Frame {
        host_id: id.clone(),
        frame: HostFrame::Snapshot {
            snapshot: snapshot.clone(),
        },
    })?;
    let resume_cursor = cursor;
    let mut event_network = network.reopen()?;
    let stream = event_network.subscribe(&resume_cursor)?;
    let command_updates = updates.clone();
    let repaint = ctx.clone();
    let commands = commands
        .take()
        .ok_or_else(|| anyhow::anyhow!("host command worker already started"))?;
    mote_host_client::command_worker(network, commands, command_updates, move || {
        repaint.request_repaint()
    });
    updates.send(HostUpdate::Connection {
        host_id: id.clone(),
        state: ConnectionState::Online,
    })?;
    ctx.request_repaint();
    let mut cursor = resume_cursor;
    let mut stream = stream;
    loop {
        match stream.receive() {
            Ok(frame) => {
                match &frame {
                    HostFrame::Event { host_id, event }
                        if host_id == &id && event.kind != "resync" =>
                    {
                        if event.seq > cursor.seq {
                            cursor.seq = event.seq;
                        }
                    }
                    HostFrame::Snapshot { snapshot } if snapshot.host_id == id => {
                        cursor = snapshot.cursor.clone()
                    }
                    _ => {}
                }
                if updates
                    .send(HostUpdate::Frame {
                        host_id: id.clone(),
                        frame,
                    })
                    .is_err()
                {
                    return Ok(());
                }
            }
            Err(_) => {
                updates.send(HostUpdate::Connection {
                    host_id: id.clone(),
                    state: ConnectionState::Offline,
                })?;
                std::thread::sleep(Duration::from_millis(500));
                if let Ok(records) =
                    crate::discovery::SimulatorBrowser.browse(Duration::from_secs(2))
                {
                    network_evidence(
                        config_path,
                        serde_json::json!({"kind":"rediscover","host_id":id,"records":records.iter().map(|record| serde_json::json!({"endpoint":record.endpoint,"txt":record.txt()})).collect::<Vec<_>>()}),
                    )?;
                    for record in records {
                        let prior = event_network.endpoint();
                        let accepted = event_network.rediscover(&record).is_ok();
                        network_evidence(
                            config_path,
                            serde_json::json!({"kind":"identity_proof","host_id":id,"candidate":record.endpoint,"accepted":accepted,"prior":prior,"current":event_network.endpoint()}),
                        )?;
                        if accepted {
                            let mut configs = configs
                                .lock()
                                .map_err(|_| anyhow::anyhow!("host metadata unavailable"))?;
                            if let Some(config) = configs
                                .iter_mut()
                                .find(|config| HostId::from_public_key(&config.public_key) == id)
                            {
                                config.endpoint = event_network.endpoint();
                            }
                            save_configs(config_path, &configs)?;
                            break;
                        }
                    }
                }
                if let Ok(reconnected) = event_network.reconnect_events(&cursor) {
                    stream = reconnected;
                    updates.send(HostUpdate::Connection {
                        host_id: id.clone(),
                        state: ConnectionState::Online,
                    })?;
                }
            }
        }
    }
}

fn save_configs(path: &Path, configs: &[HostConfig]) -> Result<()> {
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("missing metadata directory"))?,
    )?;
    file.write_all(&serde_json::to_vec(configs)?)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}

fn network_evidence(config_path: &Path, record: serde_json::Value) -> Result<()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    let _guard = SERIAL
        .lock()
        .map_err(|_| anyhow::anyhow!("network evidence unavailable"))?;
    let path = config_path.with_file_name("live-network.json");
    let mut records: Vec<serde_json::Value> = if path.exists() {
        serde_json::from_reader(std::fs::File::open(&path)?)?
    } else {
        Vec::new()
    };
    if records.len() >= 256 {
        records.remove(0);
    }
    records.push(record);
    use std::io::Write;
    let mut file = tempfile::NamedTempFile::new_in(
        path.parent()
            .ok_or_else(|| anyhow::anyhow!("missing evidence directory"))?,
    )?;
    file.write_all(&serde_json::to_vec(&records)?)?;
    file.persist(path)?;
    Ok(())
}

fn documents() -> Result<PathBuf> {
    #[cfg(target_os = "ios")]
    {
        extern "C" {
            fn mote_ios_documents() -> *const std::ffi::c_char;
        }
        let path = unsafe { std::ffi::CStr::from_ptr(mote_ios_documents()) }.to_str()?;
        Ok(PathBuf::from(path))
    }
    #[cfg(not(target_os = "ios"))]
    {
        Ok(std::env::var_os("MOTE_SIMULATOR_DATA")
            .map(PathBuf::from)
            .unwrap_or(std::env::temp_dir().join("mote-ios-offline")))
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use mote_gateway::GatewayServer;
    use mote_ipc::Dispatch;
    use mote_testkit::ScriptedServer;
    use std::{collections::BTreeMap, io};

    #[derive(Default)]
    struct MemoryStore(BTreeMap<HostId, StoredCredential>);
    impl CredentialStore for MemoryStore {
        type Credential = StoredCredential;
        fn load(&self, id: &HostId) -> io::Result<Option<StoredCredential>> {
            Ok(self.0.get(id).cloned())
        }
        fn save(&mut self, id: &HostId, credential: &StoredCredential) -> io::Result<()> {
            self.0.insert(id.clone(), credential.clone());
            Ok(())
        }
        fn delete(&mut self, id: &HostId) -> io::Result<()> {
            self.0.remove(id);
            Ok(())
        }
    }

    struct Forward<D: Dispatch>(mote_testkit::gateway::LoopbackHost<D>);
    impl<D: Dispatch> Dispatch for Forward<D> {
        fn call_with_context(
            &self,
            actor: &mote_ipc::Actor,
            method: &str,
            params: serde_json::Value,
        ) -> std::result::Result<serde_json::Value, mote_ipc::RpcError> {
            self.0.daemon().call_with_context(actor, method, params)
        }
        fn subscribe_host(&self, cursor: i64) -> Option<mote_eventlog::Replay> {
            self.0.daemon().subscribe_host(cursor)
        }
        fn call(
            &self,
            method: &str,
            params: serde_json::Value,
        ) -> std::result::Result<serde_json::Value, mote_ipc::RpcError> {
            self.0.daemon().call(method, params)
        }
    }

    #[test]
    fn saved_host_recovers_after_unavailable_app_launch() {
        let models = ScriptedServer::start().unwrap();
        let mut host = moted::test_fixture::start(&models).unwrap();
        let store = Arc::new(Mutex::new(MemoryStore::default()));
        let offer = host.open_pairing().unwrap();
        let paired = std::thread::scope(|scope| {
            let pair = scope.spawn(|| {
                NetworkHost::pair(
                    host.endpoint().unwrap(),
                    GatewayConfig::new(host.root()),
                    &offer,
                    "iPad",
                    store.clone(),
                )
            });
            while !pair.is_finished() {
                host.confirm_pending(true).unwrap();
                std::thread::sleep(Duration::from_millis(5));
            }
            pair.join().unwrap().unwrap()
        });
        drop(paired);
        let id = host.host_id().clone();
        let endpoint = host.endpoint().unwrap();
        let config = HostConfig {
            name: "Saved host".into(),
            public_key: *host.public_key(),
            endpoint,
            offer: None,
        };
        let mut gateway_config = GatewayConfig::new(host.root().join("data"));
        gateway_config.bind = endpoint;
        host.suspend();
        let root = tempfile::tempdir().unwrap();
        let (updates, received) = mpsc::channel();
        let (commands, command_rx) = mpsc::channel();
        let worker = HostWorker {
            configs: Arc::new(Mutex::new(vec![config.clone()])),
            config,
            cursor: Cursor::new(id.clone(), 0).unwrap(),
            select: true,
            commands: Some(command_rx),
            updates,
            store,
            config_path: root.path().join("live-hosts.json"),
            ctx: egui::Context::default(),
        };
        let worker = std::thread::spawn(move || worker.run());
        loop {
            let update = received.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(update.host_id(), &id);
            if matches!(
                update,
                HostUpdate::Connection {
                    state: ConnectionState::Offline,
                    ..
                }
            ) {
                break;
            }
        }
        let dispatch = Arc::new(Forward(host));
        let resumed = GatewayServer::bind_loopback(gateway_config, dispatch.clone()).unwrap();
        loop {
            let update = received.recv_timeout(Duration::from_secs(8)).unwrap();
            assert_eq!(update.host_id(), &id);
            if matches!(
                update,
                HostUpdate::Frame {
                    frame: HostFrame::Snapshot { .. },
                    ..
                }
            ) {
                break;
            }
        }
        assert!(matches!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            HostUpdate::Connection {
                state: ConnectionState::Online,
                ..
            }
        ));
        commands
            .send(HostCommand {
                host_id: id.clone(),
                frame: ClientFrame::Call {
                    request_id: RequestId::new("first-command-after-startup-recovery").unwrap(),
                    method: "ping".into(),
                    params: serde_json::json!({}),
                },
            })
            .unwrap();
        loop {
            let update = received.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(update.host_id(), &id);
            if matches!(
                update,
                HostUpdate::Frame {
                    frame: HostFrame::Result { .. },
                    ..
                }
            ) {
                break;
            }
        }
        drop(commands);
        drop(received);
        dispatch.call("matrix.render", serde_json::json!({"thread":"main","source":"screen { text \"End recovery test\" }"})).unwrap();
        worker.join().unwrap();
        drop(resumed);
    }
}
