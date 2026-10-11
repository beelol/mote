use crate::{ChatMessage, CommittedSurface, Event, MessageRole, MessageStatus, ThreadSnapshot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThreadProjection {
    pub thread: ThreadSnapshot,
    pub surface: Option<CommittedSurface>,
    streaming: bool,
}

impl ThreadProjection {
    pub fn new(thread: String) -> Self {
        Self {
            thread: ThreadSnapshot {
                thread,
                messages: Vec::new(),
            },
            surface: None,
            streaming: false,
        }
    }

    pub fn resume(thread: ThreadSnapshot) -> Self {
        let streaming = thread.messages.last().is_some_and(|message| {
            message.role == MessageRole::Assistant && message.status == MessageStatus::Sent
        });
        Self {
            thread,
            surface: None,
            streaming,
        }
    }

    pub fn handles(kind: &str) -> bool {
        matches!(
            kind,
            "turn.user"
                | "talk.delta"
                | "think.text"
                | "talk.done"
                | "think.done"
                | "talk.handoff"
                | "ui.surface"
        )
    }

    pub fn apply(&mut self, event: &Event) {
        match event.kind.as_str() {
            "turn.user" | "talk.delta" | "think.text" => {
                if let Some(text) = event.payload["text"].as_str() {
                    let user = event.kind == "turn.user";
                    if !user && self.streaming {
                        if let Some(message) = self.thread.messages.last_mut() {
                            message.text.push_str(text);
                        }
                    } else {
                        self.thread.messages.push(ChatMessage {
                            id: event.seq.to_string(),
                            role: if user {
                                MessageRole::User
                            } else {
                                MessageRole::Assistant
                            },
                            text: text.into(),
                            status: if user {
                                MessageStatus::Delivered
                            } else {
                                MessageStatus::Sent
                            },
                        });
                    }
                    self.streaming = !user;
                }
            }
            "talk.done" | "think.done" | "talk.handoff" => {
                if self.streaming {
                    if let Some(message) = self.thread.messages.last_mut() {
                        message.status = MessageStatus::Delivered;
                    }
                }
                self.streaming = false;
            }
            "ui.surface" => {
                if let Some(tree) = event
                    .payload
                    .get("matrix_surface")
                    .or_else(|| event.payload.get("surface"))
                    .and_then(serde_json::Value::as_object)
                {
                    self.surface = Some(CommittedSurface {
                        thread: self.thread.thread.clone(),
                        surface_seq: event.seq,
                        surface_contract: 1,
                        tree: tree.clone(),
                    });
                }
            }
            _ => {}
        }
    }
}
