//! `moted`, the mote-cloud daemon.
//!
//! v0 starts the control socket over the event log. Lanes and methods are added by later
//! items. A second daemon on the same live socket exits non-zero naming the socket path.

mod bench;
mod charter;
mod config;
mod gate;
mod lanes;
mod mcp;
mod watchers;

use charter::{Charter, CharterWatcher};
use config::ConfigStore;
use gate::OllamaGate;
use lanes::{run_turn, TalkInputs, TalkOutcome, TALK_BUDGET};
use mote_context::ContextStore;
use mote_localmodel::Ollama;
use mote_eventlog::{EventLog, Subscription};
use mote_ipc::{codes, Dispatch, RpcError, Server};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use watchers::{WriteDecision, Watchers};

const TALK_CTX: u32 = 32768;
const ROUTER_CTX: u32 = 32768;

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
    data_dir: PathBuf,
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

        let (talk_model, router_model, budget, charter_prose) = {
            let cfg = self.config.lock().unwrap();
            let talk = cfg.get_json("models.talk").and_then(|v| v.as_str().map(String::from));
            let router = cfg.get_json("models.router").and_then(|v| v.as_str().map(String::from));
            let budget = cfg
                .get_json("packs.talk")
                .and_then(|v| v.as_i64())
                .map(|n| n as usize)
                .unwrap_or(TALK_BUDGET);
            let prose = self.character.lock().unwrap().prose.clone();
            (talk, router, budget, prose)
        };
        let talk_model = talk_model.ok_or_else(|| RpcError::internal("models.talk unset"))?;
        let router_model = router_model.ok_or_else(|| RpcError::internal("models.router unset"))?;

        let store = self.store.lock().unwrap();
        let profile = store.get_profile("profile").ok().flatten().unwrap_or_default();
        let working_set = store.working_set_text(thread_id).unwrap_or_default();
        let inputs = TalkInputs {
            charter: &charter_prose,
            profile: &profile,
            memories: "",
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
            TalkOutcome::Handoff => json!({ "status": "handoff" }),
            TalkOutcome::Unavailable(reason) => json!({ "status": "unavailable", "reason": reason }),
        })
    }
}

impl Dispatch for Daemon {
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "ping" => Ok(json!({ "ok": true })),
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

/// Serve the MCP tool server over stdio (`moted mcp`).
fn run_mcp() -> Result<(), i32> {
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

    let store = ContextStore::open(&mote_paths::db_path()).map_err(|e| {
        eprintln!("moted: cannot open context store: {e}");
        1
    })?;

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
        data_dir: data.clone(),
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
