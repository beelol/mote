//! `moted`, the mote-cloud daemon.
//!
//! v0 starts the control socket over the event log. Lanes and methods are added by later
//! items. A second daemon on the same live socket exits non-zero naming the socket path.

mod bench;
mod charter;
mod config;
mod embed;
mod gate;
mod lanes;
mod matrix;
mod mcp;
mod pipeline;
mod service;
mod watchers;

use charter::{Charter, CharterWatcher};
use config::ConfigStore;
use gate::OllamaGate;
use lanes::{
    build_think_pack, run_think_model_call, run_turn, ThinkInputs, ThinkOutcome, TalkInputs,
    TalkOutcome, TALK_BUDGET, THINK_BUDGET,
};
use mote_context::ContextStore;
use mote_harness::OpenCodeServer;
use mote_localmodel::Ollama;
use mote_eventlog::{EventLog, Subscription};
use mote_ipc::{codes, Dispatch, RpcError, Server};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use watchers::{WriteDecision, Watchers};

const TALK_CTX: u32 = 32768;
const ROUTER_CTX: u32 = 32768;
/// A think turn may cold-start the server and load the 30b model on a thread's first turn.
const THINK_TIMEOUT: Duration = Duration::from_secs(180);

/// Per-thread persistent `opencode serve` processes for the think lane. The server and the model
/// load once per thread; later turns on that thread are warm. See docs/rfcs/v0.md section 8.
struct ThinkServers {
    data_dir: PathBuf,
    moted_exe: PathBuf,
    servers: HashMap<String, Arc<OpenCodeServer>>,
}

impl ThinkServers {
    fn new(data_dir: PathBuf) -> Self {
        let moted_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("moted"));
        Self { data_dir, moted_exe, servers: HashMap::new() }
    }

    /// Get the thread's server, starting one (with its `opencode.json`) on first use.
    fn get(&mut self, thread_id: &str, think_model: &str) -> Result<Arc<OpenCodeServer>, String> {
        if let Some(s) = self.servers.get(thread_id) {
            return Ok(s.clone());
        }
        let safe: String =
            thread_id.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        let dir = self.data_dir.join("think").join(safe);
        std::fs::create_dir_all(&dir).map_err(|e| format!("think dir: {e}"))?;
        std::fs::write(dir.join("opencode.json"), think_config(&self.moted_exe, think_model))
            .map_err(|e| format!("write opencode.json: {e}"))?;
        let server = Arc::new(OpenCodeServer::start(&dir, Duration::from_secs(60))?);
        self.servers.insert(thread_id.to_string(), server.clone());
        Ok(server)
    }
}

