use super::*;
use std::collections::VecDeque;
use std::io::{self, BufRead, Read};

pub use crate::surface::observation::DisplayedNode;

#[derive(Default)]
struct IncomingBytes(VecDeque<u8>);

impl Read for IncomingBytes {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let bytes = self.fill_buf()?;
        let len = bytes.len().min(out.len());
        out[..len].copy_from_slice(&bytes[..len]);
        self.consume(len);
        Ok(len)
    }
}

impl BufRead for IncomingBytes {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.0.is_empty() {
            Err(io::ErrorKind::WouldBlock.into())
        } else {
            Ok(self.0.make_contiguous())
        }
    }

    fn consume(&mut self, len: usize) {
        self.0.drain(..len);
    }
}

pub struct ObservationHarness {
    app: MoteApp,
    incoming: Sender<StreamUpdate>,
    outgoing: Receiver<crate::surface::SurfaceEvent>,
    decoder: mote_ipc::EventDecoder,
    bytes: IncomingBytes,
    ctx: egui::Context,
    viewport: egui::Vec2,
    pub switches: usize,
}

impl Default for ObservationHarness {
    fn default() -> Self {
        let (incoming, rx) = mpsc::channel();
        let (ui_event_tx, outgoing) = mpsc::channel();
        let (_, ui_event_rx) = mpsc::channel();
        let (turn_tx, turn_rx) = mpsc::channel();
        let (pipeline_tx, pipeline_rx) = mpsc::channel();
        let (config_tx, config_requests) = mpsc::channel();
        let (config_results, config_rx) = mpsc::channel();
        spawn_config_worker(PathBuf::new(), config_requests, config_results);
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        crate::theme::install(&ctx);
        Self {
            app: MoteApp {
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
                clock: crate::display_latency::InstantClock::shared(),
                display_latency: Default::default(),
                latency_report_path: None,
            },
            incoming,
            outgoing,
            decoder: Default::default(),
            bytes: Default::default(),
            ctx,
            viewport: egui::vec2(800.0, 600.0),
            switches: 0,
        }
    }
}

impl ObservationHarness {
    pub fn feed(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.0.extend(bytes);
        let mut delivered = 0;
        while let Some(event) = self.decoder.read(&mut self.bytes)? {
            let before = self.surface();
            self.incoming.send(event.into()).unwrap();
            self.app.drain_events();
            if before != self.surface() {
                self.switches += 1;
            }
            delivered += 1;
        }
        Ok(delivered)
    }

    pub fn event(&mut self, event: &Event) {
        assert_eq!(
            self.feed(&mote_ipc::encode_frame(&mote_ipc::event_envelope(event)).unwrap())
                .unwrap(),
            1
        );
    }

    pub fn surface(&self) -> Option<(i64, Node)> {
        self.app
            .chat
            .messages
            .iter()
            .find_map(|message| Some((message.surface_seq?, message.surface.as_ref()?.display_v0().clone())))
    }

    pub fn preview(&mut self, node: Node, viewport: egui::Vec2) {
        self.app.fixture = Some(crate::contract::Surface::V0(node));
        self.viewport = viewport;
    }

    pub fn accesskit_update(&mut self) -> accesskit::TreeUpdate {
        self.frame_output(vec![]).1.platform_output.accesskit_update
            .expect("accessibility must be enabled for observation")
    }

    pub fn frame(&mut self, events: Vec<egui::Event>) -> Vec<DisplayedNode> {
        self.frame_output(events).0
    }

