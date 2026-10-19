use crate::{Harness, HarnessError, HarnessReply, HarnessRequest, ModelStep};
use schemars::schema::RootSchema;
use std::collections::BTreeSet;

const SYSTEM_PROMPT: &str = "You are Mote's native task-step planner. Return exactly one JSON value matching the supplied ModelStep schema. Propose tools but never execute them. Use only the supplied scoped context and durable task state.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepPrompt {
    model: String,
    system: String,
    input: String,
}

impl StepPrompt {
    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn system(&self) -> &str {
        &self.system
    }

    pub fn input(&self) -> &str {
        &self.input
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawStep(String);

impl RawStep {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalStepError {
    Timeout,
    Unavailable,
    Malformed,
}

pub trait LocalStepClient: Send {
    fn complete(
        &mut self,
        prompt: &StepPrompt,
        schema: &RootSchema,
    ) -> Result<RawStep, LocalStepError>;
}

pub fn model_step_schema() -> RootSchema {
    schemars::schema_for!(ModelStep)
}

pub struct NativeAdapter {
    client: Box<dyn LocalStepClient>,
    model: String,
    allowed_tools: BTreeSet<String>,
}

impl NativeAdapter {
    pub fn new(
        client: impl LocalStepClient + 'static,
        model: impl Into<String>,
    ) -> Result<Self, String> {
        let model = model.into();
        if model.trim().is_empty() || model.trim() != model {
            return Err("native model must not be empty or padded".into());
        }
        Ok(Self {
            client: Box::new(client),
            model,
            allowed_tools: BTreeSet::new(),
        })
    }

    pub fn with_allowed_tools<I, S>(mut self, tools: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for tool in tools {
            let tool = tool.into();
            if tool.trim().is_empty() || tool.trim() != tool {
                return Err("allowed tool names must not be empty or padded".into());
            }
            self.allowed_tools.insert(tool);
        }
        Ok(self)
    }

    fn prompt(&self, request: &HarnessRequest) -> Result<StepPrompt, HarnessError> {
        let input = serde_json::to_string(request).map_err(|_| HarnessError::InvalidOutput)?;
        Ok(StepPrompt {
            model: self.model.clone(),
            system: SYSTEM_PROMPT.to_owned(),
            input,
        })
    }

    fn validate(&self, step: ModelStep) -> Result<ModelStep, HarnessError> {
        let valid = match &step {
            ModelStep::Message { text } => nonempty(text),
            ModelStep::Tool { name, .. } => {
                nonempty(name) && self.allowed_tools.contains(name.as_str())
            }
            ModelStep::Complete { text, sources } => {
                nonempty(text) && sources.iter().all(|source| nonempty(source))
            }
            ModelStep::WaitForInput { question } => nonempty(question),
        };
        if valid {
            Ok(step)
        } else {
            Err(HarnessError::InvalidOutput)
        }
    }
}

impl Harness for NativeAdapter {
    fn step(&mut self, request: HarnessRequest) -> Result<HarnessReply, HarnessError> {
        let prompt = self.prompt(&request)?;
        let schema = model_step_schema();
        let raw = self
            .client
            .complete(&prompt, &schema)
            .map_err(|error| match error {
                LocalStepError::Timeout => HarnessError::Timeout,
                LocalStepError::Unavailable => HarnessError::Unavailable,
                LocalStepError::Malformed => HarnessError::InvalidOutput,
            })?;
        let step = serde_json::from_str(raw.as_str()).map_err(|_| HarnessError::InvalidOutput)?;
        Ok(HarnessReply {
            step: self.validate(step)?,
            adapter_checkpoint: None,
        })
    }
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty() && value.trim() == value
}
