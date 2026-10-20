use mote_memory::{
    run_contract_suite, BackendStatus, Correction, CorrectionResult, DeletionTarget, ForgetResult,
    MemoryBackend, MemoryError, MemoryId, MemoryScope, MemoryWrite,
};
use mote_memory_tencent::{MemoryCoreClient, TencentConfig, TencentMemory};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use url::Url;

#[derive(Clone, Default)]
struct FakeClient {
    inner: Arc<Mutex<FakeState>>,
}

#[derive(Default)]
struct FakeState {
    records: BTreeMap<String, Value>,
    next_candidate_id: u64,
    fail_deletes: bool,
    fail_adds_after_accept: bool,
    fail_queries: bool,
    omit_search_scope: bool,
    poison_search_scope: bool,
    envelope_code: i64,
}

impl FakeClient {
    fn failing_deletes() -> Self {
        let client = Self::default();
        client.inner.lock().unwrap().fail_deletes = true;
        client
    }

    fn poison_scope(&self) {
        self.inner.lock().unwrap().poison_search_scope = true;
    }

    fn set_fail_deletes(&self, fail: bool) {
        self.inner.lock().unwrap().fail_deletes = fail;
    }

    fn set_fail_adds_after_accept(&self, fail: bool) {
        self.inner.lock().unwrap().fail_adds_after_accept = fail;
    }

    fn set_fail_queries(&self, fail: bool) {
        self.inner.lock().unwrap().fail_queries = fail;
    }

    fn omit_search_scope(&self) {
        self.inner.lock().unwrap().omit_search_scope = true;
    }

    fn replace_first_candidate_id(&self, replacement: &str) {
        let mut state = self.inner.lock().unwrap();
        let current = state.records.keys().next().unwrap().clone();
        let mut item = state.records.remove(&current).unwrap();
        item["id"] = json!(replacement);
        state.records.insert(replacement.to_owned(), item);
    }

    fn candidate_ids(&self) -> Vec<String> {
        self.inner.lock().unwrap().records.keys().cloned().collect()
    }

    fn nonzero_envelope() -> Self {
        let client = Self::default();
        client.inner.lock().unwrap().envelope_code = 9;
        client
    }
}

impl MemoryCoreClient for FakeClient {
    fn post(&mut self, path: &str, body: &Value) -> Result<Value, MemoryError> {
        for field in ["team_id", "agent_id", "user_id"] {
            if body.get(field).and_then(Value::as_str).is_none() {
                return Err(MemoryError::refused(format!("missing {field}")));
            }
        }
        let mut state = self.inner.lock().unwrap();
        let data = match path {
            "v3/conversation/add" => {
                let message = &body["messages"][0];
                assert!(message.get("id").is_none());
                state.next_candidate_id += 1;
                let id = format!("server-candidate-{}", state.next_candidate_id);
                state.records.insert(
                    id.clone(),
                    json!({
                        "id": id,
                        "role": "user",
                        "content": message["content"],
                        "session_id": body["session_id"],
                        "team_id": body["team_id"],
                        "agent_id": body["agent_id"],
                        "user_id": body["user_id"]
                    }),
                );
                if state.fail_adds_after_accept {
                    return Err(MemoryError::backend(
                        "fixture lost the response after accepting the candidate",
                    ));
                }
                json!({"accepted_ids": [id], "accepted_versions": ["v1"], "total_count": 1})
            }
            "v3/conversation/search" => {
                let query = body["query"].as_str().unwrap();
                let mut messages = state
                    .records
                    .values()
                    .filter(|item| same_scope(item, body))
                    .filter(|item| item["content"].as_str().unwrap().contains(query))
                    .cloned()
                    .collect::<Vec<_>>();
                messages.truncate(body["limit"].as_u64().unwrap() as usize);
                for item in &mut messages {
                    item["score"] = json!(1.0);
                    if state.omit_search_scope {
                        let object = item.as_object_mut().unwrap();
                        object.remove("team_id");
                        object.remove("agent_id");
                        object.remove("user_id");
                    }
                    if state.poison_search_scope {
                        item["user_id"] = json!("wrong-user");
                    }
                }
                json!({"messages": messages})
            }
            "v3/conversation/query" => {
                if state.fail_queries {
                    return Err(MemoryError::unavailable("fixture query unavailable"));
                }
                let messages = state
                    .records
                    .values()
                    .filter(|item| same_scope(item, body))
                    .filter(|item| item["session_id"] == body["session_id"])
                    .cloned()
                    .collect::<Vec<_>>();
                json!({"total": messages.len(), "messages": messages})
            }
            "v3/conversation/delete" => {
                let id = body["message_ids"][0].as_str().unwrap();
                let deleted = if state.fail_deletes {
                    0
                } else if state
                    .records
                    .get(id)
                    .is_some_and(|item| same_scope(item, body))
                {
                    state.records.remove(id);
                    1
                } else {
                    0
                };
                json!({"deleted_count": deleted})
            }
            _ => return Err(MemoryError::backend("unexpected fixture path")),
        };
        Ok(
            json!({"code": state.envelope_code, "message": "fixture", "request_id": "fixture", "data": data}),
        )
    }
}

