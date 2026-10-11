use super::{approvals, now_ms, Daemon, MatrixCandidateFinalization};
use approvals::{ApprovalIntent, ApprovalRecord, ApprovalStatus, ApprovalStore};
use mote_browser::{
    ApprovedOrigins, BrowserCommand, BrowserCommitIntent, BrowserHost, BrowserPolicy,
    BrowserResult, BrowserSession, LaunchConfig, NodeRef,
};
use mote_ipc::{Actor, RpcError};
use mote_protocol::{ActionDigest, ApprovalDecision, ApprovalVerdict, Event, HostId};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub(super) struct BrowserTask {
    host: Option<BrowserHost>,
    session: BrowserSession,
    thread: String,
    host_id: HostId,
    conn: Connection,
    log: mote_eventlog::EventLog,
    intents: HashMap<i64, BrowserCommitIntent>,
}

fn internal(error: impl std::fmt::Display) -> RpcError {
    RpcError::internal(mote_redact::redact(&error.to_string()))
}

fn text<'a>(params: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    params[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| RpcError::invalid_params(format!("{key} required")))
}

fn intent(value: &BrowserCommitIntent) -> Result<ApprovalIntent, RpcError> {
    Ok(ApprovalIntent::Browser {
        origin: value.origin.clone(),
        node_ref: value.node_ref.0.clone(),
        request_fingerprint: ActionDigest::new(value.request_fingerprint.clone())
            .map_err(internal)?,
        effect: approvals::BrowserEffect::NonGet,
        action: serde_json::to_value(value).map_err(internal)?,
    })
}

fn status(record: &ApprovalRecord) -> &'static str {
    match record.status {
        ApprovalStatus::Pending => "pending",
        ApprovalStatus::Claimed => "failed",
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Denied => "denied",
        ApprovalStatus::Expired => "expired",
        ApprovalStatus::Failed => "failed",
    }
}

fn event_payload(record: &ApprovalRecord) -> Value {
    json!({"host_id":record.host_id,"approval":{"approval_id":record.id.to_string(),"thread":record.thread_id,"kind":record.kind,"payload":record.display,"action_hash":record.action_hash,"expires_at":record.expires_at},"status":status(record)})
}

impl BrowserTask {
    fn emit(&self, record: &ApprovalRecord, created: bool) -> Result<(), RpcError> {
        self.log
            .append(
                &record.thread_id,
                if created {
                    "approval.created"
                } else {
                    "approval.updated"
                },
                &event_payload(record),
            )
            .map_err(internal)?;
        Ok(())
    }

    fn reconcile(&mut self) -> Result<(), RpcError> {
        let BrowserResult::Pending(pending) = self
            .session
            .command(BrowserCommand::Pending)
            .map_err(internal)?
        else {
            return Err(internal("unexpected browser response"));
        };
        let store = ApprovalStore::new(&self.conn, self.host_id.clone()).map_err(internal)?;
        for request in &pending {
            if self
                .intents
                .values()
                .any(|known| known.request_fingerprint == request.request_fingerprint)
            {
                continue;
            }
            let record = store
                .create(
                    &self.thread,
                    intent(request)?,
                    ActionDigest::new(request.page_state_hash.clone()).map_err(internal)?,
                    json!({}),
                    now_ms(),
                    request.expires_at,
                )
                .map_err(internal)?;
            self.intents.insert(record.id, request.clone());
            self.emit(&record, true)?;
        }
        let stale: Vec<_> = self
            .intents
            .iter()
            .filter(|(_, request)| {
                request.expires_at <= now_ms()
                    || !pending
                        .iter()
                        .any(|live| live.request_fingerprint == request.request_fingerprint)
                    || self
                        .session
                        .command(BrowserCommand::Inspect {
                            intent: (*request).clone(),
                        })
                        .is_err()
            })
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            let request = self.intents.remove(&id).expect("retained intent");
            let terminal = if request.expires_at <= now_ms() {
                "expired"
            } else {
                "failed"
            };
            self.conn.execute("UPDATE approvals SET status=?1,completed_at=?2,result='request invalidated' WHERE id=?3 AND host_id=?4 AND status='pending'", params![terminal,now_ms(),id,self.host_id.as_str()]).map_err(internal)?;
            let _ = self.session.command(BrowserCommand::Cancel {
                fingerprint: request.request_fingerprint,
            });
            if let Some(record) = store.record(id).map_err(internal)? {
                self.emit(&record, false)?;
            }
        }
        Ok(())
    }
}

