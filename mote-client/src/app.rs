//! The egui application: chat view and pipeline view.

use crate::pipeline::PipelineSnapshot;
use crate::state::{ChatState, LocalMessageId};
use crate::stream::StreamUpdate;
use eframe::egui;
use mote_eventlog::Event;
use mote_ipc::Client;
#[cfg(any(test, feature = "test-observation"))]
use mote_ui::Node;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

type TurnResult = (LocalMessageId, Result<(), String>);
type PipelineResult = Result<PipelineSnapshot, String>;
type UiEventResult = (crate::surface::SurfaceEvent, Result<(), String>);

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConfigSetRequest {
    key: String,
    value: serde_json::Value,
    revision: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct ConfigSetOutcome {
    request: ConfigSetRequest,
    result: Result<(), String>,
}

type ConfigBatch = Vec<ConfigSetRequest>;
type ConfigBatchResult = Vec<ConfigSetOutcome>;

fn ui_event_params(event: &crate::surface::SurfaceEvent) -> serde_json::Value {
    let mut params = serde_json::json!({
        "thread": "main",
        "event": event.event(),
        "args": event.args(),
    });
    if let Some(seq) = event.context().surface_seq {
        params["surface_seq"] = serde_json::json!(seq);
        params["node_path"] = serde_json::json!(event.context().node_path);
    }
    params
}

fn preview_status(event: &crate::surface::SurfaceEvent) -> String {
    match event {
        crate::surface::SurfaceEvent::Action { event, .. } => {
            format!("Preview action: {event}")
        }
        crate::surface::SurfaceEvent::Change { field, value, .. } => {
            format!("Preview change: {field} = {value}")
        }
    }
}

fn spawn_pipeline_load_with(
    tx: Sender<PipelineResult>,
    load: impl FnOnce() -> PipelineResult + Send + 'static,
) {
    std::thread::spawn(move || {
        let _ = tx.send(load());
    });
}

fn spawn_config_worker_with(
    requests: Receiver<ConfigBatch>,
    results: Sender<ConfigBatchResult>,
    mut send: impl FnMut(&ConfigSetRequest) -> Result<(), String> + Send + 'static,
) {
    std::thread::spawn(move || {
        while let Ok(batch) = requests.recv() {
            let outcomes = batch
                .into_iter()
                .map(|request| {
                    let result = send(&request);
                    ConfigSetOutcome { request, result }
                })
                .collect();
            if results.send(outcomes).is_err() {
                break;
            }
        }
    });
}

fn spawn_config_worker(
    socket: PathBuf,
    requests: Receiver<ConfigBatch>,
    results: Sender<ConfigBatchResult>,
) {
    spawn_config_worker_with(requests, results, move |request| {
        let mut client =
            Client::connect(&socket).map_err(|error| format!("cannot reach daemon: {error}"))?;
        client
            .call(
                "config.set",
                serde_json::json!({ "key": request.key, "value": request.value }),
            )
            .map_err(|error| format!("cannot set {}: {error}", request.key))?
            .map(|_| ())
            .map_err(|error| error.message)
    });
}

fn spawn_ui_event_worker_with(
    requests: Receiver<crate::surface::SurfaceEvent>,
    results: Sender<UiEventResult>,
    mut send: impl FnMut(&crate::surface::SurfaceEvent) -> Result<(), String> + Send + 'static,
) {
    std::thread::spawn(move || {
        while let Ok(event) = requests.recv() {
            let result = send(&event);
            if results.send((event, result)).is_err() {
                break;
            }
        }
    });
}

fn spawn_ui_event_worker(
    socket: PathBuf,
    requests: Receiver<crate::surface::SurfaceEvent>,
    results: Sender<UiEventResult>,
) {
    spawn_ui_event_worker_with(requests, results, move |event| {
        let mut client =
            Client::connect(&socket).map_err(|error| format!("cannot reach daemon: {error}"))?;
        client
            .call("ui.event", ui_event_params(event))
            .map_err(|error| format!("cannot send UI event: {error}"))?
            .map(|_| ())
            .map_err(|error| format!("{}: {}", error.code, error.message))
    });
}

fn apply_next_event(
    chat: &mut ChatState,
    rx: &Receiver<StreamUpdate>,
    cursor_path: Option<&std::path::Path>,
) -> Option<(Event, Option<crate::state::SurfaceReduction>, u64, std::io::Result<()>)> {
    let (event, arrived_ns) = loop {
        match rx.try_recv().ok()? {
            StreamUpdate::Event { event, arrived_ns } => break (event, arrived_ns),
            StreamUpdate::Connected { replay_through } => chat.connect_matrix(replay_through),
            StreamUpdate::Disconnected => chat.invalidate_matrix(),
        }
    };
    let reduction = chat.reduce(&event);
    let persisted = cursor_path.map_or(Ok(()), |path| crate::cursor::store(path, event.seq));
    Some((event, reduction, arrived_ns, persisted))
}

#[derive(PartialEq)]
enum Screen {
    Chat,
    Pipeline,
}

const STAGES: [&str; 5] = ["talk", "router", "think", "memory", "ledger"];
const TABS: [&str; 4] = ["Now", "Config", "Numbers", "Requests"];
const CONFIG_KEYS: [&str; 9] = [
    "models.local_only",
    "models.talk",
    "models.router",
    "models.think",
    "budgets.talk_first_token",
    "budgets.router",
    "budgets.think_first_token",
    "packs.talk",
    "packs.think",
];
const LIVE_EVENT_BUFFER_LIMIT: usize = 200;

pub struct MoteApp {
    socket: PathBuf,
    rx: Receiver<StreamUpdate>,
    chat: ChatState,
    input: String,
    screen: Screen,
    stage: usize,
    tab: usize,
    config_edits: std::collections::HashMap<String, String>,
    config_baseline: std::collections::HashMap<String, String>,
    config_revision: u64,
    config_edit_revisions: std::collections::HashMap<String, u64>,
    config_submitted: std::collections::HashMap<String, ConfigSetRequest>,
    config_pending: bool,
    config_error: Option<String>,
    config_tx: Sender<ConfigBatch>,
    config_rx: Receiver<ConfigBatchResult>,
    pipeline: Option<PipelineSnapshot>,
    pipeline_loading: bool,
    pipeline_live_events: Vec<Event>,
    pipeline_tx: Sender<PipelineResult>,
    pipeline_rx: Receiver<PipelineResult>,
    status_line: String,
    fixture: Option<crate::contract::Surface>,
    turn_tx: Sender<TurnResult>,
    turn_rx: Receiver<TurnResult>,
    ui_event_tx: Sender<crate::surface::SurfaceEvent>,
    ui_event_rx: Receiver<UiEventResult>,
    cursor_path: Option<PathBuf>,
    cursor_issue: Option<String>,
    cursor_halted: bool,
    clock: crate::display_latency::SharedClock,
    display_latency: crate::display_latency::DisplayLatency,
    latency_report_path: Option<PathBuf>,
}

impl MoteApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        socket: PathBuf,
        rx: Receiver<StreamUpdate>,
        fixture: Option<crate::contract::Surface>,
        clock: crate::display_latency::SharedClock,
        cursor_warning: Option<String>,
    ) -> Self {
        crate::theme::install(&cc.egui_ctx);
        let (turn_tx, turn_rx) = mpsc::channel();
        let (pipeline_tx, pipeline_rx) = mpsc::channel();
        let (config_tx, config_requests) = mpsc::channel();
        let (config_results, config_rx) = mpsc::channel();
        spawn_config_worker(socket.clone(), config_requests, config_results);
        let (ui_event_tx, ui_event_requests) = mpsc::channel();
        let (ui_event_results, ui_event_rx) = mpsc::channel();
        spawn_ui_event_worker(socket.clone(), ui_event_requests, ui_event_results);
        Self {
            socket,
            rx,
            chat: ChatState::default(),
            input: String::new(),
            screen: Screen::Chat,
            stage: 0,
            tab: 0,
            config_edits: std::collections::HashMap::new(),
            config_baseline: std::collections::HashMap::new(),
            config_revision: 0,
            config_edit_revisions: std::collections::HashMap::new(),
            config_submitted: std::collections::HashMap::new(),
            config_pending: false,
            config_error: None,
            config_tx,
            config_rx,
            pipeline: None,
            pipeline_loading: false,
            pipeline_live_events: Vec::new(),
            pipeline_tx,
            pipeline_rx,
            status_line: String::new(),
            fixture,
            turn_tx,
            turn_rx,
            ui_event_tx,
            ui_event_rx,
            cursor_path: Some(crate::cursor::path()),
            cursor_issue: cursor_warning,
            cursor_halted: false,
            clock,
            display_latency: Default::default(),
            latency_report_path: std::env::var_os("MOTE_UI_LATENCY_REPORT").map(PathBuf::from),
        }
    }

    fn drain_events(&mut self) {
        if !self.cursor_halted {
            while let Some((event, reduction, arrived_ns, persisted)) =
                apply_next_event(&mut self.chat, &self.rx, self.cursor_path.as_deref())
            {
                if let Some(reduction) = reduction {
                    self.display_latency.accept(
                        reduction.revision,
                        event.seq,
                        reduction.superseded_revision,
                        arrived_ns,
                    );
                }
                self.remember_pipeline_event(event);
                if persisted.is_err() {
                    self.cursor_issue = Some(
                        "Cannot save the event position. Restart mote after fixing storage.".into(),
                    );
                    self.cursor_halted = true;
                    break;
                }
            }
        }
        while let Ok((pending, result)) = self.turn_rx.try_recv() {
            self.record_turn_result(pending, result);
        }
        while let Ok(result) = self.pipeline_rx.try_recv() {
            self.pipeline_loading = false;
            match result {
                Ok(snapshot) => self.install_pipeline_snapshot(snapshot),
                Err(error) => self.status_line = error,
            }
        }
        while let Ok(result) = self.config_rx.try_recv() {
            self.record_config_batch_result(result);
        }
    }

    fn drain_ui_event_results(&mut self, ctx: &egui::Context) {
        while let Ok((event, result)) = self.ui_event_rx.try_recv() {
            self.record_ui_event_result(ctx, event, result);
        }
    }

    fn record_ui_event_result(
        &mut self,
        ctx: &egui::Context,
        event: crate::surface::SurfaceEvent,
        result: Result<(), String>,
    ) {
        if let Err(error) = result {
            crate::surface::discard_change(ctx, &event);
            self.status_line = error;
        } else {
            self.status_line.clear();
        }
    }

    fn remember_pipeline_event(&mut self, event: Event) {
        if let Some(snapshot) = &mut self.pipeline {
            snapshot.ingest_live(event.clone());
        }
        if self.pipeline_live_events.iter().any(|existing| existing.seq == event.seq) {
            return;
        }
        self.pipeline_live_events.push(event);
        self.pipeline_live_events.sort_unstable_by(|left, right| right.seq.cmp(&left.seq));
        self.pipeline_live_events.truncate(LIVE_EVENT_BUFFER_LIMIT);
    }

    fn install_pipeline_snapshot(&mut self, mut snapshot: PipelineSnapshot) {
        for event in self.pipeline_live_events.iter().cloned() {
            snapshot.ingest_live(event);
        }
        for key in CONFIG_KEYS {
            if !self.config_edit_revisions.contains_key(key) {
                if let Some(value) = snapshot.config_edit_value(key) {
                    self.config_edits.insert(key.into(), value.clone());
                    self.config_baseline.insert(key.into(), value);
                }
            }
        }
        self.pipeline = Some(snapshot);
        self.status_line.clear();
    }

    fn request_pipeline_snapshot(&mut self) {
        if self.pipeline_loading {
            return;
        }
        self.pipeline_loading = true;
        let socket = self.socket.clone();
        let tx = self.pipeline_tx.clone();
        spawn_pipeline_load_with(tx, move || {
            let mut client = Client::connect(&socket)
                .map_err(|error| format!("cannot reach daemon: {error}"))?;
            let value = client
                .call("pipeline.snapshot", serde_json::json!({}))
                .map_err(|error| format!("cannot read pipeline: {error}"))?
                .map_err(|error| format!("{}: {}", error.code, error.message))?;
            PipelineSnapshot::from_value(value)
        });
    }

    fn record_turn_result(&mut self, pending: LocalMessageId, result: Result<(), String>) {
        match result {
            Ok(()) => self.chat.mark_sent(&pending),
            Err(error) => {
                if let Some(text) = self.chat.remove_failed(&pending) {
                    self.input = text;
                }
                self.status_line = error;
            }
        }
    }

    fn send_turn(&mut self) {
        let msg = self.input.trim().to_string();
        if msg.is_empty() {
            return;
        }
        self.input.clear();
        let pending = self.chat.push_optimistic_user(msg.clone());
        let request_id = pending.clone();
        let socket = self.socket.clone();
        let result_tx = self.turn_tx.clone();
        std::thread::spawn(move || {
            let result = match Client::connect(&socket) {
                Ok(mut client) => match client.call(
                    "turn",
                    serde_json::json!({
                        "thread": "main",
                        "message": msg,
                        "client_message_id": request_id
                    }),
                ) {
                    Ok(Ok(_)) => Ok(()),
                    Ok(Err(error)) => Err(format!("{}: {}", error.code, error.message)),
                    Err(error) => Err(format!("cannot send turn: {error}")),
                },
                Err(error) => Err(format!("cannot reach daemon: {error}")),
            };
            let _ = result_tx.send((pending, result));
        });
    }

    fn send_ui_event(&self, event: crate::surface::SurfaceEvent) {
        let _ = self.ui_event_tx.send(event);
    }

    fn preview_view(&mut self, ui: &mut egui::Ui) {
        if !self.status_line.is_empty() {
            crate::theme::caption(ui, &self.status_line);
        }
        let mut clicked = Vec::new();
        #[cfg(any(test, feature = "test-observation"))]
        let mut scroll_clip = egui::Rect::NOTHING;
        let scroll = egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                #[cfg(any(test, feature = "test-observation"))]
                { scroll_clip = ui.clip_rect(); }
                if let Some(node) = &self.fixture {
                    crate::surface::draw(ui, node.display_v0(), &mut clicked);
                }
            });
        crate::accessibility::hide_id(ui.ctx(), scroll.id.with(0));
        crate::accessibility::hide_id(ui.ctx(), scroll.id.with(1));
        crate::accessibility::hide_id(ui.ctx(), scroll.id.with("area"));
        #[cfg(any(test, feature = "test-observation"))]
        crate::surface::observation::scroll("preview", scroll_clip, &scroll);
        #[cfg(not(any(test, feature = "test-observation")))]
        let _ = scroll;
        if let Some(event) = clicked.last() {
            self.status_line = preview_status(event);
        }
    }

    fn chat_view(&mut self, ui: &mut egui::Ui) -> Vec<u64> {
        let mut clicked = Vec::new();
        let mut drawn_revisions = Vec::new();
        #[cfg(any(test, feature = "test-observation"))]
        let mut scroll_clip = egui::Rect::NOTHING;
        let scroll = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height((ui.available_height() - 48.0 - 3.0 * ui.spacing().item_spacing.y).max(0.0))
            .show(ui, |ui| {
                #[cfg(any(test, feature = "test-observation"))]
                { scroll_clip = ui.clip_rect(); }
                for m in &self.chat.messages {
                    ui.horizontal(|ui| {
                        ui.strong(&m.role);
                        crate::theme::caption(ui, m.status.label());
                    });
                    if let Some(node) = &m.surface {
                        let node = node.display_v0();
                        let first_event = clicked.len();
                        if m.surface_id.as_deref() == Some("matrix") {
                            ui.push_id(("matrix", self.chat.matrix_epoch), |ui| {
                                crate::surface::draw(ui, node, &mut clicked)
                            });
                        } else {
                            ui.push_id(("surface", m.surface_id.as_deref(), m.surface_instance), |ui| crate::surface::draw(ui, node, &mut clicked));
                        }
                        for event in &mut clicked[first_event..] {
                            event.set_surface_seq(m.surface_seq);
                        }
                        if let Some(revision) = m.render_revision {
                            drawn_revisions.push(revision);
                        }
                    } else {
                        ui.label(&m.text);
                    }
                    ui.separator();
                }
                if self.chat.typing {
                    crate::theme::caption(ui, "mote is typing...");
                }
            });
        crate::accessibility::hide_id(ui.ctx(), scroll.id.with(0));
        crate::accessibility::hide_id(ui.ctx(), scroll.id.with(1));
        crate::accessibility::hide_id(ui.ctx(), scroll.id.with("area"));
        for e in clicked {
            self.send_ui_event(e);
        }
        #[cfg(any(test, feature = "test-observation"))]
        crate::surface::observation::scroll("chat", scroll_clip, &scroll);
        #[cfg(not(any(test, feature = "test-observation")))]
        let _ = scroll;

        ui.separator();
        ui.horizontal(|ui| {
            let resp = crate::surface::compose(ui, &mut self.input);
            #[cfg(any(test, feature = "test-observation"))]
            crate::surface::observation::control(ui, "Composer", &[], &resp);
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let send = crate::surface::button(ui, "Send");
            #[cfg(any(test, feature = "test-observation"))]
            crate::surface::observation::control(ui, "Send", &[], &send);
            if send.clicked() || enter {
                self.send_turn();
            }
        });
        if !self.status_line.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.status_line);
        }
        if let Some(issue) = &self.cursor_issue {
            ui.colored_label(ui.visuals().error_fg_color, issue);
        }
        drawn_revisions
    }

    fn pipeline_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (i, s) in STAGES.iter().enumerate() {
                if crate::theme::selectable_label(ui, self.stage == i, s).clicked() {
                    self.stage = i;
                }
            }
            ui.separator();
            if crate::surface::button(ui, "Refresh").clicked() {
                self.request_pipeline_snapshot();
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            for (i, t) in TABS.iter().enumerate() {
                if crate::theme::selectable_label(ui, self.tab == i, t).clicked() {
                    self.tab = i;
                }
            }
        });
        ui.separator();
        match TABS[self.tab] {
            "Config" => self.config_tab(ui),
            "Now" => self.now_tab(ui),
            "Numbers" => self.numbers_tab(ui),
            "Requests" => self.requests_tab(ui),
            _ => {}
        }
        if self.pipeline_loading {
            crate::theme::caption(ui, "Refreshing pipeline data...");
        }
        if !self.status_line.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.status_line);
        }
        if let Some(issue) = &self.cursor_issue {
            ui.colored_label(ui.visuals().error_fg_color, issue);
        }
    }

    fn config_tab(&mut self, ui: &mut egui::Ui) {
        if self.config_edits.is_empty() {
            ui.label("No configuration snapshot is available.");
            return;
        }
        for key in CONFIG_KEYS {
            let mut value = self.config_edits.get(key).cloned().unwrap_or_default();
            let changed = ui.horizontal(|ui| {
                ui.label(key);
                ui.text_edit_singleline(&mut value).changed()
            }).inner;
            if changed {
                self.set_config_edit(key, &value);
            }
        }
        let clicked = ui
            .add_enabled_ui(!self.pipeline_loading && !self.config_pending, |ui| {
                crate::surface::button(ui, "Apply").clicked()
            })
            .inner;
        if clicked {
            self.apply_config_edits();
        }
        if let Some(error) = &self.config_error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }

    fn now_tab(&self, ui: &mut egui::Ui) {
        let stage = STAGES[self.stage];
        let Some(snapshot) = &self.pipeline else {
            ui.label("No pipeline snapshot is available.");
            return;
        };
        crate::theme::caption(
            ui,
            &format!(
                "Newest {} stored events. Snapshot at {}.",
                snapshot.event_limit, snapshot.generated_at
            ),
        );
        crate::theme::caption(
            ui,
            "Main-thread events update live. Other scopes update on Refresh.",
        );
        let events = snapshot.events_for(stage);
        if events.is_empty() {
            ui.label(format!(
                "No {stage} row is present in this newest {}-event snapshot.",
                snapshot.event_limit
            ));
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for event in events {
                ui.horizontal(|ui| {
                    ui.strong(format!("#{} {}", event.seq, event.kind));
                    crate::theme::caption(ui, &format!("{} · {}", event.ts, event.scope_id));
                });
                ui.monospace(event.payload.to_string());
                ui.separator();
            }
        });
    }

    fn numbers_tab(&self, ui: &mut egui::Ui) {
        let stage = STAGES[self.stage];
        let Some(snapshot) = &self.pipeline else {
            ui.label("No pipeline snapshot is available.");
            return;
        };
        let Some(numbers) = snapshot.numbers_for(stage) else {
            ui.label(format!("No {stage} ledger data was stored in the last hour."));
            return;
        };
        crate::theme::caption(
            ui,
            &format!("Last {} minutes.", snapshot.window_ms / 60_000),
        );
        egui::Grid::new("pipeline_numbers").striped(true).show(ui, |ui| {
            ui.label("Calls");
            ui.label(numbers.calls.to_string());
            ui.end_row();
            let metric = match numbers.latency_metric.as_str() {
                "first_token" => "First-token latency",
                "total" => "Total latency",
                _ => "Latency",
            };
            ui.label(format!("{metric} samples"));
            ui.label(numbers.latency_samples.to_string());
            ui.end_row();
            ui.label(format!("{metric} p50"));
            ui.label(
                numbers
                    .latency_p50_ms
                    .map_or_else(|| "unavailable".into(), |ms| format!("{ms} ms")),
            );
            ui.end_row();
            ui.label(format!("{metric} p95"));
            ui.label(
                numbers
                    .latency_p95_ms
                    .map_or_else(|| "unavailable".into(), |ms| format!("{ms} ms")),
            );
            ui.end_row();
            ui.label("Budget");
            ui.label(numbers.budget_ms.map_or_else(|| "unavailable".into(), |ms| format!("{ms} ms")));
            ui.end_row();
            ui.label("Prompt tokens");
            ui.label(numbers.prompt_tokens.to_string());
            ui.end_row();
            ui.label("Completion tokens");
            ui.label(numbers.completion_tokens.to_string());
            ui.end_row();
            ui.label("Cost");
            ui.label(format!("${:.6}", numbers.cost_usd));
            ui.end_row();
        });
    }

    fn requests_tab(&self, ui: &mut egui::Ui) {
        let stage = STAGES[self.stage];
        let Some(snapshot) = &self.pipeline else {
            ui.label("No pipeline snapshot is available.");
            return;
        };
        let requests = snapshot.requests_for(stage);
        if requests.is_empty() {
            ui.label(format!("No retained {stage} outbound requests were found."));
            return;
        }
        crate::theme::caption(
            ui,
            &format!(
                "Retained outbound requests. Storage bound: {} rows.",
                snapshot.request_limit
            ),
        );
        egui::ScrollArea::vertical().show(ui, |ui| {
            for request in requests {
                ui.horizontal(|ui| {
                    ui.strong(format!("#{} {}", request.seq, request.lane));
                    let timing = match (request.first_token_ms, request.total_latency_ms) {
                        (Some(first), Some(total)) => {
                            format!("first token {first} ms · total {total} ms")
                        }
                        (None, Some(total)) => format!("total {total} ms"),
                        _ => "timing unavailable".into(),
                    };
                    let timing = request
                        .status
                        .as_ref()
                        .map_or(timing.clone(), |status| format!("{timing} · {status}"));
                    crate::theme::caption(ui, &format!("{} · {timing}", request.ts));
                });
                ui.monospace(request.payload.to_string());
                ui.separator();
            }
        });
    }

    fn set_config_edit(&mut self, key: &str, value: &str) {
        self.config_edits.insert(key.into(), value.into());
        self.record_config_edit(key);
    }

    fn record_config_edit(&mut self, key: &str) {
        self.config_revision = self.config_revision.saturating_add(1);
        if self.config_submitted.contains_key(key) {
            self.config_edit_revisions.insert(key.into(), self.config_revision);
        } else if self.config_edits.get(key) == self.config_baseline.get(key) {
            self.config_edit_revisions.remove(key);
        } else {
            self.config_edit_revisions.insert(key.into(), self.config_revision);
        }
    }

    fn config_value_text(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(value) => value.clone(),
            value => value.to_string(),
        }
    }

    fn recompute_config_dirty(&mut self, key: &str) {
        if self.config_edits.get(key) == self.config_baseline.get(key) {
            self.config_edit_revisions.remove(key);
        } else if !self.config_edit_revisions.contains_key(key) {
            self.config_revision = self.config_revision.saturating_add(1);
            self.config_edit_revisions.insert(key.into(), self.config_revision);
        }
    }

    fn parse_config_value(key: &str, text: &str) -> Result<serde_json::Value, String> {
        match key {
            "models.local_only" => text
                .parse::<bool>()
                .map(serde_json::Value::Bool)
                .map_err(|_| "expected true or false".into()),
            "models.talk" | "models.router" | "models.think" => {
                Ok(serde_json::Value::String(text.into()))
            }
            "budgets.talk_first_token" | "budgets.router" | "budgets.think_first_token"
            | "packs.talk" | "packs.think" => text
                .parse::<i64>()
                .map(serde_json::Value::from)
                .map_err(|_| "expected an integer".into()),
            _ => Err("unknown configuration key".into()),
        }
    }

    fn apply_config_edits(&mut self) -> bool {
        if self.pipeline_loading || self.config_pending {
            return false;
        }
        let mut batch = Vec::new();
        let mut errors = Vec::new();
        for key in CONFIG_KEYS {
            let Some(&revision) = self.config_edit_revisions.get(key) else {
                continue;
            };
            let text = self.config_edits.get(key).map(String::as_str).unwrap_or_default();
            match Self::parse_config_value(key, text) {
                Ok(value) => batch.push(ConfigSetRequest { key: key.into(), value, revision }),
                Err(error) => errors.push(format!("{key}: {error}")),
            }
        }
        self.config_error = (!errors.is_empty()).then(|| errors.join("\n"));
        if batch.is_empty() {
            return false;
        }
        if self.config_tx.send(batch.clone()).is_err() {
            self.config_error = Some("configuration worker stopped".into());
            return false;
        }
        self.config_submitted = batch
            .into_iter()
            .map(|request| (request.key.clone(), request))
            .collect();
        self.config_pending = true;
        true
    }

    fn record_config_batch_result(&mut self, outcomes: ConfigBatchResult) {
        self.config_pending = false;
        let prior_error = self.config_error.take();
        let mut succeeded = false;
        let mut errors = Vec::new();
        for outcome in outcomes {
            self.config_submitted.remove(&outcome.request.key);
            match outcome.result {
                Ok(()) => {
                    succeeded = true;
                    let request = outcome.request;
                    let value = Self::config_value_text(&request.value);
                    self.config_baseline.insert(request.key.clone(), value.clone());
                    let submitted_revision_is_current = self
                        .config_edit_revisions
                        .get(&request.key)
                        .is_some_and(|revision| *revision == request.revision);
                    if submitted_revision_is_current {
                        self.config_edits.insert(request.key.clone(), value);
                    }
                    self.recompute_config_dirty(&request.key);
                }
                Err(error) => {
                    self.recompute_config_dirty(&outcome.request.key);
                    errors.push(format!("{}: {error}", outcome.request.key));
                }
            }
        }
        self.config_submitted.clear();
        self.config_error = if errors.is_empty() {
            prior_error
        } else if let Some(prior) = prior_error {
            Some(format!("{prior}\n{}", errors.join("\n")))
        } else {
            Some(errors.join("\n"))
        };
        if succeeded {
            self.request_pipeline_snapshot();
        }
    }
}

