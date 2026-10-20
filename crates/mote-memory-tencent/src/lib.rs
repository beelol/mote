//! Opt-in, loopback-only TencentDB Agent Memory adapter.

mod client;
mod scope;
mod state;

pub use client::{MemoryCoreClient, UreqMemoryCoreClient};

use mote_memory::{
    validate_query, BackendHealth, Correction, CorrectionResult, DeletionTarget, ForgetResult,
    MemoryBackend, MemoryError, MemoryHit, MemoryId, MemoryScope, MemoryWrite, SourceRef,
};
use scope::WireScope;
use serde_json::{json, Value};
use state::{Record, StateStore};
use std::collections::BTreeSet;
use std::net::{IpAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;
use url::{Host, Url};

const BACKEND_NAME: &str = "tencent";
const MAX_CANDIDATE_RECORDS: usize = 100;

#[derive(Debug, Clone)]
pub struct TencentConfig {
    base_url: Url,
    service_id: String,
    team_id: String,
    namespace_prefix: String,
    state_path: PathBuf,
    timeout: Duration,
}

impl TencentConfig {
    pub fn new(
        base_url: Url,
        service_id: impl Into<String>,
        team_id: impl Into<String>,
        namespace_prefix: impl Into<String>,
        state_path: PathBuf,
        timeout: Duration,
    ) -> Result<Self, MemoryError> {
        let base_url = validate_loopback_url(base_url)?;
        let service_id = checked_component("service_id", service_id.into())?;
        let team_id = checked_component("team_id", team_id.into())?;
        let namespace_prefix = checked_component("namespace_prefix", namespace_prefix.into())?;
        if timeout.is_zero() || timeout > Duration::from_secs(30) {
            return Err(MemoryError::malformed(
                "Tencent timeout must be between 1 ms and 30 seconds",
            ));
        }
        if state_path.file_name().is_none() {
            return Err(MemoryError::malformed(
                "Tencent state path must name a file",
            ));
        }
        Ok(Self {
            base_url,
            service_id,
            team_id,
            namespace_prefix,
            state_path,
            timeout,
        })
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }
}

pub struct TencentMemory<C> {
    config: TencentConfig,
    client: C,
    store: StateStore,
}

impl<C: MemoryCoreClient> TencentMemory<C> {
    pub fn new(config: TencentConfig, client: C) -> Result<Self, MemoryError> {
        let store = StateStore::open(config.state_path.clone())?;
        Ok(Self {
            config,
            client,
            store,
        })
    }

    fn wire_scope(&self, scope: &MemoryScope) -> WireScope {
        WireScope::new(&self.config.team_id, &self.config.namespace_prefix, scope)
    }

    fn source(scope: &MemoryScope, id: &MemoryId, version: u64) -> Result<SourceRef, MemoryError> {
        SourceRef::new(scope.clone(), BACKEND_NAME, id.clone(), version)
    }

    fn scoped_record(&self, scope: &MemoryScope, id: &MemoryId) -> Result<Record, MemoryError> {
        let record = self
            .store
            .state
            .records
            .get(id.as_str())
            .ok_or_else(|| MemoryError::not_found("memory"))?;
        if record.account_id != scope.account_id() || record.realm != scope.realm() {
            return Err(MemoryError::refused("memory scope mismatch"));
        }
        Ok(record.clone())
    }

    fn post_data(&mut self, path: &str, body: Value) -> Result<Value, MemoryError> {
        let envelope = self.client.post(path, &body)?;
        let object = envelope
            .as_object()
            .ok_or_else(|| MemoryError::backend("Tencent envelope must be an object"))?;
        let code = object
            .get("code")
            .and_then(Value::as_i64)
            .ok_or_else(|| MemoryError::backend("Tencent envelope is missing integer code"))?;
        if code != 0 {
            let message = object
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unspecified error");
            return Err(MemoryError::backend(format!(
                "Tencent envelope code {code}: {message}"
            )));
        }
        object
            .get("data")
            .cloned()
            .ok_or_else(|| MemoryError::backend("Tencent success envelope is missing data"))
    }

    fn add_candidate(
        &mut self,
        scope: &MemoryScope,
        session_id: &str,
        text: &str,
    ) -> Result<String, MemoryError> {
        let wire = self.wire_scope(scope);
        let body = scoped_body(
            &wire,
            json!({
                "session_id": session_id,
                "messages": [{"role": "user", "content": text}]
            }),
        );
        let data = self.post_data("v3/conversation/add", body)?;
        let accepted = data
            .get("accepted_ids")
            .and_then(Value::as_array)
            .ok_or_else(|| MemoryError::backend("Tencent add response lacks accepted_ids"))?;
        if accepted.len() != 1 {
            return Err(MemoryError::refused(
                "Tencent add must accept exactly one conversation ID",
            ));
        }
        let candidate_id = accepted[0]
            .as_str()
            .ok_or_else(|| MemoryError::refused("Tencent accepted ID must be a string"))?;
        validate_candidate_id(candidate_id)?;
        if let Some(versions) = data.get("accepted_versions") {
            let versions = versions.as_array().ok_or_else(|| {
                MemoryError::refused("Tencent accepted_versions must be an array")
            })?;
            if versions.len() != 1 || versions[0].as_str() != Some("v1") {
                return Err(MemoryError::refused(
                    "Tencent add returned an unexpected accepted version",
                ));
            }
        }
        if let Some(total_count) = data.get("total_count") {
            if total_count.as_u64() != Some(1) {
                return Err(MemoryError::refused(
                    "Tencent add returned an unexpected total count",
                ));
            }
        }
        Ok(candidate_id.to_owned())
    }

    fn discover_added_candidates(
        &mut self,
        scope: &MemoryScope,
        session_id: &str,
        known_ids: &BTreeSet<String>,
    ) -> Result<Vec<String>, MemoryError> {
        let wire = self.wire_scope(scope);
        let body = scoped_body(
            &wire,
            json!({"session_id": session_id, "limit": 100, "offset": 0}),
        );
        let data = self.post_data("v3/conversation/query", body)?;
        let messages = data
            .get("messages")
            .and_then(Value::as_array)
            .ok_or_else(|| MemoryError::backend("Tencent query response lacks messages"))?;
        let total = data
            .get("total")
            .and_then(Value::as_u64)
            .ok_or_else(|| MemoryError::backend("Tencent query response lacks total"))?;
        if total != messages.len() as u64 {
            return Err(MemoryError::refused(
                "Tencent ambiguity query was incomplete",
            ));
        }
        let mut discovered = BTreeSet::new();
        for item in messages {
            wire.verify_item(item)?;
            let candidate_id = item
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| MemoryError::refused("Tencent item is missing id"))?;
            validate_candidate_id(candidate_id)?;
            if !known_ids.contains(candidate_id) {
                discovered.insert(candidate_id.to_owned());
            }
        }
        Ok(discovered.into_iter().collect())
    }

    fn delete_candidate(
        &mut self,
        scope: &MemoryScope,
        session_id: &str,
        candidate_id: &str,
    ) -> Result<bool, MemoryError> {
        let wire = self.wire_scope(scope);
        let body = scoped_body(&wire, json!({"message_ids": [candidate_id]}));
        let data = self.post_data("v3/conversation/delete", body)?;
        let deleted = data
            .get("deleted_count")
            .and_then(Value::as_u64)
            .ok_or_else(|| MemoryError::backend("Tencent delete response lacks deleted_count"))?;
        if deleted == 1 {
            return Ok(true);
        }
        self.candidate_is_absent(scope, session_id, candidate_id)
    }

    fn candidate_is_absent(
        &mut self,
        scope: &MemoryScope,
        session_id: &str,
        candidate_id: &str,
    ) -> Result<bool, MemoryError> {
        let wire = self.wire_scope(scope);
        let body = scoped_body(
            &wire,
            json!({"session_id": session_id, "limit": 100, "offset": 0}),
        );
        let data = self.post_data("v3/conversation/query", body)?;
        let messages = data
            .get("messages")
            .and_then(Value::as_array)
            .ok_or_else(|| MemoryError::backend("Tencent query response lacks messages"))?;
        let total = data
            .get("total")
            .and_then(Value::as_u64)
            .ok_or_else(|| MemoryError::backend("Tencent query response lacks total"))?;
        if total > messages.len() as u64 {
            return Ok(false);
        }
        for item in messages {
            wire.verify_item(item)?;
            let returned_id = item
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| MemoryError::refused("Tencent item is missing id"))?;
            if returned_id == candidate_id {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn attempt_pending_deletions(
        &mut self,
        scope: &MemoryScope,
        session_id: &str,
        candidates: &[String],
    ) -> Vec<String> {
        candidates
            .iter()
            .filter(|candidate| {
                !self
                    .delete_candidate(scope, session_id, candidate)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    fn verify_candidate(
        &self,
        scope: &MemoryScope,
        wire: &WireScope,
        item: &Value,
    ) -> Result<Option<(MemoryId, Record)>, MemoryError> {
        wire.verify_item(item)?;
        let candidate_id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| MemoryError::refused("Tencent item is missing id"))?;
        let current = self
            .store
            .state
            .records
            .iter()
            .find(|(_, record)| record.candidate_id == candidate_id && !record.forgotten);
        if let Some((mote_id, record)) = current {
            if record.account_id != scope.account_id() || record.realm != scope.realm() {
                return Err(MemoryError::refused(
                    "Tencent returned a cross-scope candidate",
                ));
            }
            let content = item
                .get("content")
                .and_then(Value::as_str)
                .ok_or_else(|| MemoryError::refused("Tencent item is missing content"))?;
            if content != record.text {
                return Err(MemoryError::refused(
                    "Tencent candidate content differs from sidecar authority",
                ));
            }
            return Ok(Some((MemoryId::new(mote_id.clone())?, record.clone())));
        }
        if let Some(record) = self
            .store
            .state
            .records
            .values()
            .find(|record| record.pending_deletions.iter().any(|id| id == candidate_id))
        {
            if record.account_id != scope.account_id() || record.realm != scope.realm() {
                return Err(MemoryError::refused(
                    "Tencent returned a cross-scope pending deletion",
                ));
            }
            return Ok(None);
        }
        if let Some(record) = self.store.state.records.values().find(|record| {
            record.unresolved_addition
                && item.get("session_id").and_then(Value::as_str)
                    == Some(record.session_id.as_str())
        }) {
            if record.account_id != scope.account_id() || record.realm != scope.realm() {
                return Err(MemoryError::refused(
                    "Tencent returned a cross-scope unresolved addition",
                ));
            }
            return Ok(None);
        }
        Err(MemoryError::refused(
            "Tencent returned an unbound candidate",
        ))
    }
}

impl TencentMemory<UreqMemoryCoreClient> {
    pub fn connect(config: TencentConfig) -> Result<Self, MemoryError> {
        let client = UreqMemoryCoreClient::new(
            config.base_url.clone(),
            config.service_id.clone(),
            config.timeout,
        );
        Self::new(config, client)
    }
}

impl<C: MemoryCoreClient> MemoryBackend for TencentMemory<C> {
    fn write(&mut self, scope: &MemoryScope, write: MemoryWrite) -> Result<MemoryHit, MemoryError> {
        if self.store.state.records.len() >= MAX_CANDIDATE_RECORDS {
            return Err(MemoryError::refused(
                "Tencent synthetic comparison is limited to 100 records",
            ));
        }
        let sequence = self.store.state.next_id + 1;
        let id = MemoryId::new(format!("m{sequence}"))?;
        let session_id = format!("{}-record-{sequence}", self.config.namespace_prefix);
        self.store.state.next_id = sequence;
        let pending = Record {
            account_id: scope.account_id().to_owned(),
            realm: scope.realm().to_owned(),
            session_id: session_id.clone(),
            candidate_id: String::new(),
            text: String::new(),
            version: 1,
            forgotten: true,
            unresolved_addition: true,
            pending_deletions: Vec::new(),
        };
        self.store
            .state
            .records
            .insert(id.as_str().to_owned(), pending.clone());
        self.store.persist()?;
        let candidate_id = match self.add_candidate(scope, &session_id, write.text()) {
            Ok(candidate_id) => candidate_id,
            Err(add_error) => {
                if let Ok(discovered) =
                    self.discover_added_candidates(scope, &session_id, &BTreeSet::new())
                {
                    let mut failed = pending.clone();
                    failed.pending_deletions = discovered;
                    failed.unresolved_addition = false;
                    self.store
                        .state
                        .records
                        .insert(id.as_str().to_owned(), failed);
                    self.store.persist()?;
                }
                return Err(add_error);
            }
        };
        self.store.state.records.insert(
            id.as_str().to_owned(),
            Record {
                account_id: scope.account_id().to_owned(),
                realm: scope.realm().to_owned(),
                session_id,
                candidate_id,
                text: write.text().to_owned(),
                version: 1,
                forgotten: false,
                unresolved_addition: false,
                pending_deletions: Vec::new(),
            },
        );
        if let Err(error) = self.store.persist() {
            let mut recovery = self
                .store
                .state
                .records
                .get(id.as_str())
                .expect("active record was inserted")
                .clone();
            recovery.forgotten = true;
            recovery.text.clear();
            recovery.pending_deletions = vec![recovery.candidate_id.clone()];
            self.store
                .state
                .records
                .insert(id.as_str().to_owned(), recovery);
            self.store.persist()?;
            return Err(error);
        }
        let source = Self::source(scope, &id, 1)?;
        MemoryHit::new(scope.clone(), id, source, write.into_text(), 1, 1.0)
    }

    fn retrieve(
        &mut self,
        scope: &MemoryScope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryHit>, MemoryError> {
        validate_query(query, limit)?;
        let wire = self.wire_scope(scope);
        let body = scoped_body(&wire, json!({"query": query, "limit": limit}));
        let data = self.post_data("v3/conversation/search", body)?;
        let messages = data
            .get("messages")
            .and_then(Value::as_array)
            .ok_or_else(|| MemoryError::backend("Tencent search response lacks messages"))?;
        if messages.len() > limit {
            return Err(MemoryError::refused(
                "Tencent search exceeded the requested limit",
            ));
        }
        let mut hits = Vec::new();
        for item in messages {
            if let Some((id, record)) = self.verify_candidate(scope, &wire, item)? {
                let score = item
                    .get("score")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| MemoryError::refused("Tencent search item lacks score"))?;
                let source = Self::source(scope, &id, record.version)?;
                hits.push(MemoryHit::new(
                    scope.clone(),
                    id,
                    source,
                    record.text,
                    hits.len() + 1,
                    score as f32,
                )?);
            }
        }
        Ok(hits)
    }

    fn source(
        &mut self,
        scope: &MemoryScope,
        id: &MemoryId,
    ) -> Result<Option<SourceRef>, MemoryError> {
        let record = self.scoped_record(scope, id)?;
        if record.forgotten {
            return Ok(None);
        }
        let wire = self.wire_scope(scope);
        let body = scoped_body(
            &wire,
            json!({"session_id": record.session_id, "limit": 100, "offset": 0}),
        );
        let data = self.post_data("v3/conversation/query", body)?;
        let messages = data
            .get("messages")
            .and_then(Value::as_array)
            .ok_or_else(|| MemoryError::backend("Tencent query response lacks messages"))?;
        let item = messages
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(&record.candidate_id))
            .ok_or_else(|| MemoryError::not_found("Tencent candidate"))?;
        self.verify_candidate(scope, &wire, item)?;
        Self::source(scope, id, record.version).map(Some)
    }

    fn correct(
        &mut self,
        scope: &MemoryScope,
        correction: Correction,
    ) -> Result<CorrectionResult, MemoryError> {
        let mut record = self.scoped_record(scope, correction.id())?;
        if record.forgotten {
            return Err(MemoryError::not_found("memory was forgotten"));
        }
        if record.version != correction.expected_version() {
            return Err(MemoryError::conflict("stale memory version"));
        }
        let prior = Self::source(scope, correction.id(), record.version)?;
        let old_candidate = record.candidate_id.clone();
        let new_version = record.version + 1;
        let mut checkpoint = record.clone();
        checkpoint.version = new_version;
        checkpoint.text.clear();
        checkpoint.forgotten = true;
        checkpoint.unresolved_addition = true;
        checkpoint.pending_deletions = candidate_set(
            record
                .pending_deletions
                .iter()
                .chain(std::iter::once(&old_candidate)),
        );
        self.store
            .state
            .records
            .insert(correction.id().as_str().to_owned(), checkpoint.clone());
        self.store.persist()?;
        let new_candidate =
            match self.add_candidate(scope, &record.session_id, correction.replacement()) {
                Ok(candidate_id) => candidate_id,
                Err(add_error) => {
                    let known_ids = record
                        .pending_deletions
                        .iter()
                        .chain(std::iter::once(&record.candidate_id))
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    let discovered =
                        self.discover_added_candidates(scope, &record.session_id, &known_ids);
                    let Ok(discovered) = discovered else {
                        record.unresolved_addition = true;
                        self.store
                            .state
                            .records
                            .insert(correction.id().as_str().to_owned(), record);
                        self.store.persist()?;
                        return Err(add_error);
                    };
                    record.pending_deletions.extend(discovered);
                    record.pending_deletions = candidate_set(record.pending_deletions.iter());
                    record.unresolved_addition = false;
                    self.store
                        .state
                        .records
                        .insert(correction.id().as_str().to_owned(), record);
                    if let Err(persist_error) = self.store.persist() {
                        self.store
                            .state
                            .records
                            .insert(correction.id().as_str().to_owned(), checkpoint);
                        return Err(persist_error);
                    }
                    return Err(add_error);
                }
            };
        record.version = new_version;
        record.candidate_id = new_candidate;
        record.text = correction.into_replacement();
        record.unresolved_addition = false;
        record.pending_deletions = candidate_set(
            record
                .pending_deletions
                .iter()
                .chain(std::iter::once(&old_candidate)),
        );
        self.store
            .state
            .records
            .insert(prior.memory_id().as_str().to_owned(), record.clone());
        if let Err(error) = self.store.persist() {
            let mut recovery = record.clone();
            recovery.forgotten = true;
            recovery.text.clear();
            recovery
                .pending_deletions
                .push(recovery.candidate_id.clone());
            recovery.pending_deletions = candidate_set(recovery.pending_deletions.iter());
            self.store
                .state
                .records
                .insert(prior.memory_id().as_str().to_owned(), recovery);
            self.store.persist()?;
            return Err(error);
        }
        record.pending_deletions =
            self.attempt_pending_deletions(scope, &record.session_id, &record.pending_deletions);
        self.store
            .state
            .records
            .insert(prior.memory_id().as_str().to_owned(), record.clone());
        self.store.persist()?;
        let current = Self::source(scope, prior.memory_id(), new_version)?;
        if record.pending_deletions.is_empty() {
            Ok(CorrectionResult::Applied { prior, current })
        } else {
            Ok(CorrectionResult::Partial {
                prior,
                current,
                remaining: deletion_targets(&record.pending_deletions),
            })
        }
    }

    fn forget(
        &mut self,
        scope: &MemoryScope,
        id: &MemoryId,
        expected_version: u64,
    ) -> Result<ForgetResult, MemoryError> {
        let mut record = self.scoped_record(scope, id)?;
        if record.version != expected_version {
            return Err(MemoryError::conflict("stale memory version"));
        }
        let tombstone = Self::source(scope, id, record.version)?;
        if record.forgotten && record.pending_deletions.is_empty() && !record.unresolved_addition {
            return Ok(ForgetResult::AlreadyForgotten { tombstone });
        }
        if record.unresolved_addition {
            let known_ids = record
                .pending_deletions
                .iter()
                .chain(std::iter::once(&record.candidate_id))
                .filter(|candidate| !candidate.is_empty())
                .cloned()
                .collect::<BTreeSet<_>>();
            if let Ok(discovered) =
                self.discover_added_candidates(scope, &record.session_id, &known_ids)
            {
                record.pending_deletions.extend(discovered);
                record.unresolved_addition = false;
            }
        }
        record.forgotten = true;
        record.text.clear();
        record.pending_deletions = candidate_set(
            record
                .pending_deletions
                .iter()
                .chain(std::iter::once(&record.candidate_id)),
        );
        self.store
            .state
            .records
            .insert(id.as_str().to_owned(), record.clone());
        self.store.persist()?;
        record.pending_deletions =
            self.attempt_pending_deletions(scope, &record.session_id, &record.pending_deletions);
        self.store
            .state
            .records
            .insert(id.as_str().to_owned(), record.clone());
        self.store.persist()?;
        if record.pending_deletions.is_empty() && !record.unresolved_addition {
            Ok(ForgetResult::Forgotten { tombstone })
        } else {
            Ok(ForgetResult::Partial {
                tombstone,
                remaining: remaining_deletion_targets(&record),
            })
        }
    }

    fn health(&mut self) -> BackendHealth {
        let pending = self
            .store
            .state
            .records
            .values()
            .map(|record| record.pending_deletions.len() + usize::from(record.unresolved_addition))
            .sum::<usize>();
        if pending == 0 {
            BackendHealth::healthy(BACKEND_NAME)
        } else {
            BackendHealth::degraded(
                BACKEND_NAME,
                format!("{pending} candidate cleanup items remain"),
            )
        }
    }
}

fn candidate_set<'a>(candidates: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    candidates
        .into_iter()
        .filter(|candidate| !candidate.is_empty())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn remaining_deletion_targets(record: &Record) -> Vec<DeletionTarget> {
    let mut remaining = deletion_targets(&record.pending_deletions);
    if record.unresolved_addition {
        remaining.push(DeletionTarget::Candidate(format!(
            "unresolved-session:{}",
            record.session_id
        )));
    }
    remaining
}

fn deletion_targets(candidates: &[String]) -> Vec<DeletionTarget> {
    candidates
        .iter()
        .cloned()
        .map(DeletionTarget::Candidate)
        .collect()
}

fn validate_candidate_id(candidate_id: &str) -> Result<(), MemoryError> {
    if candidate_id.is_empty()
        || candidate_id.len() > 512
        || candidate_id.trim() != candidate_id
        || candidate_id.chars().any(char::is_control)
    {
        return Err(MemoryError::refused(
            "Tencent returned an invalid candidate ID",
        ));
    }
    Ok(())
}

fn scoped_body(scope: &WireScope, extra: Value) -> Value {
    let mut object = match serde_json::to_value(scope).expect("wire scope is serializable") {
        Value::Object(object) => object,
        _ => unreachable!(),
    };
    if let Value::Object(extra) = extra {
        object.extend(extra);
    }
    Value::Object(object)
}

fn checked_component(name: &str, value: String) -> Result<String, MemoryError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(MemoryError::malformed(format!("invalid Tencent {name}")));
    }
    Ok(value)
}

fn validate_loopback_url(mut url: Url) -> Result<Url, MemoryError> {
    if url.scheme() != "http"
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(MemoryError::refused(
            "Tencent base URL must be a plain loopback HTTP origin",
        ));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| MemoryError::malformed("Tencent base URL requires a port"))?;
    let addresses = match url.host() {
        Some(Host::Ipv4(address)) => vec![IpAddr::V4(address)],
        Some(Host::Ipv6(address)) => vec![IpAddr::V6(address)],
        Some(Host::Domain(domain)) => (domain, port)
            .to_socket_addrs()
            .map_err(|error| MemoryError::refused(format!("resolve Tencent host: {error}")))?
            .map(|address| address.ip())
            .collect(),
        None => return Err(MemoryError::malformed("Tencent base URL requires a host")),
    };
    if addresses.is_empty() || addresses.iter().any(|address| !address.is_loopback()) {
        return Err(MemoryError::refused(
            "Tencent base URL must resolve exclusively to loopback",
        ));
    }
    let chosen = addresses
        .iter()
        .find(|address| address.is_ipv4())
        .copied()
        .unwrap_or(addresses[0]);
    url.set_ip_host(chosen)
        .map_err(|_| MemoryError::malformed("invalid resolved Tencent host"))?;
    Ok(url)
}
