use mote_memory::{
    run_contract_suite, AccountId, BackendHealth, BackendStatus, Correction, CorrectionResult,
    DeletionTarget, ForgetResult, MemoryBackend, MemoryError, MemoryHit, MemoryId, MemoryScope,
    MemoryWrite, Realm, SourceRef,
};
use std::collections::HashMap;

#[derive(Clone)]
struct Record {
    scope: MemoryScope,
    text: String,
    version: u64,
    forgotten: bool,
}

#[derive(Default)]
struct FakeBackend {
    records: HashMap<MemoryId, Record>,
    next_id: u64,
}

struct UnavailableBackend;

impl MemoryBackend for UnavailableBackend {
    fn write(
        &mut self,
        _scope: &MemoryScope,
        _write: MemoryWrite,
    ) -> Result<MemoryHit, MemoryError> {
        Err(MemoryError::unavailable("service is stopped"))
    }

    fn retrieve(
        &mut self,
        _scope: &MemoryScope,
        _query: &str,
        _limit: usize,
    ) -> Result<Vec<MemoryHit>, MemoryError> {
        Err(MemoryError::unavailable("service is stopped"))
    }

    fn source(
        &mut self,
        _scope: &MemoryScope,
        _id: &MemoryId,
    ) -> Result<Option<SourceRef>, MemoryError> {
        Err(MemoryError::unavailable("service is stopped"))
    }

    fn correct(
        &mut self,
        _scope: &MemoryScope,
        _correction: Correction,
    ) -> Result<CorrectionResult, MemoryError> {
        Err(MemoryError::unavailable("service is stopped"))
    }

    fn forget(
        &mut self,
        _scope: &MemoryScope,
        _id: &MemoryId,
        _expected_version: u64,
    ) -> Result<ForgetResult, MemoryError> {
        Err(MemoryError::unavailable("service is stopped"))
    }

    fn health(&mut self) -> BackendHealth {
        BackendHealth::unavailable("candidate", "service is stopped")
    }
}

impl FakeBackend {
    fn source(record: &Record, id: &MemoryId) -> SourceRef {
        SourceRef::new(record.scope.clone(), "fake", id.clone(), record.version).unwrap()
    }

    fn record(&self, scope: &MemoryScope, id: &MemoryId) -> Result<&Record, MemoryError> {
        let record = self
            .records
            .get(id)
            .ok_or_else(|| MemoryError::not_found("memory"))?;
        if &record.scope != scope {
            return Err(MemoryError::refused("memory scope mismatch"));
        }
        Ok(record)
    }
}

impl MemoryBackend for FakeBackend {
    fn write(&mut self, scope: &MemoryScope, write: MemoryWrite) -> Result<MemoryHit, MemoryError> {
        self.next_id += 1;
        let id = MemoryId::new(format!("m{}", self.next_id))?;
        let record = Record {
            scope: scope.clone(),
            text: write.text().to_owned(),
            version: 1,
            forgotten: false,
        };
        let source = Self::source(&record, &id);
        self.records.insert(id.clone(), record);
        MemoryHit::new(scope.clone(), id, source, write.into_text(), 1, 1.0)
    }

