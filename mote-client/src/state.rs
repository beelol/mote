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
    pub fn reduce(&mut self, ev: &Event) {
        match ev.kind.as_str() {
            "turn.user" => self.messages.push(Message {
                role: "user".into(),
                text: ev.payload["text"].as_str().unwrap_or("").into(),
                status: Status::Delivered,
                surface: None,
            }),
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
