//! Test doubles for mote: a scripted Ollama HTTP server, and (later) a fake harness and
//! fixtures. The scripted server speaks the subset of the Ollama HTTP API that
//! `mote-localmodel` uses, records every request, and lets a test drive the responses.

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
