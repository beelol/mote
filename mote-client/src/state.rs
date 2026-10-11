use mote_eventlog::Event;
#[cfg(test)]
use mote_ui::Node;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub type LocalMessageId = String;

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    Sending,
    Sent,
    Delivered,
    Read,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Sending => "sending",
            Self::Sent => "sent",
            Self::Delivered => "delivered",
            Self::Read => "read",
        }
    }
}

pub struct Message {
    pub local_id: Option<LocalMessageId>,
    pub role: String,
    pub text: String,
    pub status: Status,
    pub surface: Option<crate::contract::Surface>,
    pub surface_id: Option<String>,
    pub surface_seq: Option<i64>,
    pub surface_instance: i64,
    pub render_revision: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceReduction {
    pub revision: u64,
    pub superseded_revision: Option<u64>,
}

pub struct ChatState {
    pub messages: Vec<Message>,
    pub typing: bool,
    pub matrix_epoch: u64,
    matrix_replay_through: i64,
    session_id: String,
    next_local_id: u64,
    diagnostic_cursor: i64,
    next_render_revision: u64,
}

impl Default for ChatState {
    fn default() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let counter = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            messages: Vec::new(),
            typing: false,
            matrix_epoch: 0,
            matrix_replay_through: 0,
            session_id: format!("{}-{nanos}-{counter}", std::process::id()),
            next_local_id: 0,
            diagnostic_cursor: 0,
            next_render_revision: 1,
        }
    }
}

impl ChatState {
    pub fn push_optimistic_user(&mut self, text: String) -> LocalMessageId {
        let local_id = format!("{}-{}", self.session_id, self.next_local_id);
        self.next_local_id += 1;
        self.messages.push(Message {
            local_id: Some(local_id.clone()),
            role: "user".into(),
            text,
            status: Status::Sending,
            surface: None,
            surface_id: None,
            surface_seq: None,
            surface_instance: 0,
            render_revision: None,
        });
        local_id
    }

    pub fn mark_sent(&mut self, local_id: &str) {
        if let Some(message) = self
            .messages
            .iter_mut()
            .find(|message| message.local_id.as_deref() == Some(local_id))
        {
            if message.status == Status::Sending {
                message.status = Status::Sent;
            }
        }
    }

    pub fn remove_failed(&mut self, local_id: &str) -> Option<String> {
        let index = self.messages.iter().position(|message| {
            message.local_id.as_deref() == Some(local_id)
                && matches!(message.status, Status::Sending | Status::Sent)
        })?;
        Some(self.messages.remove(index).text)
    }

    pub fn invalidate_matrix(&mut self) {
        self.messages.retain(|message| message.surface_id.as_deref() != Some("matrix"));
        self.matrix_epoch = self.matrix_epoch.wrapping_add(1);
    }

    pub fn connect_matrix(&mut self, replay_through: i64) {
        self.invalidate_matrix();
        self.matrix_replay_through = replay_through;
    }