/// The `opencode.json` for a think server: the Ollama provider, the configured think model, every
/// built-in tool off, and the `moted mcp` tool server. Tools reach the model only through moted.
fn think_config(moted_exe: &Path, model: &str) -> String {
    json!({
        "$schema": "https://opencode.ai/config.json",
        "model": format!("ollama/{model}"),
        "enabled_providers": ["ollama"],
        "provider": { "ollama": { "npm": "@ai-sdk/openai-compatible", "name": "Ollama",
            "options": { "baseURL": "http://127.0.0.1:11434/v1", "apiKey": "ollama" },
            "models": { model: { "name": "think", "tool_call": true,
                "limit": { "context": 65536, "output": 4096 } } } } },
        "tools": { "read": false, "list": false, "grep": false, "glob": false, "write": false,
            "edit": false, "bash": false, "webfetch": false, "task": false, "todowrite": false,
            "todoread": false },
        "mcp": { "moted": { "type": "local",
            "command": [moted_exe.to_string_lossy(), "mcp"] } }
    })
    .to_string()
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

struct Daemon {
    log: EventLog,
    config: Mutex<ConfigStore>,
    character: Arc<Mutex<Charter>>,
    store: Mutex<ContextStore>,
    client: Ollama,
    watchers: Mutex<Watchers>,
    think: Mutex<ThinkServers>,
    matrix_sources: Mutex<HashMap<String, String>>,
    data_dir: PathBuf,
    started: std::time::Instant,
}

impl Daemon {
    fn pipeline_snapshot(&self) -> Result<pipeline::PipelineSnapshot, RpcError> {
        let config = {
            let config = self.config.lock().unwrap();
            pipeline::CONFIG_KEYS
                .into_iter()
                .filter_map(|key| {
                    config.get_json(key).map(|value| (key.to_string(), value))
                })
                .collect()
        };
        let store = self.store.lock().unwrap();
        pipeline::build_snapshot(&self.log, &store, config, now_ms())
            .map_err(|error| RpcError::internal(format!("read pipeline snapshot: {error}")))
    }

    fn compile_matrix(&self, source: &str) -> Result<Value, Value> {
        let snapshot = self.pipeline_snapshot().map_err(|error| {
            json!([{"code":"E_HOST","severity":"error","message":error.message}])
        })?;
        let bindings = matrix::host_bindings(&snapshot.config, &snapshot);
        matrix::compile(source, &bindings).map_err(|errors| matrix::diagnostics(&errors))
    }

    fn render_matrix(&self, thread: &str, source: &str, retain: bool) -> Result<Value, RpcError> {
        match self.compile_matrix(source) {
            Ok(surface) => {
                let response = commit_surface(&self.log, thread, &surface)?;
                if retain {
                    self.matrix_sources
                        .lock()
                        .unwrap()
                        .insert(thread.into(), source.into());
                }
                Ok(json!({"ok": true, "valid": response["valid"]}))
            }
            Err(diagnostics) => Ok(json!({"ok": false, "diagnostics": diagnostics})),
        }
    }

    fn refresh_matrix(&self, thread: &str) -> Result<Option<Value>, RpcError> {
        let source = self.matrix_sources.lock().unwrap().get(thread).cloned();
        source
            .map(|source| self.render_matrix(thread, &source, false))
            .transpose()
    }

    fn handle_turn(&self, params: &Value) -> Result<Value, RpcError> {
        let thread_id = params
            .get("thread")
            .and_then(|t| t.as_str())
            .ok_or_else(|| RpcError::invalid_params("turn needs a thread id"))?;
        let message = params
            .get("message")
            .and_then(|m| m.as_str())
            .ok_or_else(|| RpcError::invalid_params("turn needs a message"))?;
        let client_message_id = params
            .get("client_message_id")
            .map(|value| {
                value.as_str().ok_or_else(|| {
                    RpcError::invalid_params("turn client_message_id must be a string")
                })
            })
            .transpose()?;
        let mut user_payload = json!({ "text": message });
        if let Some(client_message_id) = client_message_id {
            user_payload["client_message_id"] = json!(client_message_id);
        }
        self.log
            .append(thread_id, "turn.user", &user_payload)
            .map_err(|error| RpcError::internal(format!("record turn: {error}")))?;

        let (talk_model, router_model, think_model, budget, think_budget, charter_prose) = {
            let cfg = self.config.lock().unwrap();
            let talk = cfg.get_json("models.talk").and_then(|v| v.as_str().map(String::from));
            let router = cfg.get_json("models.router").and_then(|v| v.as_str().map(String::from));
            let think = cfg.get_json("models.think").and_then(|v| v.as_str().map(String::from));
            let budget = cfg
                .get_json("packs.talk")
                .and_then(|v| v.as_i64())
                .map(|n| n as usize)
                .unwrap_or(TALK_BUDGET);
            let think_budget = cfg
                .get_json("packs.think")
                .and_then(|v| v.as_i64())
                .map(|n| n as usize)
                .unwrap_or(THINK_BUDGET);
            let prose = self.character.lock().unwrap().prose.clone();
            (talk, router, think, budget, think_budget, prose)
        };
        let talk_model = talk_model.ok_or_else(|| RpcError::internal("models.talk unset"))?;
        let router_model = router_model.ok_or_else(|| RpcError::internal("models.router unset"))?;
        let think_model = think_model.ok_or_else(|| RpcError::internal("models.think unset"))?;

        let store = self.store.lock().unwrap();
        let profile = store.get_profile("profile").ok().flatten().unwrap_or_default();
        let working_set = store.working_set_text(thread_id).unwrap_or_default();
        // The talk pack carries the last 8 memories by retrieval (v0.md section 6).
        let memories = store
            .memory_retrieve(message, 8)
            .map(|hits| hits.iter().map(|h| h.text.clone()).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default();
        let inputs = TalkInputs {
            charter: &charter_prose,
            profile: &profile,
            memories: &memories,
            working_set: &working_set,
            turn: message,
        };
        let outcome = run_turn(
            &self.log, &store, &self.client, thread_id, &talk_model, TALK_CTX, &router_model,
            ROUTER_CTX, &inputs, budget,
        );
        let turn_ts = now_ms();
        Ok(match outcome {
            TalkOutcome::Done { ledger_id, pack_id, answer } => {
                lanes::record_turn(&store, thread_id, turn_ts, message, &answer);
                json!({ "status": "done", "ledger_id": ledger_id, "pack_id": pack_id })
            }
            TalkOutcome::Unavailable(reason) => json!({ "status": "unavailable", "reason": reason }),
            // The router handed off: run the think lane against the thread's persistent server.
            TalkOutcome::Handoff => {
                let think_inputs = ThinkInputs {
                    charter: &charter_prose,
                    profile: &profile,
                    memories: &memories,
                    working_set: &working_set,
                    turn: message,
                };
                let (pack, system, turn) = build_think_pack(&think_inputs, think_budget);
                let full_model = format!("ollama/{think_model}");
                let server = self.think.lock().unwrap().get(thread_id, &think_model);
                match server.and_then(|server| {
                    let session = server.create_session()?;
                    Ok((server, session))
                }) {
                    Ok((server, session)) => {
                        match run_think_model_call(
                            &self.log,
                            &store,
                            thread_id,
                            &think_model,
                            &pack,
                            &system,
                            &turn,
                            || {
                                server.run_turn(
                                    &session,
                                    &full_model,
                                    Some(&system),
                                    &turn,
                                    THINK_TIMEOUT,
                                )
                            },
                        ) {
                            ThinkOutcome::Done { ledger_id, pack_id, answer } => {
                                lanes::record_turn(&store, thread_id, turn_ts, message, &answer);
                                json!({
                                    "status": "think_done", "ledger_id": ledger_id,
                                    "pack_id": pack_id
                                })
                            }
                            ThinkOutcome::Failed(reason) => {
                                json!({ "status": "think_failed", "reason": reason })
                            }
                        }
                    }
                    Err(reason) => {
                        let _ = self.log.append(
                            thread_id,
                            "think.failed",
                            &json!({ "reason": reason.clone() }),
                        );
                        json!({ "status": "think_failed", "reason": reason })
                    }
                }
            }
        })
    }
}

fn validate_surface(surface: &Value) -> (Value, bool) {
    let bytes = serde_json::to_vec(surface).expect("a JSON value always serializes");
    let (node, valid) = match mote_ui::validate(&bytes) {
        Ok(node) => (node, true),
        Err(diagnostic) => (mote_ui::Node::Text { value: diagnostic.0 }, false),
    };
    let node = serde_json::to_value(node).expect("a surface node always serializes");
    (node, valid)
}

fn commit_surface(log: &EventLog, thread: &str, surface: &Value) -> Result<Value, RpcError> {
    let (node, valid) = validate_surface(surface);
    log.append(thread, "ui.surface", &json!({ "surface": node }))
        .map_err(|error| RpcError::internal(format!("commit surface: {error}")))?;
    Ok(json!({ "ok": true, "valid": valid }))
}

impl Dispatch for Daemon {
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "ping" => Ok(json!({ "ok": true })),
            "status" => {
                let uptime = self.started.elapsed().as_secs();
                let last_seq = self.log.max_seq().unwrap_or(0);
                let loaded = self.client.ps().unwrap_or_default();
                Ok(json!({
                    "socket": mote_paths::socket_path().to_string_lossy(),
                    "uptime_s": uptime,
                    "loaded_models": loaded,
                    "last_seq": last_seq,
                }))
            }
            "config.get" => {
                let key = params.get("key").and_then(|k| k.as_str()).ok_or_else(|| {
                    RpcError::invalid_params("config.get needs a string key")
                })?;
                let value = self.config.lock().unwrap().get_json(key);
                Ok(json!({ "key": key, "value": value }))
            }
            "config.set" => {
                let key = params.get("key").and_then(|k| k.as_str()).ok_or_else(|| {
                    RpcError::invalid_params("config.set needs a string key")
                })?;
                let value = params
                    .get("value")
                    .ok_or_else(|| RpcError::invalid_params("config.set needs a value"))?;
                let seq = self
                    .config
                    .lock()
                    .unwrap()
                    .set_json(key, value)
                    .map_err(|e| RpcError::invalid_params(e.to_string()))?;
                Ok(json!({ "seq": seq }))
            }
            "config.revert" => {
                let seq = params.get("seq").and_then(|s| s.as_i64()).ok_or_else(|| {
                    RpcError::invalid_params("config.revert needs a seq")
                })?;
                let new_seq = self
                    .config
                    .lock()
                    .unwrap()
                    .revert(seq)
                    .map_err(|e| RpcError::invalid_params(e.to_string()))?;
                Ok(json!({ "seq": new_seq }))
            }
            "pipeline.snapshot" => {
                let snapshot = self.pipeline_snapshot()?;
                serde_json::to_value(snapshot)
                    .map_err(|error| RpcError::internal(format!("encode pipeline snapshot: {error}")))
            }
            "character.info" => {
                let c = self.character.lock().unwrap();
                Ok(json!({
                    "name": c.front.name,
                    "tools": c.front.tools,
                    "realms": c.front.realms,
                    "prose": c.prose,
                }))
            }
            "tools.allowed" => {
                let tool = params.get("tool").and_then(|t| t.as_str()).ok_or_else(|| {
                    RpcError::invalid_params("tools.allowed needs a tool name")
                })?;
                let c = self.character.lock().unwrap();
                match charter::check_tool(&c, tool, &self.log) {
                    Ok(()) => Ok(json!({ "allowed": true })),
                    Err(reason) => Ok(json!({ "allowed": false, "reason": reason })),
                }
            }
            "turn" => self.handle_turn(&params),
            "ui.event" => {
                let event = params.get("event").and_then(|e| e.as_str()).unwrap_or("");
                let thread = params.get("thread").and_then(|t| t.as_str()).unwrap_or("main");
                let args = params.get("args").cloned().unwrap_or_else(|| json!({}));
                if event == "change" {
                    let field = args.get("field").and_then(Value::as_str).ok_or_else(|| {
                        RpcError::invalid_params("change needs a string field")
                    })?;
                    let value = args.get("value").ok_or_else(|| {
                        RpcError::invalid_params("change needs a value")
                    })?;
                    if !matrix::accepts_change(field, value) {
                        return Err(RpcError::invalid_params(
                            "change does not match an exposed field contract",
                        ));
                    }
                    let seq = self
                        .config
                        .lock()
                        .unwrap()
                        .set_json(field, value)
                        .map_err(|error| RpcError::invalid_params(error.to_string()))?;
                    self.log
                        .append(thread, "ui.event", &json!({ "event": event, "args": args }))
                        .map_err(|error| RpcError::internal(format!("record UI event: {error}")))?;
                    let rendered = self.refresh_matrix(thread)?;
                    return Ok(json!({"ok": true, "seq": seq, "rendered": rendered}));
                }
                if event == "refresh" {
                    self.log
                        .append(thread, "ui.event", &json!({ "event": event, "args": args }))
                        .map_err(|error| RpcError::internal(format!("record UI event: {error}")))?;
                    let rendered = self.refresh_matrix(thread)?;
                    return Ok(json!({"ok": true, "rendered": rendered}));
                }
                self.log
                    .append(thread, "ui.event", &json!({ "event": event, "args": args }))
                    .ok();
                if let Some(id) = event
                    .strip_prefix("approve:")
                    .and_then(|id| id.parse::<i64>().ok())
                    .filter(|id| *id > 0)
                {
                    let executed = self
                        .watchers
                        .lock()
                        .unwrap()
                        .approve_for_thread(id, thread, now_ms())
                        .map_err(|error| RpcError::internal(error.to_string()))?;
                    return Ok(json!({ "ok": true, "approval": id, "executed": executed }));
                }
                if let Some(id) = event
                    .strip_prefix("deny:")
                    .and_then(|id| id.parse::<i64>().ok())
                    .filter(|id| *id > 0)
                {
                    let denied = self
                        .watchers
                        .lock()
                        .unwrap()
                        .deny_for_thread(id, thread, now_ms())
                        .map_err(|error| RpcError::internal(error.to_string()))?;
                    return Ok(json!({ "ok": true, "approval": id, "denied": denied }));
                }
                Ok(json!({ "ok": true }))
            }
            // Validate a surface tree fail-closed and commit it as a `ui.surface` event on the
            // thread scope for the renderer to draw. An invalid tree becomes a text node carrying
            // the diagnostic (v0.md section 12). This is the seam the inferred-UI lane and the
            // matrix runtime meet at; the tree is the consumer contract.
            "surface.render" => {
                let thread =
                    params.get("thread").and_then(|t| t.as_str()).unwrap_or("main").to_string();
                let surface = params
                    .get("surface")
                    .ok_or_else(|| RpcError::invalid_params("surface.render needs a surface"))?;
                commit_surface(&self.log, &thread, surface)
            }
            "matrix.render" => {
                let thread = params.get("thread").and_then(Value::as_str).unwrap_or("main");
                let source = params
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| RpcError::invalid_params("matrix.render needs source text"))?;
                self.render_matrix(thread, source, true)
            }
            "schedule.add" => {
                let cron = params.get("cron").and_then(|c| c.as_str()).ok_or_else(|| {
                    RpcError::invalid_params("schedule.add needs a cron")
                })?;
                let prompt = params.get("prompt").and_then(|p| p.as_str()).ok_or_else(|| {
                    RpcError::invalid_params("schedule.add needs a prompt")
                })?;
                let enabled = params.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true);
                let id = self
                    .watchers
                    .lock()
                    .unwrap()
                    .add_schedule(cron, prompt, enabled, now_ms())
                    .map_err(|e| RpcError::internal(e.to_string()))?;
                Ok(json!({ "id": id }))
            }
            "file.write" => {
                let thread = params.get("thread").and_then(|t| t.as_str()).unwrap_or("main");
                let path = params.get("path").and_then(|p| p.as_str()).ok_or_else(|| {
                    RpcError::invalid_params("file.write needs a path")
                })?;
                let content = params.get("content").and_then(|c| c.as_str()).unwrap_or("");
                let (decision, card) = {
                    let watchers = self.watchers.lock().unwrap();
                    let decision = watchers
                        .request_file_write(
                            &self.data_dir,
                            thread,
                            std::path::Path::new(path),
                            content,
                            now_ms(),
                        )
                        .map_err(|e| RpcError::internal(e.to_string()))?;
                    let card = match decision {
                        WriteDecision::Pending(id) => Some(
                            watchers
                                .approval_card(id)
                                .map_err(|e| RpcError::internal(e.to_string()))?,
                        ),
                        WriteDecision::Executed => None,
                    };
                    (decision, card)
                };
                if let Some(card) = card {
                    commit_surface(&self.log, thread, &card)?;
                }
                Ok(match decision {
                    WriteDecision::Executed => json!({ "decision": "executed" }),
                    WriteDecision::Pending(id) => json!({ "decision": "pending", "id": id }),
                })
            }
            "approve" => {
                let id = params.get("id").and_then(|i| i.as_i64()).ok_or_else(|| {
                    RpcError::invalid_params("approve needs an id")
                })?;
                let executed = self
                    .watchers
                    .lock()
                    .unwrap()
                    .approve(id, now_ms())
                    .map_err(|e| RpcError::internal(e.to_string()))?;
                Ok(json!({ "executed": executed }))
            }
            "deny" => {
                let id = params.get("id").and_then(|i| i.as_i64()).ok_or_else(|| {
                    RpcError::invalid_params("deny needs an id")
                })?;
                let denied = self
                    .watchers
                    .lock()
                    .unwrap()
                    .deny(id, now_ms())
                    .map_err(|e| RpcError::internal(e.to_string()))?;
                Ok(json!({ "ok": true, "denied": denied }))
            }
            "approval.card" => {
                let id = params.get("id").and_then(|i| i.as_i64()).ok_or_else(|| {
                    RpcError::invalid_params("approval.card needs an id")
                })?;
                let card = self
                    .watchers
                    .lock()
                    .unwrap()
                    .approval_card(id)
                    .map_err(|e| RpcError::internal(e.to_string()))?;
                Ok(card)
            }
            "approval.status" => {
                let id = params.get("id").and_then(|i| i.as_i64()).ok_or_else(|| {
                    RpcError::invalid_params("approval.status needs an id")
                })?;
                let status = self
                    .watchers
                    .lock()
                    .unwrap()
                    .status(id)
                    .map_err(|e| RpcError::internal(e.to_string()))?;
                Ok(json!({ "status": status }))
            }
            _ => Err(RpcError::new(codes::INVALID_REQUEST, format!("unknown method: {method}"))),
        }
    }
    fn subscribe(&self, scope_id: &str, cursor: i64) -> Option<Subscription> {
        self.log.subscribe(scope_id, cursor).ok()
    }
}

