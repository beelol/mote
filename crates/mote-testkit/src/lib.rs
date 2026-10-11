//! Test doubles for mote: a scripted Ollama HTTP server, and (later) a fake harness and
//! fixtures. The scripted server speaks the subset of the Ollama HTTP API that
//! `mote-localmodel` uses, records every request, and lets a test drive the responses.

pub mod gateway;

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// One request the scripted server received.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub body: Value,
}

/// What the scripted server should answer with. A test mutates this before exercising the
/// client.
#[derive(Debug, Clone)]
pub struct ScriptState {
    /// Local models as (name, size_bytes), returned by GET /api/tags.
    pub models: Vec<(String, u64)>,
    /// NDJSON lines returned by POST /api/chat.
    pub chat_chunks: Vec<String>,
    /// HTTP status for POST /api/chat (404 simulates a missing model).
    pub chat_status: u16,
}

impl Default for ScriptState {
    fn default() -> Self {
        Self {
            models: Vec::new(),
            chat_chunks: vec![
                json!({"model":"m","message":{"role":"assistant","content":"hi"},"done":false})
                    .to_string(),
                json!({"model":"m","message":{"role":"assistant","content":""},"done":true})
                    .to_string(),
            ],
            chat_status: 200,
        }
    }
}

/// A scripted Ollama server bound to a loopback port.
pub struct ScriptedServer {
    addr: SocketAddr,
    recorded: Arc<Mutex<Vec<Recorded>>>,
    state: Arc<Mutex<ScriptState>>,
    running: Arc<AtomicBool>,
}

impl ScriptedServer {
    /// Start a server on an ephemeral loopback port.
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let state = Arc::new(Mutex::new(ScriptState::default()));
        let running = Arc::new(AtomicBool::new(true));

        let r = recorded.clone();
        let s = state.clone();
        let run = running.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if !run.load(Ordering::Acquire) {
                    break;
                }
                if let Ok(stream) = stream {
                    let r = r.clone();
                    let s = s.clone();
                    std::thread::spawn(move || {
                        let _ = handle(stream, r, s);
                    });
                }
            }
        });

        Ok(Self { addr, recorded, state, running })
    }

    /// Base URL, for example `http://127.0.0.1:52345`.
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn set_models(&self, models: Vec<(String, u64)>) {
        self.state.lock().unwrap().models = models;
    }

    pub fn set_chat_chunks(&self, chunks: Vec<String>) {
        self.state.lock().unwrap().chat_chunks = chunks;
    }

    pub fn set_chat_status(&self, status: u16) {
        self.state.lock().unwrap().chat_status = status;
    }

    /// Every request recorded so far.
    pub fn recorded(&self) -> Vec<Recorded> {
        self.recorded.lock().unwrap().clone()
    }

    /// Whether any request hit the given path.
    pub fn hit(&self, path: &str) -> bool {
        self.recorded().iter().any(|r| r.path == path)
    }
}

impl Drop for ScriptedServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        // Unblock accept.
        let _ = TcpStream::connect(self.addr);
    }
}

fn handle(
    mut stream: TcpStream,
    recorded: Arc<Mutex<Vec<Recorded>>>,
    state: Arc<Mutex<ScriptState>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);

    // Request line.
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();

    // Headers.
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }

    // Body.
    let mut body_bytes = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body_bytes)?;
    }
    let body: Value = serde_json::from_slice(&body_bytes).unwrap_or(Value::Null);

    recorded.lock().unwrap().push(Recorded {
        method: method.clone(),
        path: path.clone(),
        body,
    });

    let (status, payload) = route(&method, &path, &state);
    write_response(&mut stream, status, &payload)
}