    pub fn reduce(&mut self, ev: &Event) -> Option<SurfaceReduction> {
        if ev.kind == "ui.surface" {
            return self.reduce_surface(ev);
        }
        match ev.kind.as_str() {
            "turn.user" => {
                let text = ev.payload["text"].as_str().unwrap_or("");
                let local_id = ev.payload["client_message_id"].as_str();
                let identity_conflicts = local_id.is_some_and(|local_id| {
                    self.messages
                        .iter()
                        .any(|message| message.local_id.as_deref() == Some(local_id))
                });
                if let Some(existing) = local_id.and_then(|local_id| {
                    self.messages.iter_mut().find(|message| {
                        message.local_id.as_deref() == Some(local_id)
                            && message.text == text
                            && matches!(message.status, Status::Sending | Status::Sent)
                    })
                }) {
                    existing.status = Status::Delivered;
                    return None;
                }
                self.messages.push(Message {
                    local_id: if identity_conflicts {
                        None
                    } else {
                        local_id.map(str::to_owned)
                    },
                    role: "user".into(),
                    text: text.into(),
                    status: Status::Delivered,
                    surface: None,
                    surface_id: None,
                    surface_seq: None,
                    surface_instance: 0,
                    render_revision: None,
                });
            }
            "talk.delta" => {
                self.typing = true;
                let delta = ev.payload["text"].as_str().unwrap_or("");
                if let Some(last) = self.messages.last_mut() {
                    if last.role == "assistant" && last.surface.is_none() {
                        last.text.push_str(delta);
                        return None;
                    }
                }
                self.messages.push(Message {
                    local_id: None,
                    role: "assistant".into(),
                    text: delta.into(),
                    status: Status::Delivered,
                    surface: None,
                    surface_id: None,
                    surface_seq: None,
                    surface_instance: 0,
                    render_revision: None,
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
                    local_id: None,
                    role: "system".into(),
                    text: format!("talk unavailable: {reason}"),
                    status: Status::Read,
                    surface: None,
                    surface_id: None,
                    surface_seq: None,
                    surface_instance: 0,
                    render_revision: None,
                });
            }
            "ui.diagnostic" => {
                if ev.seq <= self.diagnostic_cursor {
                    return None;
                }
                self.diagnostic_cursor = ev.seq;
                let diagnostics = ev.payload["diagnostics"].as_array()?;
                let mut text = String::from("Matrix source rejected");
                if let Some(candidate) = ev.payload["candidate"].as_str() {
                    text.push_str(&format!(" (candidate: {candidate})"));
                }
                for diagnostic in diagnostics {
                    let code = diagnostic["code"].as_str().unwrap_or("E_HOST");
                    let message = diagnostic["message"].as_str().unwrap_or("Source rejected");
                    text.push_str(&format!("\n{code}"));
                    if let (Some(start), Some(end)) = (
                        diagnostic["span"]["start"].as_u64(),
                        diagnostic["span"]["end"].as_u64(),
                    ) {
                        text.push_str(&format!(" at bytes {start}..{end}"));
                    }
                    text.push_str(&format!(": {message}"));
                    if let Some(hint) = diagnostic["hint"].as_str() {
                        text.push_str(&format!("\nHint: {hint}"));
                    }
                }
                self.messages.push(Message {
                    local_id: None,
                    role: "system".into(),
                    text,
                    status: Status::Read,
                    surface: None,
                    surface_id: None,
                    surface_seq: None,
                    surface_instance: 0,
                    render_revision: None,
                });
            }
            _ => {}
        }
        None
    }

