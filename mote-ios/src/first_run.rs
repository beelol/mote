use anyhow::{ensure, Result};
use mote_gateway::{
    discovery::{DiscoveryBrowser, DiscoveryRecord, EndpointCandidate},
    ClientCredential, GatewayConfig,
};
use mote_host_client::ConnectionState;
use mote_protocol::HostId;
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    time::Duration,
};

pub fn validated_host(record: &DiscoveryRecord, code: &str) -> Result<crate::live::HostConfig> {
    ensure!(
        mote_gateway::net::is_local_range(record.endpoint.ip()),
        "discovered host is not local"
    );
    let credential = ClientCredential::from_pairing_code(&GatewayConfig::new("."), code.trim())?;
    EndpointCandidate::new(record.clone(), &credential)?;
    Ok(crate::live::HostConfig {
        name: format!("Mote host {}", &record.fingerprint[..8]),
        public_key: *credential.host_public_key(),
        endpoint: record.endpoint,
        offer: Some(code.trim().to_owned()),
    })
}

pub struct FirstRun {
    records: Vec<DiscoveryRecord>,
    selected: Option<usize>,
    code: String,
    status: String,
    browse: Option<Receiver<Result<Vec<DiscoveryRecord>>>>,
    pending: Option<crate::live::LiveRuntime>,
    pending_host: Option<HostId>,
    mode: PairingMode,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum PairingMode {
    #[default]
    First,
    Additional,
}

impl Default for FirstRun {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            selected: None,
            code: String::new(),
            status: "Searching for local Mote hosts".into(),
            browse: None,
            pending: None,
            pending_host: None,
            mode: PairingMode::First,
        }
    }
}

impl FirstRun {
    pub fn new(ctx: &egui::Context) -> Self {
        let mut panel = Self::default();
        panel.refresh(ctx);
        panel
    }

    pub fn additional(ctx: &egui::Context) -> Self {
        let mut panel = Self {
            mode: PairingMode::Additional,
            ..Self::default()
        };
        panel.refresh(ctx);
        panel
    }

