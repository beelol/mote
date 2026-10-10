//! The egui application: chat view and pipeline view.

use crate::pipeline::PipelineSnapshot;
use crate::state::{ChatState, LocalMessageId};
use eframe::egui;
use mote_eventlog::Event;
use mote_ipc::Client;
use mote_ui::Node;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

type TurnResult = (LocalMessageId, Result<(), String>);
type PipelineResult = Result<PipelineSnapshot, String>;

fn spawn_pipeline_load_with(
    tx: Sender<PipelineResult>,
    load: impl FnOnce() -> PipelineResult + Send + 'static,
) {
    std::thread::spawn(move || {
        let _ = tx.send(load());
    });
}

fn cursor_path() -> PathBuf {
    mote_paths::data_dir().join("client_cursor")
}

/// Load the saved cursor, or 0.
pub fn load_cursor() -> i64 {
    load_cursor_from(&cursor_path())
}

fn load_cursor_from(path: &std::path::Path) -> i64 {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn save_cursor_to(path: &std::path::Path, seq: i64) {
    let _ = std::fs::write(path, seq.to_string());
}

fn apply_next_event(
    chat: &mut ChatState,
    rx: &Receiver<Event>,
    cursor_path: Option<&std::path::Path>,
) -> Option<Event> {
    let Ok(event) = rx.try_recv() else {
        return None;
    };
    chat.reduce(&event);
    if let Some(path) = cursor_path {
        save_cursor_to(path, event.seq);
    }
    Some(event)
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
    rx: Receiver<Event>,
    chat: ChatState,
    input: String,
    screen: Screen,
    stage: usize,
    tab: usize,
    config_edits: std::collections::HashMap<String, String>,
    config_revision: u64,
    config_edit_revisions: std::collections::HashMap<String, u64>,
    pipeline: Option<PipelineSnapshot>,
    pipeline_loading: bool,
    pipeline_request_revision: u64,
    pipeline_live_events: Vec<Event>,
    pipeline_tx: Sender<PipelineResult>,
    pipeline_rx: Receiver<PipelineResult>,
    status_line: String,
    fixture: Option<Node>,
    turn_tx: Sender<TurnResult>,
    turn_rx: Receiver<TurnResult>,
    cursor_path: Option<PathBuf>,
}

impl MoteApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        socket: PathBuf,
        rx: Receiver<Event>,
        fixture: Option<Node>,
    ) -> Self {
        crate::theme::install(&cc.egui_ctx);
        let (turn_tx, turn_rx) = mpsc::channel();
        let (pipeline_tx, pipeline_rx) = mpsc::channel();
        Self {
            socket,
            rx,
            chat: ChatState::default(),
            input: String::new(),
            screen: Screen::Chat,
            stage: 0,
            tab: 0,
            config_edits: std::collections::HashMap::new(),
            config_revision: 0,
            config_edit_revisions: std::collections::HashMap::new(),
            pipeline: None,
            pipeline_loading: false,
            pipeline_request_revision: 0,
            pipeline_live_events: Vec::new(),
            pipeline_tx,
            pipeline_rx,
            status_line: String::new(),
            fixture,
            turn_tx,
            turn_rx,
            cursor_path: Some(cursor_path()),
        }
    }

    fn client(&self) -> std::io::Result<Client> {
        Client::connect(&self.socket)
    }

    fn drain_events(&mut self) {
        while let Some(event) =
            apply_next_event(&mut self.chat, &self.rx, self.cursor_path.as_deref())
        {
            self.remember_pipeline_event(event);
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
            let edited_after_request = self
                .config_edit_revisions
                .get(key)
                .is_some_and(|revision| *revision > self.pipeline_request_revision);
            if !edited_after_request {
                if let Some(value) = snapshot.config_edit_value(key) {
                    self.config_edits.insert(key.into(), value);
                }
                self.config_edit_revisions.remove(key);
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
        self.pipeline_request_revision = self.config_revision;
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

    fn send_ui_event(&mut self, event: &str) {
        if let Ok(mut c) = self.client() {
            let _ = c.call("ui.event", serde_json::json!({ "event": event }));
        }
    }

    fn preview_view(&mut self, ui: &mut egui::Ui) {
        if !self.status_line.is_empty() {
            crate::theme::caption(ui, &self.status_line);
        }
        let mut clicked = Vec::new();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some(node) = &self.fixture {
                    crate::surface::draw(ui, node, &mut clicked);
                }
            });
        if let Some(event) = clicked.last() {
            self.status_line = format!("Preview action: {event}");
        }
    }

    fn chat_view(&mut self, ui: &mut egui::Ui) {
        let mut clicked: Vec<String> = Vec::new();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, m) in self.chat.messages.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.strong(&m.role);
                        crate::theme::caption(ui, m.status.label());
                    });
                    if let Some(node) = &m.surface {
                        ui.push_id(index, |ui| crate::surface::draw(ui, node, &mut clicked));
                    } else {
                        ui.label(&m.text);
                    }
                    ui.separator();
                }
                if self.chat.typing {
                    crate::theme::caption(ui, "mote is typing...");
                }
            });
        for e in clicked {
            self.send_ui_event(&e);
        }

        ui.separator();
        ui.horizontal(|ui| {
            let resp = ui.text_edit_singleline(&mut self.input);
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if crate::surface::button(ui, "Send").clicked() || enter {
                self.send_turn();
            }
        });
        if !self.status_line.is_empty() {
            ui.colored_label(ui.visuals().error_fg_color, &self.status_line);
        }
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
    }

    fn config_tab(&mut self, ui: &mut egui::Ui) {
        if self.config_edits.is_empty() {
            ui.label("No configuration snapshot is available.");
            return;
        }
        for key in CONFIG_KEYS {
            let buf = self.config_edits.entry(key.to_string()).or_default();
            ui.horizontal(|ui| {
                ui.label(key);
                if ui.text_edit_singleline(buf).changed() {
                    self.config_revision = self.config_revision.saturating_add(1);
                    self.config_edit_revisions.insert(key.into(), self.config_revision);
                }
            });
        }
        let clicked = ui
            .add_enabled_ui(!self.pipeline_loading, |ui| {
                crate::surface::button(ui, "Apply").clicked()
            })
            .inner;
        if clicked {
            self.apply_config_edits();
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

    fn apply_config(&mut self, key: &str, val: &str) {
        // Parse the value into the right JSON type.
        let value: serde_json::Value = if val == "true" || val == "false" {
            serde_json::json!(val == "true")
        } else if let Ok(n) = val.parse::<i64>() {
            serde_json::json!(n)
        } else {
            serde_json::json!(val)
        };
        if let Ok(mut c) = self.client() {
            match c.call(
                "config.set",
                serde_json::json!({ "key": key, "value": value }),
            ) {
                Ok(Ok(_)) => self.status_line = format!("set {key}"),
                Ok(Err(e)) => self.status_line = format!("{key}: {}", e.message),
                Err(e) => self.status_line = format!("{e}"),
            }
        }
    }

    fn apply_config_edits(&mut self) -> bool {
        if self.pipeline_loading {
            return false;
        }
        let edits: Vec<(String, String)> = self
            .config_edits
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        for (key, value) in edits {
            self.apply_config(&key, &value);
        }
        self.request_pipeline_snapshot();
        true
    }
}

impl eframe::App for MoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

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
            return;
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
                Screen::Pipeline => self.pipeline_view(ui),
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Status;

    fn app() -> (MoteApp, std::sync::mpsc::Sender<Event>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let (turn_tx, turn_rx) = std::sync::mpsc::channel();
        let (pipeline_tx, pipeline_rx) = std::sync::mpsc::channel();
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
                config_revision: 0,
                config_edit_revisions: Default::default(),
                pipeline: None,
                pipeline_loading: false,
                pipeline_request_revision: 0,
                pipeline_live_events: Vec::new(),
                pipeline_tx,
                pipeline_rx,
                status_line: String::new(),
                fixture: None,
                turn_tx,
                turn_rx,
                cursor_path: None,
            },
            tx,
        )
    }

    fn send(tx: &std::sync::mpsc::Sender<Event>, kind: &str, payload: serde_json::Value) {
        tx.send(Event {
            seq: 1,
            ts: 0,
            scope_id: "main".into(),
            kind: kind.into(),
            payload,
        })
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
        app.fixture = Some(Node::Action {
            label: "Preview action".into(),
            event: "gallery.preview".into(),
        });
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
            app.chat.messages[0].surface,
            Some(Node::Text {
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
        })
        .unwrap();
        tx.send(Event {
            seq: 11,
            ts: 0,
            scope_id: "main".into(),
            kind: "talk.delta".into(),
            payload: serde_json::json!({"text": "reply"}),
        })
        .unwrap();
        let mut chat = ChatState::default();

        assert!(apply_next_event(&mut chat, &rx, Some(&cursor)).is_some());
        assert_eq!(load_cursor_from(&cursor), 10);
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
            })
            .unwrap();
        let mut restarted = ChatState::default();
        assert!(apply_next_event(
            &mut restarted,
            &replay_rx,
            Some(&cursor)
        )
        .is_some());
        assert_eq!(load_cursor_from(&cursor), 11);
        assert_eq!(restarted.messages[0].text, "reply");

        std::fs::remove_file(cursor).ok();
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
        })
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
        })
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
        })
        .unwrap();
        app.drain_events();

        app.install_pipeline_snapshot(pipeline_snapshot("qwen3:4b"));

        assert_eq!(app.pipeline.as_ref().unwrap().events_for("talk")[0].seq, 13);
    }

    #[test]
    fn snapshot_preserves_only_config_edits_made_after_request_started() {
        let (mut app, _tx) = app();
        app.config_edits.insert("models.talk".into(), "before-request".into());
        app.config_edit_revisions.insert("models.talk".into(), 1);
        app.config_revision = 1;
        app.pipeline_request_revision = 1;
        app.config_edits.insert("models.talk".into(), "after-request".into());
        app.config_revision = 2;
        app.config_edit_revisions.insert("models.talk".into(), 2);

        app.install_pipeline_snapshot(pipeline_snapshot("stale-snapshot"));

        assert_eq!(app.config_edits["models.talk"], "after-request");
        app.pipeline_request_revision = 2;
        app.install_pipeline_snapshot(pipeline_snapshot("fresh-snapshot"));
        assert_eq!(app.config_edits["models.talk"], "fresh-snapshot");
    }

    #[test]
    fn apply_is_withheld_after_edit_then_refresh_until_loading_finishes() {
        let (mut app, _tx) = app();
        app.config_edits.insert("models.talk".into(), "edited-model".into());
        app.config_revision = 1;
        app.config_edit_revisions.insert("models.talk".into(), 1);

        app.pipeline_request_revision = app.config_revision;
        app.pipeline_loading = true;
        let applied = app.apply_config_edits();

        assert!(!applied);
        assert!(app.pipeline_loading);
        assert_eq!(app.config_edits["models.talk"], "edited-model");
        assert!(app.status_line.is_empty());
    }
}