    fn reduce_surface(&mut self, ev: &Event) -> Option<SurfaceReduction> {
        let surface = crate::contract::decode(&ev.payload)?;
        let surface_id = ev.payload["surface_id"].as_str();
        if surface_id == Some("matrix") && ev.seq <= self.matrix_replay_through {
            return None;
        }
        let revision = self.next_render_revision;
        self.next_render_revision = self.next_render_revision.saturating_add(1);
        let mut superseded_revision = None;
        if let Some(existing) = surface_id.and_then(|surface_id| {
            self.messages.iter_mut().find(|message| {
                message.surface_id.as_deref() == Some(surface_id)
            })
        }) {
            superseded_revision = existing.render_revision;
            existing.surface = Some(surface);
            existing.surface_seq = (surface_id == Some("matrix")).then_some(ev.seq);
            existing.render_revision = Some(revision);
            existing.status = Status::Read;
        } else {
            self.messages.push(Message {
                local_id: None,
                role: "assistant".into(),
                text: String::new(),
                status: Status::Read,
                surface: Some(surface),
                surface_id: surface_id.map(str::to_owned),
                surface_seq: (surface_id == Some("matrix")).then_some(ev.seq),
                surface_instance: ev.seq,
                render_revision: Some(revision),
            });
        }
        Some(SurfaceReduction { revision, superseded_revision })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(seq: i64, kind: &str, payload: serde_json::Value) -> Event {
        Event {
            seq,
            ts: 0,
            scope_id: "main".into(),
            kind: kind.into(),
            payload,
        }
    }

    fn user(text: &str, status: Status) -> Message {
        Message {
            local_id: None,
            role: "user".into(),
            text: text.into(),
            status,
            surface: None,
            surface_id: None,
            surface_seq: None,
            surface_instance: 0,
            render_revision: None,
        }
    }

    #[test]
    fn replayed_user_event_delivers_the_optimistic_message_without_a_duplicate() {
        for status in [Status::Sending, Status::Sent] {
            let mut chat = ChatState::default();
            let local_id = chat.push_optimistic_user("hello".into());
            let message = chat.messages.last_mut().unwrap();
            message.status = status;

            chat.reduce(&event(
                7,
                "turn.user",
                serde_json::json!({"text": "hello", "client_message_id": local_id}),
            ));

            assert_eq!(chat.messages.len(), 1);
            assert!(chat.messages[0].status == Status::Delivered);
        }
    }

    #[test]
    fn repeated_delivered_user_text_remains_a_distinct_message() {
        let mut chat = ChatState {
            ..ChatState::default()
        };
        chat.messages.push(user("hello", Status::Delivered));

        chat.reduce(&event(8, "turn.user", serde_json::json!({"text": "hello"})));

        assert_eq!(chat.messages.len(), 2);
        assert!(chat.messages[1].status == Status::Delivered);
    }

    #[test]
    fn optimistic_user_advances_from_sending_to_sent_to_delivered() {
        let mut chat = ChatState::default();

        let pending = chat.push_optimistic_user("hello".into());
        assert!(chat.messages[0].status == Status::Sending);

        chat.mark_sent(&pending);
        assert!(chat.messages[0].status == Status::Sent);

        chat.reduce(&event(
            9,
            "turn.user",
            serde_json::json!({"text": "hello", "client_message_id": pending}),
        ));
        assert_eq!(chat.messages.len(), 1);
        assert!(chat.messages[0].status == Status::Delivered);
    }

    #[test]
    fn colliding_message_id_with_different_text_stays_distinct() {
        let mut chat = ChatState::default();
        let pending = chat.push_optimistic_user("hello".into());

        chat.reduce(&event(
            10,
            "turn.user",
            serde_json::json!({"text": "goodbye", "client_message_id": pending.clone()}),
        ));

        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat.messages[0].text, "hello");
        assert!(chat.messages[0].status == Status::Sending);
        assert_eq!(chat.messages[0].local_id.as_deref(), Some(pending.as_str()));
        assert_eq!(chat.messages[1].text, "goodbye");
        assert!(chat.messages[1].status == Status::Delivered);
        assert!(chat.messages[1].local_id.is_none());
    }

    #[test]
    fn message_id_reused_after_delivery_stays_distinct() {
        let mut chat = ChatState::default();
        let pending = chat.push_optimistic_user("hello".into());
        let delivered = || {
            event(
                11,
                "turn.user",
                serde_json::json!({"text": "hello", "client_message_id": pending.clone()}),
            )
        };

        chat.reduce(&delivered());
        chat.reduce(&delivered());

        assert_eq!(chat.messages.len(), 2);
        assert!(chat.messages[0].status == Status::Delivered);
        assert_eq!(chat.messages[0].local_id.as_deref(), Some(pending.as_str()));
        assert!(chat.messages[1].status == Status::Delivered);
        assert!(chat.messages[1].local_id.is_none());
    }