impl MoteApp {
    fn draw_shell(&mut self, ctx: &egui::Context) -> Vec<u64> {
        if self.fixture.is_some() {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(ctx.style().visuals.panel_fill)
                        .inner_margin(16),
                )
                .show(ctx, |ui| {
                    self.preview_view(ui);
                });
            return Vec::new();
        }

        egui::TopBottomPanel::top("top")
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(16),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if crate::theme::selectable_label(ui, self.screen == Screen::Chat, "Chat")
                        .clicked()
                    {
                        self.screen = Screen::Chat;
                    }
                    if crate::theme::selectable_label(
                        ui,
                        self.screen == Screen::Pipeline,
                        "Pipeline",
                    )
                    .clicked()
                    {
                        self.screen = Screen::Pipeline;
                        self.request_pipeline_snapshot();
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(16),
            )
            .show(ctx, |ui| match self.screen {
                Screen::Chat => self.chat_view(ui),
                Screen::Pipeline => {
                    self.pipeline_view(ui);
                    Vec::new()
                }
            })
            .inner
    }

    fn draw_frame(&mut self, ctx: &egui::Context) {
        let revisions = self.draw_shell(ctx);
        self.display_latency.drawn(&revisions, self.clock.now_ns());
    }
}

impl eframe::App for MoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.drain_ui_event_results(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
        self.draw_frame(ctx);
    }
}

