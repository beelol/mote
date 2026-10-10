//! The think-lane harness. In v0 it drives OpenCode and parses its stream into four normalized
//! event kinds. Two drivers exist: [`run_opencode`] cold-launches `opencode run` once, and
//! [`OpenCodeServer`] keeps one `opencode serve` process alive and runs many warm turns against
//! it, which is how the daemon reaches the think lane's first-token budget. Claude and Codex are
//! gated behind `models.local_only`; the daemon refuses them at startup. See docs/rfcs/v0.md
//! section 8.

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

    // Read the stream on a worker thread so the main thread can enforce the timeout. "First
    // token" of the think lane is its first produced event of any kind (for a tool-using turn
    // that is the tool call, which is what the model emits first), not the post-tool answer text.
    type FirstTimes = (Vec<ThinkEvent>, Option<std::time::Duration>, Option<std::time::Duration>);
    let (tx, rx) = mpsc::channel::<FirstTimes>();
    std::thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        let mut first_event = None;
        let mut first_text = None;
        let mut events = Vec::new();
        for line in reader.lines() {
            let Ok(line) = line else { break };
            for ev in parse_line(&line) {
                if first_event.is_none() {
                    first_event = Some(start.elapsed());
                }
                if first_text.is_none() && matches!(ev, ThinkEvent::Text { .. }) {
                    first_text = Some(start.elapsed());
                }
                events.push(ev);
            }
        }
        let _ = tx.send((events, first_event, first_text));
    });

    match rx.recv_timeout(timeout) {
        Ok((events, first_event, _first_text)) => {
            let _ = child.wait();
            Ok((events, first_event.unwrap_or_else(|| start.elapsed())))
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(format!("opencode timed out after {:?}", timeout))
        }
    }
}

/// A persistent OpenCode server (`opencode serve`). Starting the process and loading the model
/// happens once; every turn after the first is warm, so first-token latency is the model's warm
/// prefill (cached system prompt) rather than a cold `opencode run` launch. This is how the think
/// lane runs in the daemon: one server per project directory, many turns. V0-29's first-token gate
/// is met this way. The `opencode.json` in `cwd` names the provider, model and MCP server.
pub struct OpenCodeServer {
    base_url: String,
    agent: ureq::Agent,
    child: std::process::Child,
}

impl OpenCodeServer {
    /// Spawn `opencode serve` in `cwd` on an ephemeral loopback port and wait until it is listening
    /// or `ready` elapses.
    pub fn start(cwd: &std::path::Path, ready: std::time::Duration) -> Result<Self, String> {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};

        let mut child = Command::new("opencode")
            .args(["serve", "--hostname", "127.0.0.1", "--port", "0"])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("spawn opencode serve: {e}"))?;

        // The server prints "opencode server listening on http://127.0.0.1:<port>" to stdout.
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if let Some(idx) = line.find("http://127.0.0.1:") {
                    let url = line[idx..].split_whitespace().next().unwrap_or("").to_string();
                    let _ = tx.send(Some(url));
                    return;
                }
            }
            let _ = tx.send(None);
        });

        let base_url = match rx.recv_timeout(ready) {
            Ok(Some(url)) => url.trim_end_matches('/').to_string(),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("opencode serve exited before it listened".into());
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("opencode serve not ready within {ready:?}"));
            }
        };

        let agent = ureq::AgentBuilder::new()
            .timeout_read(std::time::Duration::from_secs(600))
            .build();
        Ok(Self { base_url, agent, child })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Create a session and return its id.
    pub fn create_session(&self) -> Result<String, String> {
        let resp = self
            .agent
            .post(&format!("{}/session", self.base_url))
            .send_json(serde_json::json!({}))
            .map_err(|e| format!("create session: {e}"))?;
        let v: Value = resp.into_json().map_err(|e| format!("session json: {e}"))?;
        v["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "session response had no id".to_string())
    }

    /// Send `prompt` to `session` with `model` ("provider/model"), stream the server events, parse
    /// them into think events, and measure the time to the first produced token (the first tool
    /// call or text delta) from the moment the prompt is sent. Errors on `timeout`.
    pub fn run_turn(
        &self,
        session: &str,
        model: &str,
        prompt: &str,
        timeout: std::time::Duration,
    ) -> Result<(Vec<ThinkEvent>, std::time::Duration), String> {
        use std::io::{BufRead, BufReader};

        let (provider, model_id) = model.split_once('/').unwrap_or(("ollama", model));

        // Subscribe to the event stream before sending, so no early event is missed.
        let events_resp = self
            .agent
            .get(&format!("{}/event", self.base_url))
            .call()
            .map_err(|e| format!("subscribe /event: {e}"))?;

        type Out = (Vec<ThinkEvent>, Option<std::time::Duration>);
        let (tx, rx) = std::sync::mpsc::channel::<Out>();
        let session_owned = session.to_string();
        let start = std::time::Instant::now();
        std::thread::spawn(move || {
            let reader = BufReader::new(events_resp.into_reader());
            let mut stream = ThinkStream::new();
            let mut events = Vec::new();
            let mut first = None;
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let line = line.trim();
                let Some(data) = line.strip_prefix("data:") else { continue };
                let Ok(v) = serde_json::from_str::<Value>(data.trim()) else { continue };
                let ty = v["type"].as_str().unwrap_or_default();
                let sid = v["properties"]["sessionID"].as_str().unwrap_or_default();
                // Ignore events for other sessions. Session-less events (server.connected) pass.
                if !sid.is_empty() && sid != session_owned {
                    continue;
                }
                if first.is_none() {
                    let is_delta_text = ty == "message.part.delta"
                        && v["properties"]["field"].as_str() == Some("text");
                    let is_tool = ty == "message.part.updated"
                        && v["properties"]["part"]["type"].as_str() == Some("tool");
                    if is_delta_text || is_tool {
                        first = Some(start.elapsed());
                    }
                }
                for ev in stream.feed(&v) {
                    let done = matches!(ev, ThinkEvent::Done { .. });
                    events.push(ev);
                    if done {
                        let _ = tx.send((events, first));
                        return;
                    }
                }
            }
            let _ = tx.send((events, first));
        });

        // The message POST returns only when the turn completes, so send it off-thread.
        let body = serde_json::json!({
            "model": { "providerID": provider, "modelID": model_id },
            "parts": [ { "type": "text", "text": prompt } ],
        });
        let agent = self.agent.clone();
        let url = format!("{}/session/{}/message", self.base_url, session);
        std::thread::spawn(move || {
            let _ = agent.post(&url).send_json(body);
        });

        match rx.recv_timeout(timeout) {
            Ok((events, first)) => Ok((events, first.unwrap_or_else(|| start.elapsed()))),
            Err(_) => {
                let _ = self
                    .agent
                    .post(&format!("{}/session/{}/abort", self.base_url, session))
                    .send_json(serde_json::json!({}));
                Err(format!("opencode turn timed out after {timeout:?}"))
            }
        }
    }
}

