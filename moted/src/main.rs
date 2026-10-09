//! `moted`, the mote-cloud daemon.
//!
//! v0 starts the control socket over the event log. Lanes and methods are added by later
//! items. A second daemon on the same live socket exits non-zero naming the socket path.

mod charter;
mod config;
mod gate;
mod lanes;

use charter::{Charter, CharterWatcher};
use config::ConfigStore;
use gate::OllamaGate;
use lanes::{run_turn, TalkInputs, TalkOutcome, TALK_BUDGET};
use mote_context::ContextStore;
use mote_localmodel::Ollama;
use mote_eventlog::{EventLog, Subscription};
use mote_ipc::{codes, Dispatch, RpcError, Server};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const TALK_CTX: u32 = 32768;
const ROUTER_CTX: u32 = 32768;

struct Daemon {
    log: EventLog,
    config: Mutex<ConfigStore>,
    character: Arc<Mutex<Charter>>,
    store: Mutex<ContextStore>,
    client: Ollama,
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
            _ => Err(RpcError::new(codes::INVALID_REQUEST, format!("unknown method: {method}"))),
        }
    }
    fn subscribe(&self, scope_id: &str, cursor: i64) -> Option<Subscription> {
        self.log.subscribe(scope_id, cursor).ok()
    }
}

fn main() {
    if let Err(code) = run() {
        std::process::exit(code);
    }
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

    let socket = mote_paths::socket_path();
    let daemon = Arc::new(Daemon {
        log,
        config: Mutex::new(config),
        character,
        store: Mutex::new(store),
        client: Ollama::local(),
    });
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