pub(super) fn configure(daemon: &Daemon, params: Value) -> Result<Value, RpcError> {
    let thread = text(&params, "thread")?.to_owned();
    let origins: Vec<String> =
        serde_json::from_value(params["origins"].clone()).map_err(internal)?;
    if origins.is_empty() {
        return Err(RpcError::invalid_params("approved local origins required"));
    }
    for origin in &origins {
        let url = url::Url::parse(origin).map_err(internal)?;
        if !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")) {
            return Err(RpcError::invalid_params(
                "only explicitly configured loopback fixtures are supported",
            ));
        }
    }
    let reads: Vec<String> =
        serde_json::from_value(params["read_paths"].clone()).map_err(internal)?;
    let config = LaunchConfig::new(
        &daemon.data_dir,
        "mote",
        Path::new(text(&params, "executable")?),
    )
    .map_err(internal)?;
    let mut current = daemon.browser.lock().unwrap();
    if current.is_some() {
        return Err(RpcError::invalid_params("browser task already configured"));
    }
    let host = BrowserHost::launch(
        config,
        BrowserPolicy::new(ApprovedOrigins::new(origins).map_err(internal)?).with_read_paths(reads),
    )
    .map_err(internal)?;
    let conn = Connection::open(daemon.data_dir.join("mote.db")).map_err(internal)?;
    let store = ApprovalStore::new(&conn, daemon.host_id.clone()).map_err(internal)?;
    let old = store.pending_for_host(now_ms()).map_err(internal)?;
    conn.execute("UPDATE approvals SET status='failed',completed_at=?1,result='browser worker restarted' WHERE host_id=?2 AND kind='browser' AND status IN ('pending','claimed')",params![now_ms(),daemon.host_id.as_str()]).map_err(internal)?;
    let task = BrowserTask {
        session: host.session(),
        host: Some(host),
        thread,
        host_id: daemon.host_id.clone(),
        conn,
        log: daemon.log.clone(),
        intents: HashMap::new(),
    };
    for record in old {
        if let Some(record) = ApprovalStore::new(&task.conn, task.host_id.clone())
            .map_err(internal)?
            .record(record.id)
            .map_err(internal)?
        {
            task.emit(&record, false)?;
        }
    }
    let task = Arc::new(Mutex::new(task));
    let weak = Arc::downgrade(&task);
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(200));
        let Some(task) = weak.upgrade() else {
            break;
        };
        let _ = task.lock().unwrap().reconcile();
    });
    *current = Some(task);
    Ok(json!({"configured":true}))
}

fn snapshot(task: &BrowserTask, params: &Value) -> Result<Value, RpcError> {
    let BrowserResult::Snapshot(mut snapshot) = task
        .session
        .command(BrowserCommand::Snapshot {
            interactive_only: params["interactive_only"].as_bool().unwrap_or(false),
            token_cap: params["token_cap"].as_u64().unwrap_or(4096).min(8192) as usize,
        })
        .map_err(internal)?
    else {
        return Err(internal("unexpected browser snapshot"));
    };
    snapshot.text = snapshot.untrusted_text();
    Ok(json!({"snapshot":snapshot}))
}

fn artifact(
    daemon: &Daemon,
    task: &BrowserTask,
    suffix: &str,
    bytes: &[u8],
) -> Result<String, RpcError> {
    let directory = daemon.data_dir.join("browser-evidence");
    mote_paths::ensure_private_dir(&directory).map_err(internal)?;
    let name = format!(
        "{}-{}-{suffix}",
        now_ms(),
        task.log.max_seq().map_err(internal)?
    );
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join(&name))
        .map_err(internal)?;
    file.write_all(bytes).map_err(internal)?;
    Ok(name)
}

