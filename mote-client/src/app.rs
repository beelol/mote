//! The egui application: chat view and pipeline view.

use crate::state::ChatState;
use eframe::egui;
use mote_eventlog::Event;
use mote_ipc::Client;
use mote_ui::Node;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

type TurnResult = (usize, Result<(), String>);

fn cursor_path() -> PathBuf {
    mote_paths::data_dir().join("client_cursor")
}

/// Load the saved cursor, or 0.
pub fn load_cursor() -> i64 {
    std::fs::read_to_string(cursor_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Persist the cursor so reopening resumes with nothing shown twice.
pub fn save_cursor(seq: i64) {
    let _ = std::fs::write(cursor_path(), seq.to_string());
}

#[derive(PartialEq)]
enum Screen {
    Chat,
    Pipeline,
}

const STAGES: [&str; 5] = ["talk", "router", "think", "memory", "ledger"];
const TABS: [&str; 4] = ["Now", "Config", "Numbers", "Requests"];
const CONFIG_KEYS: [&str; 8] = [
    "models.local_only",
    "models.talk",
    "models.router",
    "models.think",
    "budgets.talk_first_token",
    "budgets.router",
    "budgets.think_first_token",
    "packs.talk",
];

pub struct MoteApp {
    socket: PathBuf,
    rx: Receiver<Event>,
    chat: ChatState,
    input: String,
    screen: Screen,
    stage: usize,
    tab: usize,
    config_edits: std::collections::HashMap<String, String>,
    status_line: String,
    fixture: Option<Node>,
    turn_tx: Sender<TurnResult>,
    turn_rx: Receiver<TurnResult>,
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
        Self {
            socket,
            rx,
            chat: ChatState::default(),
            input: String::new(),
            screen: Screen::Chat,
            stage: 0,
            tab: 0,
            config_edits: std::collections::HashMap::new(),
            status_line: String::new(),
            fixture,
            turn_tx,
            turn_rx,
        }
    }

    fn client(&self) -> std::io::Result<Client> {
        Client::connect(&self.socket)
    }

    fn drain_events(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            self.chat.reduce(&ev);
        }
        while let Ok((pending, result)) = self.turn_rx.try_recv() {
            self.record_turn_result(pending, result);
        }
    }

    fn record_turn_result(&mut self, pending: usize, result: Result<(), String>) {
        match result {
            Ok(()) => self.chat.mark_sent(pending),
            Err(error) => self.status_line = error,
        }
    }

    fn send_turn(&mut self) {
        let msg = self.input.trim().to_string();
        if msg.is_empty() {
            return;
        }
        self.input.clear();
        let pending = self.chat.push_optimistic_user(msg.clone());
        let socket = self.socket.clone();
        let result_tx = self.turn_tx.clone();
        std::thread::spawn(move || {
            let result = match Client::connect(&socket) {
                Ok(mut client) => match client.call(
                    "turn",
                    serde_json::json!({ "thread": "main", "message": msg }),
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
            "Now" => {
                ui.label(format!("Live events for the {} stage.", STAGES[self.stage]));
            }
            "Numbers" => {
                ui.label("p50/p95 latency, tokens and cost over the last hour (from the ledger).");
            }
            "Requests" => {
                ui.label("Outbound calls with redacted payloads and timing (from the ledger).");
            }
            _ => {}
        }
    }

    fn config_tab(&mut self, ui: &mut egui::Ui) {
        for key in CONFIG_KEYS {
            let current = self.read_config(key);
            let buf = self.config_edits.entry(key.to_string()).or_insert(current);
            ui.horizontal(|ui| {
                ui.label(key);
                ui.text_edit_singleline(buf);
            });
        }
        if crate::surface::button(ui, "Apply").clicked() {
            let edits: Vec<(String, String)> = self
                .config_edits
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            for (key, val) in edits {
                self.apply_config(&key, &val);
            }
        }
    }

    fn read_config(&self, key: &str) -> String {
        if let Ok(mut c) = self.client() {
            if let Ok(Ok(v)) = c.call("config.get", serde_json::json!({ "key": key })) {
                return v["value"].to_string();
            }
        }
        String::new()
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
}

impl eframe::App for MoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        if let Some(node) = &self.fixture {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(ctx.style().visuals.panel_fill)
                        .inner_margin(16),
                )
                .show(ctx, |ui| {
                    let mut clicked = Vec::new();
                    egui::ScrollArea::both()
                        .show(ui, |ui| crate::surface::draw(ui, node, &mut clicked));
                    if let Some(event) = clicked.last() {
                        self.status_line = format!("Preview action: {event}");
                    }
                    if !self.status_line.is_empty() {
                        crate::theme::caption(ui, &self.status_line);
                    }
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
                status_line: String::new(),
                fixture: None,
                turn_tx,
                turn_rx,
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

        app.record_turn_result(pending, Ok(()));
        assert!(app.chat.messages[pending].status == Status::Sent);

        send(&_tx, "turn.user", serde_json::json!({"text": "hello"}));
        app.drain_events();
        app.record_turn_result(pending, Ok(()));
        assert!(app.chat.messages[pending].status == Status::Delivered);
    }
}
