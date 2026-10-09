//! The egui application: chat view and pipeline view.

use eframe::egui;
use mote_eventlog::Event;
use mote_ipc::Client;
use mote_ui::Node;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;

fn cursor_path() -> PathBuf {
    mote_paths::data_dir().join("client_cursor")
}

/// Load the saved cursor, or 0.
pub fn load_cursor() -> i64 {
    std::fs::read_to_string(cursor_path()).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

/// Persist the cursor so reopening resumes with nothing shown twice.
pub fn save_cursor(seq: i64) {
    let _ = std::fs::write(cursor_path(), seq.to_string());
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Sending,
    Sent,
    Delivered,
    Read,
}

impl Status {
    fn dot(&self) -> &'static str {
        match self {
            Status::Sending => "○ sending",
            Status::Sent => "◔ sent",
            Status::Delivered => "◑ delivered",
            Status::Read => "● read",
        }
    }
}

struct Message {
    role: String,
    text: String,
    status: Status,
    surface: Option<Node>,
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
    messages: Vec<Message>,
    input: String,
    typing: bool,
    screen: Screen,
    stage: usize,
    tab: usize,
    config_edits: std::collections::HashMap<String, String>,
    status_line: String,
}

impl MoteApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, socket: PathBuf, rx: Receiver<Event>) -> Self {
        // eframe follows the system light/dark theme by default (NativeOptions.follow_system_theme).
        Self {
            socket,
            rx,
            messages: Vec::new(),
            input: String::new(),
            typing: false,
            screen: Screen::Chat,
            stage: 0,
            tab: 0,
            config_edits: std::collections::HashMap::new(),
            status_line: String::new(),
        }
    }

    fn client(&self) -> std::io::Result<Client> {
        Client::connect(&self.socket)
    }

    fn drain_events(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev.kind.as_str() {
                "turn.user" => {
                    let text = ev.payload["text"].as_str().unwrap_or("").to_string();
                    self.messages.push(Message {
                        role: "user".into(),
                        text,
                        status: Status::Delivered,
                        surface: None,
                    });
                }
                "talk.delta" => {
                    self.typing = true;
                    let delta = ev.payload["text"].as_str().unwrap_or("");
                    if let Some(last) = self.messages.last_mut() {
                        if last.role == "assistant" {
                            last.text.push_str(delta);
                            continue;
                        }
                    }
                    self.messages.push(Message {
                        role: "assistant".into(),
                        text: delta.to_string(),
                        status: Status::Delivered,
                        surface: None,
                    });
                }
                "talk.done" => {
                    self.typing = false;
                    if let Some(last) = self.messages.last_mut() {
                        last.status = Status::Read;
                    }
                }
                "talk.unavailable" => {
                    self.typing = false;
                    let reason = ev.payload["reason"].as_str().unwrap_or("unavailable");
                    self.messages.push(Message {
                        role: "system".into(),
                        text: format!("talk unavailable: {reason}"),
                        status: Status::Read,
                        surface: None,
                    });
                }
                "ui.surface" => {
                    if let Ok(node) = serde_json::from_value::<Node>(ev.payload["surface"].clone()) {
                        self.messages.push(Message {
                            role: "assistant".into(),
                            text: String::new(),
                            status: Status::Read,
                            surface: Some(node),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    fn send_turn(&mut self) {
        let msg = self.input.trim().to_string();
        if msg.is_empty() {
            return;
        }
        self.input.clear();
        self.messages.push(Message {
            role: "user".into(),
            text: msg.clone(),
            status: Status::Sending,
            surface: None,
        });
        match self.client() {
            Ok(mut c) => {
                let _ = c.call("turn", serde_json::json!({ "thread": "main", "message": msg }));
                if let Some(last) = self.messages.last_mut() {
                    last.status = Status::Sent;
                }
            }
            Err(e) => self.status_line = format!("cannot reach daemon: {e}"),
        }
    }

    fn send_ui_event(&mut self, event: &str) {
        if let Ok(mut c) = self.client() {
            let _ = c.call("ui.event", serde_json::json!({ "event": event }));
        }
    }

    fn chat_view(&mut self, ui: &mut egui::Ui) {
        let mut clicked: Vec<String> = Vec::new();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for m in &self.messages {
                ui.horizontal(|ui| {
                    ui.strong(&m.role);
                    ui.weak(m.status.dot());
                });
                if let Some(node) = &m.surface {
                    render_node(ui, node, &mut clicked);
                } else {
                    ui.label(&m.text);
                }
                ui.separator();
            }
            if self.typing {
                ui.weak("mote is typing…");
            }
        });
        for e in clicked {
            self.send_ui_event(&e);
        }

        ui.separator();
        ui.horizontal(|ui| {
            let resp = ui.text_edit_singleline(&mut self.input);
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Send").clicked() || enter {
                self.send_turn();
            }
        });
        if !self.status_line.is_empty() {
            ui.colored_label(egui::Color32::RED, &self.status_line);
        }
    }

    fn pipeline_view(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (i, s) in STAGES.iter().enumerate() {
                if ui.selectable_label(self.stage == i, *s).clicked() {
                    self.stage = i;
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            for (i, t) in TABS.iter().enumerate() {
                if ui.selectable_label(self.tab == i, *t).clicked() {
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
        if ui.button("Apply").clicked() {
            let edits: Vec<(String, String)> =
                self.config_edits.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
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
            match c.call("config.set", serde_json::json!({ "key": key, "value": value })) {
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

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(self.screen == Screen::Chat, "Chat").clicked() {
                    self.screen = Screen::Chat;
                }
                if ui.selectable_label(self.screen == Screen::Pipeline, "Pipeline").clicked() {
                    self.screen = Screen::Pipeline;
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.screen {
            Screen::Chat => self.chat_view(ui),
            Screen::Pipeline => self.pipeline_view(ui),
        });
    }
}

/// Render a surface tree node into egui widgets. Clicked action events are collected.
fn render_node(ui: &mut egui::Ui, node: &Node, clicked: &mut Vec<String>) {
    match node {
        Node::Text { value } => {
            ui.label(value);
        }
        Node::Stack { children } | Node::List { children } | Node::Chat { children } => {
            for c in children {
                render_node(ui, c, clicked);
            }
        }
        Node::Row { children } => {
            ui.horizontal(|ui| {
                for c in children {
                    render_node(ui, c, clicked);
                }
            });
        }
        Node::Table { columns, rows } => {
            egui::Grid::new(ui.next_auto_id()).striped(true).show(ui, |ui| {
                for c in columns {
                    ui.strong(c);
                }
                ui.end_row();
                for row in rows {
                    for cell in row {
                        ui.label(cell);
                    }
                    ui.end_row();
                }
            });
        }
        Node::Field { label, value, .. } => {
            ui.horizontal(|ui| {
                ui.label(label);
                let mut v = value.clone();
                ui.text_edit_singleline(&mut v);
            });
        }
        Node::Toggle { label, value, .. } => {
            let mut v = *value;
            ui.checkbox(&mut v, label);
        }
        Node::Select { label, options, value, .. } => {
            ui.horizontal(|ui| {
                ui.label(label);
                egui::ComboBox::from_id_source(ui.next_auto_id())
                    .selected_text(value)
                    .show_ui(ui, |ui| {
                        for o in options {
                            let mut sel = value.clone();
                            ui.selectable_value(&mut sel, o.clone(), o);
                        }
                    });
            });
        }
        Node::Action { label, event } => {
            if ui.button(label).clicked() {
                clicked.push(event.clone());
            }
        }
        Node::Progress { value } => {
            ui.add(egui::ProgressBar::new(value.clamp(0.0, 1.0)));
        }
    }
}