fn screenshot(daemon: &Daemon, task: &BrowserTask, suffix: &str) -> Result<String, RpcError> {
    let BrowserResult::Screenshot(bytes) = task
        .session
        .command(BrowserCommand::Screenshot)
        .map_err(internal)?
    else {
        return Err(internal("unexpected browser screenshot"));
    };
    artifact(daemon, task, suffix, &bytes)
}

fn account(
    daemon: &Daemon,
    thread: &str,
    method: &str,
    params: &Value,
    started: Instant,
    result: &Result<Value, RpcError>,
) -> Result<(), RpcError> {
    let output = result
        .as_ref()
        .map(|value| value.to_string())
        .unwrap_or_default();
    let status = match result {
        Ok(value) if value["status"] == "failed" => "indeterminate",
        Ok(_) => "ok",
        Err(_) => "error",
    };
    let row = mote_context::ToolLedgerEntry {
        host_id: daemon.host_id.as_str().into(),
        thread_id: thread.into(),
        action: method.into(),
        input_tokens: mote_context::count_tokens(&params.to_string()) as i64,
        output_tokens: mote_context::count_tokens(&output) as i64,
        latency_ms: started.elapsed().as_millis() as i64,
        status: status.into(),
    };
    let ledger = daemon
        .store
        .lock()
        .unwrap()
        .record_tool(&row)
        .map_err(internal)?;
    daemon.log.append(thread,"browser_step",&json!({"host_id":daemon.host_id,"action":method,"status":status,"tool_ledger_id":ledger,"latency_ms":row.latency_ms})).map_err(internal)?;
    Ok(())
}

struct ExecutionAttempt<'a> {
    daemon: &'a Daemon,
    thread: String,
    ledger_id: i64,
    started: Instant,
    dispatched: bool,
    finished: bool,
}

impl<'a> ExecutionAttempt<'a> {
    fn begin(daemon: &'a Daemon, record: &ApprovalRecord) -> Result<Self, RpcError> {
        let started = Instant::now();
        let ledger_id = daemon
            .store
            .lock()
            .unwrap()
            .record_tool(&mote_context::ToolLedgerEntry {
                host_id: daemon.host_id.as_str().into(),
                thread_id: record.thread_id.clone(),
                action: "browser.approved_request".into(),
                input_tokens: mote_context::count_tokens(&record.id.to_string()) as i64,
                output_tokens: 0,
                latency_ms: 0,
                status: "pending".into(),
            })
            .map_err(internal)?;
        let attempt = Self {
            daemon,
            thread: record.thread_id.clone(),
            ledger_id,
            started,
            dispatched: false,
            finished: false,
        };
        daemon.log.append(&record.thread_id,"browser_step",&json!({"host_id":daemon.host_id,"action":"browser.approved_request","status":"pending","approval_id":record.id.to_string(),"tool_ledger_id":ledger_id})).map_err(internal)?;
        Ok(attempt)
    }

    fn finish(&mut self, result: &Value, status: &str) -> Result<(), RpcError> {
        if !self
            .daemon
            .store
            .lock()
            .unwrap()
            .complete_tool(
                self.ledger_id,
                self.daemon.host_id.as_str(),
                mote_context::count_tokens(&result.to_string()) as i64,
                self.started.elapsed().as_millis() as i64,
                status,
            )
            .map_err(internal)?
        {
            return Err(internal("browser attempt accounting is not pending"));
        }
        self.finished = true;
        self.daemon.log.append(&self.thread,"browser_step.completed",&json!({"host_id":self.daemon.host_id,"action":"browser.approved_request","status":status,"tool_ledger_id":self.ledger_id})).map_err(internal)?;
        Ok(())
    }
}

impl Drop for ExecutionAttempt<'_> {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.finish(
                &Value::Null,
                if self.dispatched {
                    "indeterminate"
                } else {
                    "failed"
                },
            );
        }
    }
}