    pub fn frame_output(
        &mut self,
        events: Vec<egui::Event>,
    ) -> (Vec<DisplayedNode>, egui::FullOutput) {
        crate::surface::observation::take();
        crate::surface::observation::take_layout();
        let output = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    self.viewport,
                )),
                events,
                ..Default::default()
            },
            |ctx| self.app.draw_frame(ctx),
        );
        (crate::surface::observation::take(), output)
    }

    pub fn queued(&self) -> Vec<serde_json::Value> {
        self.outgoing
            .try_iter()
            .map(|event| ui_event_params(&event))
            .collect()
    }

    pub fn key(&mut self, key: egui::Key, shift: bool) -> egui::FullOutput {
        let modifiers = egui::Modifiers { shift, ..Default::default() };
        let (_, mut output) = self.frame_output(vec![egui::Event::Key {
            key, physical_key: None, pressed: true, repeat: false, modifiers,
        }]);
        let (_, released) = self.frame_output(vec![egui::Event::Key {
            key, physical_key: None, pressed: false, repeat: false, modifiers,
        }]);
        output.platform_output.events.extend(released.platform_output.events);
        output
    }

    pub fn tab_past_navigation(&mut self) {
        for label in ["Chat", "Pipeline"] {
            let output = self.key(egui::Key::Tab, false);
            assert!(output.platform_output.events.iter().any(|event| matches!(event,
                egui::output::OutputEvent::FocusGained(info) if info.label.as_deref() == Some(label))),
                "missing navigation focus: {label}");
            assert!(self.queued().is_empty());
        }
    }

    pub fn edit(&mut self, text: &str) {
        self.frame(vec![
            egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true,
                repeat: false, modifiers: egui::Modifiers::COMMAND },
            egui::Event::Text(text.into()),
            egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: false,
                repeat: false, modifiers: egui::Modifiers::COMMAND },
        ]);
    }

    pub fn click(&mut self, pos: egui::Pos2) -> Vec<DisplayedNode> {
        self.frame(vec![
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
        ])
    }

    pub fn enter(&mut self) -> Vec<DisplayedNode> {
        let result = self.frame(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }]);
        self.frame(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Default::default(),
        }]);
        result
    }

    pub fn assert_isolated(
        &mut self,
        expected: &[DisplayedNode],
        candidate_positions: &[egui::Pos2],
        prior_event: &serde_json::Value,
    ) {
        assert_eq!(self.frame(vec![]), expected);
        for pos in candidate_positions {
            assert_eq!(self.click(*pos), expected);
            assert!(self.queued().is_empty(), "candidate tap emitted an event");
            assert_eq!(self.enter(), expected);
            assert!(self.queued().is_empty(), "candidate Enter emitted an event");
        }
        let action = expected
            .iter()
            .find(|node| matches!(node.node, Node::Action { .. }))
            .unwrap();
        assert_eq!(self.click(action.rect.center()), expected);
        assert_eq!(self.queued(), vec![prior_event.clone()]);
        let layout = crate::surface::observation::take_layout();
        let action_id = layout.controls.iter()
            .find(|control| control.kind == "Action" && control.path == action.path).unwrap().id;
        for _ in 0..=layout.controls.len() + layout.scrolls.len() {
            if self.ctx.memory(|memory| memory.focused()) == Some(action_id) { break; }
            self.key(egui::Key::Tab, false);
            assert_eq!(self.frame(vec![]), expected);
            assert!(self.queued().is_empty(), "focus traversal emitted an event");
        }
        assert_eq!(self.ctx.memory(|memory| memory.focused()), Some(action_id));
        assert_eq!(self.enter(), expected);
        assert_eq!(self.queued(), vec![prior_event.clone()]);
    }
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod layout_tests;