    #[test]
    fn diagnostic_replay_is_visible_deduplicated_and_preserves_surface() {
        let surface = event(20, "ui.surface", serde_json::json!({
            "surface_id":"matrix", "surface":{"type":"text", "value":"Prior dashboard"}
        }));
        let diagnostic = event(21, "ui.diagnostic", serde_json::json!({
            "candidate":"draft", "diagnostics":[{
                "code":"E_PARSE_INCOMPLETE", "severity":"error", "span":{"start":7,"end":9},
                "declaration_path":["screen"], "message":"expected closing brace", "hint":"Close the block"
            }]
        }));
        let mut live = ChatState::default();
        live.reduce(&surface);
        live.reduce(&diagnostic);
        assert_eq!(live.messages.len(), 2);
        assert_eq!(live.messages[1].role, "system");
        for part in ["draft", "E_PARSE_INCOMPLETE", "7..9", "expected closing brace", "Close the block"] {
            assert!(live.messages[1].text.contains(part), "missing {part}");
        }
        assert!(live.messages[1].surface.is_none());
        live.reduce(&diagnostic);
        assert_eq!(live.messages.len(), 2);
        let mut replay = ChatState::default();
        replay.reduce(&surface);
        replay.reduce(&diagnostic);
        assert_eq!(replay.messages[1].text, live.messages[1].text);
        assert_eq!(live.messages[0].surface.as_ref().map(crate::contract::Surface::display_v0),
            Some(&Node::Text { value:"Prior dashboard".into() }));
        let without_hint = event(22, "ui.diagnostic", serde_json::json!({"diagnostics":[{
            "code":"E_PARSE_TOKEN", "span":{"start":1,"end":2}, "message":"unexpected token", "hint":null
        }]}));
        live.reduce(&without_hint);
        assert_eq!(live.messages.len(), 3);
        assert!(!live.messages[2].text.contains("Hint:"));
    }

    #[test]
    fn repeated_surface_identity_replaces_the_existing_dashboard() {
        let mut chat = ChatState::default();
        let first = chat.reduce(&event(
            20,
            "ui.surface",
            serde_json::json!({
                "surface_id":"matrix",
                "surface":{"type":"text", "value":"first"}
            }),
        ));
        let second = chat.reduce(&event(
            21,
            "ui.surface",
            serde_json::json!({
                "surface_id":"matrix",
                "surface":{"type":"text", "value":"second"}
            }),
        ));

        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].surface_id.as_deref(), Some("matrix"));
        assert_eq!(
            chat.messages[0].surface.as_ref().map(crate::contract::Surface::display_v0),
            Some(&Node::Text {
                value: "second".into()
            })
        );
        assert_eq!(chat.messages[0].surface_seq, Some(21));
        assert_eq!(chat.messages[0].surface_instance, 20);
        assert_eq!(first, Some(SurfaceReduction {
            revision: 1,
            superseded_revision: None,
        }));
        assert_eq!(second, Some(SurfaceReduction {
            revision: 2,
            superseded_revision: Some(1),
        }));
        assert_eq!(chat.messages[0].render_revision, Some(2));
    }

    #[test]
    fn dual_surface_reduces_once_and_both_invalid_preserves_committed_surface() {
        let mut chat = ChatState::default();
        let accepted = chat.reduce(&event(
            30,
            "ui.surface",
            serde_json::json!({
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
        ));

        assert_eq!(chat.messages.len(), 1);
        assert_eq!(accepted, Some(SurfaceReduction {
            revision: 1,
            superseded_revision: None,
        }));
        let retained = chat.messages[0].surface.as_ref().unwrap();
        assert!(retained.matrix_v1().is_some());
        assert_eq!(retained.display_v0(), &Node::Text {
            value: "Fallback dashboard".into()
        });

        let rejected = chat.reduce(&event(
            31,
            "ui.surface",
            serde_json::json!({
                "surface_id":"matrix",
                "surface":{"type":"unknown"},
                "matrix_surface":{"version":2, "root":{"type":"unknown"}}
            }),
        ));

        assert_eq!(rejected, None);
        assert_eq!(chat.messages.len(), 1);
        assert_eq!(chat.messages[0].render_revision, Some(1));
        assert!(chat.messages[0].surface.as_ref().unwrap().matrix_v1().is_some());
    }
}
