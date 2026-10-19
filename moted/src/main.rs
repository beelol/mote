//! `moted`, the mote-cloud daemon.
//!
//! v0 starts the control socket over the event log. Lanes and methods are added by later
//! items. A second daemon on the same live socket exits non-zero naming the socket path.

#[allow(dead_code)]
pub(crate) mod approvals;
mod browser;
mod bench;
mod charter;
pub(crate) mod config;
mod embed;
mod gate;
mod gateway;
#[allow(dead_code)]
mod hrns;
#[allow(dead_code)]
mod hrns_actions;
mod hrns_compat;
mod hrns_native;
pub(crate) mod host_snapshot;
pub(crate) mod inferred_ui;
pub(crate) mod lanes;
pub(crate) mod matrix;
mod mcp;
pub(crate) mod model_call;
#[allow(dead_code)]
mod native_step;
pub(crate) mod pipeline;
mod service;
mod shutdown;
mod surface_activation;
mod watchers;

#[cfg(any(test, feature = "test-fixture"))]
pub mod test_fixture;

use charter::{Charter, CharterWatcher};
use config::ConfigStore;
use gate::OllamaGate;
use lanes::{
    build_think_pack, run_think_model_call, run_turn, ThinkInputs, ThinkOutcome, TalkInputs,
    TalkOutcome, TALK_BUDGET, THINK_BUDGET,
};
use mote_context::ContextStore;
use mote_harness::{OpenCodeSession, ProcessServerFactory, ServerFactory};
use mote_localmodel::Ollama;
use mote_eventlog::{EventLog, Subscription};
use mote_ipc::{codes, Actor, Dispatch, RpcError, Server};
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
    servers: HashMap<String, Arc<dyn OpenCodeSession>>,
}

impl ThinkServers {
    fn new(data_dir: PathBuf) -> Self {
        let moted_exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("moted"));
        Self { data_dir, moted_exe, servers: HashMap::new() }
    }

    /// Get the thread's server, starting one (with its `opencode.json`) on first use.
    fn get(
        &mut self,
        thread_id: &str,
        think_model: &str,
    ) -> Result<Arc<dyn OpenCodeSession>, String> {
        if let Some(s) = self.servers.get(thread_id) {
            return Ok(s.clone());
        }
        let dir = self.prepare(thread_id, think_model, false)?;
        let server = ProcessServerFactory::new(&dir, Duration::from_secs(60)).start()?;
        self.servers.insert(thread_id.to_string(), server.clone());
        Ok(server)
    }

    fn factory(
        &self,
        thread_id: &str,
        think_model: &str,
    ) -> Result<Arc<dyn ServerFactory>, String> {
        let dir = self.prepare(thread_id, think_model, true)?;
        Ok(Arc::new(ProcessServerFactory::new(
            dir,
            Duration::from_secs(60),
        )))
    }

    fn prepare(
        &self,
        thread_id: &str,
        think_model: &str,
        guarded: bool,
    ) -> Result<PathBuf, String> {
        let safe: String =
            thread_id.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        let dir = self.data_dir.join("think").join(safe);
        std::fs::create_dir_all(&dir).map_err(|e| format!("think dir: {e}"))?;
        let config = if guarded {
            guarded_think_config(&self.moted_exe, think_model)
        } else {
            think_config(&self.moted_exe, think_model)
        };
        let mut tool_config: Value =
            serde_json::from_str(&config).map_err(|error| error.to_string())?;
        tool_config["mcp"]["moted"]["environment"] = json!({"MOTE_HOME":self.data_dir,"MOTE_THREAD_ID":thread_id});
        std::fs::write(dir.join("opencode.json"), tool_config.to_string())
            .map_err(|e| format!("write opencode.json: {e}"))?;
        Ok(dir)
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

fn guarded_think_config(moted_exe: &Path, model: &str) -> String {
    let mut config: Value = serde_json::from_str(&think_config(moted_exe, model))
        .expect("the built-in OpenCode configuration is valid JSON");
    config["permission"] = json!({"*":"deny","moted_*":"ask"});
    config.to_string()
}

#[cfg(test)]
mod think_config_tests {
    use super::*;

    #[test]
    fn compatibility_config_holds_mote_tools_and_keeps_builtins_disabled() {
        let executable = Path::new("/tmp/moted");
        let baseline: Value =
            serde_json::from_str(&think_config(executable, "qwen3-coder:30b-64k")).unwrap();
        let guarded: Value =
            serde_json::from_str(&guarded_think_config(executable, "qwen3-coder:30b-64k"))
                .unwrap();

        assert!(baseline.get("permission").is_none());
        assert_eq!(guarded["tools"], baseline["tools"]);
        assert!(guarded["tools"]
            .as_object()
            .unwrap()
            .values()
            .all(|enabled| enabled == false));
        assert_eq!(guarded["permission"], json!({"*":"deny","moted_*":"ask"}));
        assert_eq!(
            guarded["mcp"].as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["moted"]
        );
        assert_eq!(guarded["model"], "ollama/qwen3-coder:30b-64k");
    }
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
fn test_native_runtime(data_dir: &Path) -> hrns_native::NativeTaskRuntime {
    let database = data_dir.join("native-runtime.db");
    let log = EventLog::open(&database).unwrap();
    hrns_native::NativeTaskRuntime::production(
        &database,
        log,
        Ollama::new("http://127.0.0.1:9"),
        Arc::new(config::AllowAll),
    )
    .unwrap()
}

pub(crate) struct Daemon {
    browser: Mutex<Option<Arc<Mutex<browser::BrowserTask>>>>,
    host_id: mote_protocol::HostId,
    gateway: Mutex<Option<std::sync::Weak<mote_gateway::GatewayServer>>>,
    log: EventLog,
    config: Mutex<ConfigStore>,
    harness: Mutex<hrns::DaemonHarnessRuntime>,
    native: Mutex<hrns_native::NativeTaskRuntime>,
    character: Arc<Mutex<Charter>>,
    store: Mutex<ContextStore>,
    client: Ollama,
    watchers: Mutex<Watchers>,
    think: Mutex<ThinkServers>,
    matrix_sources: Mutex<HashMap<String, String>>,
    matrix_snapshots: Mutex<HashMap<String, matrix::HostSnapshot>>,
    matrix_surfaces: Mutex<HashMap<String, matrix::CommittedMatrixSurface>>,
    matrix_candidates: Mutex<HashMap<(String, String), matrix_lang::CandidateStream>>,
    inferred_retry_budget: Mutex<inferred_ui::RetryBudget>,
    inferred_surface_turns: Mutex<HashMap<String, i64>>,
    data_dir: PathBuf,
    started: std::time::Instant,
}

struct HarnessPhase<'a> {
    runtime: &'a Mutex<hrns::DaemonHarnessRuntime>,
}

impl Drop for HarnessPhase<'_> {
    fn drop(&mut self) {
        let _ = self.runtime.lock().unwrap().checkpoint();
    }
}

struct MatrixCommit<'a> {
    sources: &'a mut HashMap<String, String>,
    snapshots: &'a mut HashMap<String, matrix::HostSnapshot>,
    thread: &'a str,
    surface: &'a matrix::CompiledSurface,
    source: Option<&'a str>,
    bindings: &'a matrix::HostBindings,
    host: matrix::HostSnapshot,
}

enum MatrixCandidateFinalization {
    Committed(Value),
    Rejected(Value),
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
enum AnswerCompletion<'a> {
    TalkDone(&'a str),
    ThinkDone(&'a str),
    TalkUnavailable,
    ThinkFailed,
    HandoffUnfinished,
}

#[allow(dead_code)]
#[derive(Debug, PartialEq, Eq)]
enum SurfaceInferenceOutcome {
    Ineligible,
    AlreadyAttempted,
    None,
    Committed,
    Rejected,
}

impl Daemon {
    fn harness_phase(&self, phase: hrns::RuntimePhase) -> HarnessPhase<'_> {
        self.harness.lock().unwrap().enter(phase);
        HarnessPhase {
            runtime: &self.harness,
        }
    }

    fn set_config(&self, key: &str, value: &Value) -> Result<i64, RpcError> {
        let requested = if key == "agent.harness" {
            let harness = hrns::configured_harness(Some(value))
                .map_err(|error| RpcError::invalid_params(error.to_string()))?;
            self.harness
                .lock()
                .unwrap()
                .validate_harness(harness)
                .map_err(|error| RpcError::invalid_params(error.to_string()))?;
            Some(harness)
        } else {
            None
        };
        let seq = self
            .config
            .lock()
            .unwrap()
            .set_json(key, value)
            .map_err(|error| RpcError::invalid_params(error.to_string()))?;
        if let Some(harness) = requested {
            self.harness
                .lock()
                .unwrap()
                .request_harness(harness)
                .map_err(|error| RpcError::internal(error.to_string()))?;
        }
        Ok(seq)
    }

    fn revert_config(&self, seq: i64) -> Result<i64, RpcError> {
        let event = self
            .log
            .get(seq)
            .map_err(|error| RpcError::internal(error.to_string()))?
            .ok_or_else(|| RpcError::invalid_params(format!("no event at seq {seq}")))?;
        let requested = (event.kind == "config.changed"
            && event.payload["key"].as_str() == Some("agent.harness"))
        .then(|| hrns::configured_harness(Some(&event.payload["old"])))
        .transpose()
        .map_err(|error| RpcError::invalid_params(error.to_string()))?;
        if let Some(harness) = requested {
            self.harness
                .lock()
                .unwrap()
                .validate_harness(harness)
                .map_err(|error| RpcError::invalid_params(error.to_string()))?;
        }
        let new_seq = self
            .config
            .lock()
            .unwrap()
            .revert(seq)
            .map_err(|error| RpcError::invalid_params(error.to_string()))?;
        if let Some(harness) = requested {
            self.harness
                .lock()
                .unwrap()
                .request_harness(harness)
                .map_err(|error| RpcError::internal(error.to_string()))?;
        }
        Ok(new_seq)
    }

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
        let selection = self.harness.lock().unwrap().selection();
        let harness = pipeline::HarnessSnapshot {
            configured: hrns::harness_name(selection.configured).into(),
            active: hrns::harness_name(selection.active).into(),
            pending: selection.pending.map(hrns::harness_name).map(str::to_string),
        };
        let store = self.store.lock().unwrap();
        pipeline::build_snapshot(&self.log, &store, config, harness, now_ms())
            .map_err(|error| RpcError::internal(format!("read pipeline snapshot: {error}")))
    }

    fn compile_matrix_with_host(
        &self,
        source: &str,
        host: &matrix::HostSnapshot,
    ) -> Result<(matrix::CompiledSurface, matrix::HostBindings), Value> {
        let snapshot = self.pipeline_snapshot().map_err(|error| {
            json!([{"code":"E_HOST","severity":"error","message":error.message}])
        })?;
        let bindings = matrix::host_bindings(&snapshot.config, &snapshot, host)
            .map_err(|errors| matrix::diagnostics(&errors))?;
        matrix::compile_contract(source, &bindings)
            .map(|surface| (surface, bindings))
            .map_err(|errors| matrix::diagnostics(&errors))
    }

    #[cfg(test)]
    fn compile_matrix(&self, source: &str) -> Result<Value, Value> {
        self.compile_matrix_with_host(source, &matrix::HostSnapshot::default())
            .map(|(surface, _)| surface.fallback)
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
        let (payload, mut committed) = commit.surface.prepare(&commit.bindings.manifest)
            .map_err(|error| RpcError::internal(format!("validate surface contract: {error}")))?;
        let event = self
            .log
            .append(commit.thread, "ui.surface", &payload)
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
        Ok(matrix::render_success(true))
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
        let stream = candidates.remove(&key).unwrap();
        drop(candidates);
        match self.finalize_matrix_candidate(thread, stream)? {
            MatrixCandidateFinalization::Committed(response) => Ok(response),
            MatrixCandidateFinalization::Rejected(diagnostics) => {
                self.emit_matrix_diagnostic(thread, Some(candidate), &diagnostics)?;
                Ok(json!({"status":"rejected", "diagnostics":diagnostics}))
            }
        }
    }