fn route(method: &str, path: &str, state: &Arc<Mutex<ScriptState>>) -> (u16, String) {
    let st = state.lock().unwrap();
    // Strip any query string.
    let path = path.split('?').next().unwrap_or(path);
    match (method, path) {
        ("GET", "/api/tags") => {
            let models: Vec<Value> = st
                .models
                .iter()
                .map(|(name, size)| json!({ "name": name, "size": size }))
                .collect();
            (200, json!({ "models": models }).to_string())
        }
        ("POST", "/api/chat") => {
            if st.chat_status != 200 {
                return (st.chat_status, json!({ "error": "model not found" }).to_string());
            }
            // NDJSON: one JSON object per line.
            (200, st.chat_chunks.join("\n"))
        }
        _ => (404, json!({ "error": "not found" }).to_string()),
    }
}

/// 60 labelled router turns, 30 per route, for the accuracy gate (V0-25). Each is (text, route)
/// where route is "talk" or "think".
pub const ROUTER_TURNS: &[(&str, &str)] = &[
    // talk: greetings, chit-chat, acknowledgements, simple one-shot replies.
    ("hey there", "talk"),
    ("good morning", "talk"),
    ("thanks so much", "talk"),
    ("ok sounds good", "talk"),
    ("how are you today", "talk"),
    ("lol that's funny", "talk"),
    ("goodnight", "talk"),
    ("what's your name", "talk"),
    ("nice to meet you", "talk"),
    ("yeah exactly", "talk"),
    ("cool, thanks", "talk"),
    ("hello!", "talk"),
    ("have a great weekend", "talk"),
    ("that makes sense", "talk"),
    ("awesome work", "talk"),
    ("no worries", "talk"),
    ("sure, go ahead", "talk"),
    ("hi, how's it going", "talk"),
    ("you're welcome", "talk"),
    ("sounds perfect", "talk"),
    ("great, appreciate it", "talk"),
    ("happy to hear that", "talk"),
    ("see you later", "talk"),
    ("yep", "talk"),
    ("that's hilarious", "talk"),
    ("good point", "talk"),
    ("morning!", "talk"),
    ("thanks for the help", "talk"),
    ("okay cool", "talk"),
    ("how was your day", "talk"),
    // think: tasks needing tools, files, code, multi-step reasoning or memory.
    ("read my config file and summarise it", "think"),
    ("refactor this function to be async", "think"),
    ("find all TODO comments in the repo", "think"),
    ("what did I tell you about my sister last week", "think"),
    ("write a script to rename these files", "think"),
    ("debug why the test is failing", "think"),
    ("search my notes for the budget numbers", "think"),
    ("open the readme and list the sections", "think"),
    ("compute the p95 latency from this log", "think"),
    ("fix the bug in the parser", "think"),
    ("summarise the last ten emails", "think"),
    ("what's in the file at src/main.rs", "think"),
    ("plan the migration in five steps", "think"),
    ("grep for the error message across the logs", "think"),
    ("remember that my meeting moved to 3pm", "think"),
    ("list the files in my downloads folder", "think"),
    ("explain what this stack trace means and fix it", "think"),
    ("convert this CSV into JSON", "think"),
    ("look up when I last changed my password", "think"),
    ("write unit tests for the config module", "think"),
    ("compare these two versions of the document", "think"),
    ("find the function that handles retries", "think"),
    ("analyse the memory graph and report cycles", "think"),
    ("draft a reply to this thread and cite the ticket", "think"),
    ("check which dependencies are out of date", "think"),
    ("trace how a request flows through the daemon", "think"),
    ("extract the action items from this transcript", "think"),
    ("rename the variable everywhere it is used", "think"),
    ("what was the decision we made about storage", "think"),
    ("build a table of the model latencies", "think"),
];

/// A fake think-lane harness that drives an MCP tool server: it performs the handshake then
/// calls one tool and returns the JSON-RPC response.
pub struct FakeHarness;

impl FakeHarness {
    /// Run `initialize`, `tools/list`, then `tools/call` for `tool` with `args`. `dispatch`
    /// stands in for the MCP server's message handler.
    pub fn call_tool<F: Fn(Value) -> Value>(dispatch: F, tool: &str, args: Value) -> Value {
        let _ = dispatch(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
        let _ = dispatch(json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
        dispatch(json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": { "name": tool, "arguments": args }
        }))
    }
}

fn write_response(stream: &mut TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Status",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body.as_bytes())?;
    stream.flush()
}
