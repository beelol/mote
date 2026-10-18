use crate::{Harness, HarnessError, HarnessReply, HarnessRequest, ModelStep};
use mote_harness::{OpenCodeSession, ServerFactory, ThinkEvent};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

const PARSER_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdapterCheckpoint {
    session_id: String,
    parser_version: u32,
}

pub struct OpenCodeAdapter {
    factory: Arc<dyn ServerFactory>,
    server: Option<Arc<dyn OpenCodeSession>>,
    model: String,
    timeout: Duration,
}

impl OpenCodeAdapter {
    pub fn new(factory: Arc<dyn ServerFactory>, model: impl Into<String>) -> Result<Self, String> {
        let model = model.into();
        if model.trim().is_empty() {
            return Err("OpenCode model must not be empty".into());
        }
        Ok(Self {
            factory,
            server: None,
            model,
            timeout: Duration::from_secs(180),
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn server(&mut self) -> Result<Arc<dyn OpenCodeSession>, HarnessError> {
        if let Some(server) = self.server.as_ref() {
            return Ok(server.clone());
        }
        let server = self.factory.start().map_err(|_| HarnessError::Unavailable)?;
        self.server = Some(server.clone());
        Ok(server)
    }

    fn checkpoint(request: &HarnessRequest) -> Result<Option<AdapterCheckpoint>, HarnessError> {
        let Some(checkpoint) = request.checkpoint.as_ref() else {
            return Ok(None);
        };
        if checkpoint.task_id != request.task.id {
            return Err(HarnessError::InvalidOutput);
        }
        let Some(value) = checkpoint.adapter.clone()
        else {
            return Ok(None);
        };
        let checkpoint: AdapterCheckpoint =
            serde_json::from_value(value).map_err(|_| HarnessError::InvalidOutput)?;
        if checkpoint.session_id.trim().is_empty()
            || checkpoint.parser_version != PARSER_VERSION
        {
            return Err(HarnessError::InvalidOutput);
        }
        Ok(Some(checkpoint))
    }

    fn convert(
        events: Vec<ThinkEvent>,
        checkpoint: AdapterCheckpoint,
    ) -> Result<HarnessReply, HarnessError> {
        let adapter_checkpoint = Some(
            serde_json::to_value(checkpoint).map_err(|_| HarnessError::InvalidOutput)?,
        );
        let mut text = String::new();
        let mut done = None;
        let mut tool = None;
        for event in events {
            match event {
                ThinkEvent::ToolCall { name, input, .. } => {
                    if name.trim().is_empty() || tool.is_some() || done.is_some() {
                        return Err(HarnessError::InvalidOutput);
                    }
                    let serde_json::Value::Object(arguments) = input else {
                        return Err(HarnessError::InvalidOutput);
                    };
                    tool = Some(ModelStep::Tool { name, arguments });
                }
                ThinkEvent::Text { text: delta } => {
                    if tool.is_some() || done.is_some() {
                        return Err(HarnessError::InvalidOutput);
                    }
                    text.push_str(&delta);
                }
                ThinkEvent::Done { ok } => {
                    if done.replace(ok).is_some() || tool.is_some() {
                        return Err(HarnessError::InvalidOutput);
                    }
                }
                ThinkEvent::ToolResult { .. } => return Err(HarnessError::InvalidOutput),
            }
        }
        if let Some(step) = tool {
            return Ok(HarnessReply {
                step,
                adapter_checkpoint,
            });
        }
        if done != Some(true) {
            return Err(HarnessError::InvalidOutput);
        }
        Ok(HarnessReply {
            step: ModelStep::Complete {
                text,
                sources: Vec::new(),
            },
            adapter_checkpoint,
        })
    }
}

impl Harness for OpenCodeAdapter {
    fn step(&mut self, request: HarnessRequest) -> Result<HarnessReply, HarnessError> {
        let prior = Self::checkpoint(&request)?;
        let server = self.server()?;
        let session_id = server
            .create_session()
            .map_err(|_| HarnessError::Unavailable)?;
        let prompt = if prior.is_some() {
            let checkpoint = request
                .checkpoint
                .as_ref()
                .ok_or(HarnessError::InvalidOutput)?;
            let durable = serde_json::to_string(checkpoint)
                .map_err(|_| HarnessError::InvalidOutput)?;
            format!(
                "Continue the task from this Mote-owned durable checkpoint. Do not assume any prior OpenCode session state. Task outcome: {}\nCheckpoint: {durable}",
                request.task.outcome
            )
        } else {
            request.task.outcome.clone()
        };
        let events = server
            .run_step(
                &session_id,
                &self.model,
                Some(&request.context),
                &prompt,
                self.timeout,
            )
            .map_err(|error| {
                if error.contains("timed out") {
                    HarnessError::Timeout
                } else {
                    HarnessError::Unavailable
                }
            })?
            .0;
        Self::convert(
            events,
            AdapterCheckpoint {
                session_id,
                parser_version: PARSER_VERSION,
            },
        )
    }
}
