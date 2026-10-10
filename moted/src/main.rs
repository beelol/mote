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
mod mcp;
mod service;
mod watchers;

use charter::{Charter, CharterWatcher};
use config::ConfigStore;
use gate::OllamaGate;
use lanes::{
    build_think_pack, emit_think, run_turn, ThinkInputs, ThinkOutcome, TalkInputs, TalkOutcome,
    TALK_BUDGET, THINK_BUDGET,
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
    data_dir: PathBuf,
    started: std::time::Instant,
}

impl Daemon {
    fn handle_turn(&self, params: &Value) -> Result<Value, RpcError> {
        let thread_id = params
            .get("thread")
            .and_then(|t| t.as_str())
            .ok_or_else(|| RpcError::invalid_params("turn needs a thread id"))?;
        let message = params
            .get("message")
            .and_then(|m| m.as_str())
            .ok_or_else(|| RpcError::invalid_params("turn needs a message"))?;

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
        Ok(match outcome {
            TalkOutcome::Done { ledger_id, pack_id } => {
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
                match server.and_then(|s| {
                    let session = s.create_session()?;
                    let started = std::time::Instant::now();
                    s.run_turn(&session, &full_model, Some(&system), &turn, THINK_TIMEOUT)
                        .map(|(events, _first)| (events, started.elapsed().as_millis() as u64))
                }) {
                    Ok((events, latency_ms)) => {
                        match emit_think(
                            &self.log, &store, thread_id, &think_model, &pack, &events, latency_ms,
                        ) {
                            ThinkOutcome::Done { ledger_id, pack_id } => json!({
                                "status": "think_done", "ledger_id": ledger_id, "pack_id": pack_id
                            }),
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
                self.log.append("main", "ui.event", &json!({ "event": event })).ok();
                Ok(json!({ "ok": true }))
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
                let decision = self
                    .watchers
                    .lock()
                    .unwrap()
                    .request_file_write(&self.data_dir, thread, std::path::Path::new(path), content, now_ms())
                    .map_err(|e| RpcError::internal(e.to_string()))?;
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
                self.watchers.lock().unwrap().deny(id).map_err(|e| RpcError::internal(e.to_string()))?;
                Ok(json!({ "ok": true }))
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
    let root = std::env::current_dir().unwrap_or_else(|_| mote_paths::data_dir());
    let server = mcp::McpServer::new(Arc::new(Mutex::new(store)), root);
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
