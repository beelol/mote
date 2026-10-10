//! Ollama HTTP client and the memory gate.
//!
//! Every chat call streams `/api/chat` with `keep_alive` of 30 minutes and an explicit
//! `num_ctx`. `ensure` checks a model exists, enforces the memory gate against real free
//! memory, loads the model and returns the measured load time. It never pulls. Structured
//! output sends a JSON schema in `format` and the reply is parsed fail-closed.

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::io::BufRead;
use std::time::{Duration, Instant};

/// A model loads only when free memory is at least this multiple of its file size.
pub const MEMORY_GATE_FACTOR: f64 = 1.25;

/// Keep-alive sent on every call: 30 minutes.
pub const KEEP_ALIVE: &str = "30m";

/// Real available memory in bytes, read from the system (never a constant).
pub fn available_memory_bytes() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.available_memory()
}

/// Enforce the memory gate. Ok if `free >= 1.25 * size`, else an error naming free memory and
/// model size.
pub fn memory_gate(free_bytes: u64, model_size_bytes: u64) -> Result<(), String> {
    let needed = (model_size_bytes as f64 * MEMORY_GATE_FACTOR) as u64;
    if free_bytes >= needed {
        Ok(())
    } else {
        Err(format!(
            "model size {:.2} GB needs {:.2} GB free (1.25x) but only {:.2} GB is free",
            model_size_bytes as f64 / 1e9,
            needed as f64 / 1e9,
            free_bytes as f64 / 1e9
        ))
    }
}

/// A verdict that did not parse as exactly the expected JSON object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerdictError;

impl std::fmt::Display for VerdictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "verdict did not parse as the expected JSON object")
    }
}
impl std::error::Error for VerdictError {}

/// Parse a model verdict fail-closed: the whole string must be exactly the expected JSON object.
/// Trailing text, prose, unknown fields (when the target denies them) and missing fields all
/// fail.
pub fn parse_verdict<T: DeserializeOwned>(text: &str) -> Result<T, VerdictError> {
    serde_json::from_str::<T>(text).map_err(|_| VerdictError)
}

/// Options applied to every chat call.
#[derive(Debug, Clone)]
pub struct ChatOptions {
    pub num_ctx: u32,
    pub keep_alive: String,
    pub format: Option<Value>,
    /// When set, sent as Ollama's `think` flag. False disables chain-of-thought on thinking
    /// models (qwen3), which is what the latency-sensitive talk and router lanes want.
    pub think: Option<bool>,
    /// When set, sent as `options.temperature`. 0 gives a deterministic greedy decode.
    pub temperature: Option<f32>,
}

impl ChatOptions {
    pub fn new(num_ctx: u32) -> Self {
        Self { num_ctx, keep_alive: KEEP_ALIVE.to_string(), format: None, think: None, temperature: None }
    }
    pub fn with_format(mut self, schema: Value) -> Self {
        self.format = Some(schema);
        self
    }
    /// Disable chain-of-thought for low latency.
    pub fn no_think(mut self) -> Self {
        self.think = Some(false);
        self
    }
    /// Set the sampling temperature.
    pub fn temperature(mut self, t: f32) -> Self {
        self.temperature = Some(t);
        self
    }
}

/// One streamed chat chunk.
#[derive(Debug, Clone)]
pub struct ChatChunk {
    pub content: String,
    pub done: bool,
}

/// A structured reply together with the exact assistant content that was parsed.
#[derive(Debug, Clone)]
pub struct StructuredReply<T> {
    pub value: T,
    pub content: String,
}

#[derive(Debug)]
pub enum LmError {
    Http(String),
    Status(u16, String),
    Decode(String),
}

impl std::fmt::Display for LmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LmError::Http(e) => write!(f, "http error: {e}"),
            LmError::Status(c, m) => write!(f, "status {c}: {m}"),
            LmError::Decode(e) => write!(f, "decode error: {e}"),
        }
    }
}

#[derive(Debug)]
pub enum EnsureError {
    Missing(String),
    MemoryGate(String),
    Http(String),
}