pub(super) fn call(daemon: &Daemon, method: &str, params: Value) -> Result<Value, RpcError> {
    if method == "browser.configure" {
        return configure(daemon, params);
    }
    if method == "browser.close" {
        if let Some(task) = daemon.browser.lock().unwrap().take() {
            let mut task = task.lock().unwrap();
            drop(task.host.take());
            let store = ApprovalStore::new(&task.conn, task.host_id.clone()).map_err(internal)?;
            let pending = store.pending_for_host(now_ms()).map_err(internal)?;
            task.conn.execute("UPDATE approvals SET status='failed',completed_at=?1,result='browser worker closed' WHERE host_id=?2 AND kind='browser' AND status IN ('pending','claimed')",params![now_ms(),task.host_id.as_str()]).map_err(internal)?;
            for record in pending {
                if let Some(record) = store.record(record.id).map_err(internal)? {
                    task.emit(&record, false)?;
                }
            }
            task.intents.clear();
        }
        return Ok(json!({"closed":true}));
    }
    let thread = text(&params, "thread")?;
    let started = Instant::now();
    let result = (|| {
        let task = daemon
            .browser
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| RpcError::invalid_params("browser task not configured"))?;
        let mut task = task.lock().unwrap();
        if task.thread != thread {
            return Err(RpcError::invalid_params(
                "browser task belongs to another thread",
            ));
        }
        let reference = || text(&params, "reference").map(|value| NodeRef(value.into()));
        let result = match method {
            "browser.open" => {
                task.session
                    .command(BrowserCommand::Navigate {
                        url: text(&params, "url")?.into(),
                    })
                    .map_err(internal)?;
                snapshot(&task, &params)
            }
            "browser.snapshot" => snapshot(&task, &params),
            "browser.text" => {
                let BrowserResult::Text(value) = task
                    .session
                    .command(BrowserCommand::Read {
                        reference: reference()?,
                    })
                    .map_err(internal)?
                else {
                    return Err(internal("unexpected browser text"));
                };
                Ok(json!({"text":value}))
            }
            "browser.find" => {
                let mut value = snapshot(&task, &params)?;
                let query = text(&params, "query")?.to_lowercase();
                value["snapshot"]["nodes"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|node| {
                        node["name"]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query)
                    });
                Ok(value)
            }
            "browser.type" => {
                task.session
                    .command(BrowserCommand::Fill {
                        reference: reference()?,
                        value: text(&params, "value")?.into(),
                    })
                    .map_err(internal)?;
                Ok(json!({"filled":true}))
            }
            "browser.click" => {
                task.session
                    .command(BrowserCommand::Prepare {
                        reference: reference()?,
                    })
                    .map_err(internal)?;
                Ok(json!({"status":"approval_required"}))
            }
            "browser.select" => {
                task.session
                    .command(BrowserCommand::Select {
                        reference: reference()?,
                        value: text(&params, "value")?.into(),
                    })
                    .map_err(internal)?;
                Ok(json!({"selected":true}))
            }
            "browser.back" => {
                task.session
                    .command(BrowserCommand::Back)
                    .map_err(internal)?;
                snapshot(&task, &params)
            }
            "browser.scroll" => {
                task.session
                    .command(BrowserCommand::Scroll {
                        delta: params["delta"]
                            .as_i64()
                            .ok_or_else(|| RpcError::invalid_params("integer delta required"))?,
                    })
                    .map_err(internal)?;
                Ok(json!({"scrolled":true}))
            }
            "browser.fetch" => {
                match task
                    .session
                    .command(BrowserCommand::Fetch {
                        url: text(&params, "url")?.into(),
                        method: params["method"].as_str().unwrap_or("GET").into(),
                        body: params["body"].as_str().map(str::to_owned),
                    })
                    .map_err(internal)?
                {
                    BrowserResult::Text(value) => Ok(json!({"text":value})),
                    BrowserResult::CommitRequired { .. } => {
                        Ok(json!({"status":"approval_required"}))
                    }
                    _ => Err(internal("unexpected fetch result")),
                }
            }
            "browser.screenshot" => {
                Ok(json!({"artifact":screenshot(daemon,&task,"screenshot.png")?}))
            }
            _ => Err(RpcError::invalid_params("unsupported browser tool")),
        };
        task.reconcile()?;
        result
    })();
    account(daemon, thread, method, &params, started, &result)?;
    result
}

