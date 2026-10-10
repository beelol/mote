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
    pub switches: usize,
}

impl Default for ObservationHarness {
    fn default() -> Self {
        let (incoming, rx) = mpsc::channel();
        let (ui_event_tx, outgoing) = mpsc::channel();
        let (_, ui_event_rx) = mpsc::channel();
        let (turn_tx, turn_rx) = mpsc::channel();
        let (pipeline_tx, pipeline_rx) = mpsc::channel();
        let ctx = egui::Context::default();
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
                ui_event_tx,
                ui_event_rx,
                cursor_path: None,
            },
            incoming,
            outgoing,
            decoder: Default::default(),
            bytes: Default::default(),
            ctx,
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
            .find_map(|message| Some((message.surface_seq?, message.surface.clone()?)))
    }

    pub fn frame(&mut self, events: Vec<egui::Event>) -> Vec<DisplayedNode> {
        self.frame_output(events).0
    }

    pub fn frame_output(
        &mut self,
        events: Vec<egui::Event>,
    ) -> (Vec<DisplayedNode>, egui::FullOutput) {
        crate::surface::observation::take();
        let output = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| self.app.chat_view(ui));
            },
        );
        (crate::surface::observation::take(), output)
    }

    pub fn queued(&self) -> Vec<serde_json::Value> {
        self.outgoing
            .try_iter()
            .map(|event| ui_event_params(&event))
            .collect()
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
        self.frame(vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }]);
        self.frame(vec![egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Default::default(),
        }]);
        assert_eq!(self.enter(), expected);
        assert_eq!(self.queued(), vec![prior_event.clone()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn surface(seq: i64, node: serde_json::Value) -> Event {
        Event {
            seq,
            ts: 0,
            scope_id: "main".into(),
            kind: "ui.surface".into(),
            payload: json!({"surface_id":"matrix", "surface":node}),
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