    fn retrieve(
        &mut self,
        scope: &MemoryScope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryHit>, MemoryError> {
        mote_memory::validate_query(query, limit)?;
        let mut hits = Vec::new();
        for (id, record) in &self.records {
            if &record.scope == scope && !record.forgotten && record.text.contains(query) {
                hits.push(MemoryHit::new(
                    scope.clone(),
                    id.clone(),
                    Self::source(record, id),
                    record.text.clone(),
                    hits.len() + 1,
                    1.0,
                )?);
            }
            if hits.len() == limit {
                break;
            }
        }
        Ok(hits)
    }

    fn source(
        &mut self,
        scope: &MemoryScope,
        id: &MemoryId,
    ) -> Result<Option<SourceRef>, MemoryError> {
        let record = self.record(scope, id)?;
        Ok((!record.forgotten).then(|| Self::source(record, id)))
    }

    fn correct(
        &mut self,
        scope: &MemoryScope,
        correction: Correction,
    ) -> Result<CorrectionResult, MemoryError> {
        let id = correction.id().clone();
        let record = self.record(scope, &id)?;
        if record.version != correction.expected_version() {
            return Err(MemoryError::conflict("stale memory version"));
        }
        let prior = Self::source(record, &id);
        let record = self.records.get_mut(&id).unwrap();
        record.text = correction.into_replacement();
        record.version += 1;
        record.forgotten = false;
        Ok(CorrectionResult::Applied {
            prior,
            current: Self::source(record, &id),
        })
    }

    fn forget(
        &mut self,
        scope: &MemoryScope,
        id: &MemoryId,
        expected_version: u64,
    ) -> Result<ForgetResult, MemoryError> {
        let record = self.record(scope, id)?;
        if record.version != expected_version {
            return Err(MemoryError::conflict("stale memory version"));
        }
        let tombstone = Self::source(record, id);
        self.records.get_mut(id).unwrap().forgotten = true;
        Ok(ForgetResult::Forgotten { tombstone })
    }

    fn health(&mut self) -> BackendHealth {
        BackendHealth::healthy("fake")
    }
}

#[test]
fn fake_backend_satisfies_common_contract() {
    run_contract_suite(|| Box::<FakeBackend>::default());
}

#[test]
fn validation_rejects_malformed_values() {
    assert!(matches!(
        AccountId::new(" bad"),
        Err(MemoryError::Malformed { .. })
    ));
    assert!(matches!(
        Realm::new("bad\nrealm"),
        Err(MemoryError::Malformed { .. })
    ));
    assert!(matches!(
        MemoryScope::new("", "personal"),
        Err(MemoryError::Malformed { .. })
    ));
    assert!(matches!(
        MemoryScope::new("account", "bad\nrealm"),
        Err(MemoryError::Malformed { .. })
    ));
    assert!(matches!(
        MemoryId::new("contains:separator"),
        Err(MemoryError::Malformed { .. })
    ));
    assert!(matches!(
        MemoryWrite::new("   "),
        Err(MemoryError::Malformed { .. })
    ));
    assert!(matches!(
        Correction::new(MemoryId::new("m1").unwrap(), 0, "new"),
        Err(MemoryError::Malformed { .. })
    ));
    assert!(mote_memory::validate_query("", 5).is_err());
    assert!(mote_memory::validate_query("query", 0).is_err());
    assert!(matches!(
        MemoryWrite::new("first line\0second line"),
        Err(MemoryError::Malformed { .. })
    ));
    assert!(matches!(
        Correction::new(MemoryId::new("m1").unwrap(), 1, "first line\0second line"),
        Err(MemoryError::Malformed { .. })
    ));
    assert!(mote_memory::validate_query("first line\0second line", 5).is_err());
}

#[test]
fn body_text_allows_internal_line_breaks_and_tabs() {
    let write = MemoryWrite::new("first line\nsecond\tcolumn\rthird line").unwrap();
    assert_eq!(write.text(), "first line\nsecond\tcolumn\rthird line");

    let correction = Correction::new(
        MemoryId::new("m1").unwrap(),
        1,
        "replacement line\nsecond\tcolumn",
    )
    .unwrap();
    assert_eq!(correction.replacement(), "replacement line\nsecond\tcolumn");
    assert!(mote_memory::validate_query("first line\nsecond\tcolumn", 5).is_ok());
}

#[test]
fn unavailable_and_partial_outcomes_are_explicit() {
    let scope = MemoryScope::new("account", "personal").unwrap();
    let mut unavailable: Box<dyn MemoryBackend> = Box::new(UnavailableBackend);
    let health = unavailable.health();
    assert_eq!(health.status(), BackendStatus::Unavailable);
    assert!(matches!(
        unavailable.retrieve(&scope, "query", 5),
        Err(MemoryError::Unavailable { .. })
    ));

    let id = MemoryId::new("m1").unwrap();
    let prior = SourceRef::new(scope.clone(), "candidate", id.clone(), 1).unwrap();
    let current = SourceRef::new(scope, "candidate", id.clone(), 2).unwrap();
    let correction = CorrectionResult::Partial {
        prior: prior.clone(),
        current,
        remaining: vec![DeletionTarget::Embedding],
    };
    assert!(correction.is_partial());
    assert_eq!(correction.remaining(), &[DeletionTarget::Embedding]);
    let forget = ForgetResult::Partial {
        tombstone: prior,
        remaining: vec![DeletionTarget::Candidate("remote-record".into())],
    };
    assert!(forget.is_partial());
    assert_eq!(
        forget.remaining(),
        &[DeletionTarget::Candidate("remote-record".into())]
    );
}