pub(super) fn deliver(daemon: &Daemon, actor: &Actor, event: &Event) -> Result<(), RpcError> {
    if !matches!(event.kind.as_str(), "approval.created" | "approval.updated") {
        return Ok(());
    }
    let Actor::PairedDevice { host_id, device_id } = actor else {
        return Err(RpcError::invalid_params("paired delivery required"));
    };
    if host_id != &daemon.host_id || event.payload["host_id"] != json!(host_id) {
        return Err(RpcError::invalid_params("wrong approval host"));
    }
    let id: i64 = text(&event.payload["approval"], "approval_id")?
        .parse()
        .map_err(internal)?;
    let conn = Connection::open(daemon.data_dir.join("mote.db")).map_err(internal)?;
    let record = ApprovalStore::new(&conn, daemon.host_id.clone())
        .map_err(internal)?
        .record(id)
        .map_err(internal)?
        .ok_or_else(|| RpcError::invalid_params("unknown approval"))?;
    if event_payload(&record)["approval"] != event.payload["approval"]
        || event.scope_id != record.thread_id
    {
        return Err(RpcError::invalid_params("approval event binding changed"));
    }
    conn.execute("INSERT OR IGNORE INTO approval_devices(host_id,device_id,approval_id,action_hash,expires_at) VALUES(?1,?2,?3,?4,?5)",params![host_id.as_str(),device_id,id,record.action_hash.as_str(),record.expires_at]).map_err(internal)?;
    Ok(())
}

pub(super) fn deliver_snapshot(
    daemon: &Daemon,
    actor: &Actor,
    snapshot: &mote_protocol::HostSnapshot,
) -> Result<(), RpcError> {
    for approval in &snapshot.pending_approvals {
        if approval.kind != "browser" {
            continue;
        }
        deliver(
            daemon,
            actor,
            &Event {
                seq: snapshot.cursor.seq,
                ts: 0,
                scope_id: approval.thread.clone(),
                kind: "approval.created".into(),
                payload: json!({"host_id":snapshot.host_id,"approval":approval,"status":"pending"}),
            },
        )?;
    }
    Ok(())
}

