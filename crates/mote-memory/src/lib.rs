//! Backend-neutral, account-scoped and realm-scoped durable memory contract.

mod backend;
mod types;

pub use backend::{run_contract_suite, MemoryBackend};
pub use types::{
    validate_query, AccountId, BackendHealth, BackendStatus, Correction, CorrectionResult,
    DeletionTarget, ForgetResult, MemoryError, MemoryHit, MemoryId, MemoryScope, MemoryWrite,
    Realm, SourceRef,
};
