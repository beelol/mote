//! Product-neutral JSON Lines protocol for a local voice listener.

use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_LINE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    Production,
    Fixture,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Owner,
    Agent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptLine {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Event {
    Ready {
        input: String,
        model: String,
        t_ms: u64,
    },
    Gate {
        open: bool,
        t_ms: u64,
    },
    Level {
        source: Source,
        value: f32,
        t_ms: u64,
    },
    Words {
        id: u64,
        text: String,
        t_ms: u64,
    },
    Utterance {
        id: u64,
        text: String,
        complete: bool,
        start_ms: u64,
        end_ms: u64,
        t_ms: u64,
    },
    Dropped {
        id: u64,
        text: String,
        reason: String,
        t_ms: u64,
    },
    Barge {
        id: u64,
        text: String,
        stop_word: bool,
        t_ms: u64,
    },
    Spoke {
        line: u64,
        event: String,
        phrase: usize,
        t_ms: u64,
    },
    Error {
        message: String,
        t_ms: u64,
    },
    Paused {
        apps: Vec<String>,
        t_ms: u64,
    },
    Resumed {
        t_ms: u64,
    },
    End {
        t_ms: u64,
        #[serde(default)]
        held_max_ms: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Speak {
        line: u64,
        text: String,
    },
    Restore,
    Stop {
        line: Option<u64>,
        at: Option<String>,
    },
    Hint {
        text: String,
    },
    Suppress {
        ms: u64,
    },
    Feed {
        pcm: String,
    },
    FeedEnd,
    Simulate {
        #[serde(default)]
        speech: Option<String>,
        #[serde(default)]
        voice: Option<String>,
        #[serde(default)]
        speechlike: Option<f32>,
        #[serde(default)]
        noise: Option<String>,
        #[serde(default)]
        words: Option<String>,
        #[serde(default)]
        gain: Option<f32>,
    },
    Script {
        lines: Vec<ScriptLine>,
    },
    Quit,
}

impl Command {
    pub fn fixture_only(&self) -> bool {
        matches!(
            self,
            Self::Feed { .. } | Self::FeedEnd | Self::Simulate { .. } | Self::Script { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Frame {
    Command(Command),
    Event(Event),
}

#[derive(Debug)]
pub enum ProtocolError {
    Empty,
    TooLarge,
    MultipleLines,
    Invalid(serde_json::Error),
    FixtureCommandInProduction,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("voice protocol frame is empty"),
            Self::TooLarge => formatter.write_str("voice protocol frame exceeds the size limit"),
            Self::MultipleLines => {
                formatter.write_str("voice protocol input contains multiple lines")
            }
            Self::Invalid(error) => write!(formatter, "invalid voice protocol frame: {error}"),
            Self::FixtureCommandInProduction => {
                formatter.write_str("fixture-only voice command refused in production mode")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

pub fn decode_line(line: &str) -> Result<Frame, ProtocolError> {
    let line = checked_line(line)?;
    let value: serde_json::Value = serde_json::from_str(line).map_err(ProtocolError::Invalid)?;
    if value.get("cmd").is_some() {
        serde_json::from_value(value)
            .map(Frame::Command)
            .map_err(ProtocolError::Invalid)
    } else {
        serde_json::from_value(value)
            .map(Frame::Event)
            .map_err(ProtocolError::Invalid)
    }
}

pub fn decode_command(line: &str, mode: InputMode) -> Result<Command, ProtocolError> {
    let line = checked_line(line)?;
    let command: Command = serde_json::from_str(line).map_err(ProtocolError::Invalid)?;
    if mode == InputMode::Production && command.fixture_only() {
        return Err(ProtocolError::FixtureCommandInProduction);
    }
    Ok(command)
}

pub fn encode_line<T: Serialize>(value: &T) -> Result<String, ProtocolError> {
    let mut line = serde_json::to_string(value).map_err(ProtocolError::Invalid)?;
    if line.len() > MAX_LINE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    line.push('\n');
    Ok(line)
}

fn checked_line(line: &str) -> Result<&str, ProtocolError> {
    if line.len() > MAX_LINE_BYTES {
        return Err(ProtocolError::TooLarge);
    }
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty() {
        return Err(ProtocolError::Empty);
    }
    if line.contains(['\n', '\r']) {
        return Err(ProtocolError::MultipleLines);
    }
    Ok(line)
}