    fn finalize_matrix_candidate(
        &self,
        thread: &str,
        stream: matrix_lang::CandidateStream,
    ) -> Result<MatrixCandidateFinalization, RpcError> {
        let parsed = match stream.finalize() {
            Ok(parsed) => parsed,
            Err(errors) => {
                return Ok(MatrixCandidateFinalization::Rejected(matrix::diagnostics(&errors)));
            }
        };
        let mut sources = self.matrix_sources.lock().unwrap();
        let mut snapshots = self.matrix_snapshots.lock().unwrap();
        let snapshot = self.pipeline_snapshot()?;
        let host = snapshots.get(thread).cloned().unwrap_or_default();
        let bindings = match matrix::host_bindings(&snapshot.config, &snapshot, &host) {
            Ok(bindings) => bindings,
            Err(errors) => {
                return Ok(MatrixCandidateFinalization::Rejected(matrix::diagnostics(&errors)));
            }
        };
        match matrix::compile_surface_contract(&parsed, &bindings) {
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
                Ok(MatrixCandidateFinalization::Committed(response))
            }
            Err(errors) => Ok(MatrixCandidateFinalization::Rejected(matrix::diagnostics(&errors))),
        }
    }

    fn finalize_inferred_source(
        &self,
        thread: &str,
        source: &str,
    ) -> Result<MatrixCandidateFinalization, RpcError> {
        let mut stream = matrix_lang::CandidateStream::new();
        stream.push(source.as_bytes());
        self.finalize_matrix_candidate(thread, stream)
    }

    #[allow(dead_code)]
    fn infer_surface_candidate(
        &self,
        thread: &str,
        candidate: &str,
        request: &str,
        prose: &str,
    ) -> Result<Value, RpcError> {
        self.infer_surface_with(
            thread,
            candidate,
            request,
            prose,
            std::time::Instant::now(),
            |inputs| {
                let store = self.store.lock().unwrap();
                inferred_ui::generate_surface(&self.log, &store, &self.client, inputs)
            },
        )
    }

    fn infer_surface_with<F>(
        &self,
        thread: &str,
        candidate: &str,
        request: &str,
        prose: &str,
        now: std::time::Instant,
        mut generate: F,
    ) -> Result<Value, RpcError>
    where
        F: FnMut(&inferred_ui::SurfaceInputs<'_>) -> Result<inferred_ui::SurfaceDecision, String>,
    {
        let prior_source = self
            .matrix_sources
            .lock()
            .unwrap()
            .get(thread)
            .cloned()
            .unwrap_or_default();
        let surface_seq = self
            .matrix_surfaces
            .lock()
            .unwrap()
            .get(thread)
            .map_or(0, |surface| surface.seq);
        let safe_prior = mote_redact::redact(&prior_source);
        let first = inferred_ui::SurfaceInputs {
            request,
            prose,
            previous_source: &safe_prior,
            diagnostics: "",
        };
        let inferred_ui::SurfaceDecision::Matrix { source } = (match generate(&first) {
            Ok(decision) => decision,
            Err(_) => return Ok(json!({"status":"none"})),
        }) else {
            return Ok(json!({"status":"none"}));
        };
        let diagnostics = match self.finalize_inferred_source(thread, &source)? {
            MatrixCandidateFinalization::Committed(response) => return Ok(response),
            MatrixCandidateFinalization::Rejected(diagnostics) => diagnostics,
        };
        let may_retry = self
            .inferred_retry_budget
            .lock()
            .unwrap()
            .acquire(thread, surface_seq, now);
        if !may_retry {
            self.emit_matrix_diagnostic(thread, Some(candidate), &diagnostics)?;
            return Ok(json!({"status":"rejected", "diagnostics":diagnostics}));
        }

        let safe_diagnostics = mote_redact::redact(&diagnostics.to_string());
        let retry = inferred_ui::SurfaceInputs {
            request,
            prose,
            previous_source: &safe_prior,
            diagnostics: &safe_diagnostics,
        };
        let inferred_ui::SurfaceDecision::Matrix { source } = (match generate(&retry) {
            Ok(decision) => decision,
            Err(_) => return Ok(json!({"status":"none"})),
        }) else {
            return Ok(json!({"status":"none"}));
        };
        match self.finalize_inferred_source(thread, &source)? {
            MatrixCandidateFinalization::Committed(response) => Ok(response),
            MatrixCandidateFinalization::Rejected(diagnostics) => {
                self.emit_matrix_diagnostic(thread, Some(candidate), &diagnostics)?;
                Ok(json!({"status":"rejected", "diagnostics":diagnostics}))
            }
        }
    }

    fn infer_surface_after_answer_with<F>(
        &self,
        thread: &str,
        turn_seq: i64,
        request: &str,
        completion: AnswerCompletion<'_>,
        now: std::time::Instant,
        generate: F,
    ) -> Result<SurfaceInferenceOutcome, RpcError>
    where
        F: FnMut(&inferred_ui::SurfaceInputs<'_>) -> Result<inferred_ui::SurfaceDecision, String>,
    {
        let prose = match completion {
            AnswerCompletion::TalkDone(prose) | AnswerCompletion::ThinkDone(prose) => prose,
            AnswerCompletion::TalkUnavailable
            | AnswerCompletion::ThinkFailed
            | AnswerCompletion::HandoffUnfinished => {
                return Ok(SurfaceInferenceOutcome::Ineligible);
            }
        };
        {
            let mut turns = self.inferred_surface_turns.lock().unwrap();
            if turns.get(thread).is_some_and(|last| *last >= turn_seq) {
                return Ok(SurfaceInferenceOutcome::AlreadyAttempted);
            }
            turns.insert(thread.to_owned(), turn_seq);
        }
        let response = self.infer_surface_with(
            thread,
            &format!("surface-{turn_seq}"),
            request,
            prose,
            now,
            generate,
        )?;
        Ok(match response.get("status").and_then(Value::as_str) {
            Some("committed") => SurfaceInferenceOutcome::Committed,
            Some("rejected") => SurfaceInferenceOutcome::Rejected,
            _ => SurfaceInferenceOutcome::None,
        })
    }

    #[allow(dead_code)]
    fn infer_surface_after_answer(
        &self,
        thread: &str,
        turn_seq: i64,
        request: &str,
        completion: AnswerCompletion<'_>,
    ) -> Result<SurfaceInferenceOutcome, RpcError> {
        self.infer_surface_after_answer_with(
            thread,
            turn_seq,
            request,
            completion,
            std::time::Instant::now(),
            |inputs| {
                let store = self.store.lock().unwrap();
                inferred_ui::generate_surface(&self.log, &store, &self.client, inputs)
            },
        )
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
        let user_event = self
            .log
            .append(thread_id, "turn.user", &user_payload)
            .map_err(|error| RpcError::internal(format!("record turn: {error}")))?;
        let _harness_phase = self.harness_phase(hrns::RuntimePhase::ModelStep);

        let (talk_model, router_model, think_model, budget, think_budget, charter_prose) = {
            let cfg = self.config.lock().unwrap();
            let talk = cfg
                .get_json("models.talk")
                .and_then(|v| v.as_str().map(String::from));
            let router = cfg
                .get_json("models.router")
                .and_then(|v| v.as_str().map(String::from));
            let think = cfg
                .get_json("models.think")
                .and_then(|v| v.as_str().map(String::from));
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
        let profile = store
            .get_profile("profile")
            .ok()
            .flatten()
            .unwrap_or_default();
        let working_set = store.working_set_text(thread_id).unwrap_or_default();
        // The talk pack carries the last 8 memories by retrieval (v0.md section 6).
        let memories = store
            .memory_retrieve(message, 8)
            .map(|hits| {
                hits.iter()
                    .map(|h| h.text.clone())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let inputs = TalkInputs {
            charter: &charter_prose,
            profile: &profile,
            memories: &memories,
            working_set: &working_set,
            turn: message,
        };
        let outcome = run_turn(
            &self.log,
            &store,
            &self.client,
            thread_id,
            &talk_model,
            TALK_CTX,
            &router_model,
            ROUTER_CTX,
            &inputs,
            budget,
        );
        let turn_ts = now_ms();
        let (response, completed_answer) = match outcome {
            TalkOutcome::Done {
                ledger_id,
                pack_id,
                answer,
            } => {
                lanes::record_turn(&store, thread_id, turn_ts, message, &answer);
                (
                    json!({ "status": "done", "ledger_id": ledger_id, "pack_id": pack_id }),
                    Some((false, answer)),
                )
            }
            TalkOutcome::Unavailable(reason) => {
                (json!({ "status": "unavailable", "reason": reason }), None)
            }
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
                let active_harness = self.harness.lock().unwrap().selection().active;
                if active_harness == mote_hrns::HarnessKind::Mote {
                    let task_id = mote_hrns::TaskId::new(format!("turn-{}", user_event.seq))
                        .map_err(RpcError::internal)?;
                    let expected_task = task_id.clone();
                    let scope = mote_hrns::Scope::new("owner", "personal")
                        .map_err(RpcError::internal)?;
                    match self.native.lock().unwrap().submit(
                        task_id,
                        scope,
                        turn,
                        system,
                    ) {
                        Ok(hrns_native::NativeTurnOutcome::Complete {
                            task_id,
                            text,
                            sources,
                        }) if task_id == expected_task => {
                            lanes::record_turn(&store, thread_id, turn_ts, message, &text);
                            (
                                json!({
                                    "status":"task_done",
                                    "task_id":task_id,
                                    "sources":sources,
                                }),
                                Some((true, text)),
                            )
                        }
                        Ok(hrns_native::NativeTurnOutcome::WaitingForApproval {
                            task_id,
                            action_id,
                        }) => (
                            json!({
                                "status":"task_waiting_approval",
                                "task_id":task_id,
                                "action_id":action_id,
                            }),
                            None,
                        ),
                        Ok(hrns_native::NativeTurnOutcome::WaitingForInput {
                            task_id,
                            question,
                        }) => (
                            json!({
                                "status":"task_waiting_input",
                                "task_id":task_id,
                                "question":question,
                            }),
                            None,
                        ),
                        Ok(hrns_native::NativeTurnOutcome::Blocked { task_id }) => (
                            json!({"status":"task_blocked","task_id":task_id}),
                            None,
                        ),
                        Ok(hrns_native::NativeTurnOutcome::Complete { task_id, .. }) => (
                            json!({
                                "status":"task_queued",
                                "task_id":expected_task,
                                "active_task_id":task_id,
                            }),
                            None,
                        ),
                        Err(error) => (
                            json!({
                                "status":"task_failed",
                                "task_id":expected_task,
                                "reason":error.to_string(),
                            }),
                            None,
                        ),
                    }
                } else {
                let full_model = format!("ollama/{think_model}");
                let think_outcome = if hrns_compat::enabled() {
                    match self
                        .think
                        .lock()
                        .unwrap()
                        .factory(thread_id, &think_model)
                    {
                        Ok(factory) => run_think_model_call(
                            &self.log,
                            &store,
                            thread_id,
                            &think_model,
                            &pack,
                            &system,
                            &turn,
                            || {
                                hrns_compat::run(
                                    factory,
                                    &full_model,
                                    thread_id,
                                    &system,
                                    &turn,
                                    THINK_TIMEOUT,
                                )
                            },
                        ),
                        Err(reason) => {
                            let _ = self.log.append(
                                thread_id,
                                "think.failed",
                                &json!({"reason":reason.clone()}),
                            );
                            ThinkOutcome::Failed(reason)
                        }
                    }
                } else {
                    let server = self.think.lock().unwrap().get(thread_id, &think_model);
                    match server.and_then(|server| {
                        let session = server.create_session()?;
                        Ok((server, session))
                    }) {
                        Ok((server, session)) => run_think_model_call(
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
                        ),
                        Err(reason) => {
                            let _ = self.log.append(
                                thread_id,
                                "think.failed",
                                &json!({"reason":reason.clone()}),
                            );
                            ThinkOutcome::Failed(reason)
                        }
                    }
                };
                match think_outcome {
                    ThinkOutcome::Done {
                        ledger_id,
                        pack_id,
                        answer,
                    } => {
                        lanes::record_turn(&store, thread_id, turn_ts, message, &answer);
                        (
                            json!({
                                "status": "think_done", "ledger_id": ledger_id,
                                "pack_id": pack_id
                            }),
                            Some((true, answer)),
                        )
                    }
                    ThinkOutcome::Failed(reason) => {
                        (json!({ "status": "think_failed", "reason": reason }), None)
                    }
                }
                }
            }
        };
        drop(store);
        if let Some((is_think, answer)) = completed_answer {
            let local_only =
                self.config.lock().unwrap().get_json("models.local_only") == Some(json!(true));
            if local_only
                && surface_activation::SurfaceActivationStore::new(&self.data_dir)
                    .allows(&self.client)
            {
                let completion = if is_think {
                    AnswerCompletion::ThinkDone(&answer)
                } else {
                    AnswerCompletion::TalkDone(&answer)
                };
                let _ =
                    self.infer_surface_after_answer(thread_id, user_event.seq, message, completion);
            }
        }
        Ok(response)
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
    fn host_snapshot_delivered(&self, actor: &Actor, snapshot: &mote_protocol::HostSnapshot) -> Result<(), RpcError> {
        browser::deliver_snapshot(self, actor, snapshot)
    }
    fn host_event_delivered(&self, actor: &Actor, event: &mote_protocol::Event) -> Result<(), RpcError> {
        browser::deliver(self, actor, event)
    }
    fn call_with_context(&self, actor: &Actor, method: &str, params: Value) -> Result<Value, RpcError> {
        gateway::dispatch(self, actor, method, params)
    }
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        if method.starts_with("browser.") { return browser::call(self, method, params); }
        match method {
            "hello" => Ok(json!({"host_id":self.host_id,"protocol_major":mote_protocol::PROTOCOL_MAJOR})),
            "gateway.pair" | "gateway.pending" | "gateway.confirm" | "gateway.revoke" => gateway::local_control(self, method, params),
            "ping" => Ok(json!({ "ok": true })),
            "status" => {
                let uptime = self.started.elapsed().as_secs();
                let last_seq = self.log.max_seq().unwrap_or(0);
                let loaded = self.client.ps().unwrap_or_default();
                let selection = self.harness.lock().unwrap().selection();
                Ok(json!({
                    "socket": mote_paths::socket_path().to_string_lossy(),
                    "uptime_s": uptime,
                    "loaded_models": loaded,
                    "last_seq": last_seq,
                    "harness": {
                        "configured": hrns::harness_name(selection.configured),
                        "active": hrns::harness_name(selection.active),
                        "pending": selection.pending.map(hrns::harness_name),
                    },
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
                let seq = self.set_config(key, value)?;
                Ok(json!({ "seq": seq }))
            }
            "config.revert" => {
                let seq = params.get("seq").and_then(|s| s.as_i64()).ok_or_else(|| {
                    RpcError::invalid_params("config.revert needs a seq")
                })?;
                let new_seq = self.revert_config(seq)?;
                Ok(json!({ "seq": new_seq }))
            }
            "pipeline.snapshot" => {
                let snapshot = self.pipeline_snapshot()?;
                serde_json::to_value(snapshot)
                    .map_err(|error| RpcError::internal(format!("encode pipeline snapshot: {error}")))
            }
            "bench.talk" => {
                let model = self.config.lock().unwrap().get_json("models.talk")
                    .and_then(|value| value.as_str().map(String::from))
                    .ok_or_else(|| RpcError::internal("models.talk unset"))?;
                let store = self.store.lock().unwrap();
                bench::bench_talk(&self.log, &store, &self.client, &model, TALK_CTX, bench::RUNS)
                    .map(|result| result.to_line())
                    .map_err(RpcError::internal)
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
                        || params.get("surface_contract").is_some()
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
                    let value = matrix::canonical_change(field, value).ok_or_else(|| {
                        RpcError::invalid_params(
                            "change does not match an exposed field contract",
                        )
                    })?;
                    let seq = self.set_config(field, &value)?;
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
                let (executed, action_id) = {
                    let watchers = self.watchers.lock().unwrap();
                    let executed = watchers
                        .approve(id, now_ms())
                        .map_err(|e| RpcError::internal(e.to_string()))?;
                    let action_id = watchers
                        .hrns_action(id)
                        .map_err(|e| RpcError::internal(e.to_string()))?;
                    (executed, action_id)
                };
                if let Some(action_id) = action_id {
                    hrns_actions::reconcile_approval(
                        &self.data_dir.join("mote.db"),
                        self.log.clone(),
                        &self.host_id,
                        &action_id,
                    )
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                }
                Ok(json!({ "executed": executed }))
            }
            "deny" => {
                let id = params.get("id").and_then(|i| i.as_i64()).ok_or_else(|| {
                    RpcError::invalid_params("deny needs an id")
                })?;
                let (denied, action_id) = {
                    let watchers = self.watchers.lock().unwrap();
                    let denied = watchers
                        .deny(id, now_ms())
                        .map_err(|e| RpcError::internal(e.to_string()))?;
                    let action_id = watchers
                        .hrns_action(id)
                        .map_err(|e| RpcError::internal(e.to_string()))?;
                    (denied, action_id)
                };
                if let Some(action_id) = action_id {
                    hrns_actions::reconcile_approval(
                        &self.data_dir.join("mote.db"),
                        self.log.clone(),
                        &self.host_id,
                        &action_id,
                    )
                    .map_err(|error| RpcError::internal(error.to_string()))?;
                }
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

    fn subscribe_checked(&self, scope_id: &str, cursor: i64) -> Option<mote_eventlog::Replay> {
        self.log.subscribe_checked(scope_id, cursor).ok()
    }

    fn event_cursor(&self) -> i64 { self.log.max_seq().unwrap_or(0) }

    fn subscribe_host(&self, cursor: i64) -> Option<mote_eventlog::Replay> {
        self.log.subscribe_host(cursor).ok()
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
    mod loopback_acceptance {
        include!("gateway_acceptance_tests.rs");
    }
    use super::*;
    use charter::FrontMatter;
    use config::AllowAll;

    #[test]
    fn gateway_startup_migrates_the_existing_database_under_real_identity() {
        use std::os::unix::fs::PermissionsExt;
        let (data, _) = dirs("gateway-existing-db");
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = data.join("mote.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE retained(value TEXT); INSERT INTO retained VALUES('existing');").unwrap();
        drop(conn);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let identity = gateway::prepare_identity(&data).unwrap();
        assert_eq!(identity.host_id(), mote_gateway::HostIdentity::load_or_create(&data).unwrap().host_id());
        let gateway = mote_gateway::GatewayServer::bind_loopback({ let mut config = mote_gateway::GatewayConfig::new(&data); config.bind = "127.0.0.1:0".parse().unwrap(); config }, Arc::new(daemon(&data))).unwrap();
        assert_eq!(gateway.host_id(), identity.host_id());
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(rusqlite::Connection::open(&path).unwrap().query_row("SELECT value FROM retained", [], |r| r.get::<_,String>(0)).unwrap(), "existing");
    }

    fn paired_fixture(name: &str) -> (Arc<Daemon>, Arc<mote_gateway::GatewayServer>, Server, mote_gateway::GatewayConfig, PathBuf) {
        paired_fixture_with_client(name, None)
    }

    fn paired_fixture_with_client(name: &str, client: Option<Ollama>) -> (Arc<Daemon>, Arc<mote_gateway::GatewayServer>, Server, mote_gateway::GatewayConfig, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let (data, outside) = dirs(name);
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut daemon = daemon(&data);
        gateway::prepare_identity(&data).unwrap();
        daemon.log = EventLog::open(&data.join("mote.db")).unwrap();
        daemon.watchers = Mutex::new(Watchers::open(rusqlite::Connection::open(data.join("mote.db")).unwrap()).unwrap());
        if let Some(client) = client { daemon.client = client; }
        daemon.host_id = mote_gateway::HostIdentity::load_or_create(&data).unwrap().host_id().clone();
        let daemon = Arc::new(daemon);
        let socket = PathBuf::from("/tmp").join(format!("mote-owner-{name}-{}-{}.sock", std::process::id(), now_ms()));
        let local = Server::bind(&socket, daemon.clone()).unwrap();
        let mut config = mote_gateway::GatewayConfig::new(data);
        config.bind = "127.0.0.1:0".parse().unwrap();
        let gateway = gateway::bind(daemon.clone(), config.clone()).unwrap();
        (daemon, gateway, local, config, outside)
    }

    fn paired_client(gateway: &mote_gateway::GatewayServer, local: &Server, config: &mote_gateway::GatewayConfig) -> (mote_gateway::GatewayClient, mote_gateway::ClientCredential) {
        let mut owner = mote_ipc::Client::connect(local.path()).unwrap();
        let code = owner.call("gateway.pair", json!({})).unwrap().unwrap()["code"].as_str().unwrap().to_string();
        let credential = mote_gateway::ClientCredential::from_pairing_code(config, &code).unwrap();
        let client = std::thread::scope(|scope| {
            let client = scope.spawn(|| mote_gateway::GatewayClient::pair(gateway.local_addr(), config, &credential, "Fixture iPad", 1));
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            loop {
                let devices = owner.call("gateway.pending", json!({})).unwrap().unwrap();
                if let Some(id) = devices["devices"][0]["client_id"].as_str() {
                    owner.call("gateway.confirm", json!({"client_id":id,"approved":true})).unwrap().unwrap();
                    break;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
            client.join().unwrap().unwrap()
        });
        (client, credential)
    }

    fn remote_call(client: &mut mote_gateway::GatewayClient, id: &str, method: &str, params: Value) -> mote_protocol::HostFrame {
        client.exchange(&mote_protocol::ClientFrame::Call { request_id: mote_protocol::RequestId::new(id).unwrap(), method: method.into(), params }).unwrap()
    }

    fn remote_snapshot(client: &mut mote_gateway::GatewayClient) -> mote_protocol::HostSnapshot {
        let mote_protocol::HostFrame::Result { result, .. } = remote_call(client, "snapshot", "snapshot", json!({})) else { panic!("snapshot result"); };
        serde_json::from_value(result).unwrap()
    }

    #[test]
    fn paired_turn_uses_approved_local_models_ledger_and_redacted_events_once() {
        use mote_protocol::HostFrame;
        let models = mote_testkit::ScriptedServer::start().unwrap();
        models.set_chat_chunks(vec![json!({"message":{"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"greeting\"}"},"done":true}).to_string()]);
        let (daemon, gateway, local, config, _) = paired_fixture_with_client("paired-turn", Some(Ollama::new(models.base_url())));
        let (mut client, _) = paired_client(&gateway, &local, &config);
        let secret = "sk-abcdefghijklmnopqrstuvwxyz";
        let params = json!({"thread":"main","message":format!("hello {secret}")});
        let outcome = remote_call(&mut client, "turn-once", "turn", params.clone());
        assert!(matches!(&outcome, HostFrame::Result { result, .. } if result["status"] == "done"));
        assert_eq!(remote_call(&mut client, "turn-once", "turn", params), outcome);
        assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 2);
        assert_eq!(daemon.log.recent_kind_since("turn.user", 0, 100).unwrap().len(), 1);
        assert!(!serde_json::to_string(&daemon.log.recent_since(0, 100).unwrap()).unwrap().contains(secret));
        let requests = models.recorded();
        let calls: Vec<_> = requests.iter().filter(|r| r.path == "/api/chat").collect();
        assert_eq!(calls.len(), 2);
        for call in calls {
            let body = &call.body;
            assert!(matches!(body["model"].as_str(), Some("qwen3:4b" | "qwen3:0.6b")));
            assert_eq!(body["options"]["num_ctx"], 32768);
            assert!(!call.body.to_string().contains(secret));
        }
    }

    #[test]
    fn paired_hosts_with_colliding_ids_keep_state_and_actions_isolated() {
        use mote_host_client::{HostDirectory, PairedHost};
        use mote_protocol::{ApprovalDecision, ApprovalVerdict, ClientFrame, HostFrame, RequestId, TypedInteraction, VersionRange};
        let (home, home_gateway, home_local, home_config, home_outside) = paired_fixture("paired-home");
        let (office, office_gateway, office_local, office_config, office_outside) = paired_fixture("paired-office");
        let (mut home_client, home_credential) = paired_client(&home_gateway, &home_local, &home_config);
        let (mut office_client, office_credential) = paired_client(&office_gateway, &office_local, &office_config);
        let mut directory = HostDirectory::default();
        let mut snapshots = Vec::new();
        for (daemon, gateway, client, outside, label) in [(&home, &home_gateway, &mut home_client, &home_outside, "Home"), (&office, &office_gateway, &mut office_client, &office_outside, "Office")] {
            let target = outside.join("held.txt");
            assert_eq!(daemon.call("file.write", json!({"thread":"main","path":target,"content":label})).unwrap()["id"], 1);
            daemon.call("matrix.render", json!({"thread":"main","source":format!("screen {{ text \"{label}\" button \"Refresh\" -> refresh }}")})).unwrap();
            daemon.log.append("main", "turn.user", &json!({"text":label})).unwrap();
            let snapshot = remote_snapshot(client);
            assert_eq!(snapshot.threads[0].messages[0].text, label);
            assert!(!serde_json::to_string(&snapshot).unwrap().contains(outside.to_str().unwrap()));
            directory.register(PairedHost::new(*gateway.public_key(), label, None, VersionRange::current()).unwrap()).unwrap();
            directory.install_snapshot(gateway.host_id(), snapshot.clone()).unwrap();
            directory.select_thread(gateway.host_id(), Some(format!("{label}-selection"))).unwrap();
            snapshots.push(snapshot);
            assert!(!target.exists());
        }
        assert_ne!(home_credential.private_key_for_secure_storage(), office_credential.private_key_for_secure_storage());
        assert_eq!(snapshots[0].pending_approvals[0].approval_id, snapshots[1].pending_approvals[0].approval_id);
        assert_eq!(snapshots[0].surfaces[0].surface_seq, snapshots[1].surfaces[0].surface_seq);
        for _ in 0..5 {
            for (gateway, snapshot, label) in [(&home_gateway, &snapshots[0], "Home"), (&office_gateway, &snapshots[1], "Office")] {
                directory.select_host(gateway.host_id()).unwrap();
                let state = directory.state(gateway.host_id()).unwrap();
                assert_eq!(state.cursor(), &snapshot.cursor);
                assert_eq!(state.selected_thread(), Some(format!("{label}-selection").as_str()));
                assert_eq!(state.threads()["main"].messages[0].text, label);
                assert_eq!(state.surfaces()["main"][&2], snapshot.surfaces[0]);
                assert_eq!(state.pending_approvals()[&RequestId::new("1").unwrap()], snapshot.pending_approvals[0]);
            }
        }
        let approval = &snapshots[0].pending_approvals[0];
        let decision = ApprovalDecision { host_id: home.host_id.clone(), thread: "main".into(), approval_id: approval.approval_id.clone(), action_hash: approval.action_hash.clone(), expires_at: approval.expires_at, decision: ApprovalVerdict::Approve };
        let (mut unbound_client, _) = paired_client(&home_gateway, &home_local, &home_config);
        assert!(matches!(unbound_client.exchange(&ClientFrame::ApprovalDecision { request_id: RequestId::new("unbound").unwrap(), decision: decision.clone() }).unwrap(), HostFrame::Error { .. }));
        assert!(!home_outside.join("held.txt").exists());
        let mut cross = decision.clone();
        cross.host_id = office.host_id.clone();
        assert!(matches!(office_client.exchange(&ClientFrame::ApprovalDecision { request_id: RequestId::new("cross").unwrap(), decision: cross }).unwrap(), HostFrame::Error { .. }));
        assert!(!home_outside.join("held.txt").exists());
        assert!(!office_outside.join("held.txt").exists());
        let interaction = TypedInteraction { host_id: home.host_id.clone(), thread: "main".into(), surface_seq: 2, surface_contract: 1, node_path: vec![1], event: "refresh".into(), args: serde_json::Map::new() };
        assert!(matches!(home_client.exchange(&ClientFrame::Interaction { request_id: RequestId::new("refresh").unwrap(), interaction }).unwrap(), HostFrame::Result { .. }));
        assert_eq!(office.log.max_seq().unwrap(), 3);
        let frame = ClientFrame::ApprovalDecision { request_id: RequestId::new("approve-once").unwrap(), decision };
        let outcome = home_client.exchange(&frame).unwrap();
        assert!(matches!(outcome, HostFrame::Result { .. }));
        assert_eq!(std::fs::read_to_string(home_outside.join("held.txt")).unwrap(), "Home");
        assert!(!office_outside.join("held.txt").exists());
        let before = home.log.max_seq().unwrap();
        assert_eq!(home_client.exchange(&frame).unwrap(), outcome);
        assert_eq!(home.log.max_seq().unwrap(), before);
        let wrong_device = mote_gateway::GatewayClient::connect(office_gateway.local_addr(), &home_config, &home_credential, 2);
        assert!(wrong_device.is_err());
        assert!(matches!(remote_call(&mut office_client, "raw-approve", "approve", json!({"id":1})), HostFrame::Error { .. }));
        assert!(!office_outside.join("held.txt").exists());
        assert_eq!(home.store.lock().unwrap().count_ledger().unwrap(), 0);
        assert_eq!(office.store.lock().unwrap().count_ledger().unwrap(), 0);
    }

    #[test]
    fn checked_in_dashboard_commits_and_renders_anchored_v1_controls() {
        let (data, _) = dirs("checked-dashboard-v1");
        let daemon = daemon(&data);
        let source = include_str!("../../matrix/examples/mote-dashboard.matrix");
        assert_eq!(daemon.render_matrix("main", source, true).unwrap()["valid"], true);
        assert_eq!(daemon.log.max_seq().unwrap(), 1);
        let event = daemon.log.get(1).unwrap().unwrap();
        let envelope = mote_ui::v1::validate_envelope(&serde_json::to_vec(&event.payload).unwrap()).unwrap();
        assert_eq!(event.payload["surface"], json!({"type":"text","value":"This Matrix surface requires a newer Mote client."}));
        assert_eq!(envelope.matrix_surface.version, 1);
        let children = &event.payload["matrix_surface"]["root"]["children"];
        assert_eq!(children[0]["type"], "header");
        assert_eq!(children[1]["type"], "col");
        assert_eq!(children[2]["type"], "footer");
        assert_eq!(children[0]["children"][0]["children"][0]["fill"], true);
        fn inventory(node: &Value, kinds: &mut Vec<String>) {
            kinds.push(node["type"].as_str().unwrap().into());
            if let Some(children) = node["children"].as_array() {
                for child in children { inventory(child, kinds); }
            }
        }
        let mut kinds = Vec::new();
        inventory(&event.payload["matrix_surface"]["root"], &mut kinds);
        assert_eq!(children[1]["children"][0]["fill"], true);
        assert_eq!(kinds, ["screen", "header", "row", "text", "button", "col", "col", "toggle", "select", "select", "select", "text", "footer", "text"]);
        let (_, bindings) = daemon.compile_matrix_with_host(source, &matrix::HostSnapshot::default()).unwrap();
        let numeric_fields: Vec<_> = bindings
            .manifest
            .fields
            .iter()
            .filter(|(_, field)| {
                matches!(
                    &field.binding.ty,
                    matrix_lang::Type::Number | matrix_lang::Type::Integer
                )
            })
            .map(|(name, field)| (name.as_str(), field.binding.ty.clone()))
            .collect();
        assert_eq!(
            numeric_fields,
            [("budgets.router", matrix_lang::Type::Integer)]
        );
        let mut legacy = mote_client::ObservationHarness::legacy();
        legacy.event(&event);
        assert_eq!(legacy.surface().unwrap().1, mote_ui::validate(&serde_json::to_vec(&event.payload["surface"]).unwrap()).unwrap());
        let mut client = mote_client::ObservationHarness::default();
        client.event(&event);
        client.frame(vec![]);
        let (_, output) = client.frame_output(vec![]);
        let labels: Vec<_> = output.shapes.iter().filter_map(|shape| match &shape.shape {
            mote_client::PaintedShape::Text(text) => Some(text.galley.text()),
            _ => None,
        }).collect();
        for label in ["Agent dashboard", "[x] Local only", "Refresh", "Local model settings"] {
            assert!(labels.contains(&label), "missing {label}: {labels:?}");
        }
        assert!(!labels.contains(&"This Matrix surface requires a newer Mote client."));
        for width in [400.0, 1200.0] {
            let mut client = mote_client::ObservationHarness::default();
            client.set_viewport(width, 800.0);
            client.event(&event);
            client.frame(vec![]);
            let (_, output) = client.frame_output(vec![]);
            let nodes = &output.platform_output.accesskit_update.unwrap().nodes;
            let bounds = |label| nodes.iter().find(|(_, node)| node.label() == Some(label)).unwrap().1.bounds().unwrap();
            let header = bounds("Agent dashboard");
            let refresh = bounds("Refresh");
            let body = bounds("Local only");
            let footer = bounds("Local model settings");
            assert!(header.y1 <= body.y0 && body.y1 <= footer.y0);
            assert!(refresh.x1 <= f64::from(width) && footer.y1 <= 800.0);
            let notice = bounds("Changes apply on the next turn.");
            let mut unfilled = event.clone();
            unfilled.payload["matrix_surface"]["root"]["children"][1]["children"][0].as_object_mut().unwrap().remove("fill");
            let mut comparison = mote_client::ObservationHarness::default();
            comparison.set_viewport(width, 800.0);
            comparison.event(&unfilled);
            comparison.frame(vec![]);
            let output = comparison.frame_output(vec![]).1;
            let unfilled_notice = output.platform_output.accesskit_update.unwrap().nodes.into_iter().find(|(_, node)| node.label() == Some("Changes apply on the next turn.")).unwrap().1.bounds().unwrap();
            assert!(notice.y0 - unfilled_notice.y0 > 100.0, "body fill must consume remaining height at width {width}");
        }
        for (label, path, event_name, args, option) in [
            ("Refresh", vec![0,0,1], "refresh", json!({}), None),
            ("[x] Local only", vec![1,0,0], "change", json!({"field":"models.local_only","value":false}), None),
            ("Talk model: qwen3:4b", vec![1,0,1], "change", json!({"field":"models.talk","value":"qwen3:0.6b"}), Some("qwen3:0.6b")),
            ("Router model: qwen3:0.6b", vec![1,0,2], "change", json!({"field":"models.router","value":"qwen3:4b"}), Some("qwen3:4b")),
            ("Think model: qwen3-coder:30b-64k", vec![1,0,3], "change", json!({"field":"models.think","value":"qwen3:4b"}), Some("qwen3:4b")),
        ] {
            let mut client = mote_client::ObservationHarness::default();
            client.set_viewport(1200.0, 800.0);
            client.event(&event);
            client.frame(vec![]);
            let click_label = |client: &mut mote_client::ObservationHarness, label: &str| {
                client.frame(vec![]);
                let (_, output) = client.frame_output(vec![]);
                let mut pos = output.shapes.iter().find_map(|shape| match &shape.shape {
                    mote_client::PaintedShape::Text(text) if text.galley.text() == label => Some(text.pos + text.galley.rect.center().to_vec2()),
                    _ => None,
                }).unwrap_or_else(|| panic!("missing control {label} after {option:?}: {:?}", output.shapes.iter().filter_map(|shape| match &shape.shape { mote_client::PaintedShape::Text(text) => Some(text.galley.text()), _ => None }).collect::<Vec<_>>()));
                let control_label = label.split(": ").next().unwrap().trim_start_matches("[x] ");
                if let Some(bounds) = output.platform_output.accesskit_update.as_ref().unwrap().nodes.iter().find(|(_, node)| node.label() == Some(control_label)).and_then(|(_, node)| node.bounds()) {
                    pos.x = ((bounds.x0 + bounds.x1) / 2.0) as f32;
                    pos.y = ((bounds.y0 + bounds.y1) / 2.0) as f32;
                }
                client.click(pos);
            };
            click_label(&mut client, label);
            if let Some(option) = option { click_label(&mut client, option); }
            let params = json!({"thread":"main","surface_seq":1,"surface_contract":1,"node_path":path,"event":event_name,"args":args});
            assert_eq!(client.queued(), std::slice::from_ref(&params));
            let committed = daemon.matrix_surfaces.lock().unwrap()["main"].clone();
            assert!(committed.accepts(&params, event_name, &args));
            let mut legacy_params = params.clone();
            legacy_params.as_object_mut().unwrap().remove("surface_contract");
            assert!(!committed.accepts(&legacy_params, event_name, &args));
            let mut stale = params; stale["surface_seq"] = json!(0);
            assert!(!committed.accepts(&stale, event_name, &args));
        }
        assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_first_slice_commits_one_dual_event_with_independent_paths() {
        let (data, _) = dirs("dual-contract");
        let daemon = daemon(&data);
        let source = "screen { button \"Refresh\" -> refresh }";
        assert_eq!(daemon.render_matrix("main", source, true).unwrap()["valid"], true);
        let seq = daemon.log.max_seq().unwrap();
        assert_eq!(seq, 1);
        let event = daemon.log.get(seq).unwrap().unwrap();
        assert_eq!(event.payload, json!({
            "surface_id":"matrix",
            "surface":{"type":"action","label":"Refresh","event":"refresh"},
            "matrix_surface":{"version":1,"root":{"type":"screen","children":[
                {"type":"button","label":"Refresh","event":"refresh"}
            ]}}
        }));
        mote_ui::v1::validate_envelope(&serde_json::to_vec(&event.payload).unwrap()).unwrap();
        let committed = daemon.matrix_surfaces.lock().unwrap()["main"].clone();
        for (contract, path, accepted) in [(None, json!([]), true), (None, json!([0]), false), (Some(json!(1)), json!([0]), true), (Some(json!(1)), json!([]), false)] {
            let mut params = json!({"surface_seq":seq,"node_path":path});
            if let Some(contract) = contract { params["surface_contract"] = contract; }
            assert_eq!(committed.accepts(&params, "refresh", &json!({})), accepted);
        }
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_anchored_fill_uses_exact_inert_fallback() {
        let (data, _) = dirs("dual-anchors");
        let daemon = daemon(&data);
        let source = "screen { header { text \"Header\" } col { text \"Body\" fill } footer { button \"Refresh\" -> refresh } }";
        assert_eq!(daemon.render_matrix("main", source, true).unwrap()["valid"], true);
        let event = daemon.log.get(1).unwrap().unwrap();
        assert_eq!(event.payload["surface"], json!({"type":"text","value":"This Matrix surface requires a newer Mote client."}));
        assert_eq!(event.payload["matrix_surface"], json!({"version":1,"root":{"type":"screen","children":[
            {"type":"header","children":[{"type":"text","value":"Header"}]},
            {"type":"col","children":[{"type":"text","value":"Body","fill":true}]},
            {"type":"footer","children":[{"type":"button","label":"Refresh","event":"refresh"}]}
        ]}}));
        mote_ui::v1::validate_envelope(&serde_json::to_vec(&event.payload).unwrap()).unwrap();
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn rejected_versioned_events_have_no_observable_side_effects() {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        let (data, _) = dirs("v1-rpc-rejections");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());
        let mut manifest = matrix_lang::Manifest::default();
        manifest.fields.insert("volume".into(), matrix_lang::manifest::FieldDefinition {
            binding: matrix_lang::Type::Number.into(), initial: json!(0.3),
        });
        manifest.actions.insert("refresh".into(), matrix_lang::manifest::ActionContract::default());
        let bindings = matrix::HostBindings { manifest, snapshot: matrix_lang::BindingSnapshot::default() };
        let compiled = matrix::compile_contract("screen { slider \"Volume\" volume min 0 max 1.05 step 0.1 button \"Refresh\" disabled -> refresh }", &bindings).unwrap();
        let (payload, mut committed) = compiled.prepare(&bindings.manifest).unwrap();
        let seq = daemon.log.append("main", "ui.surface", &payload).unwrap().seq;
        committed.seq = seq;
        daemon.matrix_surfaces.lock().unwrap().insert("main".into(), committed);
        daemon.matrix_sources.lock().unwrap().insert("main".into(), "must remain untouched".into());
        daemon.matrix_snapshots.lock().unwrap().insert("main".into(), matrix::HostSnapshot::ready(0.25, ["Prior"]));
        let baseline = json!({"event":"change","args":{"field":"volume","value":0.3},"surface_contract":1,"surface_seq":seq,"node_path":[0]});
        let mut cases = Vec::new();
        for value in [json!("0.3"), json!(false), Value::Null, json!([]), json!({}), json!(-0.1), json!(1.1), json!(0.35), json!(1.05)] {
            let mut params = baseline.clone(); params["args"]["value"] = value; cases.push(params);
        }
        for (key, value) in [
            ("surface_contract", json!(0)), ("surface_contract", json!(2)), ("surface_contract", Value::Null),
            ("surface_contract", json!("1")), ("surface_contract", json!(true)), ("surface_contract", json!(1.0)),
            ("surface_seq", json!(seq - 1)), ("surface_seq", json!(seq + 1)), ("surface_seq", Value::Null),
            ("node_path", json!([])), ("node_path", json!([9])), ("node_path", json!([0.0])),
            ("event", json!("refresh")), ("args", json!({"field":"other","value":0.3})),
            ("args", json!({"field":"volume","value":0.3,"extra":0})),
        ] {
            let mut params = baseline.clone(); params[key] = value; cases.push(params);
        }
        for key in ["surface_contract", "surface_seq", "node_path", "args"] {
            let mut params = baseline.clone(); params.as_object_mut().unwrap().remove(key); cases.push(params);
        }
        cases.push(json!({"event":"refresh","args":{},"surface_contract":1,"surface_seq":seq,"node_path":[1]}));
        let config_file = std::fs::read(data.join("config.toml")).unwrap();
        let config = daemon.pipeline_snapshot().unwrap().config;
        let sources = daemon.matrix_sources.lock().unwrap().clone();
        let snapshots = daemon.matrix_snapshots.lock().unwrap().clone();
        let surfaces = daemon.matrix_surfaces.lock().unwrap().clone();
        for params in cases {
            assert!(daemon.call("ui.event", params.clone()).is_err(), "{params}");
            assert_eq!(daemon.log.max_seq().unwrap(), seq, "{params}");
            assert_eq!(daemon.log.get(seq).unwrap().unwrap().payload, payload);
            assert_eq!(std::fs::read(data.join("config.toml")).unwrap(), config_file);
            assert_eq!(daemon.pipeline_snapshot().unwrap().config, config);
            assert_eq!(*daemon.matrix_sources.lock().unwrap(), sources);
            assert_eq!(*daemon.matrix_snapshots.lock().unwrap(), snapshots);
            assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), surfaces);
            assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
            assert_eq!(daemon.store.lock().unwrap().count_packs().unwrap(), 0);
            assert!(server.recorded().is_empty());
        }
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn dual_append_failure_preserves_snapshot_and_both_tables_then_v0_replaces_them() {
        let (data, _) = dirs("dual-append-failure");
        let mut daemon = daemon(&data);
        let database = data.join("events.db");
        daemon.log = EventLog::open(&database).unwrap();
        let source = "screen { col { toggle \"Local\" models.local_only button \"Refresh\" -> refresh } }";
        daemon.render_matrix("main", source, true).unwrap();
        daemon.refresh_matrix_host_snapshot("main", matrix::HostSnapshot::ready(0.25, ["Prior"])).unwrap();
        let seq = daemon.log.max_seq().unwrap();
        let sources = daemon.matrix_sources.lock().unwrap().clone();
        let snapshots = daemon.matrix_snapshots.lock().unwrap().clone();
        let surfaces = daemon.matrix_surfaces.lock().unwrap().clone();
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection.execute_batch("CREATE TRIGGER reject_surface BEFORE INSERT ON events WHEN NEW.kind = 'ui.surface' BEGIN SELECT RAISE(FAIL, 'surface storage unavailable'); END;").unwrap();
        assert!(daemon.render_matrix("main", "screen { header { text \"Replacement\" } }", true).is_err());
        assert!(daemon.refresh_matrix_host_snapshot("main", matrix::HostSnapshot::ready(0.5, ["Replacement"])).is_err());
        assert_eq!(daemon.log.max_seq().unwrap(), seq);
        assert_eq!(*daemon.matrix_sources.lock().unwrap(), sources);
        assert_eq!(*daemon.matrix_snapshots.lock().unwrap(), snapshots);
        assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), surfaces);
        let (mut invalid, bindings) = daemon.compile_matrix_with_host(source, &matrix::HostSnapshot::default()).unwrap();
        invalid.fallback = json!({"type":"unknown"});
        assert!(daemon.commit_matrix(MatrixCommit {
            sources: &mut daemon.matrix_sources.lock().unwrap(),
            snapshots: &mut daemon.matrix_snapshots.lock().unwrap(),
            thread: "main", surface: &invalid, source: Some("Invalid replacement"),
            bindings: &bindings, host: matrix::HostSnapshot::ready(0.75, ["Invalid"]),
        }).is_err());
        assert_eq!(daemon.log.max_seq().unwrap(), seq);
        assert_eq!(*daemon.matrix_sources.lock().unwrap(), sources);
        assert_eq!(*daemon.matrix_snapshots.lock().unwrap(), snapshots);
        assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), surfaces);
        for (contract, path) in [(None, json!([1])), (Some(1), json!([0,1]))] {
            let mut params = json!({"surface_seq":seq,"node_path":path});
            if let Some(contract) = contract { params["surface_contract"] = json!(contract); }
            assert!(surfaces["main"].accepts(&params, "refresh", &json!({})));
        }
        connection.execute_batch("DROP TRIGGER reject_surface;").unwrap();
        daemon.render_matrix("main", MATRIX_DASHBOARD, true).unwrap();
        let seq = daemon.log.max_seq().unwrap();
        let latest = daemon.log.get(seq).unwrap().unwrap();
        assert!(latest.payload.get("matrix_surface").is_none());
        assert!(!daemon.matrix_surfaces.lock().unwrap()["main"].accepts(&json!({"surface_contract":1,"surface_seq":seq,"node_path":[0,1]}), "refresh", &json!({})));
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn versioned_host_controls_store_only_event_body_and_refresh_once() {
        let (data, _) = dirs("v1-host-events");
        let daemon = daemon(&data);
        let source = "screen { toggle \"Local\" models.local_only select \"Talk\" models.talk options model_options button \"Refresh\" -> refresh }";
        daemon.render_matrix("main", source, true).unwrap();
        for (index, event, args) in [
            (0, "change", json!({"field":"models.local_only","value":true})),
            (1, "change", json!({"field":"models.talk","value":"qwen3:0.6b"})),
            (2, "refresh", json!({})),
        ] {
            let before = daemon.log.max_seq().unwrap();
            assert!(daemon.call("ui.event", json!({"surface_contract":1,"surface_seq":before,"node_path":[index],"event":event,"args":args})).is_ok());
            let events = (before + 1..=daemon.log.max_seq().unwrap())
                .map(|seq| daemon.log.get(seq).unwrap().unwrap()).collect::<Vec<_>>();
            assert_eq!(events.iter().filter(|event| event.kind == "ui.surface").count(), 1);
            let stored = events.iter().filter(|event| event.kind == "ui.event").collect::<Vec<_>>();
            assert_eq!(stored.len(), 1);
            assert_eq!(stored[0].payload, json!({"event":event,"args":args}));
            assert!(events.last().unwrap().payload.get("matrix_surface").is_some());
        }
        assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 0);
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    fn daemon(data_dir: &Path) -> Daemon {
        let log = EventLog::open_in_memory().unwrap();
        let config = ConfigStore::load(
            &data_dir.join("config.toml"),
            log.clone(),
            Arc::new(AllowAll),
        )
        .unwrap();
        Daemon {
            browser: Mutex::new(None),
            host_id: mote_protocol::HostId::from_public_key(&[7;32]),
            gateway: Mutex::new(None),
            log,
            config: Mutex::new(config),
            harness: Mutex::new(hrns::daemon_runtime(mote_hrns::HarnessKind::Opencode).unwrap()),
            native: Mutex::new(test_native_runtime(data_dir)),
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
            inferred_retry_budget: Mutex::new(inferred_ui::RetryBudget::default()),
            inferred_surface_turns: Mutex::new(HashMap::new()),
            data_dir: data_dir.to_path_buf(),
            started: std::time::Instant::now(),
        }
    }

    #[test]
    fn daemon_exposes_and_switches_between_available_harnesses() {
        let (data, _) = dirs("harness-selection");
        let daemon = daemon(&data);

        let status = daemon.call("status", json!({})).unwrap();
        assert_eq!(
            status["harness"],
            json!({"configured":"opencode","active":"opencode","pending":null})
        );
        let pipeline = daemon.call("pipeline.snapshot", json!({})).unwrap();
        assert_eq!(pipeline["harness"], status["harness"]);
        {
            let _phase = daemon.harness_phase(hrns::RuntimePhase::ModelStep);
            assert_eq!(
                daemon.harness.lock().unwrap().selection().phase,
                hrns::RuntimePhase::ModelStep
            );
        }
        assert_eq!(
            daemon.harness.lock().unwrap().selection().phase,
            hrns::RuntimePhase::Checkpoint
        );

        let changed = daemon
            .call(
                "config.set",
                json!({"key":"agent.harness","value":"mote"}),
            )
            .unwrap();
        assert_eq!(
            daemon.call("status", json!({})).unwrap()["harness"],
            json!({"configured":"mote","active":"mote","pending":null})
        );
        assert_eq!(
            daemon.config.lock().unwrap().get_json("agent.harness"),
            Some(json!("mote"))
        );
        daemon
            .call("config.revert", json!({"seq":changed["seq"]}))
            .unwrap();
        assert_eq!(daemon.config.lock().unwrap().get_json("agent.harness"), None);
        assert_eq!(
            daemon.call("status", json!({})).unwrap()["harness"],
            json!({"configured":"opencode","active":"opencode","pending":null})
        );
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn matrix_can_bind_the_harness_selector_and_runtime_status() {
        let (data, _) = dirs("harness-matrix");
        let daemon = daemon(&data);
        let source = "screen { select \"Harness\" agent.harness options harness_options text harness.active text harness.pending }";
        let response = daemon.render_matrix("main", source, true).unwrap();
        assert_eq!(response["ok"], true, "{response}");
        let sequence = daemon.log.max_seq().unwrap();
        let event = daemon.log.get(sequence).unwrap().unwrap();
        assert!(event.payload.to_string().contains("opencode"));
        daemon
            .call(
                "ui.event",
                json!({
                    "thread":"main",
                    "surface_contract":1,
                    "surface_seq":sequence,
                    "node_path":[0],
                    "event":"change",
                    "args":{"field":"agent.harness","value":"mote"},
                }),
            )
            .unwrap();
        assert!(daemon.log.max_seq().unwrap() > sequence);
        assert_eq!(
            daemon.config.lock().unwrap().get_json("agent.harness"),
            Some(json!("mote"))
        );
        assert_eq!(
            daemon.call("status", json!({})).unwrap()["harness"],
            json!({"configured":"mote","active":"mote","pending":null})
        );
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn talk_benchmark_uses_live_subscribers_and_records_every_sample() {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        server.set_chat_chunks(vec![
            json!({"message":{"role":"assistant","content":"hello"},"done":false}).to_string(),
            json!({"message":{"role":"assistant","content":""},"done":true}).to_string(),
        ]);
        let (data, _) = dirs("talk-benchmark");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());
        daemon.log.append("main", "turn.user", &json!({"text":"An ordinary question"})).unwrap();
        daemon.log.append("main", "talk.delta", &json!({"text":"The completed ordinary answer"})).unwrap();
        daemon.log.append("main", "talk.done", &json!({})).unwrap();
        let before_main = daemon.log.recent_since(0, 1000).unwrap();
        let mut subscriber = daemon.log.subscribe("bench.talk", 0).unwrap();

        let result = daemon.call("bench.talk", json!({}));

        assert!(result.is_ok(), "benchmark RPC must use the live daemon: {result:?}");
        let result = result.unwrap();
        assert_eq!(result["runs"], 20);
        assert_eq!(result["extra"]["samples_ms"].as_array().unwrap().len(), 20);
        assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 20);
        let events = daemon.log.recent_since(0, 1000).unwrap();
        assert_eq!(events.iter().filter(|event| event.scope_id == "main").collect::<Vec<_>>(),
            before_main.iter().collect::<Vec<_>>());
        assert_eq!(events.iter().filter(|event| event.kind == "talk.done" && event.scope_id == "bench.talk").count(), 20);
        assert_eq!(events.iter().filter(|event| event.kind == "model.request").count(), 20);
        assert_eq!(subscriber.recv_timeout(Duration::from_millis(100)).unwrap().kind, "talk.delta");
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
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

    const INFERRED_DASHBOARD: &str = r#"screen {
  col {
    text "Agent dashboard"
    slider "Router budget" budgets.router min 50 max 1000 step 10
    toggle "Local only" models.local_only
    select "Talk model" models.talk options model_options
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

    fn decisions(
        values: impl IntoIterator<Item = Result<inferred_ui::SurfaceDecision, String>>,
    ) -> impl FnMut(&inferred_ui::SurfaceInputs<'_>) -> Result<inferred_ui::SurfaceDecision, String>
    {
        let mut values = values.into_iter();
        move |_| values.next().expect("one scripted decision per attempt")
    }

    #[test]
    fn completed_talk_answer_commits_one_inferred_surface_after_prose() {
        let (data, _) = dirs("inferred-talk-flow");
        let daemon = daemon(&data);
        let turn = daemon
            .log
            .append("main", "turn.user", &json!({"text":"show settings"}))
            .unwrap();
        daemon
            .log
            .append("main", "talk.delta", &json!({"text":"Here are the settings."}))
            .unwrap();
        daemon
            .log
            .append("main", "talk.done", &json!({"ledger_id":1,"pack_id":1}))
            .unwrap();

        let outcome = daemon
            .infer_surface_after_answer_with(
                "main",
                turn.seq,
                "show settings",
                AnswerCompletion::TalkDone("Here are the settings."),
                std::time::Instant::now(),
                decisions([Ok(inferred_ui::SurfaceDecision::Matrix {
                    source: MATRIX_DASHBOARD.into(),
                })]),
            )
            .unwrap();

        assert!(matches!(outcome, SurfaceInferenceOutcome::Committed));
        let mut events = daemon.log.recent_since(i64::MIN, 20).unwrap();
        events.reverse();
        let kinds: Vec<_> = events.iter().map(|event| event.kind.as_str()).collect();
        assert_eq!(kinds, ["turn.user", "talk.delta", "talk.done", "ui.surface"]);
        assert_eq!(
            events.iter().filter(|event| event.kind == "ui.surface").count(),
            1
        );
    }

    #[test]
    fn duplicate_completed_answer_cannot_commit_a_second_surface_for_the_turn() {
        let (data, _) = dirs("inferred-one-surface-per-turn");
        let daemon = daemon(&data);
        let mut outcomes = Vec::new();

        for label in ["First", "Second"] {
            outcomes.push(
                daemon
                    .infer_surface_after_answer_with(
                        "main",
                        23,
                        "show status",
                        AnswerCompletion::TalkDone("Status is ready."),
                        std::time::Instant::now(),
                        decisions([Ok(inferred_ui::SurfaceDecision::Matrix {
                            source: format!("screen {{ text \"{label}\" }}"),
                        })]),
                    )
                    .unwrap(),
            );
        }

        assert_eq!(
            outcomes,
            [
                SurfaceInferenceOutcome::Committed,
                SurfaceInferenceOutcome::AlreadyAttempted,
            ]
        );
        assert_eq!(
            daemon
                .log
                .recent_kind_since("ui.surface", i64::MIN, 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn completed_think_answer_commits_after_think_done() {
        let (data, _) = dirs("inferred-think-flow");
        let daemon = daemon(&data);
        let turn = daemon
            .log
            .append("main", "turn.user", &json!({"text":"inspect configuration"}))
            .unwrap();
        daemon
            .log
            .append("main", "think.text", &json!({"text":"Inspection complete."}))
            .unwrap();
        daemon
            .log
            .append("main", "think.done", &json!({"ledger_id":2,"pack_id":2}))
            .unwrap();

        let outcome = daemon
            .infer_surface_after_answer_with(
                "main",
                turn.seq,
                "inspect configuration",
                AnswerCompletion::ThinkDone("Inspection complete."),
                std::time::Instant::now(),
                decisions([Ok(inferred_ui::SurfaceDecision::Matrix {
                    source: MATRIX_DASHBOARD.into(),
                })]),
            )
            .unwrap();

        assert_eq!(outcome, SurfaceInferenceOutcome::Committed);
        let surface = daemon
            .log
            .recent_kind_since("ui.surface", i64::MIN, 10)
            .unwrap();
        assert_eq!(surface.len(), 1);
        let done = daemon
            .log
            .recent_kind_since("think.done", i64::MIN, 10)
            .unwrap();
        assert!(done[0].seq < surface[0].seq);
    }

    #[test]
    fn none_and_incomplete_answers_never_change_the_surface() {
        let (data, _) = dirs("inferred-ineligible");
        let daemon = daemon(&data);
        daemon.render_matrix("main", "screen { text \"Prior\" }", true).unwrap();
        let prior_seq = daemon.matrix_surfaces.lock().unwrap()["main"].seq;
        let mut calls = 0;

        let none = daemon
            .infer_surface_after_answer_with(
                "main",
                17,
                "ordinary chat",
                AnswerCompletion::TalkDone("Ordinary answer."),
                std::time::Instant::now(),
                |_| {
                    calls += 1;
                    Ok(inferred_ui::SurfaceDecision::None)
                },
            )
            .unwrap();
        assert_eq!(none, SurfaceInferenceOutcome::None);

        for completion in [
            AnswerCompletion::TalkUnavailable,
            AnswerCompletion::ThinkFailed,
            AnswerCompletion::HandoffUnfinished,
        ] {
            let outcome = daemon
                .infer_surface_after_answer_with(
                    "main",
                    18,
                    "unfinished",
                    completion,
                    std::time::Instant::now(),
                    |_| {
                        calls += 1;
                        Ok(inferred_ui::SurfaceDecision::None)
                    },
                )
                .unwrap();
            assert_eq!(outcome, SurfaceInferenceOutcome::Ineligible);
        }

        assert_eq!(calls, 1);
        assert_eq!(daemon.matrix_surfaces.lock().unwrap()["main"].seq, prior_seq);
        assert_eq!(
            daemon
                .log
                .recent_kind_since("ui.surface", i64::MIN, 10)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn inferred_turn_uses_exact_candidate_id_and_preserves_prose_and_prior_surface() {
        let (data, _) = dirs("inferred-turn-rejection");
        let daemon = daemon(&data);
        daemon.render_matrix("main", "screen { text \"Prior\" }", true).unwrap();
        let prior = daemon.matrix_surfaces.lock().unwrap().clone();
        let prior_source = daemon.matrix_sources.lock().unwrap().clone();
        let turn = daemon
            .log
            .append("main", "turn.user", &json!({"text":"show controls"}))
            .unwrap();
        let answer = daemon
            .log
            .append("main", "talk.done", &json!({"ledger_id":1,"pack_id":1}))
            .unwrap();

        let outcome = daemon
            .infer_surface_after_answer_with(
                "main",
                turn.seq,
                "show controls",
                AnswerCompletion::TalkDone("The prose remains visible."),
                std::time::Instant::now(),
                decisions([
                    Ok(inferred_ui::SurfaceDecision::Matrix {
                        source: "screen { text missing_binding }".into(),
                    }),
                    Ok(inferred_ui::SurfaceDecision::Matrix {
                        source: "screen { button \"Bad\" -> missing_action }".into(),
                    }),
                ]),
            )
            .unwrap();

        assert_eq!(outcome, SurfaceInferenceOutcome::Rejected);
        assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), prior);
        assert_eq!(*daemon.matrix_sources.lock().unwrap(), prior_source);
        let diagnostic = daemon
            .log
            .recent_kind_since("ui.diagnostic", i64::MIN, 10)
            .unwrap();
        assert_eq!(diagnostic.len(), 1);
        assert_eq!(
            diagnostic[0].payload["candidate"],
            format!("surface-{}", turn.seq)
        );
        assert!(answer.seq < diagnostic[0].seq);
    }

    #[test]
    fn production_turn_keeps_automatic_surface_inference_disabled_before_the_live_gate() {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        server.set_chat_chunks(vec![json!({
            "message":{"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"greeting\"}"},
            "done":true
        })
        .to_string()]);
        let (data, _) = dirs("inferred-production-gate");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());

        let response = daemon
            .handle_turn(&json!({
                "thread":"main",
                "message":"show a dashboard",
                "client_message_id":"gate-check"
            }))
            .unwrap();

        assert_eq!(response["status"], "done");
        assert_eq!(
            daemon
                .log
                .recent_kind_since("ui.surface", i64::MIN, 10)
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            daemon
                .store
                .lock()
                .unwrap()
                .ledger_since(0)
                .unwrap()
                .iter()
                .filter(|row| row.lane == "surface")
                .count(),
            0
        );
        assert_eq!(
            server
                .recorded()
                .iter()
                .filter(|request| request.path == "/api/chat")
                .count(),
            2
        );
    }

    fn activated_turn_server() -> mote_testkit::ScriptedServer {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        server.set_models(vec![("qwen3:4b".into(), 1)]);
        server.set_model_digest("qwen3:4b", "sha256:good");
        server.queue_chat_chunks(
            "qwen3:0.6b",
            vec![json!({
                "message":{"content":"{\"route\":\"talk\",\"reason\":\"simple\"}"},"done":true
            })
            .to_string()],
        );
        server.queue_chat_chunks(
            "qwen3:4b",
            vec![json!({
                "message":{"content":"Here is your dashboard."},"done":true
            })
            .to_string()],
        );
        server
    }

    fn activate_turns(data: &Path) {
        std::fs::write(
            data.join("surface-activation.json"),
            json!({
                "model":"qwen3:4b",
                "model_digest":"sha256:good",
                "corpus_hash":bench::current_surface_corpus_hash(),
                "compiler_contract_version":surface_activation::COMPILER_CONTRACT_VERSION,
                "first_try_valid":190,
                "sample_count":200
            })
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn inferred_production_turn_commits_one_surface_after_completed_prose() {
        let server = activated_turn_server();
        server.queue_chat_chunks("qwen3:4b", vec![json!({
            "message":{"content":json!({"kind":"matrix","source":"screen { text \"Ready\" }"}).to_string()},"done":true
        }).to_string()]);
        let (data, _) = dirs("inferred-activated-turn");
        activate_turns(&data);
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());

        let result = daemon
            .handle_turn(&json!({"thread":"main","message":"show a dashboard"}))
            .unwrap();

        assert_eq!(result["status"], "done");
        let mut events = daemon.log.recent_since(0, 100).unwrap();
        events.reverse();
        let done = events
            .iter()
            .position(|event| event.kind == "talk.done")
            .unwrap();
        let surface = events
            .iter()
            .position(|event| event.kind == "ui.surface")
            .unwrap();
        assert!(done < surface);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == "ui.surface")
                .count(),
            1
        );
        let rows = daemon.store.lock().unwrap().ledger_since(0).unwrap();
        assert_eq!(rows.iter().filter(|row| row.lane == "surface").count(), 1);
        assert_eq!(rows.len(), 3);
        assert_eq!(daemon.store.lock().unwrap().count_packs().unwrap(), 3);
        let request = server
            .recorded()
            .into_iter()
            .find(|request| request.body["format"].get("oneOf").is_some())
            .unwrap();
        assert!(request.body.to_string().contains("show a dashboard"));
        assert!(request.body.to_string().contains("Here is your dashboard."));
    }

    #[test]
    fn inferred_production_turn_keeps_prose_and_prior_surface_on_transport_failure() {
        let server = activated_turn_server();
        server.queue_chat_chunks("qwen3:4b", vec!["not json".into()]);
        let (data, _) = dirs("inferred-transport-failure");
        activate_turns(&data);
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());
        daemon
            .render_matrix("main", "screen { text \"Prior\" }", true)
            .unwrap();
        let prior = daemon.matrix_surfaces.lock().unwrap().clone();

        let result = daemon
            .handle_turn(&json!({"thread":"main","message":"show a dashboard"}))
            .unwrap();

        assert_eq!(result["status"], "done");
        assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), prior);
        assert_eq!(
            daemon
                .log
                .recent_kind_since("ui.surface", i64::MIN, 10)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            daemon
                .store
                .lock()
                .unwrap()
                .ledger_since(0)
                .unwrap()
                .iter()
                .filter(|row| row.lane == "surface")
                .count(),
            1
        );
        assert!(!daemon
            .log
            .recent_kind_since("talk.delta", i64::MIN, 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn inferred_production_turn_rejects_missing_malformed_and_stale_activation() {
        for case in [
            "missing",
            "malformed",
            "stale-model",
            "stale-digest",
            "stale-corpus",
            "stale-contract",
        ] {
            let server = activated_turn_server();
            server.queue_chat_chunks("qwen3:4b", vec![json!({
                "message":{"content":json!({"kind":"matrix","source":"screen { text \"Unexpected\" }"}).to_string()},
                "done":true
            }).to_string()]);
            let (data, _) = dirs(&format!("inferred-production-gate-{case}"));
            let attestation_path = data.join("surface-activation.json");
            if case != "missing" {
                activate_turns(&data);
                if case == "malformed" {
                    std::fs::write(&attestation_path, "not json").unwrap();
                } else {
                    let mut attestation: Value =
                        serde_json::from_slice(&std::fs::read(&attestation_path).unwrap()).unwrap();
                    match case {
                        "stale-model" => attestation["model"] = json!("qwen3:0.6b"),
                        "stale-digest" => attestation["model_digest"] = json!("sha256:old"),
                        "stale-corpus" => attestation["corpus_hash"] = json!("old-corpus"),
                        "stale-contract" => attestation["compiler_contract_version"] = json!(0),
                        _ => unreachable!(),
                    }
                    std::fs::write(&attestation_path, attestation.to_string()).unwrap();
                }
            }
            let mut daemon = daemon(&data);
            daemon.client = Ollama::new(server.base_url());
            daemon.render_matrix("main", "screen { text \"Prior\" }", true).unwrap();
            let prior = daemon.matrix_surfaces.lock().unwrap().clone();
            let before = daemon.log.max_seq().unwrap();

            let result = daemon.handle_turn(&json!({"thread":"main","message":"show a dashboard"})).unwrap();

            assert_eq!(result["status"], "done", "{case}");
            assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), prior, "{case}");
            let events = daemon.log.recent_since(0, 100).unwrap();
            assert!(events.iter().any(|event| event.seq > before && event.kind == "talk.delta"), "{case}");
            assert!(!events.iter().any(|event| event.seq > before && event.kind == "ui.surface"), "{case}");
            assert_eq!(server.recorded().iter().filter(|request| request.body["format"].get("oneOf").is_some()).count(), 0, "{case}");
            let store = daemon.store.lock().unwrap();
            assert_eq!(store.ledger_since(0).unwrap().iter().filter(|row| row.lane == "surface").count(), 0, "{case}");
            assert_eq!(store.count_packs().unwrap(), 2, "{case}");
        }
    }

    #[test]
    fn inferred_incomplete_turn_never_checks_activation_or_calls_surface_model() {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        server.set_chat_status(404);
        let (data, _) = dirs("inferred-incomplete-turn");
        activate_turns(&data);
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());

        let result = daemon.handle_turn(&json!({"thread":"main","message":"show a dashboard"})).unwrap();

        assert_ne!(result["status"], "done");
        assert!(!server.hit("/api/tags"));
        assert_eq!(daemon.store.lock().unwrap().ledger_since(0).unwrap().iter().filter(|row| row.lane == "surface").count(), 0);
    }

    #[test]
    fn inferred_production_turn_retries_once_and_preserves_prior_surface_on_rejection() {
        for (case, decisions, expected_new_surfaces, expected_attempts, expected_diagnostics) in [
            ("none", vec![json!({"kind":"none"}).to_string()], 0, 1, 0),
            ("recovered", vec![
                json!({"kind":"matrix","source":"screen { text missing_binding }"}).to_string(),
                json!({"kind":"matrix","source":"screen { text \"Recovered\" }"}).to_string(),
            ], 1, 2, 0),
            ("rejected", vec![
                json!({"kind":"matrix","source":"screen { text missing_binding }"}).to_string(),
                json!({"kind":"matrix","source":"screen { button \"Invented\" -> invented_action }"}).to_string(),
            ], 0, 2, 1),
        ] {
            let server = activated_turn_server();
            for decision in decisions {
                server.queue_chat_chunks("qwen3:4b", vec![json!({"message":{"content":decision},"done":true}).to_string()]);
            }
            let (data, _) = dirs(&format!("inferred-production-{case}"));
            activate_turns(&data);
            let mut daemon = daemon(&data);
            daemon.client = Ollama::new(server.base_url());
            daemon.render_matrix("main", "screen { text \"Prior\" }", true).unwrap();
            let prior = daemon.matrix_surfaces.lock().unwrap().clone();
            let before = daemon.log.max_seq().unwrap();

            let result = daemon.handle_turn(&json!({"thread":"main","message":"show a dashboard"})).unwrap();

            assert_eq!(result["status"], "done", "{case}");
            let new_events = daemon.log.recent_since(0, 100).unwrap().into_iter().filter(|event| event.seq > before).collect::<Vec<_>>();
            assert_eq!(new_events.iter().filter(|event| event.kind == "ui.surface").count(), expected_new_surfaces, "{case}");
            assert_eq!(new_events.iter().filter(|event| event.kind == "ui.diagnostic").count(), expected_diagnostics, "{case}");
            assert_eq!(daemon.store.lock().unwrap().ledger_since(0).unwrap().iter().filter(|row| row.lane == "surface").count(), expected_attempts, "{case}");
            assert_eq!(daemon.store.lock().unwrap().count_packs().unwrap(), 2 + expected_attempts as i64, "{case}");
            assert_eq!(server.recorded().iter().filter(|request| request.body["format"].get("oneOf").is_some()).count(), expected_attempts, "{case}");
            assert!(new_events.iter().any(|event| event.kind == "talk.delta"), "{case}");
            if expected_new_surfaces == 0 {
                assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), prior, "{case}");
            }
        }
    }

    #[test]
    fn completed_talk_flow_keeps_event_order_ledger_counts_and_redacted_surface_request() {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        server.set_chat_chunks(vec![json!({
            "message":{"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"greeting\"}"},
            "done":true
        })
        .to_string()]);
        let (data, _) = dirs("inferred-talk-accounting");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());
        let turn = daemon
            .log
            .append("main", "turn.user", &json!({"text":"show settings"}))
            .unwrap();
        let answer = {
            let store = daemon.store.lock().unwrap();
            let inputs = TalkInputs {
                charter: "be concise",
                profile: "",
                memories: "",
                working_set: "",
                turn: "show settings",
            };
            match run_turn(
                &daemon.log,
                &store,
                &daemon.client,
                "main",
                "talk-test",
                TALK_CTX,
                "router-test",
                ROUTER_CTX,
                &inputs,
                TALK_BUDGET,
            ) {
                TalkOutcome::Done { answer, .. } => answer,
                other => panic!("expected completed talk, got {other:?}"),
            }
        };

        let secret = "sk-abcdefghijklmnopqrstuvwxyz";
        server.set_chat_chunks(vec![json!({
            "message": {
                "role":"assistant",
                "content": json!({"kind":"matrix","source":INFERRED_DASHBOARD}).to_string()
            },
            "done":true
        })
        .to_string()]);
        let outcome = daemon
            .infer_surface_after_answer(
                "main",
                turn.seq,
                &format!("show settings using {secret}"),
                AnswerCompletion::TalkDone(&answer),
            )
            .unwrap();

        assert_eq!(outcome, SurfaceInferenceOutcome::Committed);
        let rows = daemon.store.lock().unwrap().ledger_since(0).unwrap();
        let lanes: Vec<_> = rows.iter().map(|row| row.lane.as_str()).collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(lanes.iter().filter(|lane| **lane == "talk").count(), 1);
        assert_eq!(lanes.iter().filter(|lane| **lane == "router").count(), 1);
        assert_eq!(lanes.iter().filter(|lane| **lane == "surface").count(), 1);

        let mut events = daemon.log.recent_since(i64::MIN, 100).unwrap();
        events.reverse();
        let talk_done = events.iter().position(|event| event.kind == "talk.done").unwrap();
        let surface_request = events
            .iter()
            .position(|event| {
                event.kind == "model.request" && event.payload["lane"] == "surface"
            })
            .unwrap();
        let surface = events.iter().position(|event| event.kind == "ui.surface").unwrap();
        assert!(talk_done < surface_request && surface_request < surface);
        let outbound = server
            .recorded()
            .into_iter()
            .find(|request| request.body["format"].get("oneOf").is_some())
            .unwrap();
        assert!(!outbound.body.to_string().contains(secret));
    }

    #[test]
    fn completed_think_flow_adds_exactly_one_think_and_surface_ledger_row() {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        server.set_chat_chunks(vec![json!({
            "message": {
                "role":"assistant",
                "content": json!({"kind":"matrix","source":INFERRED_DASHBOARD}).to_string()
            },
            "done":true
        })
        .to_string()]);
        let (data, _) = dirs("inferred-think-accounting");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());
        let turn = daemon
            .log
            .append("main", "turn.user", &json!({"text":"inspect settings"}))
            .unwrap();
        let (pack, system, request) = build_think_pack(
            &ThinkInputs {
                charter: "be concise",
                profile: "",
                memories: "",
                working_set: "",
                turn: "inspect settings",
            },
            THINK_BUDGET,
        );
        let answer = {
            let store = daemon.store.lock().unwrap();
            match run_think_model_call(
                &daemon.log,
                &store,
                "main",
                "think-test",
                &pack,
                &system,
                &request,
                || {
                    Ok((
                        vec![
                            mote_harness::ThinkEvent::Text {
                                text: "Inspection complete.".into(),
                            },
                            mote_harness::ThinkEvent::Done { ok: true },
                        ],
                        Duration::from_millis(3),
                    ))
                },
            ) {
                ThinkOutcome::Done { answer, .. } => answer,
                other => panic!("expected completed think, got {other:?}"),
            }
        };
        let done = daemon
            .log
            .recent_kind_since("think.done", i64::MIN, 1)
            .unwrap()[0]
            .seq;

        let outcome = daemon
            .infer_surface_after_answer(
                "main",
                turn.seq,
                "inspect settings",
                AnswerCompletion::ThinkDone(&answer),
            )
            .unwrap();

        assert_eq!(outcome, SurfaceInferenceOutcome::Committed);
        let rows = daemon.store.lock().unwrap().ledger_since(0).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.iter().filter(|row| row.lane == "think").count(), 1);
        assert_eq!(rows.iter().filter(|row| row.lane == "surface").count(), 1);
        let surface = daemon
            .log
            .recent_kind_since("ui.surface", i64::MIN, 1)
            .unwrap()[0]
            .seq;
        assert!(done < surface);
    }

    #[test]
    fn inferred_host_subset_accepts_slider_controls_and_refresh() {
        let (data, _) = dirs("inferred-host-subset");
        let daemon = daemon(&data);
        daemon.matrix_snapshots.lock().unwrap().insert(
            "main".into(),
            matrix::HostSnapshot::ready(0.5, ["Inspect", "Ship"]),
        );

        let outcome = daemon
            .infer_surface_after_answer_with(
                "main",
                7,
                "show a configuration dashboard",
                AnswerCompletion::TalkDone("Configuration is ready."),
                std::time::Instant::now(),
                decisions([Ok(inferred_ui::SurfaceDecision::Matrix {
                    source: INFERRED_DASHBOARD.into(),
                })]),
            )
            .unwrap();

        assert_eq!(outcome, SurfaceInferenceOutcome::Committed);
        let event = daemon
            .log
            .recent_kind_since("ui.surface", i64::MIN, 1)
            .unwrap()
            .pop()
            .unwrap();
        let rendered = event.payload["matrix_surface"].to_string();
        for expected in ["slider", "toggle", "select", "button"] {
            assert!(rendered.contains(expected), "missing {expected}: {rendered}");
        }
    }

    #[test]
    fn inferred_router_budget_slider_persists_integer_refreshes_and_rejects_stale_revision() {
        let (data, _) = dirs("inferred-router-budget-slider");
        let daemon = daemon(&data);
        let outcome = daemon
            .infer_surface_after_answer_with(
                "main",
                7,
                "show a configuration dashboard with a router budget slider",
                AnswerCompletion::TalkDone("Configuration is ready."),
                std::time::Instant::now(),
                decisions([Ok(inferred_ui::SurfaceDecision::Matrix {
                    source: INFERRED_DASHBOARD.into(),
                })]),
            )
            .unwrap();
        assert_eq!(outcome, SurfaceInferenceOutcome::Committed);
        let first_seq = daemon.log.max_seq().unwrap();
        let changed = daemon
            .call(
                "ui.event",
                json!({
                    "thread":"main",
                    "surface_contract":1,
                    "surface_seq":first_seq,
                    "node_path":[0,1],
                    "event":"change",
                    "args":{"field":"budgets.router","value":160.0}
                }),
            )
            .unwrap();
        assert_eq!(changed["rendered"]["ok"], true);
        let refreshed = daemon
            .log
            .recent_kind_since("ui.surface", first_seq, 1)
            .unwrap()
            .pop()
            .unwrap();
        let refreshed_seq = refreshed.seq;
        assert!(refreshed_seq > first_seq);
        assert_eq!(daemon.config.lock().unwrap().get_json("budgets.router"), Some(json!(160)));
        assert!(std::fs::read_to_string(data.join("config.toml"))
            .unwrap()
            .contains("router = 160"));
        assert_eq!(
            refreshed.payload["matrix_surface"]["root"]["children"][0]["children"][1]["value"],
            160.0
        );
        let event_count = daemon.log.max_seq().unwrap();
        let stale = daemon.call(
            "ui.event",
            json!({
                "thread":"main",
                "surface_contract":1,
                "surface_seq":first_seq,
                "node_path":[0,1],
                "event":"change",
                "args":{"field":"budgets.router","value":170.0}
            }),
        );
        assert_eq!(stale.unwrap_err().code, codes::INVALID_PARAMS);
        assert_eq!(daemon.config.lock().unwrap().get_json("budgets.router"), Some(json!(160)));
        assert_eq!(daemon.log.max_seq().unwrap(), event_count);
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn altered_router_budget_slider_contracts_preserve_prior_surface_and_config() {
        let (data, _) = dirs("router-budget-contract-drift");
        let daemon = daemon(&data);
        daemon
            .render_matrix("main", "screen { text \"Prior\" }", true)
            .unwrap();
        let prior_seq = daemon.log.max_seq().unwrap();
        let prior_surface = daemon.matrix_surfaces.lock().unwrap().clone();
        let prior_source = daemon.matrix_sources.lock().unwrap().clone();
        let prior_config = std::fs::read(data.join("config.toml")).unwrap();
        for source in [
            "screen { slider \"Router budget\" budgets.router min 40 max 1000 step 10 }",
            "screen { slider \"Router budget\" budgets.router min 50 max 1010 step 10 }",
            "screen { slider \"Router budget\" budgets.router min 50 max 1000 step 5 }",
        ] {
            let response = daemon.render_matrix("main", source, true).unwrap();
            assert_eq!(response["ok"], false, "{source}");
            assert_eq!(response["diagnostics"][0]["code"], "E_RESOLVE_CONSTRAINT");
            assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), prior_surface);
            assert_eq!(*daemon.matrix_sources.lock().unwrap(), prior_source);
            assert_eq!(std::fs::read(data.join("config.toml")).unwrap(), prior_config);
            assert_eq!(
                daemon
                    .log
                    .recent_kind_since("config.changed", i64::MIN, usize::MAX)
                    .unwrap()
                    .len(),
                0
            );
        }
        assert_eq!(
            daemon
                .log
                .recent_kind_since("ui.surface", i64::MIN, 1)
                .unwrap()[0]
                .seq,
            prior_seq
        );
        std::fs::remove_dir_all(data.parent().unwrap()).ok();
    }

    #[test]
    fn unsupported_inferred_semantics_reject_before_commitment() {
        let cases = [
            ("unknown-action", "screen { button \"Open\" -> launch }", None),
            (
                "action-argument",
                "screen { button \"Refresh\" -> refresh(reason: \"now\") }",
                None,
            ),
            (
                "keyed-list",
                "screen { list from host.items key label { item { text .label } } }",
                None,
            ),
            (
                "selected-list",
                "screen { list from host.items select one { item { text .label } } }",
                None,
            ),
            ("pending", INFERRED_DASHBOARD, Some(matrix::HostSnapshot::pending())),
            ("failed", INFERRED_DASHBOARD, Some(matrix::HostSnapshot::failed())),
        ];
        for (name, source, host) in cases {
            let (data, _) = dirs(name);
            let daemon = daemon(&data);
            daemon.render_matrix("main", "screen { text \"Prior\" }", true).unwrap();
            if let Some(host) = host {
                daemon.matrix_snapshots.lock().unwrap().insert("main".into(), host);
            }
            let prior = daemon.matrix_surfaces.lock().unwrap().clone();
            let outcome = daemon
                .infer_surface_after_answer_with(
                    "main",
                    9,
                    "show dashboard",
                    AnswerCompletion::TalkDone("Answer remains."),
                    std::time::Instant::now(),
                    decisions([
                        Ok(inferred_ui::SurfaceDecision::Matrix { source: source.into() }),
                        Ok(inferred_ui::SurfaceDecision::Matrix { source: source.into() }),
                    ]),
                )
                .unwrap();
            assert_eq!(outcome, SurfaceInferenceOutcome::Rejected, "{name}");
            assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), prior, "{name}");
        }
    }

    #[test]
    fn inferred_finalization_releases_the_context_store_lock() {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        server.set_chat_chunks(vec![json!({
            "message": {
                "role":"assistant",
                "content": json!({"kind":"matrix","source":"screen { text \"Ready\" }"}).to_string()
            },
            "done":true
        })
        .to_string()]);
        let (data, _) = dirs("inferred-store-lock");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());
        let daemon = Arc::new(daemon);
        let (sent, received) = std::sync::mpsc::channel();
        let worker = daemon.clone();
        std::thread::spawn(move || {
            let result = worker.infer_surface_after_answer(
                "main",
                11,
                "show status",
                AnswerCompletion::TalkDone("Status is ready."),
            );
            let _ = sent.send(result);
        });

        let outcome = received
            .recv_timeout(Duration::from_secs(2))
            .expect("candidate finalization deadlocked on the context store")
            .unwrap();
        assert_eq!(outcome, SurfaceInferenceOutcome::Committed);
    }

    #[test]
    fn inferred_surface_retries_one_rejection_and_commits_only_the_valid_candidate() {
        let (data, _) = dirs("inferred-valid-retry");
        let daemon = daemon(&data);
        daemon.render_matrix("main", "screen { text \"Prior\" }", true).unwrap();
        let before = daemon.log.max_seq().unwrap();
        let prior = daemon.matrix_sources.lock().unwrap()["main"].clone();
        let mut seen = Vec::new();
        let mut scripted = decisions([
            Ok(inferred_ui::SurfaceDecision::Matrix {
                source: "screen { text secret-literal }".into(),
            }),
            Ok(inferred_ui::SurfaceDecision::Matrix {
                source: "screen { text \"Ready\" }".into(),
            }),
        ]);

        let result = daemon
            .infer_surface_with(
                "main",
                "candidate-a",
                "show a dashboard",
                "The prose stays visible.",
                std::time::Instant::now(),
                |inputs| {
                    seen.push((inputs.previous_source.to_string(), inputs.diagnostics.to_string()));
                    scripted(inputs)
                },
            )
            .unwrap();

        assert_eq!(result["status"], "committed");
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[1].0, prior);
        assert!(!seen[1].1.contains("secret-literal"));
        assert!(seen[1].1.contains("E_"));
        assert_eq!(
            daemon
                .log
                .recent_kind_since("ui.surface", i64::MIN, 10)
                .unwrap()
                .into_iter()
                .filter(|event| event.seq > before)
                .count(),
            1
        );
        assert!(daemon
            .log
            .recent_kind_since("ui.diagnostic", i64::MIN, 10)
            .unwrap()
            .into_iter()
            .all(|event| event.seq <= before));
        assert_eq!(daemon.matrix_sources.lock().unwrap()["main"], "screen {\n  text \"Ready\"\n}\n");
    }

    #[test]
    fn inferred_surface_second_rejection_emits_one_redacted_diagnostic_and_preserves_state() {
        let (data, _) = dirs("inferred-double-rejection");
        let daemon = daemon(&data);
        daemon.render_matrix("main", "screen { text \"Prior\" }", true).unwrap();
        let before = daemon.log.max_seq().unwrap();
        let prior_source = daemon.matrix_sources.lock().unwrap().clone();
        let prior_surface = daemon.matrix_surfaces.lock().unwrap().clone();
        let secret = "sk-abcdefghijklmnopqrstuvwxyz";

        let result = daemon
            .infer_surface_with(
                "main",
                secret,
                "show controls",
                "The prose stays visible.",
                std::time::Instant::now(),
                decisions([
                    Ok(inferred_ui::SurfaceDecision::Matrix {
                        source: format!("screen {{ text {secret} }}"),
                    }),
                    Ok(inferred_ui::SurfaceDecision::Matrix {
                        source: format!("screen {{ button {secret} }}"),
                    }),
                ]),
            )
            .unwrap();

        assert_eq!(result["status"], "rejected");
        assert_eq!(*daemon.matrix_sources.lock().unwrap(), prior_source);
        assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), prior_surface);
        assert!(daemon
            .log
            .recent_kind_since("ui.surface", i64::MIN, 10)
            .unwrap()
            .into_iter()
            .all(|event| event.seq <= before));
        let diagnostics: Vec<_> = daemon
            .log
            .recent_kind_since("ui.diagnostic", i64::MIN, 10)
            .unwrap()
            .into_iter()
            .filter(|event| event.seq > before)
            .collect();
        assert_eq!(diagnostics.len(), 1);
        assert!(!diagnostics[0].payload.to_string().contains(secret));
        assert_eq!(diagnostics[0].payload["candidate"], "[redacted]");
    }

    #[test]
    fn inferred_retry_budget_is_three_per_minute_and_resets_for_a_new_surface() {
        let (data, _) = dirs("inferred-retry-budget");
        let daemon = daemon(&data);
        daemon.render_matrix("main", "screen { text \"Prior\" }", true).unwrap();
        let now = std::time::Instant::now();
        let mut attempts = 0;

        for turn in 0..4 {
            let result = daemon
                .infer_surface_with(
                    "main",
                    &format!("candidate-{turn}"),
                    "show controls",
                    "answer",
                    now + Duration::from_secs(turn),
                    |_| {
                        attempts += 1;
                        Ok(inferred_ui::SurfaceDecision::Matrix {
                            source: "screen { text missing_binding }".into(),
                        })
                    },
                )
                .unwrap();
            assert_eq!(result["status"], "rejected");
        }
        assert_eq!(attempts, 7, "four first attempts plus only three retries");

        daemon.render_matrix("main", "screen { text \"New revision\" }", true).unwrap();
        daemon
            .infer_surface_with(
                "main",
                "candidate-fresh",
                "show controls",
                "answer",
                now + Duration::from_secs(5),
                |_| {
                    attempts += 1;
                    Ok(inferred_ui::SurfaceDecision::Matrix {
                        source: "screen { text missing_binding }".into(),
                    })
                },
            )
            .unwrap();
        assert_eq!(attempts, 9, "the new committed sequence owns a fresh budget");
    }

    #[test]
    fn inferred_none_malformed_and_transport_failures_are_candidate_neutral() {
        let (data, _) = dirs("inferred-neutral");
        let daemon = daemon(&data);
        daemon.render_matrix("main", "screen { text \"Prior\" }", true).unwrap();
        let before = daemon.log.max_seq().unwrap();
        let prior_source = daemon.matrix_sources.lock().unwrap().clone();
        let prior_surface = daemon.matrix_surfaces.lock().unwrap().clone();

        for (name, decision) in [
            ("none", Ok(inferred_ui::SurfaceDecision::None)),
            ("malformed", Err("malformed structured reply".into())),
            ("transport", Err("transport unavailable".into())),
        ] {
            let mut calls = 0;
            let result = daemon
                .infer_surface_with(
                    "main",
                    name,
                    "ordinary request",
                    "prose remains",
                    std::time::Instant::now(),
                    |_| {
                        calls += 1;
                        decision.clone()
                    },
                )
                .unwrap();
            assert_eq!(calls, 1, "{name} retried");
            assert_eq!(result["status"], "none");
        }

        assert_eq!(*daemon.matrix_sources.lock().unwrap(), prior_source);
        assert_eq!(*daemon.matrix_surfaces.lock().unwrap(), prior_surface);
        assert!(daemon
            .log
            .recent_kind_since("ui.surface", i64::MIN, 10)
            .unwrap()
            .into_iter()
            .all(|event| event.seq <= before));
        assert!(daemon
            .log
            .recent_kind_since("ui.diagnostic", i64::MIN, 10)
            .unwrap()
            .into_iter()
            .all(|event| event.seq <= before));
    }

    #[test]
    fn inferred_attempts_each_own_accounting_and_never_store_rejected_source_secrets() {
        let server = mote_testkit::ScriptedServer::start().unwrap();
        let secret = "sk-abcdefghijklmnopqrstuvwxyz";
        server.set_chat_chunks(vec![json!({
            "message": {
                "role": "assistant",
                "content": format!(r#"{{"kind":"matrix","source":"screen {{ text {secret} }}"}}"#)
            },
            "done": true
        })
        .to_string()]);
        let (data, _) = dirs("inferred-accounting");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());
        daemon.render_matrix("main", "screen { text \"Prior\" }", true).unwrap();

        let result = daemon
            .infer_surface_candidate(
                "main",
                "candidate-secret",
                "show a dashboard",
                "The prose stays visible.",
            )
            .unwrap();

        assert_eq!(result["status"], "rejected");
        assert_eq!(daemon.store.lock().unwrap().count_packs().unwrap(), 2);
        assert_eq!(daemon.store.lock().unwrap().count_ledger().unwrap(), 2);
        assert_eq!(daemon.log.recent_kind_since("model.request", i64::MIN, 10).unwrap().len(), 2);
        assert_eq!(daemon.log.recent_kind_since("model.response", i64::MIN, 10).unwrap().len(), 2);
        let requests: Vec<_> = server
            .recorded()
            .into_iter()
            .filter(|request| request.path == "/api/chat")
            .collect();
        assert_eq!(requests.len(), 2);
        assert!(!requests[1].body.to_string().contains(secret));
        assert!(requests[1].body.to_string().contains("diagnostics"));
        let stored = daemon
            .log
            .recent_kind_since("model.request", i64::MIN, 10)
            .unwrap()
            .into_iter()
            .chain(daemon.log.recent_kind_since("model.response", i64::MIN, 10).unwrap())
            .chain(daemon.log.recent_kind_since("ui.diagnostic", i64::MIN, 10).unwrap())
            .map(|event| event.payload.to_string())
            .collect::<String>();
        assert!(!stored.contains(secret));
    }

    #[test]
    fn inferred_retry_budgets_keep_threads_isolated_at_the_same_surface_sequence() {
        let (data, _) = dirs("inferred-thread-isolation");
        let daemon = daemon(&data);
        daemon.render_matrix("left", "screen { text \"Left\" }", true).unwrap();
        daemon.render_matrix("right", "screen { text \"Right\" }", true).unwrap();
        let now = std::time::Instant::now();

        for thread in ["left", "right"] {
            let mut calls = 0;
            daemon
                .infer_surface_with(
                    thread,
                    "shared-candidate",
                    "show controls",
                    "answer",
                    now,
                    |_| {
                        calls += 1;
                        Ok(inferred_ui::SurfaceDecision::Matrix {
                            source: "screen { text missing_binding }".into(),
                        })
                    },
                )
                .unwrap();
            assert_eq!(calls, 2, "{thread} did not receive its own retry");
        }
        assert_eq!(daemon.matrix_sources.lock().unwrap()["left"], "screen { text \"Left\" }");
        assert_eq!(daemon.matrix_sources.lock().unwrap()["right"], "screen { text \"Right\" }");
    }

    #[test]
    fn validated_host_snapshot_refreshes_progress_and_list_once_without_model_traffic() {
        use mote_client::ObservationHarness;
        let server = mote_testkit::ScriptedServer::start().unwrap();
        let (data, _) = dirs("matrix-host-snapshot");
        let mut daemon = daemon(&data);
        daemon.client = Ollama::new(server.base_url());
        daemon.matrix_snapshots.lock().unwrap().insert(
            "main".into(),
            matrix::HostSnapshot::ready(0.25, ["Inspect"]),
        );

        daemon
            .render_matrix("main", HOST_SNAPSHOT_DASHBOARD, true)
            .unwrap();
        let initial = daemon.log.get(daemon.log.max_seq().unwrap()).unwrap().unwrap();
        let mut client = ObservationHarness::legacy();
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
                let mut harness = ObservationHarness::legacy();
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
                let mut probe = ObservationHarness::legacy();
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
                json!({"surface_id":"matrix", "surface":{"type":"stack","children":[{"type":"text","value":"Prior"},{"type":"action","label":"Refresh","event":"refresh"}]},"matrix_surface":{"version":1,"root":{"type":"screen","children":[{"type":"text","value":"Prior"},{"type":"button","label":"Refresh","event":"refresh"}]}}})
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
                json!({"surface_id":"matrix", "surface":{"type":"text", "value":text},"matrix_surface":{"version":1,"root":{"type":"screen","children":[{"type":"text","value":text}]}}})
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
        let mut harness = ObservationHarness::legacy();
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
        let mut field = ObservationHarness::legacy();
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
            let mut harness = ObservationHarness::legacy();
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
                assert_eq!(event.payload, json!({"surface_id":"matrix", "surface":{"type":"text","value":literal},"matrix_surface":{"version":1,"root":{"type":"screen","children":[{"type":"text","value":literal}]}}}));
                let mut harness = ObservationHarness::legacy();
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
                json!({"surface_id":"matrix","surface":{"type":"stack","children":[{"type":"text","value":"Retained source"},{"type":"action","label":"Refresh","event":"refresh"}]},"matrix_surface":{"version":1,"root":{"type":"screen","children":[{"type":"text","value":"Retained source"},{"type":"button","label":"Refresh","event":"refresh"}]}}})
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
            browser: Mutex::new(None),
            host_id: mote_protocol::HostId::from_public_key(&[7;32]),
            gateway: Mutex::new(None),
            log: log.clone(),
            config: Mutex::new(config),
            harness: Mutex::new(hrns::daemon_runtime(mote_hrns::HarnessKind::Opencode).unwrap()),
            native: Mutex::new(test_native_runtime(&data_dir)),
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
            inferred_retry_budget: Mutex::new(inferred_ui::RetryBudget::default()),
            inferred_surface_turns: Mutex::new(HashMap::new()),
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
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = match OwnerCommand::parse(&args) {
        OwnerCommand::Mcp => run_mcp(),
        OwnerCommand::Bench(name) => run_bench(name),
        OwnerCommand::Status => run_status(),
        OwnerCommand::Pair => run_gateway_command("gateway.pair", json!({})),
        OwnerCommand::Pending => run_gateway_command("gateway.pending", json!({})),
        OwnerCommand::Confirm { client_id, approved } => run_gateway_command(
            "gateway.confirm",
            json!({"client_id":client_id,"approved":approved}),
        ),
        OwnerCommand::Install => service::install().map_err(|e| {
            eprintln!("moted install: {e}");
            1
        }),
        OwnerCommand::Uninstall => service::uninstall().map_err(|e| {
            eprintln!("moted uninstall: {e}");
            1
        }),
        OwnerCommand::Host(action) => service::host(action)
            .map(|message| println!("{message}"))
            .map_err(|error| {
                eprintln!("moted host {}: {error}", action.name());
                1
            }),
        OwnerCommand::Run => run(),
        OwnerCommand::Invalid(message) => {
            eprintln!("moted: {message}");
            Err(2)
        }
    };
    if let Err(code) = result {
        std::process::exit(code);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum OwnerCommand {
    Run,
    Mcp,
    Bench(Option<String>),
    Status,
    Pair,
    Pending,
    Confirm { client_id: String, approved: bool },
    Install,
    Uninstall,
    Host(service::HostAction),
    Invalid(String),
}

impl OwnerCommand {
    fn parse(args: &[String]) -> Self {
        match args {
            [] => Self::Run,
            [command] if command == "mcp" => Self::Mcp,
            [command] if command == "bench" => Self::Bench(None),
            [command, name] if command == "bench" => Self::Bench(Some(name.clone())),
            [command] if command == "status" => Self::Status,
            [command] if command == "pair" => Self::Pair,
            [command] if command == "pending" => Self::Pending,
            [command, client_id] if command == "confirm" => Self::Confirm {
                client_id: client_id.clone(),
                approved: true,
            },
            [command, client_id] if command == "deny" => Self::Confirm {
                client_id: client_id.clone(),
                approved: false,
            },
            [command] if command == "install" => Self::Install,
            [command] if command == "uninstall" => Self::Uninstall,
            [command, action] if command == "host" => match action.as_str() {
                "start" => Self::Host(service::HostAction::Start),
                "status" => Self::Host(service::HostAction::Status),
                "restart" => Self::Host(service::HostAction::Restart),
                "stop" => Self::Host(service::HostAction::Stop),
                _ => Self::Invalid(
                    "usage: moted host [start|status|restart|stop]".into(),
                ),
            },
            _ => Self::Invalid(
                "usage: moted [status|pair|pending|confirm CLIENT_ID|deny CLIENT_ID|host ACTION]"
                    .into(),
            ),
        }
    }
}

/// Run a named benchmark (`moted bench talk|router|memory|surface|compiler`).
fn run_bench(name: Option<String>) -> Result<(), i32> {
    let name = name.ok_or_else(|| {
        eprintln!("moted bench: name required (talk|router|memory|surface|compiler)");
        1
    })?;
    let data = mote_paths::ensure_data_dir().map_err(|e| {
        eprintln!("moted bench: {e}");
        1
    })?;
    if name == "compiler" {
        let report = bench::run_compiler(&data).map_err(|error| {
            eprintln!("moted bench compiler: {error}");
            1
        })?;
        println!("{}", report.line());
        return if report.target_failed() { Err(1) } else { Ok(()) };
    }
    if name == "talk" && mote_paths::socket_path().exists() {
        let result = (|| -> Result<bench::BenchResult, String> {
            let mut client = mote_ipc::Client::connect(&mote_paths::socket_path())
                .map_err(|error| format!("running daemon required: {error}"))?;
            let value = client.call("bench.talk", json!({}))
                .map_err(|error| error.to_string())?
                .map_err(|error| error.message)?;
            let result: bench::BenchResult = serde_json::from_value(value)
                .map_err(|error| error.to_string())?;
            bench::append_bench_line(&data.join("bench.jsonl"), &result)
                .map_err(|error| error.to_string())?;
            Ok(result)
        })().map_err(|error| {
            eprintln!("moted bench talk: {error}");
            1
        })?;
        println!("bench {}: p50 {:.1} ms, p95 {:.1} ms over {} runs",
            result.name, result.p50_ms, result.p95_ms, result.runs);
        return Ok(());
    }
    let gate = Arc::new(OllamaGate::new(Ollama::local()));
    let log = EventLog::open(&mote_paths::db_path()).map_err(|_| 1)?;
    let config = ConfigStore::load(&mote_paths::config_path(), log.clone(), gate).map_err(|e| {
        eprintln!("moted bench: {e}");
        1
    })?;
    let store = ContextStore::open(&mote_paths::db_path()).map_err(|error| {
        eprintln!("moted bench: {error}");
        1
    })?;
    if name == "talk" {
        let model = config.get_json("models.talk")
            .and_then(|value| value.as_str().map(String::from)).unwrap_or_default();
        let result = bench::bench_talk(&log, &store, &Ollama::local(), &model, TALK_CTX, bench::RUNS)
            .and_then(|result| {
                bench::append_bench_line(&data.join("bench.jsonl"), &result)
                    .map_err(|error| error.to_string())?;
                Ok(result)
            }).map_err(|error| {
                eprintln!("moted bench talk: {error}");
                1
            })?;
        println!("bench {}: p50 {:.1} ms, p95 {:.1} ms over {} runs",
            result.name, result.p50_ms, result.p95_ms, result.runs);
        return Ok(());
    }
    bench::run_named(&name, &data, &Ollama::local(), &config, &log, &store);
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
            let configured = v["harness"]["configured"].as_str().unwrap_or("unknown");
            let active = v["harness"]["active"].as_str().unwrap_or("unknown");
            let pending = v["harness"]["pending"].as_str();
            println!(
                "harness:       configured {configured}, active {active}{}",
                pending.map(|value| format!(", pending {value}")).unwrap_or_default()
            );
            println!("last event:    seq {}", v["last_seq"].as_i64().unwrap_or(0));
            Ok(())
        }
        Err(e) => {
            eprintln!("moted status: {}: {}", e.code, e.message);
            Err(1)
        }
    }
}