impl Drop for OpenCodeServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Accumulates OpenCode server `/event` objects into normalized think events. The server streams
/// token deltas and repeats part updates; this keeps only assistant output and emits each tool
/// call once, each tool result once, the final assistant text, and one `Done` on `session.idle`.
#[derive(Default)]
pub struct ThinkStream {
    assistant_msgs: std::collections::HashSet<String>,
    tool_called: std::collections::HashSet<String>,
    tool_resulted: std::collections::HashSet<String>,
    text_by_part: std::collections::BTreeMap<String, String>,
}

impl ThinkStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one `/event` object; returns zero or more think events to append.
    pub fn feed(&mut self, v: &Value) -> Vec<ThinkEvent> {
        let ty = v["type"].as_str().unwrap_or_default();
        let props = &v["properties"];
        match ty {
            "message.updated" => {
                let info = &props["info"];
                if info["role"].as_str() == Some("assistant") {
                    if let Some(id) = info["id"].as_str() {
                        self.assistant_msgs.insert(id.to_string());
                    }
                }
                Vec::new()
            }
            "message.part.updated" => {
                let part = &props["part"];
                let msg = part["messageID"].as_str().unwrap_or_default();
                match part["type"].as_str().unwrap_or_default() {
                    "tool" => {
                        let call = part["callID"].as_str().unwrap_or_default().to_string();
                        let status = part["state"]["status"].as_str().unwrap_or_default();
                        // parse_value maps a {type:"tool", part} object to the right event.
                        let synthetic = serde_json::json!({ "type": "tool", "part": part });
                        if status == "completed" || status == "error" {
                            if self.tool_resulted.insert(call) {
                                return parse_value(&synthetic);
                            }
                        } else if self.tool_called.insert(call) {
                            return parse_value(&synthetic);
                        }
                        Vec::new()
                    }
                    "text" if self.assistant_msgs.contains(msg) => {
                        // Keep the latest full text per part; emit it once, at session.idle.
                        if let Some(id) = part["id"].as_str() {
                            self.text_by_part.insert(id.to_string(), s(&part["text"]));
                        }
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
            "session.idle" => {
                let mut out: Vec<ThinkEvent> = self
                    .text_by_part
                    .values()
                    .filter(|t| !t.trim().is_empty())
                    .map(|t| ThinkEvent::Text { text: t.clone() })
                    .collect();
                out.push(ThinkEvent::Done { ok: true });
                out
            }
            "error" => vec![ThinkEvent::Done { ok: false }],
            _ => Vec::new(),
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

    // The persistent server streams a different event shape than `opencode run`: parts arrive
    // wrapped in `message.part.updated`, text repeats as it grows, and the turn ends with
    // `session.idle`. These are the real shapes captured from `opencode serve` 1.15.13.
    #[test]
    fn think_stream_folds_server_events() {
        let events: &[&str] = &[
            r#"{"type":"server.connected","properties":{}}"#,
            r#"{"type":"message.updated","properties":{"sessionID":"s1","info":{"id":"mUser","role":"user"}}}"#,
            // The user's prompt is echoed as a text part; it must not become a think event.
            r#"{"type":"message.part.updated","properties":{"sessionID":"s1","part":{"type":"text","id":"pU","messageID":"mUser","text":"read notes.txt"}}}"#,
            r#"{"type":"message.updated","properties":{"sessionID":"s1","info":{"id":"mA","role":"assistant"}}}"#,
            // Tool call arrives pending (empty input) then completed (with output); emit each once.
            r#"{"type":"message.part.updated","properties":{"sessionID":"s1","part":{"type":"tool","tool":"moted_file_read","callID":"c1","messageID":"mA","state":{"status":"pending","input":{}}}}}"#,
            r#"{"type":"message.part.updated","properties":{"sessionID":"s1","part":{"type":"tool","tool":"moted_file_read","callID":"c1","messageID":"mA","state":{"status":"completed","input":{"path":"notes.txt"},"output":"The secret passphrase is blue-otter-42.\n"}}}}"#,
            // Assistant text grows; keep the final.
            r#"{"type":"message.part.updated","properties":{"sessionID":"s1","part":{"type":"text","id":"pA","messageID":"mA","text":"The passphrase"}}}"#,
            r#"{"type":"message.part.updated","properties":{"sessionID":"s1","part":{"type":"text","id":"pA","messageID":"mA","text":"The passphrase is blue-otter-42."}}}"#,
            r#"{"type":"session.idle","properties":{"sessionID":"s1"}}"#,
        ];
        let mut stream = ThinkStream::new();
        let mut out = Vec::new();
        for line in events {
            out.extend(stream.feed(&serde_json::from_str::<Value>(line).unwrap()));
        }
        let tool_calls = out
            .iter()
            .filter(|e| matches!(e, ThinkEvent::ToolCall { name, .. } if name.contains("file_read")))
            .count();
        let tool_results = out
            .iter()
            .filter(|e| matches!(e, ThinkEvent::ToolResult { name, .. } if name.contains("file_read")))
            .count();
        assert_eq!(tool_calls, 1, "one tool call, not one per update");
        assert_eq!(tool_results, 1, "one tool result");
        assert!(
            out.iter()
                .any(|e| matches!(e, ThinkEvent::Text { text } if text.contains("blue-otter-42")
                    && !text.contains("read notes.txt"))),
            "final assistant text only, never the echoed user prompt"
        );
        assert!(
            !out.iter()
                .any(|e| matches!(e, ThinkEvent::Text { text } if text.contains("read notes.txt"))),
            "the user's prompt must never surface as a think text event"
        );
        assert_eq!(out.last(), Some(&ThinkEvent::Done { ok: true }));
    }

    // V0-29: a live think turn with qwen3-coder:30b-64k reads a file through the moted mcp
    // file_read tool and answers, first-token p50 under 3 s. The think lane runs against one
    // persistent `opencode serve`: the server and the model load once (a warm turn), and every
    // measured turn after that is warm, so first-token is the model's warm prefill over a cached
    // system prompt, not a cold `opencode run` launch. Skipped unless MOTE_LIVE=1 with opencode,
    // the moted binary and a warm Ollama present.
    #[test]
    fn live_think_reads_file_via_file_read() {
        use std::path::PathBuf;
        use std::time::Duration;
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

        let model = "ollama/qwen3-coder:30b-64k";
        let prompt =
            "Use the file_read tool to read notes.txt, then tell me the secret passphrase it contains.";
        let timeout = Duration::from_secs(120);

        let server =
            OpenCodeServer::start(&dir, Duration::from_secs(60)).expect("start opencode serve");

        // One warm turn loads the model and primes the cached system prompt; it is not measured.
        let warm = server.create_session().expect("warm session");
        match server.run_turn(&warm, model, prompt, timeout) {
            Ok((_, first)) => println!("  warm turn first-token {:.2} s (ignored)", first.as_secs_f64()),
            Err(e) => println!("  warm turn failed: {e}"),
        }

        let mut firsts = Vec::new();
        let mut saw_answer = false;
        let mut saw_file_read = false;
        let mut timeouts = 0;
        for _ in 0..3 {
            let session = server.create_session().expect("session");
            match server.run_turn(&session, model, prompt, timeout) {
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
        let within_budget = saw_answer && saw_file_read && !p50.is_nan() && p50 < 3.0;
        if within_budget {
            println!("  V0-29 PASS: answer via moted_file_read, first-token p50 {p50:.2} s < 3 s");
        } else {
            println!(
                "  V0-29 OPEN: completed={completed} timeouts={timeouts} answer={saw_answer} \
                 file_read_via_mcp={saw_file_read} p50={p50:.2}s (gate 3 s). Recorded."
            );
        }
        assert!(within_budget, "V0-29 first-token gate not met; see output above");
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
