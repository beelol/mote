//! The think-lane harness. In v0 it launches OpenCode via `opencode run --format json` and
//! parses its stream into four normalized event kinds. Claude and Codex are gated behind
//! `models.local_only`; the daemon refuses them at startup. See docs/rfcs/v0.md section 8.

use serde_json::Value;

/// A normalized think-lane event. These are the only four kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum ThinkEvent {
    Text { text: String },
    ToolCall { name: String, id: Option<String>, input: Value },
    ToolResult { name: String, id: Option<String>, output: String, is_error: bool },
    Done { ok: bool },
}

fn s(v: &Value) -> String {
    v.as_str().map(str::to_string).unwrap_or_default()
}

/// Parse one OpenCode stream line (a JSON object) into zero or more think events.
pub fn parse_line(line: &str) -> Vec<ThinkEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    parse_value(&v)
}

/// Parse a decoded OpenCode event.
pub fn parse_value(v: &Value) -> Vec<ThinkEvent> {
    let ty = v["type"].as_str().unwrap_or_default();
    let part = &v["part"];
    match ty {
        "text" | "reasoning" => vec![ThinkEvent::Text { text: s(&part["text"]) }],
        "tool" | "tool_use" => {
            let name = s(&part["tool"]);
            let state = &part["state"];
            let status = s(&state["status"]);
            let id = part["callID"].as_str().map(str::to_string);
            if status == "completed" || status == "error" {
                vec![ThinkEvent::ToolResult {
                    name,
                    id,
                    output: s(&state["output"]),
                    is_error: status == "error",
                }]
            } else {
                vec![ThinkEvent::ToolCall { name, id, input: state["input"].clone() }]
            }
        }
        "step_finish" => {
            if part["reason"] == "stop" {
                vec![ThinkEvent::Done { ok: true }]
            } else {
                Vec::new()
            }
        }
        "error" => vec![ThinkEvent::Done { ok: false }],
        _ => Vec::new(),
    }
}

/// Parse a whole recorded transcript (newline-delimited JSON).
pub fn parse_transcript(text: &str) -> Vec<ThinkEvent> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .flat_map(parse_line)
        .collect()
}

/// Harness kinds. OpenCode is local; Claude and Codex are subscription harnesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harness {
    OpenCode,
    Claude,
    Codex,
}

impl Harness {
    pub fn is_subscription(self) -> bool {
        matches!(self, Harness::Claude | Harness::Codex)
    }
}

/// Classify a configured think model/harness name.
pub fn classify(name: &str) -> Harness {
    let lower = name.to_lowercase();
    if lower.contains("claude") {
        Harness::Claude
    } else if lower.contains("codex") {
        Harness::Codex
    } else {
        Harness::OpenCode
    }
}

/// Enforce `models.local_only`: while it is true, a Claude or Codex think harness is refused with
/// a reason the daemon logs at startup.
pub fn enforce_local_only(local_only: bool, think_model: &str) -> Result<(), String> {
    if local_only && classify(think_model).is_subscription() {
        return Err(format!(
            "models.local_only is true but the think harness names a subscription harness: {think_model}. \
             The owner flips the flag in person."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRANSCRIPT: &str = r#"{"type":"step_start"}
{"type":"text","part":{"text":"Let me read the file."}}
{"type":"tool","part":{"tool":"file_read","callID":"c1","state":{"status":"running","input":{"path":"a.txt"}}}}
{"type":"tool","part":{"tool":"file_read","callID":"c1","state":{"status":"completed","output":"file contents"}}}
{"type":"text","part":{"text":"The file says hello."}}
{"type":"step_finish","part":{"reason":"stop","tokens":{"input":10,"output":5}}}"#;

    #[test]
    fn parses_four_event_kinds_from_transcript() {
        let events = parse_transcript(TRANSCRIPT);
        assert!(events.iter().any(|e| matches!(e, ThinkEvent::Text { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, ThinkEvent::ToolCall { name, .. } if name == "file_read")));
        assert!(events
            .iter()
            .any(|e| matches!(e, ThinkEvent::ToolResult { output, .. } if output == "file contents")));
        assert_eq!(events.last(), Some(&ThinkEvent::Done { ok: true }));
    }

    #[test]
    fn error_event_is_done_not_ok() {
        let events = parse_line(r#"{"type":"error","error":{"message":"boom"}}"#);
        assert_eq!(events, vec![ThinkEvent::Done { ok: false }]);
    }

    #[test]
    fn local_only_refuses_claude_and_codex() {
        assert!(enforce_local_only(true, "claude-opus-4-8").is_err());
        assert!(enforce_local_only(true, "codex").is_err());
        assert!(enforce_local_only(true, "qwen3-coder:30b-64k").is_ok());
        // When the flag is off, the harness is allowed.
        assert!(enforce_local_only(false, "claude-opus-4-8").is_ok());
    }
}
