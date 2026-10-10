use mote_eventlog::Event;
use mote_ui::Node;

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
    pub role: String,
    pub text: String,
    pub status: Status,
    pub surface: Option<Node>,
}

#[derive(Default)]
pub struct ChatState {
    pub messages: Vec<Message>,
    pub typing: bool,
}

impl ChatState {
    pub fn push_optimistic_user(&mut self, text: String) -> usize {
        let index = self.messages.len();
        self.messages.push(Message {
            role: "user".into(),
            text,
            status: Status::Sending,
            surface: None,
        });
        index
    }

    pub fn mark_sent(&mut self, index: usize) {
        if let Some(message) = self.messages.get_mut(index) {
            if message.status == Status::Sending {
                message.status = Status::Sent;
            }
        }
    }

    pub fn reduce(&mut self, ev: &Event) {
        match ev.kind.as_str() {
            "turn.user" => {
                let text = ev.payload["text"].as_str().unwrap_or("");
                if let Some(message) = self.messages.iter_mut().find(|message| {
                    message.role == "user"
                        && message.text == text
                        && matches!(message.status, Status::Sending | Status::Sent)
                }) {
                    message.status = Status::Delivered;
                } else {
                    self.messages.push(Message {
                        role: "user".into(),
                        text: text.into(),
                        status: Status::Delivered,
                        surface: None,
                    });
                }
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
            role: "user".into(),
            text: text.into(),
            status,
            surface: None,
        }
    }

    #[test]
    fn replayed_user_event_delivers_the_optimistic_message_without_a_duplicate() {
        for status in [Status::Sending, Status::Sent] {
            let mut chat = ChatState {
                messages: vec![user("hello", status)],
                typing: false,
            };

            chat.reduce(&event(7, "turn.user", serde_json::json!({"text": "hello"})));

            assert_eq!(chat.messages.len(), 1);
            assert!(chat.messages[0].status == Status::Delivered);
        }
    }

    #[test]
    fn repeated_delivered_user_text_remains_a_distinct_message() {
        let mut chat = ChatState {
            messages: vec![user("hello", Status::Delivered)],
            typing: false,
        };

        chat.reduce(&event(8, "turn.user", serde_json::json!({"text": "hello"})));

        assert_eq!(chat.messages.len(), 2);
        assert!(chat.messages[1].status == Status::Delivered);
    }

    #[test]
    fn optimistic_user_advances_from_sending_to_sent_to_delivered() {
        let mut chat = ChatState::default();

        let pending = chat.push_optimistic_user("hello".into());
        assert!(chat.messages[pending].status == Status::Sending);

        chat.mark_sent(pending);
        assert!(chat.messages[pending].status == Status::Sent);

        chat.reduce(&event(9, "turn.user", serde_json::json!({"text": "hello"})));
        assert_eq!(chat.messages.len(), 1);
        assert!(chat.messages[pending].status == Status::Delivered);
    }
}