fn same_scope(item: &Value, body: &Value) -> bool {
    ["team_id", "agent_id", "user_id"]
        .iter()
        .all(|field| item[*field] == body[*field])
}

fn config(directory: &TempDir) -> TencentConfig {
    TencentConfig::new(
        Url::parse("http://127.0.0.1:8420").unwrap(),
        "default",
        "fixture-team",
        "mote-test",
        directory.path().join("state.json"),
        Duration::from_secs(1),
    )
    .unwrap()
}

#[test]
fn satisfies_common_contract_with_scoped_loopback_fixture() {
    let directory = TempDir::new().unwrap();
    run_contract_suite(|| {
        Box::new(TencentMemory::new(config(&directory), FakeClient::default()).unwrap())
    });
}

#[test]
fn correction_and_forget_expose_partial_deletion_and_hide_old_text() {
    let directory = TempDir::new().unwrap();
    let client = FakeClient::failing_deletes();
    let mut backend = TencentMemory::new(config(&directory), client.clone()).unwrap();
    let scope = MemoryScope::new("account", "personal").unwrap();
    let written = backend
        .write(&scope, MemoryWrite::new("old preference").unwrap())
        .unwrap();
    let corrected = backend
        .correct(
            &scope,
            Correction::new(written.id().clone(), 1, "new preference").unwrap(),
        )
        .unwrap();
    assert!(matches!(corrected, CorrectionResult::Partial { .. }));
    assert!(backend.retrieve(&scope, "old", 5).unwrap().is_empty());
    assert_eq!(backend.retrieve(&scope, "new", 5).unwrap().len(), 1);

    let forgotten = backend
        .forget(&scope, written.id(), corrected.current().version())
        .unwrap();
    assert_eq!(
        forgotten.remaining(),
        &[
            DeletionTarget::Candidate("server-candidate-1".to_owned()),
            DeletionTarget::Candidate("server-candidate-2".to_owned()),
        ]
    );
    assert!(backend.retrieve(&scope, "new", 5).unwrap().is_empty());
    assert!(backend.source(&scope, written.id()).unwrap().is_none());
    assert_eq!(backend.health().status(), BackendStatus::Degraded);
    let state = std::fs::read_to_string(directory.path().join("state.json")).unwrap();
    assert!(!state.contains("old preference"));
    assert!(!state.contains("new preference"));

    drop(backend);
    client.set_fail_deletes(false);
    let mut restarted = TencentMemory::new(config(&directory), client.clone()).unwrap();
    let retried = restarted
        .forget(&scope, written.id(), corrected.current().version())
        .unwrap();
    assert!(matches!(retried, ForgetResult::Forgotten { .. }));
    assert!(client.candidate_ids().is_empty());
    assert_eq!(restarted.health().status(), BackendStatus::Healthy);
    assert!(matches!(
        restarted
            .forget(&scope, written.id(), corrected.current().version())
            .unwrap(),
        ForgetResult::AlreadyForgotten { .. }
    ));
}

