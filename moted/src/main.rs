//! `moted`, the mote-cloud daemon.
//!
//! v0 starts the control socket over the event log. Lanes and methods are added by later
//! items. A second daemon on the same live socket exits non-zero naming the socket path.

mod bench;
mod charter;
mod config;
mod embed;
mod gate;
mod inferred_ui;
mod lanes;
mod matrix;
mod mcp;
mod model_call;
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
    matrix_snapshots: Mutex<HashMap<String, matrix::HostSnapshot>>,
    matrix_surfaces: Mutex<HashMap<String, matrix::CommittedMatrixSurface>>,
    matrix_candidates: Mutex<HashMap<(String, String), matrix_lang::CandidateStream>>,
    data_dir: PathBuf,
    started: std::time::Instant,
}

struct MatrixCommit<'a> {
    sources: &'a mut HashMap<String, String>,
    snapshots: &'a mut HashMap<String, matrix::HostSnapshot>,
    thread: &'a str,
    surface: &'a Value,
    source: Option<&'a str>,
    bindings: &'a matrix::HostBindings,
    host: matrix::HostSnapshot,
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

    fn compile_matrix_with_host(
        &self,
        source: &str,
        host: &matrix::HostSnapshot,
    ) -> Result<(Value, matrix::HostBindings), Value> {
        let snapshot = self.pipeline_snapshot().map_err(|error| {
            json!([{"code":"E_HOST","severity":"error","message":error.message}])
        })?;
        let bindings = matrix::host_bindings(&snapshot.config, &snapshot, host)
            .map_err(|errors| matrix::diagnostics(&errors))?;
        matrix::compile(source, &bindings)
            .map(|surface| (surface, bindings))
            .map_err(|errors| matrix::diagnostics(&errors))
    }

    #[cfg(test)]
    fn compile_matrix(&self, source: &str) -> Result<Value, Value> {
        self.compile_matrix_with_host(source, &matrix::HostSnapshot::default())
            .map(|(surface, _)| surface)
    }

    fn render_matrix_with_sources(
        &self,
        sources: &mut HashMap<String, String>,
        snapshots: &mut HashMap<String, matrix::HostSnapshot>,
        thread: &str,
        source: &str,
        retain: bool,
        host: matrix::HostSnapshot,
    ) -> Result<Value, RpcError> {
        match self.compile_matrix_with_host(source, &host) {
            Ok((surface, bindings)) => self.commit_matrix(MatrixCommit {
                sources,
                snapshots,
                thread,
                surface: &surface,
                source: retain.then_some(source),
                bindings: &bindings,
                host,
            }),
            Err(diagnostics) => {
                self.emit_matrix_diagnostic(thread, None, &diagnostics)?;
                Ok(json!({"ok": false, "diagnostics": diagnostics}))
            }
        }
    }

    fn emit_matrix_diagnostic(
        &self,
        thread: &str,
        candidate: Option<&str>,
        diagnostics: &Value,
    ) -> Result<(), RpcError> {
        let mut payload = json!({"diagnostics": diagnostics});
        if let Some(candidate) = candidate {
            payload["candidate"] = json!(candidate);
        }
        self.log
            .append(thread, "ui.diagnostic", &payload)
            .map_err(|error| RpcError::internal(format!("commit diagnostic: {error}")))?;
        Ok(())
    }

    fn commit_matrix(&self, commit: MatrixCommit<'_>) -> Result<Value, RpcError> {
        let (node, valid) = validate_surface(commit.surface);
        let typed = serde_json::from_value(node.clone())
            .map_err(|error| RpcError::internal(format!("decode surface contract: {error}")))?;
        let mut committed =
            matrix::CommittedMatrixSurface::new(typed, &commit.bindings.manifest);
        let event = self
            .log
            .append(commit.thread, "ui.surface", &matrix::surface_payload(&node))
            .map_err(|error| RpcError::internal(format!("commit surface: {error}")))?;
        committed.seq = event.seq;
        self.matrix_surfaces
            .lock()
            .unwrap()
            .insert(commit.thread.into(), committed);
        if let Some(source) = commit.source {
            commit.sources.insert(commit.thread.into(), source.into());
        }
        commit
            .snapshots
            .insert(commit.thread.into(), commit.host);
        Ok(matrix::render_success(valid))
    }

    fn matrix_candidate(&self, params: &Value) -> Result<Value, RpcError> {
        use matrix_lang::CandidateStatus;

        let thread = params
            .get("thread")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("matrix.candidate needs a thread id"))?;
        let candidate = params
            .get("candidate")
            .and_then(Value::as_str)
            .ok_or_else(|| RpcError::invalid_params("matrix.candidate needs a candidate id"))?;
        let finish = params
            .get("finish")
            .and_then(Value::as_bool)
            .ok_or_else(|| RpcError::invalid_params("matrix.candidate needs a finish boolean"))?;
        let chunk = params
            .get("chunk")
            .and_then(Value::as_array)
            .ok_or_else(|| RpcError::invalid_params("matrix.candidate needs a byte array"))?
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .and_then(|byte| u8::try_from(byte).ok())
                    .ok_or_else(|| {
                        RpcError::invalid_params("chunk bytes must be integers from 0 through 255")
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let key = (thread.to_owned(), candidate.to_owned());
        let mut candidates = self.matrix_candidates.lock().unwrap();
        let status = candidates.entry(key.clone()).or_default().push(&chunk);
        if !finish {
            return Ok(match status {
                CandidateStatus::Incomplete => json!({"status":"incomplete"}),
                CandidateStatus::Complete(_) => json!({"status":"complete"}),
                CandidateStatus::Rejected(errors) => {
                    json!({"status":"rejected", "diagnostics":matrix::diagnostics(&errors)})
                }
            });
        }
        let parsed = match candidates.remove(&key).unwrap().finalize() {
            Ok(parsed) => parsed,
            Err(errors) => {
                let diagnostics = matrix::diagnostics(&errors);
                self.emit_matrix_diagnostic(thread, Some(candidate), &diagnostics)?;
                return Ok(json!({"status":"rejected", "diagnostics":diagnostics}));
            }
        };
        let mut sources = self.matrix_sources.lock().unwrap();
        let mut snapshots = self.matrix_snapshots.lock().unwrap();
        let snapshot = self.pipeline_snapshot()?;
        let host = snapshots.get(thread).cloned().unwrap_or_default();
        let bindings = match matrix::host_bindings(&snapshot.config, &snapshot, &host) {
            Ok(bindings) => bindings,
            Err(errors) => {
                let diagnostics = matrix::diagnostics(&errors);
                self.emit_matrix_diagnostic(thread, Some(candidate), &diagnostics)?;
                return Ok(json!({"status":"rejected", "diagnostics":diagnostics}));
            }
        };
        match matrix::compile_surface(&parsed, &bindings) {
            Ok(surface) => {
                let source = matrix_lang::format_surface(&parsed);
                let mut response = self.commit_matrix(MatrixCommit {
                    sources: &mut sources,
                    snapshots: &mut snapshots,
                    thread,
                    surface: &surface,
                    source: Some(&source),
                    bindings: &bindings,
                    host,
                })?;
                response["status"] = json!("committed");
                Ok(response)
            }
            Err(errors) => {
                let diagnostics = matrix::diagnostics(&errors);
                self.emit_matrix_diagnostic(thread, Some(candidate), &diagnostics)?;
                Ok(json!({"status":"rejected", "diagnostics":diagnostics}))
            }
        }
    }

    fn render_matrix(&self, thread: &str, source: &str, retain: bool) -> Result<Value, RpcError> {
        let mut sources = self.matrix_sources.lock().unwrap();
        let mut snapshots = self.matrix_snapshots.lock().unwrap();
        let host = snapshots.get(thread).cloned().unwrap_or_default();
        self.render_matrix_with_sources(
            &mut sources,
            &mut snapshots,
            thread,
            source,
            retain,
            host,
        )
    }

    fn refresh_matrix_with_sources(
        &self,
        sources: &mut HashMap<String, String>,
        snapshots: &mut HashMap<String, matrix::HostSnapshot>,
        thread: &str,
    ) -> Result<Option<Value>, RpcError> {
        let source = sources.get(thread).cloned();
        let host = snapshots.get(thread).cloned().unwrap_or_default();
        source
            .map(|source| {
                self.render_matrix_with_sources(
                    sources,
                    snapshots,
                    thread,
                    &source,
                    false,
                    host,
                )
            })
            .transpose()
    }

    #[allow(dead_code)]
    fn refresh_matrix_host_snapshot(
        &self,
        thread: &str,
        host: matrix::HostSnapshot,
    ) -> Result<Value, RpcError> {
        let mut sources = self.matrix_sources.lock().unwrap();
        let mut snapshots = self.matrix_snapshots.lock().unwrap();
        let Some(source) = sources.get(thread).cloned() else {
            return Err(RpcError::invalid_params(
                "host snapshot needs a retained Matrix source",
            ));
        };
        self.render_matrix_with_sources(
            &mut sources,
            &mut snapshots,
            thread,
            &source,
            false,
            host,
        )
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
                let approval = event
                    .strip_prefix("approve:")
                    .map(|id| (true, id))
                    .or_else(|| event.strip_prefix("deny:").map(|id| (false, id)))
                    .and_then(|(approve, id)| {
                        id.parse::<i64>().ok().filter(|id| *id > 0).map(|id| (approve, id))
                    });
                if let Some((approve, id)) = approval {
                    if params.get("surface_seq").is_some()
                        || params.get("node_path").is_some()
                        || args.as_object().is_none_or(|args| !args.is_empty())
                    {
                        return Err(RpcError::invalid_params("approval needs its trusted card contract"));
                    }
                    let watchers = self.watchers.lock().unwrap();
                    let changed = if approve {
                        watchers
                            .approve_for_thread(id, thread, now_ms())
                            .map_err(|error| RpcError::internal(error.to_string()))?
                    } else {
                        watchers
                            .deny_for_thread(id, thread, now_ms())
                            .map_err(|error| RpcError::internal(error.to_string()))?
                    };
                    if changed {
                        self.log
                            .append(thread, "ui.event", &json!({"event":event,"args":args}))
                            .map_err(|error| RpcError::internal(format!("record UI event: {error}")))?;
                    }
                    return Ok(if approve {
                        json!({"ok":true,"approval":id,"executed":changed})
                    } else {
                        json!({"ok":true,"approval":id,"denied":changed})
                    });
                }
                let mut sources = self.matrix_sources.lock().unwrap();
                let accepted = self.matrix_surfaces.lock().unwrap().get(thread).is_some_and(|surface| {
                    params.get("args").is_some() && surface.accepts(&params, event, &args)
                });
                if !accepted {
                    return Err(RpcError::invalid_params(
                        "event does not match the committed surface action or field contract",
                    ));
                }
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
                    let mut snapshots = self.matrix_snapshots.lock().unwrap();
                    let rendered = self.refresh_matrix_with_sources(
                        &mut sources,
                        &mut snapshots,
                        thread,
                    )?;
                    return Ok(json!({"ok": true, "seq": seq, "rendered": rendered}));
                }
                if event == "refresh" {
                    self.log
                        .append(thread, "ui.event", &json!({ "event": event, "args": args }))
                        .map_err(|error| RpcError::internal(format!("record UI event: {error}")))?;
                    let mut snapshots = self.matrix_snapshots.lock().unwrap();
                    let rendered = self.refresh_matrix_with_sources(
                        &mut sources,
                        &mut snapshots,
                        thread,
                    )?;
                    return Ok(json!({"ok": true, "rendered": rendered}));
                }
                Err(RpcError::invalid_params("unsupported Matrix action"))
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
            "matrix.candidate" => self.matrix_candidate(&params),
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
            matrix_snapshots: Mutex::new(HashMap::new()),
            matrix_surfaces: Mutex::new(HashMap::new()),
            matrix_candidates: Mutex::new(HashMap::new()),
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

    const HOST_SNAPSHOT_DASHBOARD: &str = r#"screen {
  col {
    progress value host.progress
    list from host.items {
      item { text .label }
    }
  }
}"#;

    #[test]
    fn validated_host_snapshot_refreshes_progress_and_list_once_without_model_traffic() {
        use mote_client::ObservationHarness;
        let server = mote_testkit::ScriptedServer::start().unwrap();
        let (data, _) = dirs("matrix-host-snapshot");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(&server.base_url());
        daemon.matrix_snapshots.lock().unwrap().insert(
            "main".into(),
            matrix::HostSnapshot::ready(0.25, ["Inspect"]),
        );

        daemon
            .render_matrix("main", HOST_SNAPSHOT_DASHBOARD, true)
            .unwrap();
        let initial = daemon.log.get(daemon.log.max_seq().unwrap()).unwrap().unwrap();
        let mut client = ObservationHarness::default();
        client.event(&initial);
        assert_eq!(client.switches, 1);
        let initial_display = client.frame(vec![]);
        assert_eq!(initial.payload["surface"]["children"][0]["value"], json!(0.25));
        assert_eq!(initial.payload["surface"]["children"][1]["children"].as_array().unwrap().len(), 1);

        let requests = server.recorded().len();
        let ledger = daemon.store.lock().unwrap().count_ledger().unwrap();
        let pre_refresh_seq = daemon.log.max_seq().unwrap();
        let mut emitted = daemon.log.subscribe("main", pre_refresh_seq).unwrap();
        daemon
            .refresh_matrix_host_snapshot(
                "main",
                matrix::HostSnapshot::ready(0.75, ["Review", "Ship", "Observe"]),
            )
            .unwrap();

        let mut events = Vec::new();
        while let Some(event) = emitted.recv_timeout(Duration::from_millis(5)) {
            events.push(event);
        }
        assert_eq!(events.len(), 1, "host refresh emitted extra consumer events");
        assert_eq!(events.iter().filter(|event| event.kind == "ui.surface").count(), 1);
        let refreshed = &events[0];
        assert_eq!(refreshed.kind, "ui.surface");
        assert_eq!(refreshed.payload["surface"]["children"][0]["value"], json!(0.75));
        assert_eq!(
            refreshed.payload["surface"]["children"][1]["children"],
            json!([
                {"type":"text","value":"Review"},
                {"type":"text","value":"Ship"},
                {"type":"text","value":"Observe"}
            ])
        );
        assert!(!refreshed.payload["surface"].to_string().contains("host."));
        for event in &events {
            client.event(event);
        }
        assert_eq!(client.switches, 2, "one accepted snapshot caused more than one redraw");
        assert_eq!(client.surface().unwrap().0, refreshed.seq);
        let refreshed_display = client.frame(vec![]);
        assert_ne!(refreshed_display, initial_display);
        assert!(!format!("{refreshed_display:?}").contains("host."));
        assert_eq!(server.recorded().len(), requests);
        assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), ledger);
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn rejected_host_snapshot_and_unsupported_bindings_preserve_committed_state() {
        let (data, _) = dirs("matrix-host-snapshot-rejection");
        let daemon = daemon(&data);
        daemon
            .render_matrix("main", HOST_SNAPSHOT_DASHBOARD, true)
            .unwrap();
        let committed = daemon.matrix_surfaces.lock().unwrap().clone();
        let source = daemon.matrix_sources.lock().unwrap()["main"].clone();
        let accepted = daemon.matrix_snapshots.lock().unwrap()["main"].clone();
        let surface = daemon.log.get(daemon.log.max_seq().unwrap()).unwrap().unwrap();

        let rejected = daemon
            .refresh_matrix_host_snapshot(
                "main",
                matrix::HostSnapshot::ready(1.25, ["Must not commit"]),
            )
            .unwrap();
        assert_eq!(rejected["ok"], false);
        assert_eq!(rejected["diagnostics"][0]["code"], "E_RESOLVE_CONSTRAINT");
        assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), committed);
        assert_eq!(daemon.matrix_sources.lock().unwrap()["main"], source);
        assert_eq!(daemon.matrix_snapshots.lock().unwrap()["main"], accepted);
        assert_eq!(
            daemon.log.get(surface.seq).unwrap().unwrap().payload,
            surface.payload
        );

        for proposed in [matrix::HostSnapshot::pending(), matrix::HostSnapshot::failed()] {
            let response = daemon
                .refresh_matrix_host_snapshot("main", proposed)
                .unwrap();
            assert_eq!(response["ok"], false);
            assert_eq!(response["diagnostics"][0]["code"], "E_LOWER_UNSUPPORTED");
            assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), committed);
            assert_eq!(daemon.matrix_sources.lock().unwrap()["main"], source);
            assert_eq!(daemon.matrix_snapshots.lock().unwrap()["main"], accepted);
        }

        for unsupported in [
            r#"screen { list from host.items key label { item { text .label } } }"#,
            r#"screen { list from host.items select one { item { text .label } } }"#,
        ] {
            let response = daemon.render_matrix("main", unsupported, true).unwrap();
            assert_eq!(response["ok"], false, "unsupported source committed: {unsupported}");
            assert_eq!(response["diagnostics"][0]["code"], "E_LOWER_UNSUPPORTED");
            assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), committed);
            assert_eq!(daemon.matrix_sources.lock().unwrap()["main"], source);
            assert_eq!(daemon.matrix_snapshots.lock().unwrap()["main"], accepted);
        }
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn committed_matrix_rejects_forged_events_without_side_effects() {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        let cases = [
            ("unknown action", json!({"event":"launch", "args":{}, "node_path":[4]})),
            ("absent field", json!({"event":"change", "args":{"field":"models.router","value":"qwen3:4b"}, "node_path":[2]})),
            ("missing field", json!({"event":"change", "args":{"value":false}, "node_path":[1]})),
            ("missing value", json!({"event":"change", "args":{"field":"models.local_only"}, "node_path":[1]})),
            ("extra change argument", json!({"event":"change", "args":{"field":"models.local_only","value":false,"extra":0}, "node_path":[1]})),
            ("wrong boolean type", json!({"event":"change", "args":{"field":"models.local_only","value":"false"}, "node_path":[1]})),
            ("wrong select type", json!({"event":"change", "args":{"field":"models.talk","value":true}, "node_path":[2]})),
            ("unapproved model", json!({"event":"change", "args":{"field":"models.talk","value":"unapproved:latest"}, "node_path":[2]})),
            ("extra action argument", json!({"event":"refresh", "args":{"extra":0}, "node_path":[4]})),
            ("missing args", json!({"event":"refresh", "node_path":[4]})),
            ("nonobject args", json!({"event":"refresh", "args":[], "node_path":[4]})),
            ("unknown node", json!({"event":"refresh", "args":{}, "node_path":[99]})),
            ("missing node", json!({"event":"refresh", "args":{}})),
            ("negative path", json!({"event":"refresh", "args":{}, "node_path":[-1]})),
            ("fractional path", json!({"event":"refresh", "args":{}, "node_path":[4.5]})),
            ("string path", json!({"event":"refresh", "args":{}, "node_path":"4"})),
            ("inert node", json!({"event":"refresh", "args":{}, "node_path":[0]})),
            ("wrong event for node", json!({"event":"refresh", "args":{}, "node_path":[1]})),
            ("wrong field for node", json!({"event":"change", "args":{"field":"models.talk","value":"qwen3:0.6b"}, "node_path":[1]})),
            ("stale revision", json!({"event":"refresh", "args":{}, "node_path":[4], "surface_seq":0})),
            ("null revision", json!({"event":"refresh", "args":{}, "node_path":[4], "surface_seq":null})),
            ("missing revision", json!({"event":"refresh", "args":{}, "node_path":[4]})),
            ("candidate field", json!({"event":"change", "args":{"field":"models.think","value":"qwen3:4b"}, "node_path":[0]})),
            ("approval through Matrix", json!({"event":"approve:1", "args":{}, "node_path":[4]})),
        ];
        let mut failures = Vec::new();
        for (name, mut params) in cases {
            let (data, outside) = dirs(name);
            let mut daemon = daemon(&data);
            daemon.client = Ollama::new(server.base_url());
            let target = outside.join("held.txt");
            let id = daemon.call("file.write", json!({"thread":"main","path":target,"content":"held"})).unwrap()["id"].as_i64().unwrap();
            daemon.call("matrix.render", json!({"thread":"main","source":MATRIX_DASHBOARD})).unwrap();
            let surface_seq = daemon.log.max_seq().unwrap();
            let committed = daemon.matrix_surfaces.lock().unwrap().clone();
            let rejected = candidate(&daemon, "main", "bad", b"screen { select \"Think\" models.think options model_options icon \"bad\" }", true);
            assert_eq!(rejected["status"], "rejected");
            assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), committed);
            let before = daemon.log.max_seq().unwrap();
            let config = std::fs::read(data.join("config.toml")).unwrap();
            let sources = daemon.matrix_sources.lock().unwrap().clone();
            let contracts = daemon.matrix_surfaces.lock().unwrap().clone();
            let config_values = daemon.pipeline_snapshot().unwrap().config;
            params["thread"] = json!("main");
            if params.get("surface_seq").is_none() && name != "missing revision" {
                params["surface_seq"] = json!(surface_seq);
            }
            let result = daemon.call("ui.event", params);
            if result.is_ok() || daemon.log.max_seq().unwrap() != before {
                failures.push(format!("{name}: accepted={} appended={}", result.is_ok(), daemon.log.max_seq().unwrap() - before));
            }
            if std::fs::read(data.join("config.toml")).unwrap() != config
                || *daemon.matrix_sources.lock().unwrap() != sources
                || *daemon.matrix_surfaces.lock().unwrap() != contracts
                || daemon.pipeline_snapshot().unwrap().config != config_values
                || daemon.watchers.lock().unwrap().status(id).unwrap() != "pending"
                || target.exists()
            {
                failures.push(format!("{name}: changed protected state"));
            }
            assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
            assert_eq!(daemon.store.lock().unwrap().count_packs().unwrap(), 0);
            assert!(server.recorded().is_empty());
            std::fs::remove_dir_all(data.parent().unwrap()).ok();
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn matrix_refresh_requires_exposure_and_legacy_events_require_a_trusted_card() {
        let (data, outside) = dirs("event-contracts");
        let daemon = daemon(&data);
        let target = outside.join("held.txt");
        let id = daemon.call("file.write", json!({"thread":"main","path":target,"content":"held"})).unwrap()["id"].as_i64().unwrap();
        daemon.call("matrix.render", json!({"source":"screen { text \"No action\" }"})).unwrap();
        let seq = daemon.log.max_seq().unwrap();
        for params in [
            json!({"event":"refresh","args":{},"surface_seq":seq,"node_path":[]}),
            json!({"event":"refresh","args":{},"node_path":[]}),
            json!({"event":"anything","args":{}}),
            json!({"event":"approve:bogus","args":{}}),
            json!({"event":format!("approve:{id}"),"args":{"extra":true}}),
            json!({"event":format!("deny:{id}"),"args":[]}),
            json!({"thread":"other","event":"refresh","args":{},"surface_seq":seq,"node_path":[]}),
        ] {
            assert_eq!(daemon.call("ui.event", params).unwrap_err().code, "invalid_params");
            assert_eq!(daemon.log.max_seq().unwrap(), seq);
            assert_eq!(daemon.watchers.lock().unwrap().status(id).unwrap(), "pending");
            assert!(!target.exists());
        }
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn failed_matrix_append_preserves_source_and_committed_contract() {
        let (data, _) = dirs("failed-append");
        let mut daemon = daemon(&data);
        let database = data.join("events.db");
        daemon.log = EventLog::open(&database).unwrap();
        daemon.call("matrix.render", json!({"source":MATRIX_DASHBOARD})).unwrap();
        let before = daemon.log.max_seq().unwrap();
        let sources = daemon.matrix_sources.lock().unwrap().clone();
        let surfaces = daemon.matrix_surfaces.lock().unwrap().clone();
        let connection = rusqlite::Connection::open(database).unwrap();
        connection.execute_batch("CREATE TRIGGER reject_surface BEFORE INSERT ON events WHEN NEW.kind = 'ui.surface' BEGIN SELECT RAISE(FAIL, 'surface storage unavailable'); END;").unwrap();
        assert_eq!(daemon.call("matrix.render", json!({"source":"screen { text \"Replacement\" }"})).unwrap_err().code, "internal");
        assert!(daemon.call("matrix.candidate", json!({"thread":"main","candidate":"replacement","chunk":b"screen { text \"Candidate\" }".to_vec(),"finish":true})).is_err());
        assert_eq!(daemon.log.max_seq().unwrap(), before);
        assert_eq!(*daemon.matrix_sources.lock().unwrap(), sources);
        assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), surfaces);
        assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
        connection.execute_batch("DROP TRIGGER reject_surface;").unwrap();
        assert!(daemon.call("ui.event", json!({"event":"refresh","args":{},"surface_seq":before,"node_path":[4]})).is_ok());
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_retention_failure_preserves_display_contract_and_source() {
        let (data, _) = dirs("retention-failure");
        let mut daemon = daemon(&data);
        let database = data.join("events.db");
        daemon.log = EventLog::open(&database).unwrap();
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.execute_batch(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 4999)
             INSERT INTO events(ts, scope_id, kind, payload) SELECT 0, 'main', 'seed', '{}' FROM n;",
        ).unwrap();
        daemon.call("matrix.render", json!({"source":MATRIX_DASHBOARD})).unwrap();
        let before = daemon.log.max_seq().unwrap();
        let displayed = daemon.log.get(before).unwrap();
        let contracts = daemon.matrix_surfaces.lock().unwrap().clone();
        let sources = daemon.matrix_sources.lock().unwrap().clone();
        let mut subscriber = daemon.log.subscribe("main", before).unwrap();
        connection.execute_batch("CREATE TRIGGER reject_retention BEFORE DELETE ON events BEGIN SELECT RAISE(FAIL, 'retention failed'); END;").unwrap();

        assert!(daemon.call("matrix.render", json!({"source":"screen { button \"Replacement\" -> refresh }"})).is_err());
        assert_eq!(daemon.log.max_seq().unwrap(), before, "failed surface must not enter replay");
        assert!(subscriber.recv_timeout(Duration::from_millis(5)).is_none(), "failed surface reached the renderer");
        assert_eq!(daemon.log.get(before).unwrap(), displayed);
        assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), contracts);
        assert_eq!(*daemon.matrix_sources.lock().unwrap(), sources);
        connection.execute_batch("DROP TRIGGER reject_retention;").unwrap();
        assert!(daemon.call("ui.event", json!({"event":"refresh","args":{},"surface_seq":before,"node_path":[4]})).is_ok());
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn restarted_daemon_rejects_prior_matrix_controls_until_a_fresh_commit() {
        let (data, _) = dirs("restart-admission");
        let database = data.join("events.db");
        let old_sequence = {
            let mut old = daemon(&data);
            old.log = EventLog::open(&database).unwrap();
            old.call("matrix.render", json!({"source":MATRIX_DASHBOARD})).unwrap();
            old.log.max_seq().unwrap()
        };
        let mut restarted = daemon(&data);
        restarted.log = EventLog::open(&database).unwrap();
        let prior_event = json!({"event":"refresh","args":{},"surface_seq":old_sequence,"node_path":[4]});
        assert!(restarted.call("ui.event", prior_event).is_err());
        assert_eq!(restarted.log.max_seq().unwrap(), old_sequence);
        assert!(restarted.matrix_sources.lock().unwrap().is_empty());
        assert!(restarted.matrix_surfaces.lock().unwrap().is_empty());
        restarted.call("matrix.render", json!({"source":MATRIX_DASHBOARD})).unwrap();
        let fresh = restarted.log.max_seq().unwrap();
        assert!(fresh > old_sequence);
        assert!(restarted.call("ui.event", json!({"event":"refresh","args":{},"surface_seq":fresh,"node_path":[4]})).is_ok());
        assert_eq!(restarted.store.lock().unwrap().count_ledger().unwrap(), 0);
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_candidate_contract_accepts_all_four_typed_fields_and_expires_old_sequence() {
        let (data, _) = dirs("typed-fields");
        let daemon = daemon(&data);
        let source = b"screen { toggle \"Local\" models.local_only select \"Talk\" models.talk options model_options select \"Router\" models.router options model_options select \"Think\" models.think options model_options button \"Refresh\" -> refresh }";
        assert_eq!(candidate(&daemon, "main", "valid", source, true)["status"], "committed");
        for (path, field, value) in [
            (0, "models.local_only", json!(false)),
            (1, "models.talk", json!("qwen3:0.6b")),
            (2, "models.router", json!("qwen3:4b")),
            (3, "models.think", json!("qwen3:4b")),
        ] {
            let seq = daemon.log.max_seq().unwrap();
            let params = json!({"event":"change","args":{"field":field,"value":value},"surface_seq":seq,"node_path":[path]});
            assert!(daemon.call("ui.event", params.clone()).is_ok());
            assert_eq!(daemon.config.lock().unwrap().get_json(field), Some(value));
            let fresh = daemon.log.max_seq().unwrap();
            assert!(fresh > seq);
            assert!(daemon.call("ui.event", params).is_err());
            assert_eq!(daemon.log.max_seq().unwrap(), fresh);
        }
        let seq = daemon.log.max_seq().unwrap();
        assert!(daemon.call("ui.event", json!({"event":"refresh","args":{},"surface_seq":seq,"node_path":[4]})).is_ok());
        assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

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
        assert_eq!(event.payload["surface_id"], "matrix");
        assert_eq!(event.payload["surface"]["type"], "stack");
        assert!(event.payload.to_string().contains("models.local_only"));
        assert!(event.payload.to_string().contains("qwen3:4b"));
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    fn candidate(daemon: &Daemon, thread: &str, id: &str, chunk: &[u8], finish: bool) -> Value {
        daemon
            .call(
                "matrix.candidate",
                json!({
                    "thread": thread, "candidate": id, "chunk": chunk, "finish": finish
                }),
            )
            .unwrap()
    }

    #[test]
    fn matrix_candidate_prefixes_preserve_real_client_display_and_event_contracts() {
        use mote_client::{assert_v0_payload, ObservationHarness};
        let (data, _) = dirs("candidate-client-isolation");
        let prior_source = format!("screen {{ {} button \"Prior Refresh\" -> refresh }}", "text \"Committed spacer\" ".repeat(8));
        let action = "screen { button \"Candidate action\" -> refresh }";
        let form = "screen { form \"Candidate form\" { toggle \"Candidate field\" models.local_only button \"Candidate submit\" primary -> refresh } }";
        matrix_lang::parse_surface(form).unwrap();
        let mut prefixes = 0;
        let mut rejected = 0;
        let mut committed = 0;
        for source in [action, form] {
            for cut in 0..=source.len() {
                let daemon = daemon(&data);
                daemon.call("matrix.render", json!({"thread":"main", "source":prior_source})).unwrap();
                let prior = daemon.log.get(daemon.log.max_seq().unwrap()).unwrap().unwrap();
                let mut harness = ObservationHarness::default();
                harness.event(&prior);
                let display = harness.frame(vec![]);
                let event = json!({"thread":"main", "event":"refresh", "args":{}, "surface_seq":prior.seq, "node_path":[8]});
                let contracts = daemon.matrix_surfaces.lock().unwrap().clone();
                let retained = daemon.matrix_sources.lock().unwrap().clone();
                let config = std::fs::read(data.join("config.toml")).unwrap();

                let probe_source = if source == action { action } else {
                    "screen { toggle \"Candidate field\" models.local_only button \"Candidate submit\" -> refresh }"
                };
                let probe_surface = daemon.compile_matrix(probe_source).unwrap();
                let mut probe = ObservationHarness::default();
                probe.event(&mote_eventlog::Event { seq: prior.seq + 1, ts: 0, scope_id: "main".into(), kind: "ui.surface".into(),
                    payload: json!({"surface_id":"matrix", "surface":probe_surface}) });
                let positions: Vec<_> = probe.frame(vec![]).iter().filter(|node| {
                    matches!(node.node, mote_ui::Node::Action { .. } | mote_ui::Node::Toggle { .. })
                }).map(|node| node.rect.center()).collect();
                assert!(!positions.is_empty());
                let old_action = display.iter().find(|node| matches!(node.node, mote_ui::Node::Action { .. })).unwrap();
                assert!(positions.iter().all(|position| !old_action.rect.contains(*position)));

                candidate(&daemon, "main", "truncated", &source.as_bytes()[..cut], false);
                assert_eq!(daemon.log.max_seq().unwrap(), prior.seq);
                harness.assert_isolated(&display, &positions, &event);
                prefixes += 1;
                if cut < source.len() || source == form {
                    let response = candidate(&daemon, "main", "truncated", b"", true);
                    assert_eq!(response["status"], "rejected", "cut {cut}: {source}");
                    if cut == source.len() {
                        assert_eq!(response["diagnostics"][0]["code"], "E_LOWER_UNSUPPORTED");
                    }
                    let diagnostic = daemon.log.get(prior.seq + 1).unwrap().unwrap();
                    assert_eq!(diagnostic.kind, "ui.diagnostic");
                    harness.event(&diagnostic);
                    harness.assert_isolated(&display, &positions, &event);
                    assert_eq!(harness.switches, 1);
                    rejected += 1;
                }
                assert_eq!(*daemon.matrix_sources.lock().unwrap(), retained);
                assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), contracts);
                assert_eq!(std::fs::read(data.join("config.toml")).unwrap(), config);

                candidate(&daemon, "main", "finish", &source.as_bytes()[..cut], false);
                let before = daemon.log.max_seq().unwrap();
                candidate(&daemon, "main", "finish", &source.as_bytes()[cut..], false);
                assert_eq!(daemon.log.max_seq().unwrap(), before);
                harness.assert_isolated(&display, &positions, &event);
                let result = candidate(&daemon, "main", "finish", b"", true);
                assert_eq!(daemon.log.max_seq().unwrap(), before + 1);
                let finalized = daemon.log.get(before + 1).unwrap().unwrap();
                harness.event(&finalized);
                if source == action {
                    assert_eq!(result["status"], "committed");
                    assert_eq!(finalized.kind, "ui.surface");
                    assert_v0_payload(&finalized.payload["surface"]);
                    assert_eq!(harness.switches, 2);
                    let replaced = harness.frame(vec![]);
                    assert_ne!(replaced, display);
                    assert_eq!(harness.frame(vec![]), replaced);
                    let action = replaced.iter().find(|node| matches!(node.node, mote_ui::Node::Action { .. })).unwrap();
                    harness.click(action.rect.center());
                    assert_eq!(harness.queued(), vec![json!({"thread":"main", "event":"refresh", "args":{}, "surface_seq":finalized.seq, "node_path":[]})]);
                    assert!(daemon.call("ui.event", event).is_err());
                    committed += 1;
                } else {
                    assert_eq!(result["status"], "rejected");
                    assert_eq!(result["diagnostics"][0]["code"], "E_LOWER_UNSUPPORTED");
                    harness.assert_isolated(&display, &positions, &event);
                    assert_eq!(harness.switches, 1);
                    assert_eq!(*daemon.matrix_sources.lock().unwrap(), retained);
                    assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), contracts);
                    assert!(daemon.call("ui.event", event).is_ok());
                    rejected += 1;
                }
                assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
                assert_eq!(daemon.store.lock().unwrap().count_packs().unwrap(), 0);
            }
        }
        eprintln!("{prefixes} Matrix byte prefixes, {rejected} rejected finalizations, {committed} complete switches");
        std::fs::remove_dir_all(data.parent().unwrap()).unwrap();
    }

    fn positive_matrix_sources() -> Vec<String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../matrix");
        let mut sources = vec![
            MATRIX_DASHBOARD.to_owned(),
            "screen { text \"é\\n\\u263A\\\"\" }".into(),
            "screen { text \"\\uD83C\\uDF19\" }".into(),
        ];
        let mut markdown = vec![std::fs::read_to_string(root.join("examples.md")).unwrap()];
        for entry in std::fs::read_dir(root.join("recipes")).unwrap() {
            markdown.push(std::fs::read_to_string(entry.unwrap().path()).unwrap());
        }
        for text in markdown {
            for (index, block) in text.split("```\n").enumerate() {
                let source = block.trim();
                if index % 2 == 1 && !source.starts_with('{') && !source.starts_with("PullRequest {") {
                    sources.push(source.to_owned());
                }
            }
        }
        for entry in std::fs::read_dir(root.join("conformance/v0")).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "matrix") {
                sources.push(std::fs::read_to_string(path).unwrap());
            }
        }
        sources
            .into_iter()
            .map(|source| {
                if source.starts_with("screen ") || source.starts_with("screen{") {
                    source
                } else {
                    format!("screen {{ {source} }}")
                }
            })
            .collect()
    }

    #[test]
    fn matrix_candidate_every_positive_fixture_byte_split_defers_commit_until_finish() {
        let (data, _) = dirs("candidate-prefixes");
        for source in positive_matrix_sources() {
            let daemon = daemon(&data);
            let parsed = matrix_lang::parse_surface(&source).unwrap();
            let expected = daemon.compile_matrix(&source);
            daemon
                .call(
                    "matrix.render",
                    json!({
                        "thread":"thread-7", "source":"screen { text \"Prior\" }"
                    }),
                )
                .unwrap();
            for cut in 0..=source.len() {
                let before = daemon.log.max_seq().unwrap();
                let retained = daemon.matrix_sources.lock().unwrap().clone();
                let prefix = &source.as_bytes()[..cut];
                let parses = std::str::from_utf8(prefix)
                    .ok()
                    .is_some_and(|text| matrix_lang::parse_surface(text).is_ok());
                let response = candidate(&daemon, "thread-7", "split", prefix, false);
                assert_eq!(
                    response["status"],
                    if parses { "complete" } else { "incomplete" },
                    "cut {cut}: {source}"
                );
                assert_eq!(daemon.log.max_seq().unwrap(), before);
                assert_eq!(*daemon.matrix_sources.lock().unwrap(), retained);
                let response = candidate(
                    &daemon,
                    "thread-7",
                    "split",
                    &source.as_bytes()[cut..],
                    false,
                );
                assert_eq!(response["status"], "complete");
                assert_eq!(daemon.log.max_seq().unwrap(), before);
                assert_eq!(*daemon.matrix_sources.lock().unwrap(), retained);
                let response = candidate(&daemon, "thread-7", "split", b"", true);
                match &expected {
                    Ok(surface) => {
                        assert_eq!(response["status"], "committed");
                        assert_eq!(response["valid"], true);
                        assert_eq!(daemon.log.max_seq().unwrap(), before + 1);
                        let event = daemon
                            .log
                            .subscribe("thread-7", before)
                            .unwrap()
                            .recv_timeout(Duration::from_millis(10))
                            .unwrap();
                        assert_eq!(event.kind, "ui.surface");
                        assert_eq!(event.payload["surface"], *surface);
                        assert_eq!(
                            daemon.matrix_sources.lock().unwrap()["thread-7"],
                            matrix_lang::format_surface(&parsed)
                        );
                    }
                    Err(diagnostics) => {
                        assert_eq!(response["status"], "rejected");
                        assert_eq!(response["diagnostics"], *diagnostics);
                        assert_eq!(daemon.log.max_seq().unwrap(), before + 1);
                        let event = daemon.log.get(before + 1).unwrap().unwrap();
                        assert_eq!(event.kind, "ui.diagnostic");
                        assert_eq!(event.payload["diagnostics"], *diagnostics);
                        assert_eq!(*daemon.matrix_sources.lock().unwrap(), retained);
                    }
                }
            }
        }
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_candidate_rejections_preserve_exact_prior_surface_source_and_refresh() {
        let (data, _) = dirs("candidate-rejections");
        let daemon = daemon(&data);
        let prior = "screen { text \"Prior\" button \"Refresh\" -> refresh }";
        daemon
            .call(
                "matrix.render",
                json!({"thread":"thread-7", "source":prior}),
            )
            .unwrap();
        for (chunks, code) in [
            (
                vec![b"screen { text \"Bad\" }".to_vec(), b" garbage".to_vec()],
                "E_PARSE_TOKEN",
            ),
            (
                vec![b"screen { text \"Bad".to_vec()],
                "E_LEX_INCOMPLETE_STRING",
            ),
            (
                vec![b"screen { text \"Bad\"".to_vec()],
                "E_PARSE_INCOMPLETE",
            ),
            (vec![b"screen { text \"\xc3".to_vec()], "E_LEX_CHARACTER"),
            (
                vec![vec![b' '; 262_144], b"x".to_vec()],
                "E_SOURCE_TOO_LONG",
            ),
            (vec![b"screen { text missing }".to_vec()], "E_RESOLVE_PATH"),
            (
                vec![b"screen { icon \"warning\" }".to_vec()],
                "E_LOWER_UNSUPPORTED",
            ),
        ] {
            let before = daemon.log.max_seq().unwrap();
            let mut rejection = None;
            for chunk in chunks {
                let response = candidate(&daemon, "thread-7", "bad", &chunk, false);
                if response["status"] == "rejected" {
                    rejection = Some(response["diagnostics"].clone());
                }
                assert_eq!(daemon.log.max_seq().unwrap(), before);
                assert_eq!(daemon.matrix_sources.lock().unwrap()["thread-7"], prior);
            }
            if let Some(errors) = &rejection {
                let response = candidate(&daemon, "thread-7", "bad", b"screen {}", false);
                assert_eq!(response["status"], "rejected");
                assert_eq!(&response["diagnostics"], errors);
            }
            let response = candidate(&daemon, "thread-7", "bad", b"", true);
            assert_eq!(response["status"], "rejected");
            assert_eq!(response["diagnostics"][0]["code"], code, "{response}");
            if let Some(errors) = rejection {
                assert_eq!(response["diagnostics"], errors);
            }
            assert_eq!(daemon.log.max_seq().unwrap(), before + 1);
            let event = daemon.log.get(before + 1).unwrap().unwrap();
            assert_eq!(event.kind, "ui.diagnostic");
            assert_eq!(event.payload["diagnostics"], response["diagnostics"]);
            assert_eq!(daemon.matrix_sources.lock().unwrap()["thread-7"], prior);
            daemon
                .call(
                    "ui.event",
                    json!({"thread":"thread-7", "event":"refresh", "args":{}, "surface_seq":before, "node_path":[1]}),
                )
                .unwrap();
            let event = daemon
                .log
                .recent_kind_since("ui.surface", i64::MIN, 1)
                .unwrap()
                .pop()
                .unwrap();
            assert_eq!(
                event.payload,
                json!({"surface_id":"matrix", "surface":{"type":"stack","children":[{"type":"text","value":"Prior"},{"type":"action","label":"Refresh","event":"refresh"}]}})
            );
        }
        assert_eq!(
            candidate(&daemon, "thread-7", "bad", b"screen { text \"New\" }", true)["status"],
            "committed"
        );
        assert_eq!(
            candidate(&daemon, "thread-7", "bad", b"", true)["status"],
            "rejected"
        );
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_candidate_ids_and_threads_are_isolated_and_terminal_ids_can_restart() {
        let (data, _) = dirs("candidate-ids");
        let daemon = daemon(&data);
        for (thread, id, text) in [("left", "a", "A"), ("left", "b", "B"), ("right", "a", "C")] {
            assert_eq!(
                candidate(
                    &daemon,
                    thread,
                    id,
                    format!("screen {{ text \"{text}").as_bytes(),
                    false
                )["status"],
                "incomplete"
            );
        }
        assert_eq!(daemon.log.max_seq().unwrap(), 0);
        for (thread, id, text) in [("left", "b", "B"), ("right", "a", "C"), ("left", "a", "A")] {
            let before = daemon.log.max_seq().unwrap();
            assert_eq!(
                candidate(&daemon, thread, id, b"\" }", true)["status"],
                "committed"
            );
            let event = daemon
                .log
                .subscribe(thread, before)
                .unwrap()
                .recv_timeout(Duration::from_millis(10))
                .unwrap();
            assert_eq!(
                event.payload,
                json!({"surface_id":"matrix", "surface":{"type":"text", "value":text}})
            );
        }
        assert_eq!(
            candidate(&daemon, "left", "a", b"screen { text \"Restart\" }", true)["status"],
            "committed"
        );
        assert_eq!(
            daemon.matrix_sources.lock().unwrap()["left"],
            "screen {\n  text \"Restart\"\n}\n"
        );
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_candidate_invalid_params_do_not_mutate_the_accumulator() {
        let (data, _) = dirs("candidate-params");
        let daemon = daemon(&data);
        assert_eq!(
            candidate(&daemon, "main", "a", b"screen { text \"", false)["status"],
            "incomplete"
        );
        for chunk in [
            json!([-1]),
            json!([256]),
            json!([1.5]),
            json!([1.0]),
            json!([true]),
            json!([null]),
            json!(["x"]),
            json!("bytes"),
            json!([65, -1]),
        ] {
            let error = daemon
                .call(
                    "matrix.candidate",
                    json!({"thread":"main", "candidate":"a", "chunk":chunk, "finish":true}),
                )
                .unwrap_err();
            assert_eq!(error.code, "invalid_params");
        }
        for params in [
            json!({"candidate":"a", "chunk":[], "finish":false}),
            json!({"thread":"main", "chunk":[], "finish":false}),
            json!({"thread":"main", "candidate":"a", "finish":false}),
            json!({"thread":"main", "candidate":"a", "chunk":[]}),
            json!({"thread":"main", "candidate":"a", "chunk":[], "finish":"true"}),
        ] {
            assert_eq!(
                daemon.call("matrix.candidate", params).unwrap_err().code,
                "invalid_params"
            );
        }
        assert_eq!(daemon.log.max_seq().unwrap(), 0);
        assert_eq!(
            candidate(&daemon, "main", "a", b"Intact\" }", true)["status"],
            "committed"
        );
        let event = daemon
            .log
            .subscribe("main", 0)
            .unwrap()
            .recv_timeout(Duration::from_millis(10))
            .unwrap();
        assert_eq!(
            event.payload["surface"],
            json!({"type":"text", "value":"Intact"})
        );
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_terminal_rejections_emit_one_diagnostic_and_preserve_dashboard() {
        let (data, _) = dirs("matrix-visible-rejections");
        let daemon = daemon(&data);
        daemon.call("matrix.render", json!({"thread":"main", "source":MATRIX_DASHBOARD})).unwrap();
        let prior = daemon.log.get(daemon.log.max_seq().unwrap()).unwrap().unwrap().payload;
        assert!(matrix_lang::parse_surface("screen { row wrap {} }").is_ok());
        for (name, source, code) in [
            ("truncated string", "screen { text \"unfinished", "E_LEX_INCOMPLETE_STRING"),
            ("truncated brace", "screen { text \"unfinished\"", "E_PARSE_INCOMPLETE"),
            ("stray brace", "screen { text \"bad\" }}", "E_PARSE_TOKEN"),
            ("quoted modifier", "screen { row \"wrap\" {} }", "E_PARSE_TOKEN"),
            ("invalid suffix", "screen { text \"bad\" }\ngarbage", "E_PARSE_TOKEN"),
        ] {
            for streamed in [false, true] {
                let before = daemon.log.max_seq().unwrap();
                let mut events = daemon.log.subscribe("main", before).unwrap();
                let response = if streamed {
                    for chunk in source.as_bytes().chunks(1) {
                        candidate(&daemon, "main", name, chunk, false);
                        assert_eq!(daemon.log.max_seq().unwrap(), before);
                    }
                    candidate(&daemon, "main", name, b"", true)
                } else {
                    daemon.call("matrix.render", json!({"thread":"main", "source":source})).unwrap()
                };
                assert_eq!(response["diagnostics"][0]["code"], code, "{name}: {response}");
                let diagnostic = events.recv_timeout(Duration::from_millis(10)).expect("visible diagnostic event");
                assert_eq!(diagnostic.kind, "ui.diagnostic");
                assert_eq!(diagnostic.payload["diagnostics"], response["diagnostics"]);
                assert_eq!(diagnostic.payload["candidate"], if streamed { json!(name) } else { Value::Null });
                assert!(diagnostic.payload.get("source").is_none());
                assert_eq!(daemon.log.max_seq().unwrap(), before + 1);
                assert!(events.recv_timeout(Duration::from_millis(2)).is_none());
                assert_eq!(daemon.matrix_sources.lock().unwrap()["main"], MATRIX_DASHBOARD);
                daemon.call("ui.event", json!({"thread":"main", "event":"refresh", "args":{}, "surface_seq":before,"node_path":[4]})).unwrap();
                assert_eq!(events.recv_timeout(Duration::from_millis(10)).unwrap().kind, "ui.event");
                let refreshed = events.recv_timeout(Duration::from_millis(10)).unwrap();
                assert_eq!(refreshed.kind, "ui.surface");
                assert_eq!(refreshed.payload, prior);
                assert!(events.recv_timeout(Duration::from_millis(2)).is_none());
                println!("{name}, streamed={streamed}: one diagnostic, prior dashboard and source retained");
            }
        }
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_diagnostic_event_redacts_candidate_and_diagnostic_fields() {
        let (data, _) = dirs("matrix-diagnostic-redaction");
        let daemon = daemon(&data);
        let secret = "ghp_0123456789abcdefghijklmnopqrstuvwx";
        let source = format!("screen {{ {secret} }}");
        let response = candidate(&daemon, "main", secret, source.as_bytes(), true);
        assert_eq!(response["status"], "rejected");
        assert!(response["diagnostics"].to_string().contains(secret));
        let event = daemon.log.get(daemon.log.max_seq().unwrap()).unwrap().expect("stored diagnostic");
        assert_eq!(event.kind, "ui.diagnostic");
        assert_eq!(event.payload["candidate"], "[redacted]");
        assert!(!event.payload.to_string().contains(secret));
        assert!(event.payload["diagnostics"].to_string().contains("[redacted]"));
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_diagnostic_events_exclude_malformed_source_literals() {
        let (data, _) = dirs("matrix-literal-diagnostic");
        let daemon = daemon(&data);
        for (literal, private_text) in [
            (r#""{\"password\":\"planted-private-password\"}""#, "planted-private-password"),
            (r#""{\"api_key\":\"planted-private-key\"}""#, "planted-private-key"),
            (r#""planted-private-note""#, "planted-private-note"),
            ("12345678901234567890", "12345678901234567890"),
        ] {
            let source = format!("screen {{ {literal} }}");
            for streamed in [false, true] {
                let before = daemon.log.max_seq().unwrap();
                let response = if streamed {
                    candidate(&daemon, "main", "private-literal", source.as_bytes(), true)
                } else {
                    daemon.call("matrix.render", json!({"thread":"main", "source":source})).unwrap()
                };
                let stored = daemon.log.get(before + 1).unwrap().unwrap();
                assert_eq!(stored.kind, "ui.diagnostic");
                assert!(!stored.payload.to_string().contains(private_text), "persisted rejected literal: {stored:?}");
                assert_eq!(stored.payload["diagnostics"], json!([{
                    "code":"E_PARSE_TOKEN", "severity":"error",
                    "span":{"start":9, "end":9 + literal.len()},
                    "declaration_path":["screen[0]"],
                    "message":"expected an identifier", "hint":null
                }]));
                assert_eq!(response["diagnostics"], stored.payload["diagnostics"]);
                let replayed = daemon.log.subscribe("main", before).unwrap()
                    .recv_timeout(Duration::from_millis(10)).unwrap();
                assert_eq!(replayed, stored);
                assert_eq!(daemon.log.max_seq().unwrap(), before + 1);
            }
        }
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
        assert_eq!(events.recv_timeout(Duration::from_millis(10)).unwrap().kind, "ui.diagnostic");
        assert!(events.recv_timeout(Duration::from_millis(10)).is_none());
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn every_forbidden_matrix_candidate_preserves_the_committed_surface() {
        let (data, _) = dirs("matrix-negative-corpus");
        let daemon = daemon(&data);
        daemon
            .call(
                "matrix.render",
                json!({"thread":"thread-7", "source": MATRIX_DASHBOARD}),
            )
            .unwrap();
        let mut events = daemon.log.subscribe("thread-7", 1).unwrap();
        for (category, source) in [
            ("unknown element", "screen { unknown }") ,
            ("unknown modifier", "screen { text \"x\" sparkle }") ,
            ("unknown icon", "screen { icon \"not-fixed\" }") ,
            ("duplicate property", "screen { row wrap wrap {} }") ,
            ("trailing token", "screen {} trailing") ,
            ("comment", "screen { // comment\n }") ,
            ("expression", "screen { text 1 + 2 }") ,
            ("control flow", "screen { if condition {} }") ,
            ("script", "screen { script \"code\" }") ,
            ("handler", "screen { on click {} }") ,
            ("timer", "screen { timer 1 }") ,
            ("raw newline", "screen { text \"first\nsecond\" }") ,
            ("invalid escape", r#"screen { text "\q" }"#) ,
            ("nonfinite number", "screen { progress NaN }") ,
            ("invalid Unicode", "screen { text \"\u{1}\" }") ,
        ] {
            let response = daemon
                .call(
                    "matrix.render",
                    json!({"thread":"thread-7", "source":source}),
                )
                .unwrap();
            assert_eq!(response["ok"], false, "accepted {category}: {source}");
            assert!(
                response["diagnostics"].as_array().is_some_and(|items| !items.is_empty()),
                "missing diagnostics for {category}"
            );
            assert_eq!(events.recv_timeout(Duration::from_millis(10)).unwrap().kind, "ui.diagnostic");
            assert!(
                events.recv_timeout(Duration::from_millis(2)).is_none(),
                "replaced the committed surface for {category}"
            );
        }
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn forbidden_semantics_corpus_preserves_surface_source_contract_and_refresh() {
        use mote_client::ObservationHarness;
        let cases: Vec<Value> = serde_json::from_str(include_str!("../../matrix/conformance/diagnostics.json")).unwrap();
        let server = mote_testkit::ScriptedServer::start().unwrap();
        let (data, _) = dirs("forbidden-semantics");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());
        daemon.call("matrix.render", json!({"thread":"main", "source":MATRIX_DASHBOARD})).unwrap();
        let original = daemon.log.get(daemon.log.max_seq().unwrap()).unwrap().unwrap();
        let config_file = std::fs::read(data.join("config.toml")).unwrap();
        let config = daemon.pipeline_snapshot().unwrap().config;
        let sources = daemon.matrix_sources.lock().unwrap().clone();
        let mut harness = ObservationHarness::default();
        harness.event(&original);
        let display = harness.frame(vec![]);
        let secret = "ghp_0123456789abcdefghijklmnopqrstuvwx";
        let mut checked = 0;
        for case in cases.iter().filter(|case| case.get("category").is_some()) {
            let source = case["source"].as_str().unwrap();
            let literal = case["private_literal"].as_str().unwrap();
            for streamed in [false, true] {
                let before = daemon.log.max_seq().unwrap();
                let committed = daemon.matrix_surfaces.lock().unwrap().clone();
                let prior_surface = harness.surface();
                let prior_switches = harness.switches;
                let mut events = daemon.log.subscribe("main", before).unwrap();
                let response = if streamed {
                    candidate(&daemon, "main", secret, source.as_bytes(), false);
                    assert_eq!(daemon.log.max_seq().unwrap(), before);
                    assert_eq!(*daemon.matrix_sources.lock().unwrap(), sources);
                    assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), committed);
                    candidate(&daemon, "main", secret, b"", true)
                } else {
                    daemon.call("matrix.render", json!({"thread":"main", "source":source})).unwrap()
                };
                assert_eq!(response[if streamed { "status" } else { "ok" }], if streamed { json!("rejected") } else { json!(false) });
                assert_eq!(response["diagnostics"], case["diagnostics"], "{} streamed={streamed}", case["category"]);
                let diagnostic = events.recv_timeout(Duration::from_millis(10)).unwrap();
                assert_eq!(diagnostic.kind, "ui.diagnostic");
                let mut expected = json!({"diagnostics":case["diagnostics"]});
                if streamed { expected["candidate"] = json!("[redacted]"); }
                assert_eq!(diagnostic.payload, expected);
                assert_eq!(daemon.log.get(before + 1).unwrap(), Some(diagnostic.clone()));
                assert_eq!(daemon.log.max_seq().unwrap(), before + 1);
                assert!(events.recv_timeout(Duration::from_millis(2)).is_none());
                for value in [&response, &diagnostic.payload] {
                    assert!(!value.to_string().contains(literal));
                    assert!(!value.to_string().contains(secret));
                    assert!(!value.to_string().contains(source));
                }
                assert_eq!(*daemon.matrix_sources.lock().unwrap(), sources);
                assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), committed);
                assert_eq!(daemon.log.recent_kind_since("ui.surface", i64::MIN, 1).unwrap()[0].payload, original.payload);
                harness.event(&diagnostic);
                assert_eq!(harness.surface(), prior_surface);
                assert_eq!(harness.frame(vec![]), display);
                assert_eq!(harness.switches, prior_switches);
                assert!(harness.queued().is_empty());
                assert_eq!(std::fs::read(data.join("config.toml")).unwrap(), config_file);
                assert_eq!(daemon.pipeline_snapshot().unwrap().config, config);
                assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
                assert_eq!(daemon.store.lock().unwrap().count_packs().unwrap(), 0);
                assert!(server.recorded().is_empty());

                let refresh = display.iter().find(|node| matches!(node.node, mote_ui::Node::Action { .. })).unwrap();
                harness.click(refresh.rect.center());
                let queued = harness.queued();
                assert_eq!(queued, vec![json!({"thread":"main", "event":"refresh", "args":{}, "surface_seq":committed["main"].seq,"node_path":[4]})]);
                daemon.call("ui.event", queued[0].clone()).unwrap();
                assert_eq!(events.recv_timeout(Duration::from_millis(10)).unwrap().kind, "ui.event");
                let refreshed = events.recv_timeout(Duration::from_millis(10)).unwrap();
                assert_eq!(refreshed.kind, "ui.surface");
                assert_eq!(refreshed.payload, original.payload);
                assert!(events.recv_timeout(Duration::from_millis(2)).is_none());
                harness.event(&refreshed);
                assert_eq!(harness.frame(vec![]), display);
                assert_eq!(*daemon.matrix_sources.lock().unwrap(), sources);
                assert_eq!(std::fs::read(data.join("config.toml")).unwrap(), config_file);
                assert_eq!(daemon.pipeline_snapshot().unwrap().config, config);
                assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
                assert_eq!(daemon.store.lock().unwrap().count_packs().unwrap(), 0);
                assert!(server.recorded().is_empty());
                checked += 1;
            }
        }
        assert_eq!(checked, 34);
        println!("17 corpus cases, 34 terminal rejections, 34 prior-source Refresh activations, zero model requests");
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn runtime_keyboard_edits_and_activations_keep_models_and_history_host_owned() {
        use mote_client::{InputKey, ObservationHarness};
        let server = mote_testkit::ScriptedServer::start().unwrap();
        let (data, _) = dirs("runtime-keyboard");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());
        daemon.call("surface.render", json!({"surface":{
            "type":"field","label":"Name","name":"name","value":"host value"
        }})).unwrap();
        let original = daemon.log.get(daemon.log.max_seq().unwrap()).unwrap().unwrap();
        let mut field = ObservationHarness::default();
        field.event(&original);
        field.frame(vec![]);
        field.tab_past_navigation();
        field.key(InputKey::Tab, false);
        for text in ["first edit", "second edit"] {
            field.edit(text);
            field.key(InputKey::Space, false);
            assert!(field.queued().is_empty());
            let (_, output) = field.frame_output(vec![]);
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                mote_client::PaintedShape::Text(value) if value.galley.text() == text)));
            assert_eq!(daemon.log.max_seq().unwrap(), original.seq);
            assert_eq!(daemon.log.get(original.seq).unwrap(), Some(original.clone()));
            assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
            assert_eq!(daemon.store.lock().unwrap().count_packs().unwrap(), 0);
            assert!(server.recorded().is_empty());
        }
        field.enter();
        assert!(field.queued().is_empty());

        let source = "screen { toggle \"Local\" models.local_only select \"Talk\" models.talk options model_options button \"Refresh\" -> refresh }";
        daemon.call("matrix.render", json!({"source":source})).unwrap();
        for (key, tabs, expected_event, expected_args, path) in [
            (InputKey::Space, 1, "change", json!({"field":"models.local_only","value":false}), vec![0]),
            (InputKey::Enter, 1, "change", json!({"field":"models.local_only","value":true}), vec![0]),
            (InputKey::Space, 3, "refresh", json!({}), vec![2]),
            (InputKey::Enter, 3, "refresh", json!({}), vec![2]),
            (InputKey::Enter, 2, "change", json!({"field":"models.talk","value":"qwen3:0.6b"}), vec![1]),
        ] {
            let committed = daemon.log.get(daemon.log.max_seq().unwrap()).unwrap().unwrap();
            assert_eq!(committed.kind, "ui.surface");
            let mut harness = ObservationHarness::default();
            harness.event(&committed);
            harness.frame(vec![]);
            harness.tab_past_navigation();
            for _ in 0..tabs { harness.key(InputKey::Tab, false); }
            assert!(harness.queued().is_empty());
            assert_eq!(daemon.log.max_seq().unwrap(), committed.seq);
            harness.key(key, false);
            if tabs == 2 {
                assert!(harness.queued().is_empty());
                for _ in 0..3 { harness.key(InputKey::Tab, false); }
                assert!(harness.queued().is_empty());
                harness.key(InputKey::Tab, true);
                harness.key(InputKey::Tab, false);
                harness.key(key, false);
            }
            let queued = harness.queued();
            assert_eq!(queued, [json!({"thread":"main", "event":expected_event, "args":expected_args,
                "surface_seq":committed.seq, "node_path":path})]);
            assert_eq!(daemon.log.max_seq().unwrap(), committed.seq);
            daemon.call("ui.event", queued[0].clone()).unwrap();
            let config_events = i64::from(expected_event == "change");
            if config_events == 1 {
                assert_eq!(daemon.log.get(committed.seq + 1).unwrap().unwrap().kind, "config.changed");
            }
            assert_eq!(daemon.log.get(committed.seq + config_events + 1).unwrap().unwrap().kind, "ui.event");
            let refreshed = daemon.log.get(committed.seq + config_events + 2).unwrap().unwrap();
            assert_eq!(refreshed.kind, "ui.surface");
            assert_eq!(daemon.log.max_seq().unwrap(), refreshed.seq);
            assert_eq!(daemon.log.get(committed.seq).unwrap(), Some(committed.clone()));
            harness.event(&refreshed);
            harness.frame(vec![]);
            assert!(harness.queued().is_empty());
            if expected_event == "change" {
                let field = expected_args["field"].as_str().unwrap();
                assert_eq!(daemon.config.lock().unwrap().get_json(field), Some(expected_args["value"].clone()));
                assert_eq!(refreshed.payload["surface"]["children"][path[0]]["value"], expected_args["value"]);
            }
            assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
            assert_eq!(daemon.store.lock().unwrap().count_packs().unwrap(), 0);
            assert!(server.recorded().is_empty());
        }
        assert_eq!(daemon.log.get(original.seq).unwrap(), Some(original));
        println!("Field edits and traversal emit no events; five keyboard activations dispatch exact UI contracts; zero model requests, packs or ledger rows");
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_literal_strings_reach_the_renderer_as_inert_text() {
        use mote_client::ObservationHarness;
        let (data, _) = dirs("literal-rendering");
        let daemon = daemon(&data);
        for literal in [
            "<b>hi</b>",
            "<script>alert('hi')</script>",
            "# **bold** _italic_ `code` [link](https://example.com) ![image](asset)",
        ] {
            let source = format!("screen {{ text {} }}", serde_json::to_string(literal).unwrap());
            for streamed in [false, true] {
                if streamed {
                    assert_eq!(candidate(&daemon, "main", "literal", source.as_bytes(), true)["status"], "committed");
                } else {
                    assert_eq!(daemon.call("matrix.render", json!({"source":source})).unwrap()["ok"], true);
                }
                let event = daemon.log.get(daemon.log.max_seq().unwrap()).unwrap().unwrap();
                assert_eq!(event.payload, json!({"surface_id":"matrix", "surface":{"type":"text","value":literal}}));
                let mut harness = ObservationHarness::default();
                harness.event(&event);
                let (display, output) = harness.frame_output(vec![]);
                assert_eq!(display.len(), 1);
                assert_eq!(display[0].node, mote_ui::Node::Text { value: literal.into() });
                assert!(display[0].path.is_empty());
                let labels: Vec<_> = output.shapes.iter().filter_map(|shape| match &shape.shape {
                    mote_client::PaintedShape::Text(text) if display[0].rect.contains(text.pos) => Some(text.galley.text()),
                    _ => None,
                }).collect();
                assert_eq!(labels, [literal]);
                assert!(output.platform_output.commands.is_empty());
                harness.click(display[0].rect.center());
                harness.enter();
                assert!(harness.queued().is_empty());
                assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
            }
        }
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_limits_preserve_the_surface_and_refresh_the_prior_source() {
        let oversized = format!("screen {{}}{}", " ".repeat(262_136));
        assert_eq!(oversized.len(), 262_145);
        let depth = format!(
            "screen {{{}text \"x\"{}}}",
            "col {".repeat(11),
            "}".repeat(11)
        );
        let nodes = format!(
            "screen {{ {}{} }}",
            "list from model_options { item { text \"x\" } } ".repeat(285),
            "text \"x\" ".repeat(5)
        );
        let bytes = format!(
            "screen {{ list from model_options {{ item {{ text \"{}\" }} }} }}",
            "x".repeat(90_000)
        );
        let depth_start = depth.find("text").unwrap();
        let node_start = nodes.rfind("text").unwrap();
        let byte_start = bytes.find("text").unwrap();
        let depth_path: Vec<_> = std::iter::once("screen[0]")
            .chain(std::iter::repeat_n("col[0]", 11))
            .chain(["text[0]"])
            .collect();
        for (category, source, code, start, end, path, message) in [
            (
                "source bytes",
                oversized,
                "E_SOURCE_TOO_LONG",
                0,
                262_145,
                json!([]),
                "Matrix source exceeds 256 KiB",
            ),
            (
                "source depth",
                depth,
                "E_PARSE_DEPTH",
                depth_start,
                depth_start + 4,
                json!(depth_path),
                "declaration depth exceeds 12",
            ),
            (
                "expanded nodes",
                nodes,
                "E_RESOLVE_LIMIT",
                node_start,
                node_start + 8,
                json!(["screen", "289"]),
                "Resolved tree exceeds depth or node limit",
            ),
            (
                "expanded retained bytes",
                bytes,
                "E_RESOLVE_LIMIT",
                byte_start,
                byte_start + 90_007,
                json!(["screen", "0", "0", "0"]),
                "Resolved content exceeds 256 KiB",
            ),
        ] {
            let (data, _) = dirs(category);
            let daemon = daemon(&data);
            let prior = "screen { text \"Retained source\" button \"Refresh\" -> refresh }";
            assert_eq!(
                daemon
                    .call(
                        "matrix.render",
                        json!({"thread":"thread-7", "source":prior})
                    )
                    .unwrap(),
                json!({"ok":true,"valid":true})
            );
            let mut events = daemon.log.subscribe("thread-7", 1).unwrap();
            let response = daemon
                .call(
                    "matrix.render",
                    json!({"thread":"thread-7", "source":source}),
                )
                .unwrap();
            assert_eq!(
                response,
                json!({"ok":false,"diagnostics":[{
                    "code":code,"severity":"error","span":{"start":start,"end":end},
                    "declaration_path":path,"message":message,"hint":null
                }]}),
                "{category}"
            );
            assert_eq!(events.recv_timeout(Duration::from_millis(10)).unwrap().kind, "ui.diagnostic");
            assert!(
                events.recv_timeout(Duration::from_millis(2)).is_none(),
                "{category} replaced the committed surface"
            );
            assert_eq!(
                daemon
                    .matrix_sources
                    .lock()
                    .unwrap()
                    .get("thread-7")
                    .map(String::as_str),
                Some(prior)
            );
            let refreshed = daemon
                .call(
                    "ui.event",
                    json!({"thread":"thread-7", "event":"refresh", "args":{}, "surface_seq":1,"node_path":[1]}),
                )
                .unwrap();
            assert_eq!(
                refreshed,
                json!({"ok":true,"rendered":{"ok":true,"valid":true}})
            );
            assert_eq!(
                events.recv_timeout(Duration::from_millis(10)).unwrap().kind,
                "ui.event"
            );
            let event = events.recv_timeout(Duration::from_millis(10)).unwrap();
            assert_eq!(event.kind, "ui.surface");
            assert_eq!(
                event.payload,
                json!({"surface_id":"matrix","surface":{"type":"stack","children":[{"type":"text","value":"Retained source"},{"type":"action","label":"Refresh","event":"refresh"}]}})
            );
            assert!(events.recv_timeout(Duration::from_millis(2)).is_none());
            std::fs::remove_dir_all(data.parent().unwrap()).ok();
        }
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
                    "surface_seq":1,
                    "node_path":[1],
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
    fn concurrent_matrix_renders_keep_retained_source_and_refresh_consistent() {
        let (data, _) = dirs("matrix-concurrent");
        let daemon = Arc::new(daemon(&data));
        for iteration in 0..20 {
            let first = format!("screen {{ text \"first {iteration}\" }}");
            let second = format!("screen {{ text \"second {iteration}\" }}");
            let left = daemon.clone();
            let right = daemon.clone();
            let left = std::thread::spawn(move || {
                left.call(
                    "matrix.render",
                    json!({"thread":"thread-7", "source":first}),
                )
                .unwrap();
            });
            let right = std::thread::spawn(move || {
                right
                    .call(
                        "matrix.render",
                        json!({"thread":"thread-7", "source":second}),
                    )
                    .unwrap();
            });
            left.join().unwrap();
            right.join().unwrap();
        }

        let retained = daemon
            .matrix_sources
            .lock()
            .unwrap()
            .get("thread-7")
            .cloned()
            .unwrap();
        let expected = daemon.compile_matrix(&retained).unwrap();
        daemon.render_matrix("thread-7", &retained, false).unwrap();
        let latest = daemon
            .log
            .recent_kind_since("ui.surface", i64::MIN, 1)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(latest.payload["surface"], expected);
        assert_eq!(latest.payload["surface_id"], "matrix");
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
    fn failed_inside_file_write_returns_an_internal_error() {
        let (data, _) = dirs("inside-write-failure");
        let target = data.join("directory-target");
        std::fs::create_dir_all(&target).unwrap();
        let daemon = daemon(&data);

        let error = daemon
            .call(
                "file.write",
                json!({"thread":"main", "path":target, "content":"nope"}),
            )
            .unwrap_err();

        assert_eq!(error.code, codes::INTERNAL);
        assert!(target.is_dir());
        assert_eq!(daemon.log.max_seq().unwrap(), 0);
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn failed_approved_file_write_returns_an_internal_error_without_accepting_event() {
        let (data, outside) = dirs("approved-write-failure");
        let target = outside.join("directory-target");
        std::fs::create_dir_all(&target).unwrap();
        let daemon = daemon(&data);
        let id = daemon
            .call(
                "file.write",
                json!({"thread":"main", "path":target, "content":"nope"}),
            )
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        let before_approval = daemon.log.max_seq().unwrap();

        let error = daemon
            .call(
                "ui.event",
                json!({"thread":"main", "event":format!("approve:{id}")}),
            )
            .unwrap_err();

        assert_eq!(error.code, codes::INTERNAL);
        assert_eq!(daemon.log.max_seq().unwrap(), before_approval);
        assert_eq!(daemon.watchers.lock().unwrap().status(id).unwrap(), "pending");
        let denied = daemon
            .call(
                "ui.event",
                json!({"thread":"main", "event":format!("deny:{id}")}),
            )
            .unwrap();
        assert_eq!(denied["denied"], true);
        assert_eq!(daemon.watchers.lock().unwrap().status(id).unwrap(), "denied");
        assert!(target.is_dir());
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[cfg(unix)]
    #[test]
    fn retargeted_pending_symlink_returns_an_internal_error_without_accepting_event() {
        use std::os::unix::fs::symlink;

        let (data, outside) = dirs("pending-symlink-retarget");
        let first = outside.join("first.txt");
        let second = outside.join("second.txt");
        let link = outside.join("selected.txt");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        symlink(&first, &link).unwrap();
        let daemon = daemon(&data);
        let id = daemon
            .call(
                "file.write",
                json!({"thread":"main", "path":link, "content":"blocked"}),
            )
            .unwrap()["id"]
            .as_i64()
            .unwrap();
        let before_approval = daemon.log.max_seq().unwrap();
        std::fs::remove_file(&link).unwrap();
        symlink(&second, &link).unwrap();

        let error = daemon
            .call(
                "ui.event",
                json!({"thread":"main", "event":format!("approve:{id}")}),
            )
            .unwrap_err();

        assert_eq!(error.code, codes::INTERNAL);
        assert_eq!(daemon.log.max_seq().unwrap(), before_approval);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), "second");
        assert_eq!(daemon.watchers.lock().unwrap().status(id).unwrap(), "pending");
        let denied = daemon
            .call(
                "ui.event",
                json!({"thread":"main", "event":format!("deny:{id}")}),
            )
            .unwrap();
        assert_eq!(denied["denied"], true);
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
            matrix_snapshots: Mutex::new(HashMap::new()),
            matrix_surfaces: Mutex::new(HashMap::new()),
            matrix_candidates: Mutex::new(HashMap::new()),
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
        matrix_snapshots: Mutex::new(HashMap::new()),
        matrix_surfaces: Mutex::new(HashMap::new()),
        matrix_candidates: Mutex::new(HashMap::new()),
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