impl std::fmt::Display for EnsureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnsureError::Missing(m) => write!(f, "model not found: {m}"),
            EnsureError::MemoryGate(r) => write!(f, "{r}"),
            EnsureError::Http(e) => write!(f, "http error: {e}"),
        }
    }
}

/// A client for a local Ollama server.
#[derive(Clone)]
pub struct Ollama {
    base_url: String,
    agent: ureq::Agent,
}

impl Ollama {
    /// Client for the given base URL, for example `http://127.0.0.1:11434`.
    pub fn new(base_url: impl Into<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout(Duration::from_secs(600))
            .build();
        Self { base_url: base_url.into(), agent }
    }

    /// Client for the default local Ollama.
    pub fn local() -> Self {
        Self::new("http://127.0.0.1:11434")
    }

    fn build_body(&self, model: &str, messages: Value, opts: &ChatOptions, stream: bool) -> Value {
        let mut body = json!({
            "model": model,
            "messages": messages,
            "stream": stream,
            "keep_alive": opts.keep_alive,
            "options": { "num_ctx": opts.num_ctx },
        });
        if let Some(t) = opts.temperature {
            body["options"]["temperature"] = json!(t);
        }
        if let Some(schema) = &opts.format {
            body["format"] = schema.clone();
        }
        if let Some(think) = opts.think {
            body["think"] = json!(think);
        }
        body
    }

    fn post(&self, path: &str, body: &Value) -> Result<ureq::Response, LmError> {
        let url = format!("{}{}", self.base_url, path);
        match self.agent.post(&url).send_json(body.clone()) {
            Ok(resp) => Ok(resp),
            Err(ureq::Error::Status(code, resp)) => {
                let msg = resp.into_string().unwrap_or_default();
                Err(LmError::Status(code, msg))
            }
            Err(e) => Err(LmError::Http(e.to_string())),
        }
    }

