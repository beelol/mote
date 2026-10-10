use mote_eventlog::Event;
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
    pub surface: Option<Node>,
}

pub struct ChatState {
    pub messages: Vec<Message>,
    pub typing: bool,
    session_id: String,
    next_local_id: u64,
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
            session_id: format!("{}-{nanos}-{counter}", std::process::id()),
            next_local_id: 0,
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

    pub fn reduce(&mut self, ev: &Event) {
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
                    return;
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
                });
            }
            "talk.delta" => {
                self.typing = true;
                let delta = ev.payload["text"].as_str().unwrap_or("");
                if let Some(last) = self.messages.last_mut() {
                    if last.role == "assistant" && last.surface.is_none() {
                        last.text.push_str(delta);
                        return;
                    }
                }
                self.messages.push(Message {
                    local_id: None,
                    role: "assistant".into(),
                    text: delta.into(),
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
                    local_id: None,
                    role: "system".into(),
                    text: format!("talk unavailable: {reason}"),
                    status: Status::Read,
                    surface: None,
                });
            }
            "ui.surface" => {
                if let Ok(bytes) = serde_json::to_vec(&ev.payload["surface"]) {
                    if let Ok(node) = mote_ui::validate(&bytes) {
                        self.messages.push(Message {
                            local_id: None,
                            role: "assistant".into(),
                            text: String::new(),
                            status: Status::Read,
                            surface: Some(node),
                        });
                    }
                }
            }
            _ => {}
        }
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
}