#[cfg(test)]
mod surface_tests {
    use super::*;

    #[test]
    fn valid_surface_is_preserved() {
        let surface = json!({ "type": "text", "value": "Hello" });

        let (node, valid) = validate_surface(&surface);

        assert!(valid);
        assert_eq!(node, surface);
    }

    #[test]
    fn invalid_surface_fails_closed_to_a_diagnostic_text_node() {
        let surface = json!({ "type": "unknown" });

        let (node, valid) = validate_surface(&surface);

        assert!(!valid);
        assert_eq!(node["type"], "text");
        assert!(node["value"].as_str().is_some_and(|value| !value.is_empty()));
    }

    #[test]
    fn committing_a_surface_emits_it_on_the_requested_thread() {
        let log = EventLog::open_in_memory().unwrap();
        let surface = json!({ "type": "text", "value": "Hello" });

        let response = commit_surface(&log, "thread-7", &surface).unwrap();

        assert_eq!(response, json!({ "ok": true, "valid": true }));
        let event = log
            .subscribe("thread-7", 0)
            .unwrap()
            .recv_timeout(Duration::from_millis(10))
            .unwrap();
        assert_eq!(event.kind, "ui.surface");
        assert_eq!(event.payload["surface"], surface);
    }
}

#[cfg(test)]
mod approval_dispatch_tests {
    use super::*;
    use charter::FrontMatter;
    use config::AllowAll;