    /// List local models as (name, size_bytes) via GET /api/tags.
    pub fn list_models(&self) -> Result<Vec<(String, u64)>, LmError> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = match self.agent.get(&url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) => {
                return Err(LmError::Status(code, r.into_string().unwrap_or_default()))
            }
            Err(e) => return Err(LmError::Http(e.to_string())),
        };
        let v: Value = resp.into_json().map_err(|e| LmError::Decode(e.to_string()))?;
        let mut out = Vec::new();
        if let Some(models) = v.get("models").and_then(|m| m.as_array()) {
            for m in models {
                let name = m.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let size = m.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
                out.push((name, size));
            }
        }
        Ok(out)
    }

    /// Names of currently loaded models (GET /api/ps).
    pub fn ps(&self) -> Result<Vec<String>, LmError> {
        let url = format!("{}/api/ps", self.base_url);
        let resp = match self.agent.get(&url).call() {
            Ok(r) => r,
            Err(ureq::Error::Status(code, r)) => {
                return Err(LmError::Status(code, r.into_string().unwrap_or_default()))
            }
            Err(e) => return Err(LmError::Http(e.to_string())),
        };
        let v: Value = resp.into_json().map_err(|e| LmError::Decode(e.to_string()))?;
        let names = v
            .get("models")
            .and_then(|m| m.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Ok(names)
    }

    /// Stream a chat, calling `on_chunk` for each streamed chunk. Always sends keep_alive and an
    /// explicit num_ctx.
    pub fn chat_stream(
        &self,
        model: &str,
        messages: Value,
        opts: &ChatOptions,
        mut on_chunk: impl FnMut(ChatChunk),
    ) -> Result<(), LmError> {
        let body = self.build_body(model, messages, opts, true);
        let resp = self.post("/api/chat", &body)?;
        let reader = std::io::BufReader::new(resp.into_reader());
        for line in reader.lines() {
            let line = line.map_err(|e| LmError::Decode(e.to_string()))?;
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = serde_json::from_str(&line).map_err(|e| LmError::Decode(e.to_string()))?;
            let content = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            let done = v.get("done").and_then(|d| d.as_bool()).unwrap_or(false);
            on_chunk(ChatChunk { content, done });
        }
        Ok(())
    }

    /// Structured output: send the schema in `format`, read the single reply, and parse its
    /// content fail-closed into `T`.
    pub fn chat_structured<T: DeserializeOwned>(
        &self,
        model: &str,
        messages: Value,
        schema: Value,
        num_ctx: u32,
    ) -> Result<T, VerdictError> {
        self.chat_structured_with_raw(model, messages, schema, num_ctx)
            .map(|reply| reply.value)
    }

    /// Structured output with both the parsed value and exact assistant content.
    pub fn chat_structured_with_raw<T: DeserializeOwned>(
        &self,
        model: &str,
        messages: Value,
        schema: Value,
        num_ctx: u32,
    ) -> Result<StructuredReply<T>, VerdictError> {
        let opts = ChatOptions::new(num_ctx).with_format(schema).no_think().temperature(0.0);
        let body = self.build_body(model, messages, &opts, false);
        let resp = self.post("/api/chat", &body).map_err(|_| VerdictError)?;
        let v: Value = resp.into_json().map_err(|_| VerdictError)?;
        let content = v
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or(VerdictError)?
            .to_string();
        let value = parse_verdict::<T>(&content)?;
        Ok(StructuredReply { value, content })
    }

    /// Ensure a model is loaded. Checks it exists, enforces the memory gate against real free
    /// memory, loads it, and returns the measured load time. Never pulls.
    pub fn ensure(&self, model: &str, num_ctx: u32) -> Result<Duration, EnsureError> {
        self.ensure_with_free(model, num_ctx, available_memory_bytes())
    }

    /// Like [`Ollama::ensure`] but with an explicit free-memory figure, for tests.
    pub fn ensure_with_free(
        &self,
        model: &str,
        num_ctx: u32,
        free_bytes: u64,
    ) -> Result<Duration, EnsureError> {
        let models = self.list_models().map_err(|e| EnsureError::Http(e.to_string()))?;
        let size = models
            .iter()
            .find(|(name, _)| name == model)
            .map(|(_, size)| *size)
            .ok_or_else(|| EnsureError::Missing(model.to_string()))?;
        memory_gate(free_bytes, size).map_err(EnsureError::MemoryGate)?;

        // Load by issuing a minimal chat with keep_alive and num_ctx. No pull.
        let opts = ChatOptions::new(num_ctx);
        let started = Instant::now();
        self.chat_stream(model, json!([]), &opts, |_| {})
            .map_err(|e| EnsureError::Http(e.to_string()))?;
        Ok(started.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_testkit::ScriptedServer;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(deny_unknown_fields)]
    struct Verdict {
        route: String,
        reason: String,
    }

    #[test]
    fn chat_sends_keep_alive_and_num_ctx() {
        let srv = ScriptedServer::start().unwrap();
        let client = Ollama::new(srv.base_url());
        let opts = ChatOptions::new(32768);
        let mut chunks = Vec::new();
        client
            .chat_stream("qwen3:4b", json!([{"role":"user","content":"hi"}]), &opts, |c| {
                chunks.push(c)
            })
            .unwrap();
        assert!(!chunks.is_empty());
        let rec = srv.recorded();
        let chat = rec.iter().find(|r| r.path == "/api/chat").unwrap();
        assert_eq!(chat.body["keep_alive"], "30m");
        assert_eq!(chat.body["options"]["num_ctx"], 32768);
        assert_eq!(chat.body["stream"], true);
    }

    #[test]
    fn ensure_missing_memory_gate_and_load_time() {
        let srv = ScriptedServer::start().unwrap();
        let client = Ollama::new(srv.base_url());

        // Missing model: not in /api/tags.
        srv.set_models(vec![]);
        assert!(matches!(
            client.ensure_with_free("qwen3:4b", 32768, 128_000_000_000),
            Err(EnsureError::Missing(_))
        ));

        // Memory gate: a 500 GB model cannot load with ordinary free memory.
        srv.set_models(vec![("big:500g".to_string(), 500_000_000_000)]);
        let gated = client.ensure_with_free("big:500g", 32768, 8_000_000_000);
        match gated {
            Err(EnsureError::MemoryGate(reason)) => {
                assert!(reason.contains("500"), "reason names the model size: {reason}");
                assert!(reason.contains("free"), "reason names free memory: {reason}");
            }
            other => panic!("expected memory gate error, got {other:?}"),
        }

        // Success path: a small model with plenty of memory loads and returns a duration.
        srv.set_models(vec![("qwen3:4b".to_string(), 2_500_000_000)]);
        let dur = client.ensure_with_free("qwen3:4b", 32768, 128_000_000_000).unwrap();
        assert!(dur.as_nanos() > 0);

        // ensure never pulls.
        assert!(!srv.hit("/api/pull"), "ensure must not pull");
    }

    #[test]
    fn structured_output_parses_fail_closed() {
        // Valid parses.
        let good = r#"{"route":"think","reason":"needs tools"}"#;
        assert!(parse_verdict::<Verdict>(good).is_ok());

        // Five malformed fixtures all fail.
        let bad = [
            r#"{"route":"think","reason":"ok"} trailing words"#, // trailing text
            r#"{"route":"think","reason":"ok","extra":1}"#,       // unknown field
            r#"{"route":"think"}"#,                                // missing field
            r#"the route is think"#,                               // prose, not json
            r#"{"route":123,"reason":"ok"}"#,                     // wrong type
        ];
        for (i, b) in bad.iter().enumerate() {
            assert!(parse_verdict::<Verdict>(b).is_err(), "fixture {i} must fail: {b}");
        }
    }

    // V0-18: live check that ensure() succeeds for each approved model. Skipped unless
    // MOTE_LIVE=1 so the no-network workspace run stays green.
    #[test]
    fn live_ensure_approved_models() {
        if std::env::var("MOTE_LIVE").is_err() {
            return;
        }
        let client = Ollama::local();
        for (model, num_ctx) in
            [("qwen3:0.6b", 32768u32), ("qwen3:4b", 32768), ("qwen3-coder:30b-64k", 65536)]
        {
            let dur = client.ensure(model, num_ctx).unwrap_or_else(|e| {
                panic!("ensure({model}) failed: {e}");
            });
            println!("LIVE ensure {model} loaded in {} ms", dur.as_millis());
        }
    }

    #[test]
    fn structured_output_sends_format_schema() {
        let srv = ScriptedServer::start().unwrap();
        // Scripted reply: one object whose content is the verdict JSON.
        srv.set_chat_chunks(vec![json!({
            "message": {"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"chit chat\"}"},
            "done": true
        })
        .to_string()]);
        let client = Ollama::new(srv.base_url());
        let schema = json!({"type":"object","properties":{"route":{"type":"string"},"reason":{"type":"string"}}});
        let v: Verdict = client
            .chat_structured("qwen3:0.6b", json!([{"role":"user","content":"hey"}]), schema, 32768)
            .unwrap();
        assert_eq!(v.route, "talk");
        let rec = srv.recorded();
        let chat = rec.iter().find(|r| r.path == "/api/chat").unwrap();
        assert!(chat.body.get("format").is_some(), "format schema must be sent");
        assert_eq!(chat.body["model"], "qwen3:0.6b");
        assert_eq!(chat.body["options"]["num_ctx"], 32768);
        assert_eq!(chat.body["options"]["temperature"], 0.0);
        assert_eq!(chat.body["think"], false);
        assert_eq!(chat.body["stream"], false);
    }

    #[test]
    fn structured_output_returns_raw_content_with_the_parsed_value() {
        let srv = ScriptedServer::start().unwrap();
        let content = r#"{"route":"talk","reason":"chit chat"}"#;
        srv.set_chat_chunks(vec![json!({
            "message": {"role":"assistant","content": content},
            "done": true
        })
        .to_string()]);
        let client = Ollama::new(srv.base_url());
        let schema = json!({
            "type":"object",
            "properties":{"route":{"type":"string"},"reason":{"type":"string"}}
        });

        let reply: StructuredReply<Verdict> = client
            .chat_structured_with_raw(
                "qwen3:0.6b",
                json!([{"role":"user","content":"hey"}]),
                schema,
                32768,
            )
            .unwrap();

        assert_eq!(reply.value.route, "talk");
        assert_eq!(reply.content, content);
    }
}