pub(super) fn decide(
    daemon: &Daemon,
    device: &str,
    decision: &ApprovalDecision,
) -> Result<Option<Value>, RpcError> {
    let id: i64 = decision.approval_id.as_str().parse().map_err(internal)?;
    let conn = Connection::open(daemon.data_dir.join("mote.db")).map_err(internal)?;
    let store = ApprovalStore::new(&conn, daemon.host_id.clone()).map_err(internal)?;
    let Some(record) = store.record(id).map_err(internal)? else {
        return Ok(None);
    };
    let delivered: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM approval_devices WHERE host_id=?1 AND device_id=?2 AND approval_id=?3 AND action_hash=?4 AND expires_at=?5)",params![daemon.host_id.as_str(),device,id,decision.action_hash.as_str(),decision.expires_at],|row|row.get(0)).map_err(internal)?;
    if !delivered
        || decision.host_id != daemon.host_id
        || record.thread_id != decision.thread
        || record.action_hash != decision.action_hash
        || record.expires_at != decision.expires_at
    {
        return Err(RpcError::invalid_params(
            "approval binding stale or not delivered",
        ));
    }
    if !matches!(record.status, ApprovalStatus::Pending) {
        return Ok(Some(json!({"status":status(&record),"changed":false})));
    }
    let task = daemon
        .browser
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| RpcError::invalid_params("browser worker unavailable"))?;
    let mut task = task.lock().unwrap();
    let binding = approvals::ApprovalDecision {
        id,
        host_id: daemon.host_id.clone(),
        thread_id: decision.thread.clone(),
        action_hash: record.action_hash.clone(),
        page_state_hash: record.page_state_hash.clone(),
        expires_at: record.expires_at,
    };
    if decision.decision == ApprovalVerdict::Deny {
        store.deny(&binding, now_ms()).map_err(internal)?;
        if let Some(request) = task.intents.remove(&id) {
            let _ = task.session.command(BrowserCommand::Cancel {
                fingerprint: request.request_fingerprint,
            });
        }
        let record = store.record(id).map_err(internal)?.unwrap();
        task.emit(&record, false)?;
        return Ok(Some(json!({"status":status(&record)})));
    }
    let request = task
        .intents
        .get(&id)
        .cloned()
        .ok_or_else(|| RpcError::invalid_params("request invalidated"))?;
    let before = screenshot(daemon, &task, "before.png")?;
    task.session
        .command(BrowserCommand::Inspect {
            intent: request.clone(),
        })
        .map_err(internal)?;
    let claim = store
        .claim(
            &binding,
            &intent(&request)?,
            &record.page_state_hash,
            now_ms(),
        )
        .map_err(internal)?
        .ok_or_else(|| RpcError::invalid_params("approval no longer pending"))?;
    task.intents.remove(&id);
    let mut attempt = match ExecutionAttempt::begin(daemon, &record) {
        Ok(attempt) => attempt,
        Err(error) => {
            let _ = task.session.command(BrowserCommand::Cancel {
                fingerprint: request.request_fingerprint.clone(),
            });
            let failed = store
                .complete(
                    claim,
                    Err("execution accounting unavailable; request not released".into()),
                    now_ms(),
                )
                .map_err(internal)?;
            task.emit(&failed, false)?;
            return Err(error);
        }
    };
    attempt.dispatched = true;
    let release = task
        .session
        .command(BrowserCommand::Release {
            intent: request.clone(),
        })
        .map(|_| ())
        .map_err(|_| "approved request outcome is indeterminate; authority consumed".to_string());
    let record = store.complete(claim, release, now_ms()).map_err(internal)?;
    task.emit(&record, false)?;
    let after = screenshot(daemon, &task, "after.png").ok();
    let mut response_code = None;
    let mut request_url = url::Url::parse(&request.request_url).map_err(internal)?;
    request_url.set_query(None);
    request_url.set_fragment(None);
    let har = if let Ok(BrowserResult::Evidence(evidence)) =
        task.session.command(BrowserCommand::Evidence)
    {
        response_code = evidence
            .entries
            .iter()
            .rev()
            .find(|entry| {
                entry.method == request.method
                    && entry.url == request_url.as_str()
                    && entry.response_code.is_some()
            })
            .and_then(|entry| entry.response_code);
        Some(artifact(
            daemon,
            &task,
            "network.har",
            &serde_json::to_vec(&evidence.har()).map_err(internal)?,
        )?)
    } else {
        None
    };
    let result = Ok(json!({"status":status(&record),"before":before,"after":after,"har":har}));
    if record.status == ApprovalStatus::Approved {
        let summary = format!(
            "Approved request completed. {} on {} returned HTTP {}. Approval {} was consumed once.",
            request.method,
            record.origin,
            response_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".into()),
            record.id
        );
        let source = format!(
            "screen {{ text {} }}",
            serde_json::to_string(&mote_redact::redact(&summary)).map_err(internal)?
        );
        match daemon.finalize_inferred_source(&record.thread_id, &source)? {
            MatrixCandidateFinalization::Committed(_) => {}
            MatrixCandidateFinalization::Rejected(_) => {
                return Err(internal("browser result failed Matrix validation"))
            }
        }
    }
    attempt.finish(
        result.as_ref().expect("browser result"),
        if record.status == ApprovalStatus::Approved {
            "ok"
        } else {
            "indeterminate"
        },
    )?;
    result.map(Some)
}
