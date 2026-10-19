use serde::Serialize;
use std::error::Error;
use std::fmt;

const MAX_SCOPE_COMPONENT_BYTES: usize = 256;
const MAX_OPAQUE_ID_BYTES: usize = 256;
const MAX_TEXT_BYTES: usize = 1024 * 1024;
const MAX_QUERY_BYTES: usize = 16 * 1024;
const MAX_RETRIEVAL_LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct AccountId(String);

impl AccountId {
    pub fn new(value: impl Into<String>) -> Result<Self, MemoryError> {
        let value = value.into();
        validate_text_component("account_id", &value, MAX_SCOPE_COMPONENT_BYTES)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Realm(String);

impl Realm {
    pub fn new(value: impl Into<String>) -> Result<Self, MemoryError> {
        let value = value.into();
        validate_text_component("realm", &value, MAX_SCOPE_COMPONENT_BYTES)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Realm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct MemoryScope {
    account_id: AccountId,
    realm: Realm,
}

impl MemoryScope {
    pub fn new(
        account_id: impl Into<String>,
        realm: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let account_id = AccountId::new(account_id)?;
        let realm = Realm::new(realm)?;
        Ok(Self { account_id, realm })
    }

    pub fn from_parts(account_id: AccountId, realm: Realm) -> Self {
        Self { account_id, realm }
    }

    pub fn account(&self) -> &AccountId {
        &self.account_id
    }

    pub fn realm_id(&self) -> &Realm {
        &self.realm
    }

    pub fn account_id(&self) -> &str {
        self.account_id.as_str()
    }

    pub fn realm(&self) -> &str {
        self.realm.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct MemoryId(String);

impl MemoryId {
    pub fn new(value: impl Into<String>) -> Result<Self, MemoryError> {
        let value = value.into();
        validate_opaque_component("memory_id", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MemoryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceRef {
    scope: MemoryScope,
    source_id: String,
    backend: String,
    memory_id: MemoryId,
    version: u64,
}

impl SourceRef {
    pub fn new(
        scope: MemoryScope,
        backend: impl Into<String>,
        memory_id: MemoryId,
        version: u64,
    ) -> Result<Self, MemoryError> {
        let backend = backend.into();
        validate_opaque_component("backend", &backend)?;
        if version == 0 {
            return Err(MemoryError::malformed("source version must be positive"));
        }
        let source_id = format!("memory:{backend}:{memory_id}:{version}");
        Ok(Self {
            scope,
            source_id,
            backend,
            memory_id,
            version,
        })
    }

    pub fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    pub fn backend(&self) -> &str {
        &self.backend
    }

    pub fn memory_id(&self) -> &MemoryId {
        &self.memory_id
    }

    pub fn version(&self) -> u64 {
        self.version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryWrite {
    text: String,
    kind: String,
    authoritative_source: Option<String>,
}

impl MemoryWrite {
    pub fn new(text: impl Into<String>) -> Result<Self, MemoryError> {
        let text = text.into();
        validate_body("memory text", &text, MAX_TEXT_BYTES)?;
        Ok(Self {
            text,
            kind: "fact".to_owned(),
            authoritative_source: None,
        })
    }

    pub fn with_kind(mut self, kind: impl Into<String>) -> Result<Self, MemoryError> {
        let kind = kind.into();
        validate_opaque_component("memory kind", &kind)?;
        self.kind = kind;
        Ok(self)
    }

    pub fn with_authoritative_source(
        mut self,
        source: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let source = source.into();
        validate_text_component("authoritative source", &source, MAX_OPAQUE_ID_BYTES)?;
        self.authoritative_source = Some(source);
        Ok(self)
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn authoritative_source(&self) -> Option<&str> {
        self.authoritative_source.as_deref()
    }

    pub fn into_text(self) -> String {
        self.text
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemoryHit {
    scope: MemoryScope,
    id: MemoryId,
    source: SourceRef,
    text: String,
    rank: usize,
    score: f32,
}

impl MemoryHit {
    pub fn new(
        scope: MemoryScope,
        id: MemoryId,
        source: SourceRef,
        text: impl Into<String>,
        rank: usize,
        score: f32,
    ) -> Result<Self, MemoryError> {
        let text = text.into();
        validate_body("memory hit text", &text, MAX_TEXT_BYTES)?;
        if source.scope() != &scope || source.memory_id() != &id {
            return Err(MemoryError::refused(
                "memory hit source scope or id mismatch",
            ));
        }
        if rank == 0 {
            return Err(MemoryError::malformed("memory hit rank must be positive"));
        }
        if !score.is_finite() {
            return Err(MemoryError::malformed("memory hit score must be finite"));
        }
        Ok(Self {
            scope,
            id,
            source,
            text,
            rank,
            score,
        })
    }

    pub fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    pub fn id(&self) -> &MemoryId {
        &self.id
    }

    pub fn source(&self) -> &SourceRef {
        &self.source
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn rank(&self) -> usize {
        self.rank
    }

    pub fn score(&self) -> f32 {
        self.score
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Correction {
    id: MemoryId,
    expected_version: u64,
    replacement: String,
}

impl Correction {
    pub fn new(
        id: MemoryId,
        expected_version: u64,
        replacement: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        if expected_version == 0 {
            return Err(MemoryError::malformed("expected version must be positive"));
        }
        let replacement = replacement.into();
        validate_body("replacement text", &replacement, MAX_TEXT_BYTES)?;
        Ok(Self {
            id,
            expected_version,
            replacement,
        })
    }

    pub fn id(&self) -> &MemoryId {
        &self.id
    }

    pub fn expected_version(&self) -> u64 {
        self.expected_version
    }

    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    pub fn into_replacement(self) -> String {
        self.replacement
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeletionTarget {
    SearchText,
    Embedding,
    GraphEdges,
    Candidate(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CorrectionResult {
    Applied {
        prior: SourceRef,
        current: SourceRef,
    },
    Partial {
        prior: SourceRef,
        current: SourceRef,
        remaining: Vec<DeletionTarget>,
    },
}

impl CorrectionResult {
    pub fn prior(&self) -> &SourceRef {
        match self {
            Self::Applied { prior, .. } | Self::Partial { prior, .. } => prior,
        }
    }

    pub fn current(&self) -> &SourceRef {
        match self {
            Self::Applied { current, .. } | Self::Partial { current, .. } => current,
        }
    }

    pub fn is_partial(&self) -> bool {
        matches!(self, Self::Partial { .. })
    }

    pub fn remaining(&self) -> &[DeletionTarget] {
        match self {
            Self::Applied { .. } => &[],
            Self::Partial { remaining, .. } => remaining,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ForgetResult {
    Forgotten {
        tombstone: SourceRef,
    },
    AlreadyForgotten {
        tombstone: SourceRef,
    },
    Partial {
        tombstone: SourceRef,
        remaining: Vec<DeletionTarget>,
    },
}

impl ForgetResult {
    pub fn tombstone(&self) -> &SourceRef {
        match self {
            Self::Forgotten { tombstone }
            | Self::AlreadyForgotten { tombstone }
            | Self::Partial { tombstone, .. } => tombstone,
        }
    }

    pub fn is_partial(&self) -> bool {
        matches!(self, Self::Partial { .. })
    }

    pub fn remaining(&self) -> &[DeletionTarget] {
        match self {
            Self::Forgotten { .. } | Self::AlreadyForgotten { .. } => &[],
            Self::Partial { remaining, .. } => remaining,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendStatus {
    Healthy,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackendHealth {
    backend: String,
    status: BackendStatus,
    detail: Option<String>,
}

impl BackendHealth {
    pub fn healthy(backend: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            status: BackendStatus::Healthy,
            detail: None,
        }
    }

    pub fn degraded(backend: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            status: BackendStatus::Degraded,
            detail: Some(detail.into()),
        }
    }

    pub fn unavailable(backend: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            backend: backend.into(),
            status: BackendStatus::Unavailable,
            detail: Some(detail.into()),
        }
    }

    pub fn backend(&self) -> &str {
        &self.backend
    }

    pub fn status(&self) -> BackendStatus {
        self.status
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum MemoryError {
    Malformed { detail: String },
    Refused { detail: String },
    Unavailable { detail: String },
    NotFound { detail: String },
    Conflict { detail: String },
    Backend { detail: String },
}

impl MemoryError {
    pub fn malformed(detail: impl Into<String>) -> Self {
        Self::Malformed {
            detail: detail.into(),
        }
    }

    pub fn refused(detail: impl Into<String>) -> Self {
        Self::Refused {
            detail: detail.into(),
        }
    }

    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self::Unavailable {
            detail: detail.into(),
        }
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::NotFound {
            detail: detail.into(),
        }
    }

    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::Conflict {
            detail: detail.into(),
        }
    }

    pub fn backend(detail: impl Into<String>) -> Self {
        Self::Backend {
            detail: detail.into(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Malformed { .. } => "malformed",
            Self::Refused { .. } => "refused",
            Self::Unavailable { .. } => "unavailable",
            Self::NotFound { .. } => "not_found",
            Self::Conflict { .. } => "conflict",
            Self::Backend { .. } => "backend",
        }
    }
}

impl fmt::Display for MemoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let detail = match self {
            Self::Malformed { detail }
            | Self::Refused { detail }
            | Self::Unavailable { detail }
            | Self::NotFound { detail }
            | Self::Conflict { detail }
            | Self::Backend { detail } => detail,
        };
        write!(formatter, "{}: {detail}", self.kind())
    }
}

impl Error for MemoryError {}

pub fn validate_query(query: &str, limit: usize) -> Result<(), MemoryError> {
    validate_body("memory query", query, MAX_QUERY_BYTES)?;
    if !(1..=MAX_RETRIEVAL_LIMIT).contains(&limit) {
        return Err(MemoryError::malformed(format!(
            "retrieval limit must be between 1 and {MAX_RETRIEVAL_LIMIT}"
        )));
    }
    Ok(())
}

fn validate_body(name: &str, value: &str, max_bytes: usize) -> Result<(), MemoryError> {
    if value.is_empty() {
        return Err(MemoryError::malformed(format!("{name} must not be empty")));
    }
    if value.len() > max_bytes {
        return Err(MemoryError::malformed(format!(
            "{name} exceeds {max_bytes} bytes"
        )));
    }
    if value.trim().is_empty() {
        return Err(MemoryError::malformed(format!("{name} must not be blank")));
    }
    if value.trim() != value {
        return Err(MemoryError::malformed(format!(
            "{name} has surrounding whitespace"
        )));
    }
    if value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(MemoryError::malformed(format!(
            "{name} has an unsupported control character"
        )));
    }
    Ok(())
}

fn validate_text_component(name: &str, value: &str, max_bytes: usize) -> Result<(), MemoryError> {
    if value.is_empty() {
        return Err(MemoryError::malformed(format!("{name} must not be empty")));
    }
    if value.len() > max_bytes {
        return Err(MemoryError::malformed(format!(
            "{name} exceeds {max_bytes} bytes"
        )));
    }
    if value.trim() != value || value.chars().any(char::is_control) {
        return Err(MemoryError::malformed(format!(
            "{name} has surrounding whitespace or control characters"
        )));
    }
    Ok(())
}

fn validate_opaque_component(name: &str, value: &str) -> Result<(), MemoryError> {
    validate_text_component(name, value, MAX_OPAQUE_ID_BYTES)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(MemoryError::malformed(format!(
            "{name} contains an unsupported character"
        )));
    }
    Ok(())
}
