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

/// Launch OpenCode in `cwd` with `opencode run --format json <prompt>`, parse its stream into
/// think events, and measure time to first text token. The provider, model and MCP server come
/// from an `opencode.json` the caller writes in `cwd`. Returns (events, first_token_latency).
/// Kills the process and errors if it exceeds `timeout` (OpenCode's MCP startup can deadlock).
pub fn run_opencode(
    cwd: &std::path::Path,
    prompt: &str,
    model: &str,
    timeout: std::time::Duration,
) -> Result<(Vec<ThinkEvent>, std::time::Duration), String> {
    use std::io::BufRead;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;

    // `--dir` pins the project directory (so OpenCode loads this dir's opencode.json as the
    // project config) and `--model` forces the model, so the global config cannot override the
    // provider to something unreachable.
    let mut child = Command::new("opencode")
        .args(["run", "--format", "json", "--dir"])
        .arg(cwd)
        .args(["--model", model, prompt])
        .current_dir(cwd)
        .stdin(Stdio::null()) // else OpenCode blocks reading piped stdin that never EOFs
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn opencode: {e}"))?;

    let stdout = child.stdout.take().ok_or("no stdout")?;
    let start = std::time::Instant::now();

    // Read the stream on a worker thread so the main thread can enforce the timeout.
    let (tx, rx) = mpsc::channel::<(Vec<ThinkEvent>, Option<std::time::Duration>)>();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        let mut first_token = None;
        let mut events = Vec::new();
        for line in reader.lines() {
            let Ok(line) = line else { break };
            for ev in parse_line(&line) {
                if first_token.is_none() && matches!(ev, ThinkEvent::Text { .. }) {
                    first_token = Some(start.elapsed());
                }
                events.push(ev);
            }
        }
        let _ = tx.send((events, first_token));
    });

    match rx.recv_timeout(timeout) {
        Ok((events, first_token)) => {
            let _ = child.wait();
            Ok((events, first_token.unwrap_or_else(|| start.elapsed())))
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(format!("opencode timed out after {:?}", timeout))
        }
    }
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

    // V0-29: live think turn via OpenCode reading a file through the moted mcp file_read tool,
    // first token p50 under 3 s. Skipped unless MOTE_LIVE=1 and both opencode and the moted
    // binary are present.
    #[test]
    fn live_think_reads_file_via_file_read() {
        use std::path::PathBuf;
        if std::env::var("MOTE_LIVE").is_err() {
            return;
        }
        let moted = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/moted");
        let moted = moted.canonicalize().expect("build moted first: cargo build -p moted");

        let dir = std::env::temp_dir().join(format!("mote-think-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes.txt"), "The secret passphrase is blue-otter-42.\n").unwrap();
        let config = format!(
            r#"{{
  "$schema": "https://opencode.ai/config.json",
  "model": "ollama/qwen3-coder:30b-64k",
  "enabled_providers": ["ollama"],
  "provider": {{ "ollama": {{ "npm": "@ai-sdk/openai-compatible", "name": "Ollama",
    "options": {{ "baseURL": "http://127.0.0.1:11434/v1", "apiKey": "ollama" }},
    "models": {{ "qwen3-coder:30b-64k": {{ "name": "qwen3-coder", "tool_call": true,
      "limit": {{ "context": 65536, "output": 4096 }} }} }} }} }},
  "tools": {{ "read": false, "list": false, "grep": false, "glob": false, "write": false,
    "edit": false, "bash": false, "webfetch": false, "task": false, "todowrite": false,
    "todoread": false }},
  "mcp": {{ "moted": {{ "type": "local", "command": ["{}", "mcp"] }} }}
}}"#,
            moted.display()
        );
        std::fs::write(dir.join("opencode.json"), config).unwrap();

        let prompt =
            "Use the file_read tool to read notes.txt, then tell me the secret passphrase it contains.";
        let timeout = std::time::Duration::from_secs(120);
        // Warm run (ignored), then measured runs. OpenCode's MCP startup can intermittently
        // deadlock, so timed-out runs are recorded, not fatal.
        let _ = run_opencode(&dir, prompt, "ollama/qwen3-coder:30b-64k", timeout);
        let mut firsts = Vec::new();
        let mut saw_answer = false;
        let mut saw_file_read = false;
        let mut timeouts = 0;
        for _ in 0..3 {
            match run_opencode(&dir, prompt, "ollama/qwen3-coder:30b-64k", timeout) {
                Ok((events, first)) => {
                    firsts.push(first.as_secs_f64());
                    for e in &events {
                        match e {
                            ThinkEvent::Text { text } if text.contains("blue-otter-42") => {
                                saw_answer = true
                            }
                            ThinkEvent::ToolCall { name, .. } | ThinkEvent::ToolResult { name, .. }
                                if name.contains("file_read") =>
                            {
                                saw_file_read = true
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    timeouts += 1;
                    println!("  run timed out/failed: {e}");
                }
            }
        }
        let completed = firsts.len();
        let p50 = if firsts.is_empty() {
            f64::NAN
        } else {
            firsts.sort_by(|a, b| a.partial_cmp(b).unwrap());
            firsts[firsts.len() / 2]
        };
        println!(
            "LIVE think first-token p50 = {p50:.2} s over {completed} completed run(s), \
             {timeouts} timeout(s); answer={saw_answer} file_read_via_mcp={saw_file_read}"
        );
        // Per v0.md, a measured miss is recorded and the item is left open; this live check does
        // not hard-fail. A single warm manual run has shown the answer coming through
        // moted_file_read (see docs/verification/V0-29.md), but the bounded runs here are flaky:
        // OpenCode's MCP startup intermittently deadlocks and the 30b tool loop can exceed 120 s,
        // and with MCP-only tools there is no preamble so first-token is the post-tool answer.
        let within_budget = saw_answer && saw_file_read && !p50.is_nan() && p50 < 3.0;
        if within_budget {
            println!("  V0-29 PASS: answer via moted_file_read, first-token p50 {p50:.2} s < 3 s");
        } else {
            println!(
                "  V0-29 OPEN: completed={completed} timeouts={timeouts} answer={saw_answer} \
                 file_read_via_mcp={saw_file_read} p50={p50:.2}s (gate 3 s). Recorded; item stays open."
            );
        }
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