fn run_gateway_command(method: &str, params: Value) -> Result<(), i32> {
    let socket = mote_paths::socket_path();
    let mut client = mote_ipc::Client::connect(&socket).map_err(|error| {
        eprintln!("moted: cannot connect to {}: {error}", socket.display());
        1
    })?;
    let value = client
        .call(method, params)
        .map_err(|error| {
            eprintln!("moted: {error}");
            1
        })?
        .map_err(|error| {
            eprintln!("moted: {}: {}", error.code, error.message);
            1
        })?;
    match method {
        "gateway.pair" => {
            println!("{}", value["code"].as_str().unwrap_or(""));
            eprintln!("Enter this code in Mote on the iPad.");
        }
        "gateway.pending" => {
            let devices = value["devices"].as_array().cloned().unwrap_or_default();
            if devices.is_empty() {
                println!("No iPad is waiting for approval.");
            } else {
                for device in devices {
                    println!(
                        "{}\t{}",
                        device["client_id"].as_str().unwrap_or(""),
                        device["name"].as_str().unwrap_or("Unnamed device")
                    );
                }
            }
        }
        "gateway.confirm" => println!("Pairing decision recorded."),
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod owner_command_tests {
    use super::{service::HostAction, OwnerCommand};

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn parses_owner_pairing_commands_without_exposing_remote_control() {
        assert_eq!(OwnerCommand::parse(&args(&["pair"])), OwnerCommand::Pair);
        assert_eq!(OwnerCommand::parse(&args(&["pending"])), OwnerCommand::Pending);
        assert_eq!(
            OwnerCommand::parse(&args(&["confirm", "ipad-1"])),
            OwnerCommand::Confirm { client_id: "ipad-1".into(), approved: true }
        );
        assert_eq!(
            OwnerCommand::parse(&args(&["deny", "ipad-1"])),
            OwnerCommand::Confirm { client_id: "ipad-1".into(), approved: false }
        );
        assert!(matches!(OwnerCommand::parse(&args(&["confirm"])), OwnerCommand::Invalid(_)));
    }

    #[test]
    fn parses_mote_host_lifecycle_commands() {
        assert_eq!(
            OwnerCommand::parse(&args(&["host", "start"])),
            OwnerCommand::Host(HostAction::Start)
        );
        assert_eq!(
            OwnerCommand::parse(&args(&["host", "status"])),
            OwnerCommand::Host(HostAction::Status)
        );
        assert_eq!(
            OwnerCommand::parse(&args(&["host", "restart"])),
            OwnerCommand::Host(HostAction::Restart)
        );
        assert_eq!(
            OwnerCommand::parse(&args(&["host", "stop"])),
            OwnerCommand::Host(HostAction::Stop)
        );
        assert!(matches!(
            OwnerCommand::parse(&args(&["host", "remove"])),
            OwnerCommand::Invalid(_)
        ));
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
    let server = mcp::McpServer::new(Arc::new(Mutex::new(store)), file_root()).with_browser(
        mote_paths::socket_path(),
        std::env::var("MOTE_THREAD_ID").unwrap_or_else(|_| "main".into()),
    );
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
    let shutdown = shutdown::Shutdown::install().map_err(|error| {
        eprintln!("moted: cannot install shutdown handlers: {error}");
        1
    })?;
    let data = mote_paths::ensure_data_dir().map_err(|e| {
        eprintln!("moted: cannot create data directory: {e}");
        1
    })?;

    let identity = gateway::prepare_identity(&data).map_err(|e| {
        eprintln!("moted: cannot prepare host identity: {e}");
        1
    })?;

    let log = EventLog::open(&mote_paths::db_path()).map_err(|e| {
        eprintln!("moted: cannot open event log: {e}");
        1
    })?;

    let model_client = Ollama::local();
    let gate: Arc<dyn config::ModelGate> = Arc::new(OllamaGate::new(model_client.clone()));
    let config = ConfigStore::load(&mote_paths::config_path(), log.clone(), gate.clone()).map_err(|e| {
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
    watchers.bind_local_host(identity.host_id()).map_err(|e| {
        eprintln!("moted: cannot bind approvals to local host: {e}");
        1
    })?;

    let configured_harness_value = config.get_json("agent.harness");
    let configured_harness = hrns::configured_harness(configured_harness_value.as_ref()).map_err(
        |error| {
            let reason = error.to_string();
            let _ = log.append("daemon", "startup.refused", &json!({ "reason": reason }));
            eprintln!("moted: {error}");
            1
        },
    )?;
    let harness = hrns::daemon_runtime(configured_harness).map_err(|error| {
        let reason = error.to_string();
        let _ = log.append("daemon", "startup.refused", &json!({ "reason": reason }));
        eprintln!("moted: {error}");
        1
    })?;
    let native = hrns_native::NativeTaskRuntime::production(
        &mote_paths::db_path(),
        log.clone(),
        model_client.clone(),
        gate,
    )
    .map_err(|error| {
        let reason = format!("cannot initialize native harness: {error}");
        let _ = log.append("daemon", "startup.refused", &json!({ "reason": reason }));
        eprintln!("moted: {reason}");
        1
    })?;

    let socket = mote_paths::socket_path();
    let daemon = Arc::new(Daemon {
        browser: Mutex::new(None),
        host_id: identity.host_id().clone(),
        gateway: Mutex::new(None),
        log,
        config: Mutex::new(config),
        harness: Mutex::new(harness),
        native: Mutex::new(native),
        character,
        store: Mutex::new(store),
        client: model_client,
        watchers: Mutex::new(watchers),
        think: Mutex::new(ThinkServers::new(data.clone())),
        matrix_sources: Mutex::new(HashMap::new()),
        matrix_snapshots: Mutex::new(HashMap::new()),
        matrix_surfaces: Mutex::new(HashMap::new()),
        matrix_candidates: Mutex::new(HashMap::new()),
        inferred_retry_budget: Mutex::new(inferred_ui::RetryBudget::default()),
        inferred_surface_turns: Mutex::new(HashMap::new()),
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
    let _server = match Server::bind(&socket, daemon.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("moted: {e}");
            return Err(1);
        }
    };
    let mut gateway_config = mote_gateway::GatewayConfig::new(data);
    gateway_config.bind = std::env::var("MOTE_GATEWAY_BIND")
        .unwrap_or_else(|_| "0.0.0.0:47810".into())
        .parse()
        .map_err(|error| {
            eprintln!("moted: invalid MOTE_GATEWAY_BIND: {error}");
            1
        })?;
    gateway_config.discovery_enabled = std::env::var("MOTE_DISCOVERY").as_deref() != Ok("0");
    let _gateway = gateway::bind(daemon, gateway_config).map_err(|e| {
        eprintln!("moted: cannot bind local gateway: {e}");
        1
    })?;

    eprintln!("moted: listening on {}", socket.display());
    while !shutdown.requested() {
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