#[cfg(test)]
#[path = "accessibility_tests.rs"]
mod accessibility_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn populated_pipeline_snapshot() -> PipelineSnapshot {
        let stages = ["talk", "router", "think", "memory", "ledger"];
        let events: Vec<_> = stages
            .iter()
            .enumerate()
            .map(|(index, stage)| {
                let kind = if *stage == "ledger" {
                    "ledger.marker".to_string()
                } else {
                    format!("{stage}.marker")
                };
                let mut payload = json!({"marker": format!("now-{stage}")});
                if *stage == "ledger" {
                    payload["ledger_id"] = json!(500 + index);
                }
                json!({
                    "seq": 100 + index,
                    "ts": 5_000 + index,
                    "scope_id": "main",
                    "kind": kind,
                    "payload": payload
                })
            })
            .collect();
        let numbers: Vec<_> = stages
            .iter()
            .enumerate()
            .map(|(index, stage)| json!({
                "stage": stage,
                "calls": 101 + index,
                "latency_metric": if *stage == "talk" { "first_token" } else { "total" },
                "latency_samples": 11 + index,
                "latency_p50_ms": 21 + index,
                "latency_p95_ms": 31 + index,
                "prompt_tokens": 41 + index,
                "completion_tokens": 51 + index,
                "cost_usd": 0.0,
                "budget_ms": 61 + index
            }))
            .collect();
        let requests: Vec<_> = stages
            .iter()
            .enumerate()
            .map(|(index, stage)| json!({
                "seq": 200 + index,
                "ts": 6_000 + index,
                "lane": stage,
                "payload": {"marker": format!("request-{stage}")},
                "total_latency_ms": 71 + index,
                "first_token_ms": 81 + index,
                "status": "ok"
            }))
            .collect();
        PipelineSnapshot::from_value(json!({
            "generated_at": 5_000,
            "event_limit": 200,
            "request_limit": 200,
            "window_ms": 3_600_000,
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
            "events": events,
            "numbers": numbers,
            "requests": requests
        }))
        .unwrap()
    }

    fn rendered_labels(output: &egui::FullOutput) -> Vec<&str> {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect()
    }

    fn click_shell_text(harness: &mut ObservationHarness, label: &str) -> egui::FullOutput {
        let (_, output) = harness.frame_output(vec![]);
        let position = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing production-shell control {label}"));
        harness.frame_output(vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ])
        .1
    }

    #[test]
    fn populated_pipeline_shell_exposes_every_stage_and_tab_combination() {
        let mut harness = ObservationHarness::default();
        harness.viewport = egui::vec2(1280.0, 800.0);
        harness.app.install_pipeline_snapshot(populated_pipeline_snapshot());
        click_shell_text(&mut harness, "Pipeline");

        for (stage_index, stage) in STAGES.iter().enumerate() {
            click_shell_text(&mut harness, stage);
            for tab in TABS {
                let output = click_shell_text(&mut harness, tab);
                let labels = rendered_labels(&output);
                let text = labels.join("\n");
                for label in STAGES.into_iter().chain(TABS) {
                    assert!(
                        labels.contains(&label),
                        "{stage}/{tab} omitted {label}: {text}"
                    );
                }
                match tab {
                    "Now" => assert!(text.contains(&format!("now-{stage}")), "{stage}/{tab}: {text}"),
                    "Config" => {
                        for key in CONFIG_KEYS {
                            assert!(
                                labels.contains(&key),
                                "{stage}/{tab} omitted {key}: {text}"
                            );
                        }
                    }
                    "Numbers" => assert!(
                        labels.contains(&(101 + stage_index).to_string().as_str()),
                        "{stage}/{tab}: {text}"
                    ),
                    "Requests" => assert!(
                        text.contains(&format!("request-{stage}")),
                        "{stage}/{tab}: {text}"
                    ),
                    _ => unreachable!(),
                }
            }
        }
    }

    fn surface(seq: i64, node: serde_json::Value) -> Event {
        Event {
            seq,
            ts: 0,
            scope_id: "main".into(),
            kind: "ui.surface".into(),
            payload: json!({"surface_id":"matrix", "surface":node}),
        }
    }

    fn field() -> serde_json::Value {
        json!({"type":"field", "label":"Name", "name":"name", "value":"original"})
    }

    fn texts(harness: &mut ObservationHarness) -> Vec<String> {
        harness.frame_output(vec![]).1.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        }).collect()
    }

    fn edited_field() -> ObservationHarness {
        let mut harness = ObservationHarness::default();
        harness.event(&surface(1, field()));
        harness.frame(vec![]);
        harness.tab_past_navigation();
        harness.key(egui::Key::Tab, false);
        harness.edit("edited");
        assert!(texts(&mut harness).contains(&"edited".into()));
        assert!(harness.queued().is_empty());
        harness
    }

    #[test]
    fn interaction_field_survives_same_live_identity_and_host_value_wins() {
        let mut harness = edited_field();
        let mut replacement = field();
        replacement["label"] = json!("Renamed");
        harness.event(&surface(2, replacement.clone()));
        assert!(texts(&mut harness).contains(&"edited".into()));
        harness.edit("still focused");
        assert!(texts(&mut harness).contains(&"still focused".into()));
        replacement["value"] = json!("host update");
        harness.event(&surface(3, replacement));
        assert!(texts(&mut harness).contains(&"host update".into()));
        assert!(!texts(&mut harness).contains(&"still focused".into()));
        assert!(harness.queued().is_empty());
    }

    #[test]
    fn interaction_removed_or_retyped_field_never_resurrects_edits() {
        for replacement in [json!({"type":"text", "value":"absent"}),
            json!({"type":"toggle", "label":"Name", "name":"name", "value":true}),
            json!({"type":"select", "label":"Name", "name":"name", "value":"original", "options":["original","other"]})] {
            let mut harness = edited_field();
            harness.event(&surface(2, replacement));
            harness.frame(vec![]);
            harness.event(&surface(3, field()));
            let shown = texts(&mut harness);
            assert!(shown.contains(&"original".into()), "removed edit returned: {shown:?}");
            assert!(!shown.contains(&"edited".into()));
            harness.edit("orphan input");
            assert!(!texts(&mut harness).iter().any(|text| text.contains("orphan")));
            assert!(harness.queued().is_empty());
        }
    }

    #[test]
    fn interaction_changed_path_discards_field_and_returning_path_is_fresh() {
        let mut harness = edited_field();
        harness.event(&surface(2, json!({"type":"stack", "children":[field()]})));
        assert!(texts(&mut harness).contains(&"original".into()));
        assert!(!texts(&mut harness).contains(&"edited".into()));
        harness.event(&surface(3, field()));
        assert!(texts(&mut harness).contains(&"original".into()));
        assert!(!texts(&mut harness).contains(&"edited".into()));
        assert!(harness.queued().is_empty());
    }

    #[test]
    fn interaction_separate_surface_and_reconnect_epoch_do_not_share_edits() {
        let mut harness = edited_field();
        let mut second = surface(2, field());
        second.payload["surface_id"] = json!("other");
        harness.event(&second);
        let shown = texts(&mut harness);
        assert_eq!(shown.iter().filter(|text| *text == "edited").count(), 1);
        assert_eq!(shown.iter().filter(|text| *text == "original").count(), 1);
        harness.incoming.send(StreamUpdate::Connected { replay_through: 2 }).unwrap();
        harness.app.drain_events();
        harness.event(&surface(3, field()));
        let shown = texts(&mut harness);
        assert!(!shown.contains(&"edited".into()));
        assert_eq!(shown.iter().filter(|text| *text == "original").count(), 2);
        assert!(harness.queued().is_empty());
    }

    #[test]
    fn interaction_other_surface_edit_survives_matrix_removal_from_chat() {
        for identity in [json!("other"), serde_json::Value::Null] {
        let mut harness = ObservationHarness::default();
        harness.event(&surface(1, json!({"type":"text","value":"matrix"})));
        let mut second = surface(2, field());
        second.payload["surface_id"] = identity;
        harness.event(&second);
        harness.frame(vec![]);
        harness.tab_past_navigation();
        harness.key(egui::Key::Tab, false);
        harness.edit("edited");
        assert!(texts(&mut harness).contains(&"edited".into()));
        harness.incoming.send(StreamUpdate::Disconnected).unwrap();
        harness.app.drain_events();
        assert!(texts(&mut harness).contains(&"edited".into()));
        assert!(harness.queued().is_empty());
        }
    }

    fn text_position(harness: &mut ObservationHarness, value: &str) -> egui::Pos2 {
        harness.frame_output(vec![]).1.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == value => Some(text.pos + text.galley.rect.center().to_vec2()),
            _ => None,
        }).unwrap_or_else(|| panic!("missing {value}"))
    }

    #[test]
    fn interaction_removed_select_closes_popup_before_reinsertion() {
        let mut harness = ObservationHarness::default();
        let select = json!({"type":"select", "label":"Mode", "name":"mode", "value":"Alpha", "options":["Alpha","Bravo"]});
        harness.event(&surface(1, select.clone()));
        let pos = text_position(&mut harness, "Alpha");
        harness.click(pos);
        assert!(texts(&mut harness).contains(&"Bravo".into()));
        harness.event(&surface(2, json!({"type":"text","value":"absent"})));
        harness.frame(vec![]);
        harness.event(&surface(3, select));
        assert!(!texts(&mut harness).contains(&"Bravo".into()), "removed popup reopened");
        assert!(harness.queued().is_empty());
    }

    #[test]
    fn interaction_retyped_control_does_not_activate_with_the_old_focus() {
        let controls = [
            json!({"type":"toggle","label":"Same","name":"enabled","value":true}),
            json!({"type":"action","label":"Same","event":"refresh"}),
        ];
        for (old, replacement) in [(&controls[0], &controls[1]), (&controls[1], &controls[0])] {
            let mut harness = ObservationHarness::default();
            harness.event(&surface(1, old.clone()));
            harness.frame(vec![]);
            harness.tab_past_navigation();
            harness.key(egui::Key::Tab, false);
            harness.event(&surface(2, replacement.clone()));
            harness.key(egui::Key::Space, false);
            assert!(harness.queued().is_empty(), "new type inherited old focus");
        }
    }

    #[test]
    fn interaction_surviving_toggle_keeps_focus_when_earlier_matrix_is_removed() {
        assert_control_focus_survives_matrix_removal(
            json!({"type":"toggle","label":"Survivor","name":"enabled","value":true}),
            json!({"thread":"main","event":"change","args":{"field":"enabled","value":false}}),
        );
    }

    #[test]
    fn interaction_surviving_action_keeps_focus_when_earlier_matrix_is_removed() {
        assert_control_focus_survives_matrix_removal(
            json!({"type":"action","label":"Survivor","event":"survivor.activate"}),
            json!({"thread":"main","event":"survivor.activate","args":{}}),
        );
    }

    fn assert_control_focus_survives_matrix_removal(node: serde_json::Value, expected: serde_json::Value) {
        for identity in [json!("survivor"), serde_json::Value::Null] {
            let mut harness = ObservationHarness::default();
            harness.event(&surface(1, json!({"type":"text","value":"Earlier Matrix"})));
            let mut survivor = surface(2, node.clone());
            survivor.payload["surface_id"] = identity;
            harness.event(&survivor);
            harness.frame(vec![]);
            harness.tab_past_navigation();
            harness.key(egui::Key::Tab, false);
            let focused = harness.ctx.memory(|memory| memory.focused());
            assert!(focused.is_some());
            assert!(harness.queued().is_empty());
            harness.incoming.send(StreamUpdate::Disconnected).unwrap();
            harness.app.drain_events();
            harness.frame(vec![]);
            assert_eq!(harness.ctx.memory(|memory| memory.focused()), focused,
                "surviving control lost focus when an earlier message disappeared");
            harness.key(egui::Key::Space, false);
            assert_eq!(harness.queued(), [expected.clone()]);
        }
    }

    #[test]
    fn interaction_removed_toggle_and_action_do_not_pass_focus_to_another_surface() {
        for node in [
            json!({"type":"toggle","label":"Same","name":"enabled","value":true}),
            json!({"type":"action","label":"Same","event":"refresh"}),
        ] {
            let mut harness = ObservationHarness::default();
            harness.event(&surface(1, node.clone()));
            let mut survivor = surface(2, node);
            survivor.payload["surface_id"] = json!("survivor");
            harness.event(&survivor);
            harness.frame(vec![]);
            harness.tab_past_navigation();
            harness.key(egui::Key::Tab, false);
            assert!(harness.ctx.memory(|memory| memory.focused()).is_some());
            harness.incoming.send(StreamUpdate::Disconnected).unwrap();
            harness.app.drain_events();
            harness.key(egui::Key::Space, false);
            assert!(harness.queued().is_empty(), "removed surface transferred focus");
        }
    }

    #[test]
    fn interaction_removed_toggle_and_select_discard_local_values() {
        let mut toggle = ObservationHarness::default();
        let node = json!({"type":"toggle","label":"Enabled","name":"enabled","value":true});
        toggle.event(&surface(1, node.clone()));
        toggle.frame(vec![]);
        toggle.tab_past_navigation();
        toggle.key(egui::Key::Tab, false);
        toggle.key(egui::Key::Space, false);
        assert_eq!(toggle.queued()[0]["args"]["value"], false);
        toggle.event(&surface(2, json!({"type":"text","value":"absent"})));
        toggle.frame(vec![]);
        toggle.event(&surface(3, node));
        toggle.frame(vec![]);
        let pos = text_position(&mut toggle, "Enabled");
        toggle.click(pos);
        assert_eq!(toggle.queued()[0]["args"]["value"], false);

        let mut select = ObservationHarness::default();
        let node = json!({"type":"select","label":"Mode","name":"mode","value":"Alpha","options":["Alpha","Bravo"]});
        select.event(&surface(1, node.clone()));
        let pos = text_position(&mut select, "Alpha");
        select.click(pos);
        let pos = text_position(&mut select, "Bravo");
        select.click(pos);
        assert_eq!(select.queued()[0]["args"]["value"], "Bravo");
        select.event(&surface(2, json!({"type":"text","value":"absent"})));
        select.frame(vec![]);
        select.event(&surface(3, node));
        assert!(texts(&mut select).contains(&"Alpha".into()));
        assert!(!texts(&mut select).contains(&"Bravo".into()));
        assert!(select.queued().is_empty());
    }

    #[test]
    fn interaction_host_toggle_and_select_updates_override_local_changes() {
        let mut harness = ObservationHarness::default();
        let toggle = json!({"type":"toggle", "label":"Enabled", "name":"enabled", "value":true});
        harness.event(&surface(1, toggle.clone()));
        harness.frame(vec![]);
        harness.tab_past_navigation();
        harness.key(egui::Key::Tab, false);
        harness.key(egui::Key::Space, false);
        assert_eq!(harness.queued(), [json!({"thread":"main","event":"change","args":{"field":"enabled","value":false},"surface_seq":1,"node_path":[]})]);
        let mut host = toggle.clone();
        host["value"] = json!(false);
        harness.event(&surface(2, host));
        harness.frame(vec![]);
        harness.event(&surface(3, toggle));
        harness.frame(vec![]);
        harness.key(egui::Key::Enter, false);
        assert_eq!(harness.queued(), [json!({"thread":"main","event":"change","args":{"field":"enabled","value":false},"surface_seq":3,"node_path":[]})]);

        let select = json!({"type":"select", "label":"Mode", "name":"mode", "value":"Alpha", "options":["Alpha","Bravo","Charlie"]});
        harness.event(&surface(4, select.clone()));
        let pos = text_position(&mut harness, "Alpha");
        harness.click(pos);
        let pos = text_position(&mut harness, "Bravo");
        harness.click(pos);
        assert_eq!(harness.queued(), [json!({"thread":"main","event":"change","args":{"field":"mode","value":"Bravo"},"surface_seq":4,"node_path":[]})]);
        assert!(texts(&mut harness).contains(&"Bravo".into()));
        let mut host = select;
        host["value"] = json!("Charlie");
        harness.event(&surface(5, host));
        assert!(texts(&mut harness).contains(&"Charlie".into()));
        assert!(!texts(&mut harness).contains(&"Bravo".into()));
        assert!(harness.queued().is_empty());
    }

    fn focus_type(output: &egui::FullOutput) -> Vec<egui::WidgetType> {
        output.platform_output.events.iter().filter_map(|event| match event {
            egui::output::OutputEvent::FocusGained(info) => Some(info.typ),
            _ => None,
        }).collect()
    }

    #[test]
    fn fixture_controls_follow_tab_and_shift_tab_reading_order() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/mote-ui/fixtures");
        let mut fixtures: Vec<_> = std::fs::read_dir(root).unwrap().map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "json")).collect();
        fixtures.sort();
        assert_eq!(fixtures.len(), 10);
        let mut controls = Vec::new();
        for fixture in fixtures {
            let node: serde_json::Value = serde_json::from_slice(&std::fs::read(fixture).unwrap()).unwrap();
            let expected = match node["type"].as_str().unwrap() {
                "field" => egui::WidgetType::TextEdit,
                "toggle" => egui::WidgetType::Checkbox,
                "select" => egui::WidgetType::ComboBox,
                "action" => egui::WidgetType::Button,
                _ => continue,
            };
            controls.push(node.clone());
            let mut harness = ObservationHarness::default();
            harness.event(&surface(1, node));
            harness.frame(vec![]);
            harness.tab_past_navigation();
            assert_eq!(focus_type(&harness.key(egui::Key::Tab, false)), [expected]);
            let focused = harness.ctx.memory(|memory| memory.focused());
            harness.key(egui::Key::Tab, false);
            assert_eq!(focus_type(&harness.key(egui::Key::Tab, true)), [expected]);
            assert_eq!(harness.ctx.memory(|memory| memory.focused()), focused);
            assert!(harness.queued().is_empty());
        }
        assert_eq!(controls.len(), 4);
        let mut harness = ObservationHarness::default();
        harness.event(&surface(2, json!({"type":"stack","children":controls})));
        harness.frame(vec![]);
        harness.tab_past_navigation();
        let order = [egui::WidgetType::TextEdit, egui::WidgetType::Checkbox, egui::WidgetType::ComboBox, egui::WidgetType::Button];
        let mut identities = Vec::new();
        for expected in order {
            assert_eq!(focus_type(&harness.key(egui::Key::Tab, false)), [expected]);
            identities.push(harness.ctx.memory(|memory| memory.focused()).unwrap());
            assert!(harness.queued().is_empty());
        }
        harness.key(egui::Key::Tab, false);
        for (expected, identity) in order.into_iter().zip(identities).rev() {
            assert_eq!(focus_type(&harness.key(egui::Key::Tab, true)), [expected]);
            assert_eq!(harness.ctx.memory(|memory| memory.focused()), Some(identity));
            assert!(harness.queued().is_empty());
        }
    }

    #[test]
    fn fixture_controls_keyboard_activation_emits_only_permitted_events() {
        for (fixture, expected_event, field) in [
            (include_str!("../../crates/mote-ui/fixtures/06_field.json"), "", ""),
            (include_str!("../../crates/mote-ui/fixtures/07_toggle.json"), "change", "local_only"),
            (include_str!("../../crates/mote-ui/fixtures/09_action.json"), "approve", ""),
        ] {
            let mut harness = ObservationHarness::default();
            harness.event(&surface(7, serde_json::from_str(fixture).unwrap()));
            harness.frame(vec![]);
            harness.tab_past_navigation();
            harness.key(egui::Key::Tab, false);
            assert!(harness.queued().is_empty());
            for (key, value) in [(egui::Key::Space, false), (egui::Key::Enter, true)] {
                if expected_event.is_empty() { harness.edit("runtime text"); }
                harness.key(key, false);
                let expected = match expected_event {
                    "" => vec![],
                    "change" => vec![json!({"thread":"main","event":"change","args":{"field":field,"value":value},"surface_seq":7,"node_path":[]})],
                    _ => vec![json!({"thread":"main","event":expected_event,"args":{},"surface_seq":7,"node_path":[]})],
                };
                assert_eq!(harness.queued(), expected);
            }
        }
        for key in [egui::Key::Space, egui::Key::Enter] {
            let mut harness = ObservationHarness::default();
            harness.event(&surface(8, serde_json::from_str(include_str!("../../crates/mote-ui/fixtures/08_select.json")).unwrap()));
            harness.frame(vec![]);
            harness.tab_past_navigation();
            harness.key(egui::Key::Tab, false);
            harness.key(key, false);
            assert!(texts(&mut harness).contains(&"qwen3:0.6b".into()));
            assert!(harness.queued().is_empty());
            assert!(focus_type(&harness.key(egui::Key::Tab, false)).is_empty());
            let focus = harness.key(egui::Key::Tab, false);
            assert!(focus.platform_output.events.iter().any(|event| matches!(event,
                egui::output::OutputEvent::FocusGained(info) if info.label.as_deref() == Some("qwen3:4b"))));
            let first = harness.ctx.memory(|memory| memory.focused());
            let focus = harness.key(egui::Key::Tab, false);
            assert!(focus.platform_output.events.iter().any(|event| matches!(event,
                egui::output::OutputEvent::FocusGained(info) if info.label.as_deref() == Some("qwen3:0.6b"))));
            let second = harness.ctx.memory(|memory| memory.focused());
            assert!(first.is_some() && second.is_some() && first != second);
            harness.key(egui::Key::Tab, true);
            assert_eq!(harness.ctx.memory(|memory| memory.focused()), first);
            harness.key(egui::Key::Tab, false);
            assert_eq!(harness.ctx.memory(|memory| memory.focused()), second);
            assert!(harness.queued().is_empty());
            harness.key(key, false);
            assert_eq!(harness.queued(), [json!({"thread":"main","event":"change","args":{"field":"models.talk","value":"qwen3:0.6b"},"surface_seq":8,"node_path":[]})]);
            assert!(texts(&mut harness).contains(&"qwen3:0.6b".into()));
            assert!(!texts(&mut harness).contains(&"qwen3:4b".into()));
        }
    }

    #[test]
    fn every_fixture_frame_split_preserves_committed_display_and_events() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/mote-ui/fixtures");
        let mut fixtures: Vec<_> = std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect();
        fixtures.sort();
        assert_eq!(fixtures.len(), 10);
        let mut children = vec![json!({"type":"text", "value":"Committed spacer"}); 8];
        children.push(json!({"type":"action", "label":"Prior Refresh", "event":"refresh"}));
        let prior = surface(19, json!({"type":"stack", "children":children}));
        let prior_event = json!({"thread":"main", "event":"refresh", "args":{}, "surface_seq":19, "node_path":[8]});
        let mut splits = 0;
        for fixture in fixtures {
            let bytes = std::fs::read(&fixture).unwrap();
            let node: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_v0_payload(&node);
            let candidate = surface(20, node.clone());
            let mut frame = br#"{"event":{"seq":20,"ts":0,"scope_id":"main","kind":"ui.surface","payload":{"surface_id":"matrix","surface":"#.to_vec();
            frame.extend(bytes.trim_ascii_end());
            frame.extend(b"}}}\n");
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&frame).unwrap(),
                mote_ipc::event_envelope(&candidate)
            );
            let mut prospective = ObservationHarness::default();
            prospective.event(&candidate);
            let prospective_display = prospective.frame(vec![]);
            let positions: Vec<_> = prospective_display
                .iter()
                .map(|node| node.rect.center())
                .collect();
            for cut in 0..frame.len() {
                let mut harness = ObservationHarness::default();
                harness.event(&prior);
                let display = harness.frame(vec![]);
                assert!(!display.is_empty());
                let old_action = display
                    .iter()
                    .find(|node| matches!(node.node, Node::Action { .. }))
                    .unwrap();
                assert!(positions
                    .iter()
                    .all(|position| !old_action.rect.contains(*position)));
                assert_eq!(
                    harness.feed(&frame[..cut]).unwrap(),
                    0,
                    "{} cut {cut}",
                    fixture.display()
                );
                harness.assert_isolated(&display, &positions, &prior_event);
                assert_eq!(harness.switches, 1);
                assert_eq!(harness.feed(&frame[cut..]).unwrap(), 1);
                assert_eq!(harness.switches, 2);
                assert_eq!(
                    harness.surface(),
                    Some((20, mote_ui::validate(&bytes).unwrap()))
                );
                assert_eq!(harness.frame(vec![]), prospective_display);
                assert_ne!(harness.frame(vec![]), display);
                assert!(harness.queued().is_empty());
                assert_eq!(harness.feed(&[]).unwrap(), 0);
                assert_eq!(harness.switches, 2);

                let mut malformed = ObservationHarness::default();
                malformed.event(&prior);
                let display = malformed.frame(vec![]);
                assert_eq!(malformed.feed(&frame[..cut]).unwrap(), 0);
                assert_eq!(
                    malformed.feed(b"!\n").unwrap_err().kind(),
                    io::ErrorKind::InvalidData
                );
                malformed.assert_isolated(&display, &positions, &prior_event);
                assert_eq!(malformed.switches, 1);
                malformed.event(&candidate);
                assert_eq!(malformed.switches, 2);
                assert_eq!(malformed.frame(vec![]), prospective_display);
                splits += 1;
            }
            for invalid in [
                json!({"type":"action", "label":"Candidate", "event":"candidate", "open":true}),
                json!({"type":"stack", "children":[node, {"type":"unknown"}]}),
            ] {
                let mut harness = ObservationHarness::default();
                harness.event(&prior);
                let display = harness.frame(vec![]);
                harness.event(&surface(20, invalid));
                harness.assert_isolated(&display, &positions, &prior_event);
                assert_eq!(harness.switches, 1);
            }
        }
        eprintln!("10 fixtures, {splits} framed byte splits, {splits} malformed frames, 20 malformed payloads");
    }
}

pub fn assert_v0_payload(surface: &serde_json::Value) {
    fn keys(value: &serde_json::Value) -> Vec<String> {
        match value {
            serde_json::Value::Object(object) => object
                .iter()
                .flat_map(|(key, value)| {
                    std::iter::once(key.clone()).chain(
                        keys(value)
                            .into_iter()
                            .map(move |child| format!("{key}.{child}")),
                    )
                })
                .collect(),
            serde_json::Value::Array(values) => values.iter().flat_map(keys).collect(),
            _ => Vec::new(),
        }
    }
    let node = mote_ui::validate(&serde_json::to_vec(surface).unwrap()).unwrap();
    assert_eq!(keys(surface), keys(&serde_json::to_value(node).unwrap()));
}