    fn daemon(data_dir: &Path) -> Daemon {
        let log = EventLog::open_in_memory().unwrap();
        let config = ConfigStore::load(
            &data_dir.join("config.toml"),
            log.clone(),
            Arc::new(AllowAll),
        )
        .unwrap();
        Daemon {
            log,
            config: Mutex::new(config),
            character: Arc::new(Mutex::new(Charter {
                front: FrontMatter {
                    name: "mote".into(),
                    tools: Vec::new(),
                    realms: vec!["personal".into()],
                },
                prose: String::new(),
            })),
            store: Mutex::new(ContextStore::open_in_memory().unwrap()),
            client: Ollama::new("http://127.0.0.1:9"),
            watchers: Mutex::new(Watchers::open_in_memory().unwrap()),
            think: Mutex::new(ThinkServers::new(data_dir.to_path_buf())),
            matrix_sources: Mutex::new(HashMap::new()),
            data_dir: data_dir.to_path_buf(),
            started: std::time::Instant::now(),
        }
    }

    fn dirs(name: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "mote-approval-dispatch-{name}-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let data = root.join("data");
        let outside = root.join("outside");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        (data, outside)
    }

    const MATRIX_DASHBOARD: &str = r#"screen {
  col {
    text "Agent dashboard"
    toggle "Local only" models.local_only
    select "Talk model" models.talk options model_options
    list from pipeline.stages {
      item { row { text .stage text .latency } }
    }
    button "Refresh" -> refresh
  }
}"#;

    #[test]
    fn matrix_render_compiles_the_daemon_manifest_and_commits_the_surface() {
        let (data, _) = dirs("matrix-render");
        let daemon = daemon(&data);

        let response = daemon
            .call(
                "matrix.render",
                json!({"thread":"thread-7", "source": MATRIX_DASHBOARD}),
            )
            .unwrap();

        assert_eq!(response, json!({"ok":true, "valid":true}));
        let event = daemon
            .log
            .subscribe("thread-7", 0)
            .unwrap()
            .recv_timeout(Duration::from_millis(10))
            .unwrap();
        assert_eq!(event.kind, "ui.surface");
        assert_eq!(event.payload["surface"]["type"], "stack");
        assert!(event.payload.to_string().contains("models.local_only"));
        assert!(event.payload.to_string().contains("qwen3:4b"));
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn invalid_matrix_source_returns_diagnostics_without_replacing_the_surface() {
        let (data, _) = dirs("matrix-invalid");
        let daemon = daemon(&data);
        daemon
            .call(
                "matrix.render",
                json!({"thread":"thread-7", "source": MATRIX_DASHBOARD}),
            )
            .unwrap();
        let mut events = daemon.log.subscribe("thread-7", 1).unwrap();

        let response = daemon
            .call(
                "matrix.render",
                json!({"thread":"thread-7", "source":"screen { slider"}),
            )
            .unwrap();

        assert_eq!(response["ok"], false);
        assert_eq!(response["diagnostics"][0]["severity"], "error");
        assert!(response["diagnostics"][0]["code"]
            .as_str()
            .is_some_and(|code| code.starts_with("E_")));
        assert!(events.recv_timeout(Duration::from_millis(10)).is_none());
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn typed_change_updates_config_and_recompiles_the_retained_source() {
        let (data, _) = dirs("matrix-change");
        let daemon = daemon(&data);
        daemon
            .call(
                "matrix.render",
                json!({"thread":"thread-7", "source": MATRIX_DASHBOARD}),
            )
            .unwrap();
        let mut events = daemon.log.subscribe("thread-7", 1).unwrap();

        let response = daemon
            .call(
                "ui.event",
                json!({
                    "thread":"thread-7",
                    "event":"change",
                    "args":{"field":"models.local_only", "value":false}
                }),
            )
            .unwrap();

        assert_eq!(response["ok"], true);
        assert_eq!(
            daemon.config.lock().unwrap().get_json("models.local_only"),
            Some(json!(false))
        );
        assert_eq!(events.recv_timeout(Duration::from_millis(10)).unwrap().kind, "ui.event");
        let surface = events.recv_timeout(Duration::from_millis(10)).unwrap();
        assert_eq!(surface.kind, "ui.surface");
        let toggle = &surface.payload["surface"]["children"][1];
        assert_eq!(toggle["name"], "models.local_only");
        assert_eq!(toggle["value"], false);
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn typed_change_rejects_fields_outside_the_host_manifest() {
        let (data, _) = dirs("matrix-field");
        let daemon = daemon(&data);

        let error = daemon
            .call(
                "ui.event",
                json!({"event":"change", "args":{"field":"secret", "value":true}}),
            )
            .unwrap_err();

        assert!(error.message.contains("field contract"));
        assert_eq!(daemon.log.max_seq().unwrap(), 0);
        let error = daemon
            .call(
                "ui.event",
                json!({
                    "event":"change",
                    "args":{"field":"models.talk", "value":"unapproved:latest"}
                }),
            )
            .unwrap_err();
        assert!(error.message.contains("field contract"));
        assert_eq!(daemon.log.max_seq().unwrap(), 0);
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn pending_file_write_emits_an_inline_approval_card() {
        let (data, outside) = dirs("card");
        let target = outside.join("note.txt");
        let daemon = daemon(&data);

        let response = daemon
            .call(
                "file.write",
                json!({
                    "thread": "thread-7",
                    "path": target,
                    "content": "held"
                }),
            )
            .unwrap();

        let id = response["id"].as_i64().unwrap();
        let event = daemon
            .log
            .subscribe("thread-7", 0)
            .unwrap()
            .recv_timeout(Duration::from_millis(10))
            .unwrap();
        assert_eq!(event.kind, "ui.surface");
        assert!(event.payload.to_string().contains(&format!("approve:{id}")));
        assert!(event.payload.to_string().contains(&format!("deny:{id}")));
        assert!(!target.exists());

        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn pipeline_snapshot_rpc_is_read_only_and_returns_all_config_keys() {
        let (data, _outside) = dirs("pipeline-snapshot");
        let daemon = daemon(&data);

        let response = daemon.call("pipeline.snapshot", json!({})).unwrap();

        assert_eq!(response["event_limit"], pipeline::PIPELINE_EVENT_LIMIT);
        let config = response["config"].as_object().unwrap();
        assert_eq!(config.len(), 9);
        assert_eq!(config["models.talk"], "qwen3:4b");
        assert_eq!(config["budgets.router"], 150);
        assert_eq!(response["numbers"], json!([]));
        assert_eq!(response["requests"], json!([]));

        std::fs::remove_dir_all(data).ok();
    }

    #[test]
    fn approval_card_actions_execute_or_deny_the_held_write() {
        let (data, outside) = dirs("actions");
        let daemon = daemon(&data);
        let approved = outside.join("approved.txt");
        let denied = outside.join("denied.txt");

        let approve_id = daemon
            .call(
                "file.write",
                json!({"thread":"main", "path":approved, "content":"written"}),
            )
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        daemon
            .call(
                "ui.event",
                json!({"thread":"main", "event":format!("approve:{approve_id}")}),
            )
            .unwrap();
        assert_eq!(std::fs::read_to_string(&approved).unwrap(), "written");
        let stale_deny = daemon
            .call(
                "ui.event",
                json!({"thread":"main", "event":format!("deny:{approve_id}")}),
            )
            .unwrap();
        assert_eq!(stale_deny["denied"], false);
        assert_eq!(
            daemon.watchers.lock().unwrap().status(approve_id).unwrap(),
            "approved"
        );

        let deny_id = daemon
            .call(
                "file.write",
                json!({"thread":"main", "path":denied, "content":"blocked"}),
            )
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        let denied_response = daemon
            .call(
                "ui.event",
                json!({"thread":"main", "event":format!("deny:{deny_id}")}),
            )
            .unwrap();
        assert_eq!(denied_response["denied"], true);
        assert!(!denied.exists());
        assert_eq!(daemon.watchers.lock().unwrap().status(deny_id).unwrap(), "denied");
        let repeated = daemon
            .call(
                "ui.event",
                json!({"thread":"main", "event":format!("deny:{deny_id}")}),
            )
            .unwrap();
        assert_eq!(repeated["denied"], false);
        let unknown = daemon
            .call("ui.event", json!({"thread":"main", "event":"deny:999999"}))
            .unwrap();
        assert_eq!(unknown["denied"], false);

        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn approval_card_actions_cannot_cross_threads_or_revive_expired_rows() {
        let (data, outside) = dirs("scope");
        let daemon = daemon(&data);
        let target = outside.join("scoped.txt");
        let id = daemon
            .call(
                "file.write",
                json!({"thread":"thread-7", "path":target, "content":"secret"}),
            )
            .unwrap()["id"]
            .as_i64()
            .unwrap();

        let cross_thread = daemon
            .call(
                "ui.event",
                json!({"thread":"main", "event":format!("approve:{id}")}),
            )
            .unwrap();
        assert_eq!(cross_thread["executed"], false);
        assert!(!target.exists());
        assert_eq!(daemon.watchers.lock().unwrap().status(id).unwrap(), "pending");

        let expired_target = outside.join("expired.txt");
        let expired_id = daemon
            .watchers
            .lock()
            .unwrap()
            .request_file_write(&data, "main", &expired_target, "late", 0)
            .unwrap();
        let WriteDecision::Pending(expired_id) = expired_id else {
            panic!("outside write must be pending");
        };
        let expired = daemon
            .call(
                "ui.event",
                json!({"thread":"main", "event":format!("deny:{expired_id}")}),
            )
            .unwrap();
        assert_eq!(expired["denied"], false);
        assert_eq!(
            daemon.watchers.lock().unwrap().status(expired_id).unwrap(),
            "expired"
        );
        assert!(!expired_target.exists());

        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }
}

#[cfg(test)]
mod turn_event_tests {
    use super::*;
    use charter::FrontMatter;
    use config::AllowAll;
    use mote_ipc::{Client, EventStream};
    use mote_testkit::ScriptedServer;

    fn collect(stream: &mut EventStream) -> Vec<mote_eventlog::Event> {
        let mut events = Vec::new();
        while let Some(event) = stream.next_event() {
            events.push(event);
        }
        events
    }

    #[test]
    fn ordinary_turn_emits_one_redacted_user_event_before_lane_events() {
        let scripted = ScriptedServer::start().unwrap();
        scripted.set_chat_chunks(vec![
            json!({"message":{"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"greeting\"}"},"done":true}).to_string(),
        ]);
        let data_dir = std::env::temp_dir().join(format!(
            "mote-turn-event-{}-{}",
            std::process::id(),
            now_ms()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        let log = EventLog::open_in_memory().unwrap();
        let config = ConfigStore::load(
            &data_dir.join("config.toml"),
            log.clone(),
            Arc::new(AllowAll),
        )
        .unwrap();
        let daemon = Arc::new(Daemon {
            log: log.clone(),
            config: Mutex::new(config),
            character: Arc::new(Mutex::new(Charter {
                front: FrontMatter {
                    name: "mote".into(),
                    tools: Vec::new(),
                    realms: vec!["personal".into()],
                },
                prose: String::new(),
            })),
            store: Mutex::new(ContextStore::open_in_memory().unwrap()),
            client: Ollama::new(scripted.base_url()),
            watchers: Mutex::new(Watchers::open_in_memory().unwrap()),
            think: Mutex::new(ThinkServers::new(data_dir.clone())),
            matrix_sources: Mutex::new(HashMap::new()),
            data_dir: data_dir.clone(),
            started: std::time::Instant::now(),
        });
        let socket = data_dir.join("moted.sock");
        let server = Server::bind(&socket, daemon).unwrap();
        let mut live = EventStream::connect(&socket, "thread-7", 0).unwrap();
        let mut client = Client::connect(&socket).unwrap();

        let response = client
            .call(
                "turn",
                json!({
                "thread": "thread-7",
                "message": "my key is sk-abcdefghijklmnopqrstuvwxyz",
                "client_message_id": "session-1"
            }),
            )
            .unwrap()
            .unwrap();
        assert_eq!(response["status"], "done");

        let events = collect(&mut live);
        assert!(!events.is_empty());
        assert_eq!(events[0].kind, "turn.user");
        assert_eq!(events[0].payload["text"], "my key is [redacted]");
        assert_eq!(events[0].payload["client_message_id"], "session-1");
        assert_eq!(
            events.iter().filter(|event| event.kind == "turn.user").count(),
            1
        );

        let mut replay = EventStream::connect(&socket, "thread-7", 0).unwrap();
        let replayed = collect(&mut replay);
        assert_eq!(
            replayed.iter().map(|event| event.seq).collect::<Vec<_>>(),
            events.iter().map(|event| event.seq).collect::<Vec<_>>()
        );
        assert_eq!(replayed[0].kind, "turn.user");

        drop(server);
        std::fs::remove_dir_all(data_dir).ok();
    }
}

fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        Some("mcp") => run_mcp(),
        Some("bench") => run_bench(std::env::args().nth(2)),
        Some("status") => run_status(),
        Some("install") => service::install().map_err(|e| {
            eprintln!("moted install: {e}");
            1
        }),
        Some("uninstall") => service::uninstall().map_err(|e| {
            eprintln!("moted uninstall: {e}");
            1
        }),
        _ => run(),
    };
    if let Err(code) = result {
        std::process::exit(code);
    }
}

/// Run a named benchmark (`moted bench talk|router|memory`).
fn run_bench(name: Option<String>) -> Result<(), i32> {
    let name = name.ok_or_else(|| {
        eprintln!("moted bench: name required (talk|router|memory)");
        1
    })?;
    let data = mote_paths::ensure_data_dir().map_err(|e| {
        eprintln!("moted bench: {e}");
        1
    })?;
    let gate = Arc::new(OllamaGate::new(Ollama::local()));
    let log = EventLog::open(&mote_paths::db_path()).map_err(|_| 1)?;
    let config = ConfigStore::load(&mote_paths::config_path(), log, gate).map_err(|e| {
        eprintln!("moted bench: {e}");
        1
    })?;
    bench::run_named(&name, &data, &Ollama::local(), &config);
    Ok(())
}

/// Print daemon status (`moted status`): socket, uptime, loaded models and last event seq.
fn run_status() -> Result<(), i32> {
    let socket = mote_paths::socket_path();
    let mut client = mote_ipc::Client::connect(&socket).map_err(|e| {
        eprintln!("moted status: cannot connect to {}: {e}", socket.display());
        1
    })?;
    let res = client.call("status", json!({})).map_err(|e| {
        eprintln!("moted status: {e}");
        1
    })?;
    match res {
        Ok(v) => {
            println!("socket:        {}", v["socket"].as_str().unwrap_or(""));
            println!("uptime:        {} s", v["uptime_s"].as_i64().unwrap_or(0));
            let models: Vec<String> = v["loaded_models"]
                .as_array()
                .map(|a| a.iter().filter_map(|m| m.as_str().map(String::from)).collect())
                .unwrap_or_default();
            println!("loaded models: {}", if models.is_empty() { "(none)".into() } else { models.join(", ") });
            println!("last event:    seq {}", v["last_seq"].as_i64().unwrap_or(0));
            Ok(())
        }
        Err(e) => {
            eprintln!("moted status: {}: {}", e.code, e.message);
            Err(1)
        }
    }
}

/// Serve the MCP tool server over stdio (`moted mcp`).
/// The base directory `moted mcp` resolves relative `file_read`/`file_list` paths against.
/// `moted mcp` is spawned by OpenCode with its cwd set to the think server's scratch directory, so
/// the cwd is useless as a file root. Prefer an explicit `MOTE_FILE_ROOT`, then the owner's `HOME`
/// so a think turn can read the owner's own files, then fall back to the cwd and the data dir.
/// This gives a think turn reach into the owner's files; it is the owner's personal agent on the
/// owner's machine. A tighter sandbox or a configured workspace root is a later refinement.
fn file_root() -> PathBuf {
    std::env::var_os("MOTE_FILE_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .filter(|p| p.is_dir())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| mote_paths::data_dir()))
}

fn run_mcp() -> Result<(), i32> {
    // The MCP server may be spawned before the daemon ever ran, so ensure the data directory.
    mote_paths::ensure_data_dir().map_err(|e| {
        eprintln!("moted mcp: cannot create data directory: {e}");
        1
    })?;
    let store = ContextStore::open(&mote_paths::db_path()).map_err(|e| {
        eprintln!("moted mcp: cannot open context store: {e}");
        1
    })?;
    let server = mcp::McpServer::new(Arc::new(Mutex::new(store)), file_root());
    let stdin = std::io::stdin();
    server.serve(stdin.lock(), std::io::stdout()).map_err(|e| {
        eprintln!("moted mcp: {e}");
        1
    })
}

/// Read (local_only, think model) from config for the startup guard.
fn config_snapshot(config: &ConfigStore) -> (bool, String) {
    let local_only = config.get_json("models.local_only").and_then(|v| v.as_bool()).unwrap_or(true);
    let think = config
        .get_json("models.think")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    (local_only, think)
}

fn run() -> Result<(), i32> {
    let data = mote_paths::ensure_data_dir().map_err(|e| {
        eprintln!("moted: cannot create data directory: {e}");
        1
    })?;

    let log = EventLog::open(&mote_paths::db_path()).map_err(|e| {
        eprintln!("moted: cannot open event log: {e}");
        1
    })?;

    let gate = Arc::new(OllamaGate::new(Ollama::local()));
    let config = ConfigStore::load(&mote_paths::config_path(), log.clone(), gate).map_err(|e| {
        eprintln!("moted: cannot load config: {e}");
        1
    })?;

    // Refuse a subscription think harness while local_only is true.
    {
        let cfg = config_snapshot(&config);
        if let Err(reason) = mote_harness::enforce_local_only(cfg.0, &cfg.1) {
            let _ = log.append("daemon", "startup.refused", &json!({ "reason": reason.clone() }));
            eprintln!("moted: {reason}");
            return Err(1);
        }
    }

    charter::create_defaults(&data).map_err(|e| {
        eprintln!("moted: cannot create charter: {e}");
        1
    })?;
    let watcher = CharterWatcher::start(&charter::character_path(&data, "mote"), log.clone())
        .map_err(|e| {
            eprintln!("moted: cannot watch charter: {e}");
            1
        })?;
    let character = watcher.current.clone();

    let mut store = ContextStore::open(&mote_paths::db_path()).map_err(|e| {
        eprintln!("moted: cannot open context store: {e}");
        1
    })?;
    // Use the contract's embedder in production; fall back to the local embedder if it cannot load.
    match embed::PotionEmbedder::try_new() {
        Ok(e) => store.set_embedder(Arc::new(e)),
        Err(reason) => {
            let _ = log.append("daemon", "embedder.fallback", &json!({ "reason": reason }));
            eprintln!("moted: embedder fallback (local): {reason}");
        }
    }

    let watchers = Watchers::open(rusqlite::Connection::open(mote_paths::db_path()).map_err(|e| {
        eprintln!("moted: cannot open watchers: {e}");
        1
    })?)
    .map_err(|e| {
        eprintln!("moted: cannot create watchers tables: {e}");
        1
    })?;

    let socket = mote_paths::socket_path();
    let daemon = Arc::new(Daemon {
        log,
        config: Mutex::new(config),
        character,
        store: Mutex::new(store),
        client: Ollama::local(),
        watchers: Mutex::new(watchers),
        think: Mutex::new(ThinkServers::new(data.clone())),
        matrix_sources: Mutex::new(HashMap::new()),
        data_dir: data.clone(),
        started: std::time::Instant::now(),
    });

    // Schedule tick loop.
    {
        let d = daemon.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let w = d.watchers.lock().unwrap();
            let _ = w.tick(now_ms(), &d.log);
            let _ = w.expire_due(now_ms());
        });
    }
    let _server = match Server::bind(&socket, daemon) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("moted: {e}");
            return Err(1);
        }
    };

    eprintln!("moted: listening on {}", socket.display());
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