#[test]
fn later_corrections_preserve_every_outstanding_candidate() {
    let directory = TempDir::new().unwrap();
    let mut backend =
        TencentMemory::new(config(&directory), FakeClient::failing_deletes()).unwrap();
    let scope = MemoryScope::new("account", "personal").unwrap();
    let written = backend
        .write(&scope, MemoryWrite::new("first preference").unwrap())
        .unwrap();
    let second = backend
        .correct(
            &scope,
            Correction::new(written.id().clone(), 1, "second preference").unwrap(),
        )
        .unwrap();
    assert_eq!(
        second.remaining(),
        &[DeletionTarget::Candidate("server-candidate-1".to_owned())]
    );

    let third = backend
        .correct(
            &scope,
            Correction::new(written.id().clone(), 2, "third preference").unwrap(),
        )
        .unwrap();
    assert_eq!(
        third.remaining(),
        &[
            DeletionTarget::Candidate("server-candidate-1".to_owned()),
            DeletionTarget::Candidate("server-candidate-2".to_owned()),
        ]
    );
    assert!(backend.retrieve(&scope, "first", 5).unwrap().is_empty());
    assert!(backend.retrieve(&scope, "second", 5).unwrap().is_empty());
    assert_eq!(backend.retrieve(&scope, "third", 5).unwrap().len(), 1);

    let forgotten = backend
        .forget(&scope, written.id(), third.current().version())
        .unwrap();
    assert_eq!(
        forgotten.remaining(),
        &[
            DeletionTarget::Candidate("server-candidate-1".to_owned()),
            DeletionTarget::Candidate("server-candidate-2".to_owned()),
            DeletionTarget::Candidate("server-candidate-3".to_owned()),
        ]
    );
}

#[test]
fn ambiguous_replacement_add_keeps_prior_version_authoritative() {
    let directory = TempDir::new().unwrap();
    let client = FakeClient::default();
    let scope = MemoryScope::new("account", "personal").unwrap();
    let mut backend = TencentMemory::new(config(&directory), client.clone()).unwrap();
    let written = backend
        .write(&scope, MemoryWrite::new("trusted preference").unwrap())
        .unwrap();
    client.set_fail_adds_after_accept(true);

    assert!(matches!(
        backend.correct(
            &scope,
            Correction::new(written.id().clone(), 1, "uncertain preference").unwrap(),
        ),
        Err(MemoryError::Backend { .. })
    ));
    assert_eq!(backend.retrieve(&scope, "trusted", 5).unwrap().len(), 1);
    assert!(backend.retrieve(&scope, "uncertain", 5).unwrap().is_empty());
    assert_eq!(
        backend
            .source(&scope, written.id())
            .unwrap()
            .unwrap()
            .version(),
        1
    );
    assert_eq!(backend.health().status(), BackendStatus::Degraded);

    drop(backend);
    client.set_fail_adds_after_accept(false);
    let mut restarted = TencentMemory::new(config(&directory), client.clone()).unwrap();
    assert_eq!(restarted.retrieve(&scope, "trusted", 5).unwrap().len(), 1);
    assert!(restarted
        .retrieve(&scope, "uncertain", 5)
        .unwrap()
        .is_empty());
    assert!(matches!(
        restarted.forget(&scope, written.id(), 1).unwrap(),
        ForgetResult::Forgotten { .. }
    ));
    assert!(client.candidate_ids().is_empty());
}

#[test]
fn unresolved_correction_keeps_prior_version_authoritative() {
    let directory = TempDir::new().unwrap();
    let client = FakeClient::default();
    let scope = MemoryScope::new("account", "personal").unwrap();
    let mut backend = TencentMemory::new(config(&directory), client.clone()).unwrap();
    let written = backend
        .write(&scope, MemoryWrite::new("trusted correction base").unwrap())
        .unwrap();
    client.set_fail_adds_after_accept(true);
    client.set_fail_queries(true);

    assert!(matches!(
        backend.correct(
            &scope,
            Correction::new(written.id().clone(), 1, "uncertain correction").unwrap(),
        ),
        Err(MemoryError::Backend { .. })
    ));
    assert_eq!(backend.health().status(), BackendStatus::Degraded);
    assert_eq!(
        backend
            .retrieve(&scope, "trusted correction", 5)
            .unwrap()
            .len(),
        1
    );
    assert!(backend
        .retrieve(&scope, "uncertain correction", 5)
        .unwrap()
        .is_empty());
    let state: Value =
        serde_json::from_slice(&std::fs::read(directory.path().join("state.json")).unwrap())
            .unwrap();
    assert_eq!(state["records"]["m1"]["version"], 1);
    assert_eq!(state["records"]["m1"]["text"], "trusted correction base");
    assert_eq!(state["records"]["m1"]["unresolved_addition"], true);

    drop(backend);
    client.set_fail_adds_after_accept(false);
    client.set_fail_queries(false);
    let mut restarted = TencentMemory::new(config(&directory), client.clone()).unwrap();
    assert!(matches!(
        restarted.forget(&scope, written.id(), 1).unwrap(),
        ForgetResult::Forgotten { .. }
    ));
    assert!(client.candidate_ids().is_empty());
}