impl Drop for MoteApp {
    fn drop(&mut self) {
        let Some(path) = &self.latency_report_path else {
            return;
        };
        if let Err(error) = self.display_latency.write_report(path) {
            eprintln!("cannot write UI latency report {}: {error}", path.display());
        }
    }
}

#[cfg(any(test, feature = "test-observation"))]
#[path = "observation.rs"]
pub mod observation;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Status;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct ManualClock(AtomicU64);

    impl crate::display_latency::Clock for ManualClock {
        fn now_ns(&self) -> u64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    impl ManualClock {
        fn set(&self, value: u64) {
            self.0.store(value, Ordering::SeqCst);
        }
    }

    fn app() -> (MoteApp, std::sync::mpsc::Sender<StreamUpdate>) {
        app_with_clock(crate::display_latency::InstantClock::shared())
    }

    fn app_with_clock(
        clock: crate::display_latency::SharedClock,
    ) -> (MoteApp, std::sync::mpsc::Sender<StreamUpdate>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let (turn_tx, turn_rx) = std::sync::mpsc::channel();
        let (pipeline_tx, pipeline_rx) = std::sync::mpsc::channel();
        let (config_tx, config_requests) = std::sync::mpsc::channel();
        let (config_results, config_rx) = std::sync::mpsc::channel();
        spawn_config_worker(PathBuf::new(), config_requests, config_results);
        let (ui_event_tx, ui_event_requests) = std::sync::mpsc::channel();
        let (ui_event_results, ui_event_rx) = std::sync::mpsc::channel();
        spawn_ui_event_worker(PathBuf::new(), ui_event_requests, ui_event_results);
        (
            MoteApp {
                socket: PathBuf::new(),
                rx,
                chat: ChatState::default(),
                input: String::new(),
                screen: Screen::Chat,
                stage: 0,
                tab: 0,
                config_edits: Default::default(),
                config_baseline: Default::default(),
                config_revision: 0,
                config_edit_revisions: Default::default(),
                config_submitted: Default::default(),
                config_pending: false,
                config_error: None,
                config_tx,
                config_rx,
                pipeline: None,
                pipeline_loading: false,
                pipeline_live_events: Vec::new(),
                pipeline_tx,
                pipeline_rx,
                status_line: String::new(),
                fixture: None,
                turn_tx,
                turn_rx,
                ui_event_tx,
                ui_event_rx,
                cursor_path: None,
                cursor_issue: None,
                cursor_halted: false,
                clock,
                display_latency: Default::default(),
                latency_report_path: None,
            },
            tx,
        )
    }

    #[test]
    fn accepted_surface_records_only_its_first_real_chat_draw() {
        let clock = std::sync::Arc::new(ManualClock(AtomicU64::new(0)));
        let (mut app, tx) = app_with_clock(clock.clone());
        tx.send(StreamUpdate::Connected { replay_through: 5 }).unwrap();
        for (seq, arrived_ns, surface) in [
            (6, 100_000_000, serde_json::json!({"type":"text","value":"old"})),
            (7, 105_000_000, serde_json::json!({"type":"text","value":"new"})),
            (8, 106_000_000, serde_json::json!({"type":"unknown"})),
            (5, 107_000_000, serde_json::json!({"type":"text","value":"stale"})),
        ] {
            tx.send(StreamUpdate::timed(Event {
                seq, ts: 0, scope_id: "main".into(), kind: "ui.surface".into(),
                payload: serde_json::json!({"surface_id":"matrix","surface":surface}),
            }, arrived_ns)).unwrap();
        }
        app.drain_events();

        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        app.screen = Screen::Pipeline;
        clock.set(111_000_000);
        let _ = ctx.run(Default::default(), |ctx| app.draw_frame(ctx));
        assert!(app.display_latency.samples().is_empty());

        app.screen = Screen::Chat;
        clock.set(112_000_000);
        let _ = ctx.run(Default::default(), |ctx| app.draw_frame(ctx));
        clock.set(120_000_000);
        let _ = ctx.run(Default::default(), |ctx| app.draw_frame(ctx));
        assert_eq!(app.display_latency.samples(), &[crate::display_latency::Sample {
            sequence: 7,
            arrival_offset_ns: 105_000_000,
            render_offset_ns: 112_000_000,
            duration_ns: 7_000_000,
        }]);
    }

    #[test]
    fn clean_shutdown_writes_the_opt_in_latency_report() {
        let path = std::env::temp_dir().join(format!(
            "mote-ui-latency-{}-{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let (mut app, _) = app();
        app.latency_report_path = Some(path.clone());

        drop(app);

        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(report["sample_count"], 0);
        assert_eq!(report["boundary"], crate::display_latency::DISPLAY_BOUNDARY);
        assert!(report.get("payload").is_none());
        assert!(report.get("source").is_none());
    }

    fn send(tx: &std::sync::mpsc::Sender<StreamUpdate>, kind: &str, payload: serde_json::Value) {
        tx.send(Event {
            seq: 1,
            ts: 0,
            scope_id: "main".into(),
            kind: kind.into(),
            payload,
        }.into())
        .unwrap();
    }

    fn pipeline_snapshot(talk_model: &str) -> PipelineSnapshot {
        PipelineSnapshot::from_value(serde_json::json!({
            "generated_at": 5000,
            "event_limit": 200,
            "request_limit": 5000,
            "window_ms": 3600000,
            "config": {
                "models.local_only": true,
                "models.talk": talk_model,
                "models.router": "qwen3:0.6b",
                "models.think": "qwen3-coder:30b-64k",
                "budgets.talk_first_token": 300,
                "budgets.router": 150,
                "budgets.think_first_token": 3000,
                "packs.talk": 600,
                "packs.think": 8000
            },
            "events": [],
            "numbers": [],
            "requests": []
        }))
        .unwrap()
    }

    #[test]
    fn preview_action_displays_its_event_without_a_daemon() {
        let (mut app, _) = app();
        app.fixture = Some(crate::contract::Surface::V0(Node::Action {
            label: "Preview action".into(),
            event: "gallery.preview".into(),
        }));
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let render = |app: &mut MoteApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.preview_view(ui));
                },
            )
        };
        let first = render(&mut app, vec![]);
        let pos = first
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Preview action" => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .expect("action must render");
        render(
            &mut app,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ],
        );
        let displayed = render(&mut app, vec![]);
        assert!(displayed.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Preview action: gallery.preview")));
    }

    #[test]
    fn restart_between_status_and_subscription_rechecks_the_watermark() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        use std::time::{Duration, Instant};
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("mote-handshake-{}-{unique}.sock", std::process::id()));
        let cursor_path = path.with_extension("cursor");
        let daemon_a = UnixListener::bind(&path).unwrap();
        let server_path = path.clone();
        let (finish, finish_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let read = |socket: &std::os::unix::net::UnixStream| {
                socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut line = String::new();
                BufReader::new(socket.try_clone().unwrap()).read_line(&mut line).unwrap();
                serde_json::from_str::<serde_json::Value>(&line).unwrap()
            };
            let reply = |socket: &mut std::os::unix::net::UnixStream, request: &serde_json::Value, last_seq| {
                socket.write_all(&mote_ipc::encode_frame(&mote_ipc::success_envelope(request["id"].clone(), serde_json::json!({"last_seq":last_seq}))).unwrap()).unwrap();
            };
            let (mut old, _) = daemon_a.accept().unwrap();
            let status = read(&old);
            assert_eq!(status["method"], "status");
            drop(daemon_a);
            std::fs::remove_file(&server_path).unwrap();
            let daemon_b = UnixListener::bind(&server_path).unwrap();
            old.shutdown(std::net::Shutdown::Read).unwrap();
            reply(&mut old, &status, 5);
            old.shutdown(std::net::Shutdown::Write).unwrap();
            drop(old);

            let (mut socket, _) = daemon_b.accept().unwrap();
            let mut request = read(&socket);
            let checked_daemon_b = request["method"] == "status";
            if checked_daemon_b {
                reply(&mut socket, &request, 6);
                request = read(&socket);
            }
            assert_eq!(request["method"], "events.subscribe");
            assert_eq!(request["params"]["cursor"], 0);
            for event in [
                Event { seq:6, ts:0, scope_id:"main".into(), kind:"ui.surface".into(), payload:serde_json::json!({"surface_id":"matrix","surface":{"type":"action","label":"Expired","event":"refresh"}}) },
                Event { seq:7, ts:0, scope_id:"main".into(), kind:"turn.user".into(), payload:serde_json::json!({"text":"Replayed chat"}) },
            ] {
                socket.write_all(&mote_ipc::encode_frame(&mote_ipc::event_envelope(&event)).unwrap()).unwrap();
            }
            finish_rx.recv().unwrap();
            let event = Event { seq:8, ts:0, scope_id:"main".into(), kind:"talk.done".into(), payload:serde_json::json!({}) };
            socket.write_all(&mote_ipc::encode_frame(&mote_ipc::event_envelope(&event)).unwrap()).unwrap();
            checked_daemon_b
        });
        let (mut app, tx) = app();
        app.cursor_path = Some(cursor_path.clone());
        let stream_path = path.clone();
        let stream = std::thread::spawn(move || crate::stream::run(
            &stream_path, "main", 0, tx,
            crate::display_latency::InstantClock::shared(), || {},
        ));
        let deadline = Instant::now() + Duration::from_secs(3);
        while crate::cursor::load(&cursor_path).seq < 7 && Instant::now() < deadline {
            app.drain_events();
            std::thread::sleep(Duration::from_millis(2));
        }
        let cursor = crate::cursor::load(&cursor_path).seq;
        let expired_surface = app.chat.messages.iter().any(|message| message.surface_seq == Some(6));
        let chat_count = app.chat.messages.iter().filter(|message| message.text == "Replayed chat").count();
        drop(app);
        finish.send(()).unwrap();
        stream.join().unwrap();
        let checked_daemon_b = server.join().unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(cursor_path).unwrap();
        assert_eq!(cursor, 7);
        assert_eq!(chat_count, 1);
        assert!(!expired_surface, "daemon B replayed an expired surface above daemon A's watermark");
        assert!(checked_daemon_b, "subscription used a different daemon lifetime from its watermark");
    }

    #[test]
    fn reconnect_removes_expired_matrix_controls_and_preserves_chat_and_cursor() {
        use std::io::{BufRead, BufReader, Write};
        use std::time::{Duration, Instant};
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("mote-reconnect-{}-{unique}.sock", std::process::id()));
        let cursor_path = path.with_extension("cursor");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let (disconnect, disconnect_rx) = mpsc::channel();
        let (fresh, fresh_rx) = mpsc::channel();
        let (finish, finish_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let accept = || {
                let (socket, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(socket.try_clone().unwrap()).read_line(&mut line).unwrap();
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                (socket, request)
            };
            let status = |last_seq| {
                let (mut socket, request) = accept();
                assert_eq!(request["method"], "status", "a connection must establish its replay watermark before subscribing");
                socket.write_all(&mote_ipc::encode_frame(&mote_ipc::success_envelope(request["id"].clone(), serde_json::json!({"last_seq":last_seq}))).unwrap()).unwrap();
                socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                let mut line = String::new();
                let count = BufReader::new(socket.try_clone().unwrap()).read_line(&mut line).unwrap();
                assert!(count > 0, "status connection closed before subscription");
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], "events.subscribe");
                (socket, request)
            };
            let write = |socket: &mut std::os::unix::net::UnixStream, seq, kind: &str, payload| {
                let event = Event { seq, ts:0, scope_id:"main".into(), kind:kind.into(), payload };
                socket.write_all(&mote_ipc::encode_frame(&mote_ipc::event_envelope(&event)).unwrap()).unwrap();
            };
            let surface = serde_json::json!({"surface_id":"matrix", "surface":{"type":"stack","children":[
                {"type":"toggle","label":"Local only","name":"models.local_only","value":true},
                {"type":"action","label":"Expired Refresh","event":"refresh"}
            ]}});
            let (mut first, request) = status(0);
            assert_eq!(request["method"], "events.subscribe");
            assert_eq!(request["params"]["cursor"], 0);
            let (mut submission, request) = accept();
            assert_eq!(request["method"], "matrix.render");
            assert_eq!(request["params"]["source"], "screen { button \"Refresh\" -> refresh }");
            submission.write_all(&mote_ipc::encode_frame(&mote_ipc::success_envelope(request["id"].clone(), serde_json::json!({"ok":true,"valid":true}))).unwrap()).unwrap();
            write(&mut first, 1, "turn.user", serde_json::json!({"text":"Keep this chat"}));
            write(&mut first, 2, "ui.surface", serde_json::json!({"surface":{"type":"text","value":"Keep this card"}}));
            write(&mut first, 3, "ui.surface", surface.clone());
            disconnect_rx.recv().unwrap();
            drop(first);
            let (mut second, request) = status(5);
            assert_eq!(request["params"]["cursor"], 3);
            write(&mut second, 1, "turn.user", serde_json::json!({"text":"Keep this chat"}));
            write(&mut second, 3, "ui.surface", surface.clone());
            write(&mut second, 4, "ui.surface", surface.clone());
            write(&mut second, 5, "ui.surface", surface.clone());
            write(&mut second, 6, "talk.done", serde_json::json!({}));
            fresh_rx.recv().unwrap();
            write(&mut second, 7, "ui.surface", surface);
            finish_rx.recv().unwrap();
            write(&mut second, 8, "talk.done", serde_json::json!({}));
        });
        let (mut app, tx) = app();
        app.cursor_path = Some(cursor_path.clone());
        let (outgoing, events) = mpsc::channel();
        app.ui_event_tx = outgoing;
        let stream_path = path.clone();
        let submission_path = path.clone();
        let stream = std::thread::spawn(move || crate::stream::run_after_connect(
            &stream_path, "main", 0, tx,
            crate::display_latency::InstantClock::shared(), || {}, || {
                crate::submit_matrix(&submission_path, "screen { button \"Refresh\" -> refresh }").unwrap();
            },
        ));
        let receive_through = |app: &mut MoteApp, seq| {
            let deadline = Instant::now() + Duration::from_secs(2);
            while crate::cursor::load(&cursor_path).seq < seq && Instant::now() < deadline {
                app.drain_events();
                std::thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(crate::cursor::load(&cursor_path).seq, seq);
        };
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let render = |app: &mut MoteApp, input| ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events: input, ..Default::default()
        }, |ctx| { egui::CentralPanel::default().show(ctx, |ui| app.chat_view(ui)); });
        let text_position = |output: &egui::FullOutput, label: &str| output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => Some(text.pos + text.galley.rect.center().to_vec2()),
            _ => None,
        });
        let click = |position| vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton { pos:position, button:egui::PointerButton::Primary, pressed:true, modifiers:Default::default() },
            egui::Event::PointerButton { pos:position, button:egui::PointerButton::Primary, pressed:false, modifiers:Default::default() },
        ];
        receive_through(&mut app, 3);
        let displayed = render(&mut app, vec![]);
        let old_button = text_position(&displayed, "Expired Refresh").unwrap();
        let old_toggle = text_position(&displayed, "Local only").unwrap();
        render(&mut app, click(old_toggle));
        assert_eq!(events.try_recv().unwrap().args()["value"], false);

        disconnect.send(()).unwrap();
        receive_through(&mut app, 6);
        let reconnected = render(&mut app, vec![]);
        assert!(text_position(&reconnected, "Expired Refresh").is_none(), "expired Matrix control remains displayed after reconnect");
        assert!(app.chat.messages.iter().all(|message| message.surface_seq.is_none()));
        assert_eq!(app.chat.messages.iter().filter(|message| message.text == "Keep this chat").count(), 1);
        assert!(text_position(&reconnected, "Keep this card").is_some());
        render(&mut app, click(old_button));
        assert!(events.try_recv().is_err(), "expired Matrix control emitted an event");
        assert_eq!(crate::cursor::load(&cursor_path).seq, 6);

        fresh.send(()).unwrap();
        receive_through(&mut app, 7);
        let restored = render(&mut app, vec![]);
        render(&mut app, click(text_position(&restored, "Local only").unwrap()));
        let event = events.try_recv().unwrap();
        assert_eq!(event.args()["value"], false, "expired control edit returned with the fresh surface");
        assert_eq!(event.context().surface_seq, Some(7));
        drop(app);
        finish.send(()).unwrap();
        stream.join().unwrap();
        server.join().unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(cursor_path).unwrap();
    }

    #[test]
    fn matrix_click_uses_committed_sequence_and_structural_path_in_worker_params() {
        let (mut app, tx) = app();
        let (requests, request_rx) = mpsc::channel();
        let (results, _) = mpsc::channel();
        let (observed, observed_rx) = mpsc::channel();
        app.ui_event_tx = requests;
        spawn_ui_event_worker_with(request_rx, results, move |event| {
            observed.send(ui_event_params(event)).unwrap();
            Ok(())
        });
        for seq in [7, 19] {
            tx.send(Event {
                seq, ts: 0, scope_id: "main".into(), kind: "ui.surface".into(),
                payload: serde_json::json!({"surface_id":"matrix", "surface":{
                    "type":"stack", "children":[{"type":"row", "children":[
                        {"type":"text", "value":"Status"},
                        {"type":"action", "label":"Refresh", "event":"refresh"}
                    ]}]
                }}),
            }.into()).unwrap();
        }
        app.drain_events();
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let render = |app: &mut MoteApp, events| ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events, ..Default::default()
        }, |ctx| { egui::CentralPanel::default().show(ctx, |ui| app.chat_view(ui)); });
        let output = render(&mut app, vec![]);
        let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Refresh" => Some(text.pos + text.galley.rect.center().to_vec2()),
            _ => None,
        }).unwrap();
        render(&mut app, vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton { pos, button:egui::PointerButton::Primary, pressed:true, modifiers:Default::default() },
            egui::Event::PointerButton { pos, button:egui::PointerButton::Primary, pressed:false, modifiers:Default::default() },
        ]);
        assert_eq!(observed_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap(), serde_json::json!({
            "thread":"main", "event":"refresh", "args":{}, "surface_seq":19, "node_path":[0,1]
        }));
    }

    #[test]
    fn dual_surface_interaction_emits_one_v0_event_without_contract_context() {
        let (mut app, tx) = app();
        let (events, event_rx) = mpsc::channel();
        app.ui_event_tx = events;
        tx.send(Event {
            seq: 23, ts: 0, scope_id: "main".into(), kind: "ui.surface".into(),
            payload: serde_json::json!({
                "surface_id":"matrix",
                "surface":{"type":"stack", "children":[
                    {"type":"text", "value":"Fallback controls"},
                    {"type":"action", "label":"Refresh fallback", "event":"refresh"}
                ]},
                "matrix_surface":{"version":1, "root":{"type":"screen", "children":[
                    {"type":"text", "value":"Retained Matrix surface"}
                ]}}
            }),
        }.into()).unwrap();
        app.drain_events();
        assert!(app.chat.messages[0].surface.as_ref().unwrap().matrix_v1().is_some());

        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let render = |app: &mut MoteApp, events| ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events, ..Default::default()
        }, |ctx| { egui::CentralPanel::default().show(ctx, |ui| app.chat_view(ui)); });
        let output = render(&mut app, vec![]);
        let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Refresh fallback" => {
                Some(text.pos + text.galley.rect.center().to_vec2())
            }
            _ => None,
        }).unwrap();
        render(&mut app, vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton { pos, button:egui::PointerButton::Primary, pressed:true, modifiers:Default::default() },
            egui::Event::PointerButton { pos, button:egui::PointerButton::Primary, pressed:false, modifiers:Default::default() },
        ]);

        let params = ui_event_params(&event_rx.try_recv().unwrap());
        assert_eq!(params, serde_json::json!({
            "thread":"main", "event":"refresh", "args":{}, "surface_seq":23, "node_path":[1]
        }));
        assert!(params.get("surface_contract").is_none());
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn ui_event_params_include_thread_action_name_and_empty_args() {
        let event = crate::surface::SurfaceEvent::Action {
            context: Default::default(),
            event: "job.run".into(),
        };
        assert_eq!(
            ui_event_params(&event),
            serde_json::json!({
                "thread": "main",
                "event": "job.run",
                "args": {}
            })
        );
    }

    #[test]
    fn ui_event_params_include_typed_field_change_args() {
        let event = crate::surface::SurfaceEvent::Change {
            context: crate::surface::SurfaceContext { surface_seq: Some(23), node_path: vec![1] },
            field: "models.local_only".into(),
            value: serde_json::Value::Bool(true),
            state_id: egui::Id::NULL,
        };
        assert_eq!(
            ui_event_params(&event),
            serde_json::json!({
                "thread": "main",
                "event": "change",
                "surface_seq": 23,
                "node_path": [1],
                "args": {
                    "field": "models.local_only",
                    "value": true
                }
            })
        );
        assert_eq!(
            preview_status(&event),
            "Preview change: models.local_only = true"
        );
    }

    #[test]
    fn blocked_ui_event_work_does_not_block_the_render_thread() {
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let (observed_tx, observed_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let first = crate::surface::SurfaceEvent::Change {
            context: Default::default(),
            field: "models.talk".into(),
            value: serde_json::json!("qwen3:0.6b"),
            state_id: egui::Id::NULL,
        };
        let second = crate::surface::SurfaceEvent::Change {
            context: Default::default(),
            field: "models.talk".into(),
            value: serde_json::json!("qwen3:4b"),
            state_id: egui::Id::NULL,
        };
        spawn_ui_event_worker_with(request_rx, result_tx, move |event| {
            observed_tx.send(event.clone()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        });

        request_tx.send(first.clone()).unwrap();
        request_tx.send(second.clone()).unwrap();
        assert_eq!(
            observed_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            first
        );
        assert!(observed_rx.try_recv().is_err());
        assert!(result_rx.try_recv().is_err());
        release_tx.send(()).unwrap();
        assert_eq!(
            result_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            (first, Ok(()))
        );
        assert_eq!(
            observed_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            second.clone()
        );
        release_tx.send(()).unwrap();
        assert_eq!(
            result_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            (second, Ok(()))
        );
    }

    #[test]
    fn failed_ui_event_is_visible_and_rolls_back_local_control_state() {
        let (mut app, _) = app();
        let ctx = egui::Context::default();
        let id = egui::Id::new("rejected-select");
        ctx.data_mut(|data| {
            data.insert_temp(id, ("qwen3:4b".to_string(), "qwen3:0.6b".to_string()))
        });
        app.record_ui_event_result(
            &ctx,
            crate::surface::SurfaceEvent::Change {
                context: Default::default(),
                field: "models.talk".into(),
                value: serde_json::json!("qwen3:0.6b"),
                state_id: id,
            },
            Err("model gate rejected selection".into()),
        );

        assert_eq!(app.status_line, "model gate rejected selection");
        assert!(ctx
            .data_mut(|data| data.get_temp::<(String, String)>(id))
            .is_none());
    }

    #[test]
    fn diagnostic_renders_in_chat_with_prior_surface_and_advances_cursor() {
        let (mut app, tx) = app();
        let cursor = std::env::temp_dir().join(format!("mote-diagnostic-cursor-{}", std::process::id()));
        app.cursor_path = Some(cursor.clone());
        for (seq, kind, payload) in [
            (1, "ui.surface", serde_json::json!({"surface_id":"matrix", "surface":{"type":"text", "value":"Prior dashboard"}})),
            (2, "ui.diagnostic", serde_json::json!({"diagnostics":[{
                "code":"E_PARSE_INCOMPLETE", "span":{"start":12,"end":12}, "message":"expected closing brace", "hint":"Close the block"
            }]})),
        ] {
            tx.send(Event { seq, ts:0, scope_id:"main".into(), kind:kind.into(), payload }.into()).unwrap();
        }
        app.drain_events();
        assert_eq!(crate::cursor::load(&cursor).seq, 2);
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let output = ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 800.0))),
            ..Default::default()
        }, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.chat_view(ui));
        });
        let text = output.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        }).collect::<Vec<_>>().join("\n");
        for part in ["Prior dashboard", "E_PARSE_INCOMPLETE", "12..12", "expected closing brace", "Close the block"] {
            assert!(text.contains(part), "not rendered: {part}; {text}");
        }
        std::fs::remove_file(cursor).ok();
    }

    #[test]
    fn valid_surface_becomes_a_displayed_typed_surface() {
        let (mut app, tx) = app();
        send(
            &tx,
            "ui.surface",
            serde_json::json!({"surface": {"type":"text", "value":"ready"}}),
        );
        app.drain_events();
        assert_eq!(app.chat.messages.len(), 1);
        assert_eq!(
            app.chat.messages[0].surface.as_ref().map(crate::contract::Surface::display_v0),
            Some(&Node::Text {
                value: "ready".into()
            })
        );
    }

    #[test]
    fn malformed_surface_is_ignored_fail_closed() {
        let (mut app, tx) = app();
        for surface in [
            serde_json::json!({"type":"text", "value":"bad", "key":"invented"}),
            serde_json::json!({"type":"text"}),
            serde_json::json!({"type":"unknown"}),
        ] {
            send(&tx, "ui.surface", serde_json::json!({"surface":surface}));
        }
        app.drain_events();
        assert!(app.chat.messages.is_empty());
    }

    #[test]
    fn talk_events_retain_text_typing_and_delivery_behavior() {
        let (mut app, tx) = app();
        send(&tx, "turn.user", serde_json::json!({"text":"hello"}));
        send(&tx, "talk.delta", serde_json::json!({"text":"one"}));
        send(&tx, "talk.delta", serde_json::json!({"text":" two"}));
        app.drain_events();
        assert!(app.chat.typing);
        assert_eq!(app.chat.messages.len(), 2);
        assert_eq!(app.chat.messages[0].text, "hello");
        assert_eq!(app.chat.messages[1].text, "one two");
        send(&tx, "talk.done", serde_json::json!({}));
        app.drain_events();
        assert!(!app.chat.typing);
        assert!(app.chat.messages[1].status == Status::Read);
        send(
            &tx,
            "talk.unavailable",
            serde_json::json!({"reason":"offline"}),
        );
        app.drain_events();
        assert_eq!(app.chat.messages[2].text, "talk unavailable: offline");
    }

    #[test]
    fn streamed_text_after_a_surface_starts_a_visible_message() {
        let (mut app, tx) = app();
        send(&tx, "talk.delta", serde_json::json!({"text":"before"}));
        send(
            &tx,
            "ui.surface",
            serde_json::json!({"surface": {"type":"text", "value":"card"}}),
        );
        send(&tx, "talk.delta", serde_json::json!({"text":"after"}));

        app.drain_events();

        assert_eq!(app.chat.messages.len(), 3);
        assert_eq!(app.chat.messages[2].text, "after");
        assert!(app.chat.messages[2].surface.is_none());
    }

    #[test]
    fn turn_response_marks_a_pending_message_sent_without_downgrading_delivery() {
        let (mut app, _tx) = app();
        let pending = app.chat.push_optimistic_user("hello".into());

        app.record_turn_result(pending.clone(), Ok(()));
        assert!(app.chat.messages[0].status == Status::Sent);

        send(
            &_tx,
            "turn.user",
            serde_json::json!({"text": "hello", "client_message_id": pending.clone()}),
        );
        app.drain_events();
        app.record_turn_result(pending, Ok(()));
        assert!(app.chat.messages[0].status == Status::Delivered);
    }

    #[test]
    fn cursor_advances_only_after_each_event_is_reduced() {
        let cursor = std::env::temp_dir().join(format!(
            "mote-client-cursor-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Event {
            seq: 10,
            ts: 0,
            scope_id: "main".into(),
            kind: "turn.user".into(),
            payload: serde_json::json!({"text": "hello"}),
        }.into())
        .unwrap();
        tx.send(Event {
            seq: 11,
            ts: 0,
            scope_id: "main".into(),
            kind: "talk.delta".into(),
            payload: serde_json::json!({"text": "reply"}),
        }.into())
        .unwrap();
        let mut chat = ChatState::default();

        assert!(apply_next_event(&mut chat, &rx, Some(&cursor)).is_some());
        assert_eq!(crate::cursor::load(&cursor).seq, 10);
        assert_eq!(chat.messages.len(), 1);
        drop(rx);

        let (replay_tx, replay_rx) = std::sync::mpsc::channel();
        replay_tx
            .send(Event {
                seq: 11,
                ts: 0,
                scope_id: "main".into(),
                kind: "talk.delta".into(),
                payload: serde_json::json!({"text": "reply"}),
            }.into())
            .unwrap();
        let mut restarted = ChatState::default();
        assert!(apply_next_event(
            &mut restarted,
            &replay_rx,
            Some(&cursor)
        )
        .is_some());
        assert_eq!(crate::cursor::load(&cursor).seq, 11);
        assert_eq!(restarted.messages[0].text, "reply");

        std::fs::remove_file(cursor).ok();
    }

    #[test]
    fn dual_surface_replaces_once_draws_fallback_and_advances_one_cursor() {
        let cursor = std::env::temp_dir().join(format!(
            "mote-client-contract-cursor-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut chat = ChatState::default();
        chat.reduce(&Event {
            seq: 29, ts: 0, scope_id: "main".into(), kind: "ui.surface".into(),
            payload: serde_json::json!({
                "surface_id":"matrix",
                "surface":{"type":"text", "value":"Prior dashboard"}
            }),
        });
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Event {
            seq: 30, ts: 0, scope_id: "main".into(), kind: "ui.surface".into(),
            payload: serde_json::json!({
                "surface_id":"matrix",
                "surface":{"type":"text", "value":"Fallback dashboard"},
                "matrix_surface":{
                    "version":1,
                    "root":{
                        "type":"screen",
                        "children":[{"type":"text", "value":"Matrix dashboard"}]
                    }
                }
            }),
        }.into()).unwrap();

        let (_, accepted, _, persisted) = apply_next_event(&mut chat, &rx, Some(&cursor)).unwrap();
        persisted.unwrap();
        assert_eq!(accepted, Some(crate::state::SurfaceReduction {
            revision: 2,
            superseded_revision: Some(1),
        }));
        assert_eq!(crate::cursor::load(&cursor).seq, 30);
        assert_eq!(chat.messages.len(), 1);
        assert!(chat.messages[0].surface.as_ref().unwrap().matrix_v1().is_some());

        tx.send(Event {
            seq: 31, ts: 0, scope_id: "main".into(), kind: "ui.surface".into(),
            payload: serde_json::json!({
                "surface_id":"matrix",
                "surface":{"type":"unknown"},
                "matrix_surface":{"version":2, "root":{"type":"unknown"}}
            }),
        }.into()).unwrap();
        let (_, rejected, _, persisted) = apply_next_event(&mut chat, &rx, Some(&cursor)).unwrap();
        persisted.unwrap();
        assert_eq!(rejected, None);
        assert_eq!(crate::cursor::load(&cursor).seq, 31);
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].render_revision, Some(2));

        let (mut app, _) = app();
        app.chat = chat;
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let output = ctx.run(Default::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.chat_view(ui));
        });
        let text = output.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        }).collect::<Vec<_>>().join("\n");
        assert!(text.contains("Fallback dashboard"));
        assert!(!text.contains("Matrix dashboard"));

        std::fs::remove_file(cursor).ok();
    }

    #[test]
    fn cursor_write_failure_keeps_reduced_event_visible_and_halts_later_events() {
        let root = std::env::temp_dir().join(format!(
            "mote-client-cursor-failure-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let unwritable_cursor = root.join("cursor");
        std::fs::create_dir(&unwritable_cursor).unwrap();
        let clock = std::sync::Arc::new(ManualClock(AtomicU64::new(20_000_000)));
        let (mut app, tx) = app_with_clock(clock.clone());
        app.cursor_path = Some(unwritable_cursor);
        tx.send(StreamUpdate::timed(
            Event {
                seq: 10,
                ts: 0,
                scope_id: "main".into(),
                kind: "ui.surface".into(),
                payload: serde_json::json!({
                    "surface_id":"matrix",
                    "surface":{"type":"text","value":"Kept after failure"}
                }),
            },
            10_000_000,
        ))
        .unwrap();
        tx.send(Event {
            seq: 11,
            ts: 0,
            scope_id: "main".into(),
            kind: "turn.user".into(),
            payload: serde_json::json!({"text":"Must remain queued"}),
        }
        .into())
        .unwrap();

        app.drain_events();

        assert!(app.cursor_halted);
        assert_eq!(app.chat.messages.len(), 1);
        assert_eq!(
            app.chat.messages[0].surface.as_ref().map(crate::contract::Surface::display_v0),
            Some(&Node::Text {
                value: "Kept after failure".into(),
            })
        );
        assert_eq!(
            app.pipeline_live_events
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            [10]
        );

        app.record_ui_event_result(
            &egui::Context::default(),
            crate::surface::SurfaceEvent::Action {
                context: Default::default(),
                event: "refresh".into(),
            },
            Ok(()),
        );
        app.drain_events();
        assert_eq!(
            app.chat.messages.len(),
            1,
            "later queued event reduced after failure"
        );
        let issue = app
            .cursor_issue
            .as_deref()
            .expect("cursor issue must remain visible");
        assert!(issue.contains("event position"));

        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 480.0),
                )),
                ..Default::default()
            },
            |ctx| app.draw_frame(ctx),
        );
        assert!(output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text().contains("event position")
        )));
        assert_eq!(app.display_latency.samples().len(), 1);
        assert_eq!(app.display_latency.samples()[0].sequence, 10);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn close_and_reopen_subscribes_from_durable_cursor_and_ignores_stale_replay() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        use std::time::{Duration, Instant};

        let root = PathBuf::from("/tmp").join(format!(
            "mcr-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let cursor_path = root.join("client_cursor");
        let socket_path = root.join("moted.sock");

        let (mut first, first_tx) = app();
        first.cursor_path = Some(cursor_path.clone());
        first_tx
            .send(
                Event {
                    seq: 10,
                    ts: 0,
                    scope_id: "main".into(),
                    kind: "turn.user".into(),
                    payload: serde_json::json!({"text":"Before close"}),
                }
                .into(),
            )
            .unwrap();
        first.drain_events();
        assert_eq!(crate::cursor::load(&cursor_path).seq, 10);
        drop(first);

        let listener = UnixListener::bind(&socket_path).unwrap();
        let (finish, finish_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let read_request = |socket: &std::os::unix::net::UnixStream| {
                let mut line = String::new();
                BufReader::new(socket.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                serde_json::from_str::<serde_json::Value>(&line).unwrap()
            };
            let status = read_request(&socket);
            assert_eq!(status["method"], "status");
            socket
                .write_all(
                    &mote_ipc::encode_frame(&mote_ipc::success_envelope(
                        status["id"].clone(),
                        serde_json::json!({"last_seq":11}),
                    ))
                    .unwrap(),
                )
                .unwrap();
            let subscription = read_request(&socket);
            assert_eq!(subscription["method"], "events.subscribe");
            assert_eq!(subscription["params"]["cursor"], 10);
            for (seq, text) in [(10, "Stale replay"), (11, "After reopen")] {
                let event = Event {
                    seq,
                    ts: 0,
                    scope_id: "main".into(),
                    kind: "turn.user".into(),
                    payload: serde_json::json!({"text":text}),
                };
                socket
                    .write_all(
                        &mote_ipc::encode_frame(&mote_ipc::event_envelope(&event)).unwrap(),
                    )
                    .unwrap();
            }
            finish_rx.recv().unwrap();
            let event = Event {
                seq: 12,
                ts: 0,
                scope_id: "main".into(),
                kind: "talk.done".into(),
                payload: serde_json::json!({}),
            };
            socket
                .write_all(&mote_ipc::encode_frame(&mote_ipc::event_envelope(&event)).unwrap())
                .unwrap();
        });

        let loaded = crate::cursor::load(&cursor_path);
        assert_eq!(loaded.warning, None);
        let (mut reopened, reopened_tx) = app();
        reopened.cursor_path = Some(cursor_path.clone());
        let stream_path = socket_path.clone();
        let stream = std::thread::spawn(move || {
            crate::stream::run(
                &stream_path,
                "main",
                loaded.seq,
                reopened_tx,
                crate::display_latency::InstantClock::shared(),
                || {},
            )
        });
        let deadline = Instant::now() + Duration::from_secs(3);
        while crate::cursor::load(&cursor_path).seq < 11 && Instant::now() < deadline {
            reopened.drain_events();
            std::thread::sleep(Duration::from_millis(2));
        }

        assert_eq!(crate::cursor::load(&cursor_path).seq, 11);
        assert_eq!(
            reopened
                .chat
                .messages
                .iter()
                .map(|message| message.text.as_str())
                .collect::<Vec<_>>(),
            ["After reopen"]
        );
        drop(reopened);
        finish.send(()).unwrap();
        stream.join().unwrap();
        server.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_send_is_removed_and_identical_retry_uses_a_new_message_id() {
        let (mut app, tx) = app();
        let failed = app.chat.push_optimistic_user("hello".into());

        app.record_turn_result(failed.clone(), Err("offline".into()));

        assert!(app.chat.messages.is_empty());
        assert_eq!(app.input, "hello");
        let retry = app.chat.push_optimistic_user(std::mem::take(&mut app.input));
        assert_ne!(failed, retry);
        app.record_turn_result(retry.clone(), Ok(()));
        send(
            &tx,
            "turn.user",
            serde_json::json!({"text":"hello", "client_message_id": retry}),
        );
        app.drain_events();

        assert_eq!(app.chat.messages.len(), 1);
        assert!(app.chat.messages[0].status == Status::Delivered);
    }

    #[test]
    fn failed_response_does_not_remove_an_already_delivered_message() {
        let (mut app, tx) = app();
        let pending = app.chat.push_optimistic_user("hello".into());
        send(
            &tx,
            "turn.user",
            serde_json::json!({"text":"hello", "client_message_id": pending.clone()}),
        );
        app.drain_events();

        app.record_turn_result(pending, Err("late failure".into()));

        assert_eq!(app.chat.messages.len(), 1);
        assert!(app.chat.messages[0].status == Status::Delivered);
        assert!(app.input.is_empty());
    }

    #[test]
    fn config_tab_contains_every_section_four_key() {
        let mut actual = CONFIG_KEYS.to_vec();
        actual.sort_unstable();
        let mut expected = vec![
            "budgets.router",
            "budgets.talk_first_token",
            "budgets.think_first_token",
            "models.local_only",
            "models.router",
            "models.talk",
            "models.think",
            "packs.talk",
            "packs.think",
        ];
        expected.sort_unstable();

        assert_eq!(actual, expected);
    }

    #[test]
    fn pipeline_snapshot_loading_runs_off_the_rendering_thread() {
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();

        spawn_pipeline_load_with(result_tx, move || {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Err("offline".to_string())
        });

        started_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
        assert!(matches!(result_rx.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
        release_tx.send(()).unwrap();
        assert_eq!(
            result_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap().unwrap_err(),
            "offline"
        );
    }

    #[test]
    fn installing_a_snapshot_seeds_all_config_edits_and_clears_errors() {
        let (mut app, _tx) = app();
        app.status_line = "offline".into();
        let snapshot = PipelineSnapshot::from_value(serde_json::json!({
            "generated_at": 5000,
            "event_limit": 200,
            "request_limit": 200,
            "window_ms": 3600000,
            "config": {
                "models.local_only": true,
                "models.talk": "qwen3:4b",
                "models.router": "qwen3:0.6b",
                "models.think": "qwen3-coder:30b-64k",
                "budgets.talk_first_token": 300,
                "budgets.router": 150,
                "budgets.think_first_token": 3000,
                "packs.talk": 600,
                "packs.think": 8000
            },
            "events": [],
            "numbers": [],
            "requests": []
        }))
        .unwrap();

        app.install_pipeline_snapshot(snapshot);

        assert_eq!(app.config_edits.len(), 9);
        assert_eq!(app.config_edits["models.talk"], "qwen3:4b");
        assert!(app.status_line.is_empty());
    }

    #[test]
    fn subscribed_main_events_update_the_cached_pipeline_snapshot() {
        let (mut app, tx) = app();
        app.install_pipeline_snapshot(
            PipelineSnapshot::from_value(serde_json::json!({
                "generated_at": 5000,
                "event_limit": 200,
                "request_limit": 200,
                "window_ms": 3600000,
                "config": {
                    "models.local_only": true,
                    "models.talk": "qwen3:4b",
                    "models.router": "qwen3:0.6b",
                    "models.think": "qwen3-coder:30b-64k",
                    "budgets.talk_first_token": 300,
                    "budgets.router": 150,
                    "budgets.think_first_token": 3000,
                    "packs.talk": 600,
                    "packs.think": 8000
                },
                "events": [],
                "numbers": [],
                "requests": []
            }))
            .unwrap(),
        );
        tx.send(Event {
            seq: 12,
            ts: 5100,
            scope_id: "main".into(),
            kind: "talk.delta".into(),
            payload: serde_json::json!({"text": "live"}),
        }.into())
        .unwrap();

        app.drain_events();

        let snapshot = app.pipeline.as_ref().unwrap();
        assert_eq!(snapshot.events_for("talk")[0].seq, 12);
    }

    #[test]
    fn first_snapshot_merges_events_received_while_it_was_loading() {
        let (mut app, tx) = app();
        tx.send(Event {
            seq: 12,
            ts: 5100,
            scope_id: "main".into(),
            kind: "talk.delta".into(),
            payload: serde_json::json!({"text": "newer than snapshot"}),
        }.into())
        .unwrap();

        app.drain_events();
        assert!(app.pipeline.is_none());
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));

        assert_eq!(app.pipeline.as_ref().unwrap().events_for("talk")[0].seq, 12);
    }

    #[test]
    fn refresh_snapshot_cannot_erase_a_newer_live_event() {
        let (mut app, tx) = app();
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));
        tx.send(Event {
            seq: 13,
            ts: 5200,
            scope_id: "main".into(),
            kind: "talk.done".into(),
            payload: serde_json::json!({"ledger_id": 9}),
        }.into())
        .unwrap();
        app.drain_events();

        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));

        assert_eq!(app.pipeline.as_ref().unwrap().events_for("talk")[0].seq, 13);
    }

    #[test]
    fn snapshot_preserves_dirty_config_edits_until_their_request_succeeds() {
        let (mut app, _tx) = app();
        app.config_edits.insert("models.talk".into(), "before-request".into());
        app.config_edit_revisions.insert("models.talk".into(), 1);
        app.config_revision = 1;
        app.config_edits.insert("models.talk".into(), "after-request".into());
        app.config_revision = 2;
        app.config_edit_revisions.insert("models.talk".into(), 2);

        app.install_pipeline_snapshot(pipeline_snapshot("stale-snapshot"));

        assert_eq!(app.config_edits["models.talk"], "after-request");
        app.install_pipeline_snapshot(pipeline_snapshot("fresh-snapshot"));
        assert_eq!(app.config_edits["models.talk"], "after-request");
    }

    #[test]
    fn apply_is_withheld_after_edit_then_refresh_until_loading_finishes() {
        let (mut app, _tx) = app();
        app.config_edits.insert("models.talk".into(), "edited-model".into());
        app.config_revision = 1;
        app.config_edit_revisions.insert("models.talk".into(), 1);

        app.pipeline_loading = true;
        let applied = app.apply_config_edits();

        assert!(!applied);
        assert!(app.pipeline_loading);
        assert_eq!(app.config_edits["models.talk"], "edited-model");
        assert!(app.status_line.is_empty());
    }

    #[test]
    fn blocked_config_rpc_does_not_block_the_render_thread() {
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let request = ConfigSetRequest {
            key: "models.talk".into(),
            value: serde_json::json!("qwen3:0.6b"),
            revision: 1,
        };
        let second = ConfigSetRequest {
            key: "packs.talk".into(),
            value: serde_json::json!(700),
            revision: 2,
        };
        spawn_config_worker_with(request_rx, result_tx, move |actual| {
            started_tx.send(actual.clone()).unwrap();
            release_rx.recv().unwrap();
            Ok(())
        });

        request_tx.send(vec![request.clone(), second.clone()]).unwrap();
        assert_eq!(
            started_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            request
        );
        assert!(result_rx.try_recv().is_err());
        release_tx.send(()).unwrap();
        assert_eq!(
            started_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            second
        );
        assert!(result_rx.try_recv().is_err(), "partial batches must stay hidden");
        release_tx.send(()).unwrap();
        assert_eq!(
            result_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap(),
            vec![
                ConfigSetOutcome { request, result: Ok(()) },
                ConfigSetOutcome { request: second, result: Ok(()) },
            ]
        );
    }

    #[test]
    fn apply_queues_only_dirty_typed_values_in_fixed_key_order() {
        let (mut app, _tx) = app();
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));
        let (config_tx, config_rx) = std::sync::mpsc::channel();
        app.config_tx = config_tx;

        app.set_config_edit("packs.think", "9000");
        app.set_config_edit("models.talk", "qwen3:0.6b");
        app.set_config_edit("budgets.router", "151");
        app.set_config_edit("budgets.router", "150");
        app.set_config_edit("models.local_only", "not-a-boolean");
        app.set_config_edit("packs.talk", "not-an-integer");

        assert!(app.apply_config_edits());
        assert!(!app.apply_config_edits(), "a pending batch must not be duplicated");
        let batch = config_rx.recv().unwrap();
        assert_eq!(
            batch,
            vec![
                ConfigSetRequest {
                    key: "models.talk".into(),
                    value: serde_json::json!("qwen3:0.6b"),
                    revision: 2,
                },
                ConfigSetRequest {
                    key: "packs.think".into(),
                    value: serde_json::json!(9000),
                    revision: 1,
                },
            ]
        );
        assert!(config_rx.try_recv().is_err());
        assert!(app.config_error.as_deref().unwrap().contains("models.local_only"));
        assert!(app.config_error.as_deref().unwrap().contains("packs.talk"));
        app.record_config_batch_result(
            batch
                .into_iter()
                .map(|request| ConfigSetOutcome { request, result: Ok(()) })
                .collect(),
        );
        assert!(app.config_error.as_deref().unwrap().contains("models.local_only"));
        assert!(app.config_error.as_deref().unwrap().contains("packs.talk"));
    }

    #[test]
    fn edit_then_revert_queues_no_configuration_request() {
        let (mut app, _tx) = app();
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));
        let (config_tx, config_rx) = std::sync::mpsc::channel();
        app.config_tx = config_tx;

        app.set_config_edit("models.talk", "qwen3:0.6b");
        app.set_config_edit("models.talk", "qwen3:4b");

        assert!(!app.apply_config_edits());
        assert!(config_rx.try_recv().is_err());
    }

    #[test]
    fn batch_outcomes_preserve_failures_concurrent_edits_and_error_across_refresh() {
        let (mut app, _tx) = app();
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));
        let (config_tx, config_rx) = std::sync::mpsc::channel();
        app.config_tx = config_tx;

        app.set_config_edit("models.talk", "qwen3:0.6b");
        app.set_config_edit("models.router", "qwen3:4b");
        assert!(app.apply_config_edits());
        let batch = config_rx.recv().unwrap();
        app.set_config_edit("models.talk", "qwen3-coder:30b-64k");

        app.record_config_batch_result(vec![
            ConfigSetOutcome { request: batch[0].clone(), result: Ok(()) },
            ConfigSetOutcome {
                request: batch[1].clone(),
                result: Err("model gate rejected router".into()),
            },
        ]);

        assert!(!app.config_pending);
        assert!(app.pipeline_loading, "refresh starts only after the complete batch");
        assert_eq!(app.config_edits["models.talk"], "qwen3-coder:30b-64k");
        assert_eq!(app.config_edits["models.router"], "qwen3:4b");
        assert!(app.config_edit_revisions.contains_key("models.talk"));
        assert!(app.config_edit_revisions.contains_key("models.router"));
        assert_eq!(app.config_error.as_deref(), Some("models.router: model gate rejected router"));

        let mut refreshed = pipeline_snapshot("stale-snapshot");
        refreshed
            .config
            .insert("models.think".into(), serde_json::json!("clean-snapshot-value"));
        app.install_pipeline_snapshot(refreshed);
        assert_eq!(app.config_edits["models.talk"], "qwen3-coder:30b-64k");
        assert_eq!(app.config_edits["models.router"], "qwen3:4b");
        assert_eq!(app.config_error.as_deref(), Some("models.router: model gate rejected router"));
        assert_eq!(app.config_edits["models.think"], "clean-snapshot-value");
    }

    #[test]
    fn all_failed_batch_does_not_start_a_snapshot_refresh() {
        let (mut app, _tx) = app();
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));
        app.set_config_edit("models.talk", "too-large-model");
        let revision = app.config_edit_revisions["models.talk"];
        app.config_pending = true;

        app.record_config_batch_result(vec![ConfigSetOutcome {
            request: ConfigSetRequest {
                key: "models.talk".into(),
                value: serde_json::json!("too-large-model"),
                revision,
            },
            result: Err("memory gate rejected model".into()),
        }]);

        assert!(!app.pipeline_loading);
        assert!(app.config_edit_revisions.contains_key("models.talk"));
        assert_eq!(app.config_edits["models.talk"], "too-large-model");
        assert_eq!(
            app.config_error.as_deref(),
            Some("models.talk: memory gate rejected model")
        );
    }

    #[test]
    fn successful_current_revision_clears_dirty_state_before_refresh() {
        let (mut app, _tx) = app();
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));
        app.set_config_edit("models.talk", "qwen3:0.6b");
        let revision = app.config_edit_revisions["models.talk"];
        app.config_pending = true;

        app.record_config_batch_result(vec![ConfigSetOutcome {
            request: ConfigSetRequest {
                key: "models.talk".into(),
                value: serde_json::json!("qwen3:0.6b"),
                revision,
            },
            result: Ok(()),
        }]);

        assert!(!app.config_edit_revisions.contains_key("models.talk"));
        assert!(app.pipeline_loading);
    }

    #[test]
    fn edit_back_to_old_baseline_during_apply_survives_successful_refresh() {
        let (mut app, _tx) = app();
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));
        let (config_tx, config_rx) = std::sync::mpsc::channel();
        app.config_tx = config_tx;

        app.set_config_edit("models.talk", "qwen3:0.6b");
        assert!(app.apply_config_edits());
        let submitted = config_rx.recv().unwrap();
        app.set_config_edit("models.talk", "qwen3:4b");

        assert!(
            app.config_edit_revisions.contains_key("models.talk"),
            "a post-submit edit must compare against the submitted value"
        );
        app.record_config_batch_result(vec![ConfigSetOutcome {
            request: submitted[0].clone(),
            result: Ok(()),
        }]);
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:0.6b"));
        app.pipeline_loading = false;

        assert_eq!(app.config_edits["models.talk"], "qwen3:4b");
        assert_eq!(app.config_baseline["models.talk"], "qwen3:0.6b");
        assert!(app.config_edit_revisions.contains_key("models.talk"));
        assert!(app.apply_config_edits());
        let follow_up = config_rx.recv().unwrap();
        assert_eq!(
            follow_up,
            vec![ConfigSetRequest {
                key: "models.talk".into(),
                value: serde_json::json!("qwen3:4b"),
                revision: app.config_edit_revisions["models.talk"],
            }]
        );
    }

    #[test]
    fn edit_back_to_submitted_value_during_apply_is_clean_after_success() {
        let (mut app, _tx) = app();
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));
        let (config_tx, config_rx) = std::sync::mpsc::channel();
        app.config_tx = config_tx;

        app.set_config_edit("models.talk", "qwen3:0.6b");
        assert!(app.apply_config_edits());
        let submitted = config_rx.recv().unwrap();
        app.set_config_edit("models.talk", "qwen3-coder:30b-64k");
        app.set_config_edit("models.talk", "qwen3:0.6b");

        app.record_config_batch_result(vec![ConfigSetOutcome {
            request: submitted[0].clone(),
            result: Ok(()),
        }]);
        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:0.6b"));
        app.pipeline_loading = false;

        assert_eq!(app.config_edits["models.talk"], "qwen3:0.6b");
        assert_eq!(app.config_baseline["models.talk"], "qwen3:0.6b");
        assert!(!app.config_edit_revisions.contains_key("models.talk"));
        assert!(!app.apply_config_edits());
        assert!(config_rx.try_recv().is_err());
    }
}