    fn refresh(&mut self, ctx: &egui::Context) {
        if self.browse.is_some() || self.pending.is_some() {
            return;
        }
        self.status = "Searching for local Mote hosts".into();
        let (send, receive) = mpsc::channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::discovery::SimulatorBrowser.browse(Duration::from_secs(3));
            let _ = send.send(result);
            repaint.request_repaint();
        });
        self.browse = Some(receive);
    }

    pub fn poll(&mut self) -> Option<crate::live::LiveRuntime> {
        if let Some(receiver) = self.browse.take() {
            match receiver.try_recv() {
                Ok(Ok(mut records)) => {
                    records.sort_by(|left, right| {
                        left.fingerprint
                            .cmp(&right.fingerprint)
                            .then_with(|| left.endpoint.cmp(&right.endpoint))
                    });
                    records.dedup();
                    self.records = records;
                    self.selected = (self.records.len() == 1).then_some(0);
                    self.status = if self.records.is_empty() {
                        "No local Mote hosts found. Check that the Mac gateway is running, then refresh".into()
                    } else {
                        "Select a host and enter its MOTE1 pairing code".into()
                    };
                }
                Ok(Err(_)) | Err(TryRecvError::Disconnected) => {
                    self.status =
                        "Local host search failed. Check local-network access and refresh".into();
                }
                Err(TryRecvError::Empty) => self.browse = Some(receiver),
            }
        }
        let mut runtime = self.pending.take()?;
        runtime.drain();
        let Some(host) = self.pending_host.as_ref() else {
            self.pending = Some(runtime);
            return None;
        };
        let state = runtime.directory.state(host);
        match state.map(|state| state.connection()) {
            Ok(ConnectionState::Online) => {
                self.code.clear();
                self.pending_host = None;
                Some(runtime)
            }
            Ok(ConnectionState::Connecting) => {
                self.pending = Some(runtime);
                None
            }
            Ok(ConnectionState::Offline)
            | Ok(ConnectionState::Incompatible { .. })
            | Ok(ConnectionState::IdentityChanged)
            | Err(_) => {
                self.status =
                    "Pairing did not complete. Confirm the code on the Mac and try again".into();
                self.code.clear();
                self.pending_host = None;
                None
            }
        }
    }

    pub fn draw(&mut self, ctx: &egui::Context, safe: egui::Margin) -> bool {
        let mut view_gallery = false;
        let mut header = egui::Frame::side_top_panel(&ctx.style());
        header.inner_margin.top = header.inner_margin.top.saturating_add(safe.top);
        egui::TopBottomPanel::top("first-run-header")
            .frame(header)
            .show(ctx, |ui| {
                ui.heading("Connect to a Mote host");
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(16.0);
            self.label(
                ui,
                "instructions",
                "The Mac must be on this local network with its Mote gateway enabled.",
            );
            let refresh = ui
                .add_enabled_ui(self.browse.is_none() && self.pending.is_none(), |ui| {
                    ui.add_sized([120.0, 48.0], egui::Button::new("Refresh"))
                })
                .inner;
            self.control(
                ui,
                &refresh,
                "Refresh",
                mote_client_core::native_accessibility::Role::Button,
                None,
            );
            if refresh.clicked() {
                self.refresh(ctx);
            }
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for (index, record) in self.records.iter().enumerate() {
                        let label = format!(
                            "Mote host {} at {}",
                            &record.fingerprint[..8],
                            record.endpoint
                        );
                        let response = ui.add_sized(
                            [ui.available_width(), 48.0],
                            egui::Button::new(&label).selected(self.selected == Some(index)),
                        );
                        self.control(
                            ui,
                            &response,
                            &label,
                            mote_client_core::native_accessibility::Role::Button,
                            None,
                        );
                        if response.clicked() {
                            self.selected = Some(index);
                        }
                    }
                });
            ui.label("MOTE1 pairing code");
            let field = mote_client_core::native_text::singleline(
                ui,
                egui::Id::new("first-run-pairing-code"),
                &mut self.code,
                egui::vec2(ui.available_width(), 48.0),
                "MOTE1 pairing code",
                true,
            );
            self.control(
                ui,
                &field,
                "MOTE1 pairing code",
                mote_client_core::native_accessibility::Role::TextField,
                None,
            );
            let pair = ui
                .add_enabled_ui(
                    self.selected.is_some()
                        && !self.code.trim().is_empty()
                        && self.pending.is_none(),
                    |ui| ui.add_sized([120.0, 48.0], egui::Button::new("Pair")),
                )
                .inner;
            self.control(
                ui,
                &pair,
                "Pair",
                mote_client_core::native_accessibility::Role::Button,
                None,
            );
            if pair.clicked() {
                let record = &self.records[self
                    .selected
                    .expect("enabled pair requires a selected host")];
                let runtime = validated_host(record, &self.code).and_then(|host| {
                    let pending_host = HostId::from_public_key(&host.public_key);
                    let runtime = match self.mode {
                        PairingMode::First => {
                            crate::live::LiveRuntime::from_first_pairing(ctx.clone(), host)
                        }
                        PairingMode::Additional => {
                            crate::live::LiveRuntime::from_additional_pairing(ctx.clone(), host)
                        }
                    }?;
                    Ok((runtime, pending_host))
                });
                match runtime {
                    Ok((runtime, pending_host)) => {
                        self.pending = Some(runtime);
                        self.pending_host = Some(pending_host);
                        self.status = "Waiting for pairing confirmation on the Mac".into();
                    }
                    Err(_) => {
                        self.status =
                            "The code does not match the selected Mote host or cannot be used"
                                .into()
                    }
                }
            }
            self.label(ui, "status", &self.status);
            let exit_label = if self.mode == PairingMode::Additional {
                "Cancel"
            } else {
                "View offline gallery"
            };
            let gallery = ui
                .add_enabled_ui(self.pending.is_none(), |ui| {
                    ui.add_sized([ui.available_width(), 48.0], egui::Button::new(exit_label))
                })
                .inner;
            self.control(
                ui,
                &gallery,
                exit_label,
                mote_client_core::native_accessibility::Role::Button,
                None,
            );
            view_gallery = gallery.clicked();
        });
        view_gallery
    }

    fn label(&self, ui: &mut egui::Ui, key: &'static str, text: &str) {
        let response = ui.label(text);
        self.control(
            ui,
            &response,
            text,
            mote_client_core::native_accessibility::Role::Label,
            Some(key),
        );
    }

    fn control(
        &self,
        ui: &egui::Ui,
        response: &egui::Response,
        label: &str,
        role: mote_client_core::native_accessibility::Role,
        key: Option<&'static str>,
    ) {
        #[cfg(any(test, target_os = "ios"))]
        mote_client_core::native_accessibility::record(
            ui,
            key.map_or(response.id, |key| ui.make_persistent_id(("first-run", key))),
            role,
            label,
            None,
            response,
        );
        #[cfg(not(any(test, target_os = "ios")))]
        let _ = (ui, response, label, role, key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_gateway::{discovery::DiscoveryRecord, HostIdentity, PairingOffer};
    use std::time::Instant;

    #[test]
    fn first_run_draws_native_connection_controls_without_a_saved_host() {
        let ctx = egui::Context::default();
        let mut first_run = FirstRun::default();
        mote_client_core::native_accessibility::begin_frame();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(834.0, 1210.0),
                )),
                ..Default::default()
            },
            |ctx| {
                first_run.draw(ctx, egui::Margin::ZERO);
            },
        );
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        for expected in [
            "Connect to a Mote host",
            "Refresh",
            "Pair",
            "MOTE1 pairing code",
            "View offline gallery",
        ] {
            assert!(
                texts.iter().any(|text| text.contains(expected)),
                "missing {expected}"
            );
        }
        let controls = mote_client_core::native_accessibility::take_frame();
        for label in [
            "Refresh",
            "Pair",
            "MOTE1 pairing code",
            "View offline gallery",
        ] {
            assert!(controls.iter().any(|control| control.label == label));
        }
        let code = controls
            .iter()
            .find(|control| control.label == "MOTE1 pairing code")
            .unwrap();
        assert_eq!(
            code.role,
            mote_client_core::native_accessibility::Role::TextField
        );
        assert_eq!(code.id, egui::Id::new("first-run-pairing-code").value());
    }

    #[test]
    fn additional_pairing_exposes_cancel_instead_of_offline_gallery() {
        let ctx = egui::Context::default();
        let mut first_run = FirstRun {
            mode: PairingMode::Additional,
            ..FirstRun::default()
        };
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(834.0, 1210.0),
                )),
                ..Default::default()
            },
            |ctx| {
                first_run.draw(ctx, egui::Margin::ZERO);
            },
        );
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|text| text == "Cancel"));
        assert!(!texts.iter().any(|text| text == "View offline gallery"));
    }

    #[test]
    fn additional_pairing_waits_for_the_new_host_not_an_existing_online_host() {
        use mote_host_client::{HostDirectory, HostUpdate, PairedHost};
        use mote_protocol::VersionRange;
        let home = PairedHost::new([1; 32], "Home", None, VersionRange::current()).unwrap();
        let office = PairedHost::new([2; 32], "Office", None, VersionRange::current()).unwrap();
        let home_id = home.host_id().clone();
        let office_id = office.host_id().clone();
        let mut directory = HostDirectory::default();
        directory.register(home).unwrap();
        directory.register(office).unwrap();
        directory.select_host(&home_id).unwrap();
        directory
            .reduce_update(HostUpdate::Connection {
                host_id: home_id,
                state: ConnectionState::Online,
            })
            .unwrap();
        directory
            .reduce_update(HostUpdate::Connection {
                host_id: office_id.clone(),
                state: ConnectionState::Connecting,
            })
            .unwrap();
        let (persist, rx) = std::sync::mpsc::channel();
        let runtime = crate::live::LiveRuntime {
            directory,
            rx,
            persist,
            commands: Default::default(),
            status: Default::default(),
            approval_requests: Default::default(),
            authenticated: true,
        };
        let mut first_run = FirstRun {
            mode: PairingMode::Additional,
            pending: Some(runtime),
            pending_host: Some(office_id),
            ..FirstRun::default()
        };
        assert!(first_run.poll().is_none());
        assert!(first_run.pending.is_some());
    }

    #[test]
    fn selected_bonjour_fingerprint_must_match_pairing_offer() {
        let root = tempfile::tempdir().unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = mote_gateway::GatewayConfig::new(root.path());
        let identity = HostIdentity::load_or_create(root.path()).unwrap();
        let code = PairingOffer::open(&config, &identity, Instant::now(), &[])
            .unwrap()
            .code();
        let matching = DiscoveryRecord::new(
            "192.168.1.20:47810".parse().unwrap(),
            mote_protocol::PROTOCOL_MAJOR,
            identity.public_key(),
        )
        .unwrap();
        let another = DiscoveryRecord::new(
            "192.168.1.20:47810".parse().unwrap(),
            mote_protocol::PROTOCOL_MAJOR,
            &[7; 32],
        )
        .unwrap();

        let host = validated_host(&matching, &code).unwrap();
        assert_eq!(host.public_key, *identity.public_key());
        assert_eq!(host.endpoint, matching.endpoint);
        assert_eq!(host.offer.as_deref(), Some(code.as_str()));
        assert!(!serde_json::to_string(&host).unwrap().contains("MOTE1-"));
        assert!(validated_host(&another, &code).is_err());
        assert!(validated_host(&matching, "MOTE1-invalid").is_err());
    }
}