#[test]
fn unresolved_addition_stays_degraded_without_retaining_raw_text() {
    let directory = TempDir::new().unwrap();
    let client = FakeClient::default();
    client.set_fail_adds_after_accept(true);
    client.set_fail_queries(true);
    let scope = MemoryScope::new("account", "personal").unwrap();
    let mut backend = TencentMemory::new(config(&directory), client.clone()).unwrap();

    assert!(matches!(
        backend.write(
            &scope,
            MemoryWrite::new("secret unresolved preference").unwrap()
        ),
        Err(MemoryError::Backend { .. })
    ));
    assert_eq!(backend.health().status(), BackendStatus::Degraded);
    let state = std::fs::read_to_string(directory.path().join("state.json")).unwrap();
    assert!(!state.contains("secret unresolved preference"));
    assert!(state.contains("unresolved_addition"));

    drop(backend);
    client.set_fail_adds_after_accept(false);
    client.set_fail_queries(false);
    let mut restarted = TencentMemory::new(config(&directory), client.clone()).unwrap();
    assert!(matches!(
        restarted
            .forget(&scope, &MemoryId::new("m1").unwrap(), 1)
            .unwrap(),
        ForgetResult::Forgotten { .. }
    ));
    assert!(client.candidate_ids().is_empty());
    assert_eq!(restarted.health().status(), BackendStatus::Healthy);
}

#[test]
fn omitted_scope_is_accepted_only_for_the_bound_server_candidate() {
    let directory = TempDir::new().unwrap();
    let client = FakeClient::default();
    let mut backend = TencentMemory::new(config(&directory), client.clone()).unwrap();
    let scope = MemoryScope::new("account", "personal").unwrap();
    backend
        .write(&scope, MemoryWrite::new("bound memory").unwrap())
        .unwrap();
    assert_eq!(client.candidate_ids(), vec!["server-candidate-1"]);

    client.omit_search_scope();
    assert_eq!(backend.retrieve(&scope, "bound", 5).unwrap().len(), 1);
    client.replace_first_candidate_id("unknown-server-candidate");
    assert!(matches!(
        backend.retrieve(&scope, "bound", 5),
        Err(MemoryError::Refused { .. })
    ));
}

#[test]
fn mismatched_returned_scope_fails_closed() {
    let directory = TempDir::new().unwrap();
    let client = FakeClient::default();
    let mut backend = TencentMemory::new(config(&directory), client.clone()).unwrap();
    let scope = MemoryScope::new("account", "personal").unwrap();
    backend
        .write(&scope, MemoryWrite::new("scoped memory").unwrap())
        .unwrap();
    client.poison_scope();
    assert!(matches!(
        backend.retrieve(&scope, "scoped", 5),
        Err(MemoryError::Refused { .. })
    ));
}

#[test]
fn state_survives_restart_without_opening_mote_database() {
    let directory = TempDir::new().unwrap();
    let client = FakeClient::default();
    let scope = MemoryScope::new("account", "personal").unwrap();
    let id = {
        let mut backend = TencentMemory::new(config(&directory), client.clone()).unwrap();
        backend
            .write(&scope, MemoryWrite::new("durable fixture").unwrap())
            .unwrap()
            .id()
            .clone()
    };
    let mut restarted = TencentMemory::new(config(&directory), client).unwrap();
    assert_eq!(
        restarted.retrieve(&scope, "durable", 5).unwrap()[0].id(),
        &id
    );
}

#[test]
fn nonzero_service_envelope_fails_closed() {
    let directory = TempDir::new().unwrap();
    let mut backend =
        TencentMemory::new(config(&directory), FakeClient::nonzero_envelope()).unwrap();
    let scope = MemoryScope::new("account", "personal").unwrap();
    assert!(matches!(
        backend.write(&scope, MemoryWrite::new("fixture memory").unwrap()),
        Err(MemoryError::Backend { .. })
    ));
}
