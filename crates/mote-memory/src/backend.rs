use crate::{
    BackendHealth, Correction, CorrectionResult, ForgetResult, MemoryError, MemoryHit, MemoryId,
    MemoryScope, MemoryWrite, SourceRef,
};

pub trait MemoryBackend: Send {
    fn write(&mut self, scope: &MemoryScope, write: MemoryWrite) -> Result<MemoryHit, MemoryError>;

    fn retrieve(
        &mut self,
        scope: &MemoryScope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryHit>, MemoryError>;

    fn source(
        &mut self,
        scope: &MemoryScope,
        id: &MemoryId,
    ) -> Result<Option<SourceRef>, MemoryError>;

    fn correct(
        &mut self,
        scope: &MemoryScope,
        correction: Correction,
    ) -> Result<CorrectionResult, MemoryError>;

    fn forget(
        &mut self,
        scope: &MemoryScope,
        id: &MemoryId,
        expected_version: u64,
    ) -> Result<ForgetResult, MemoryError>;

    fn health(&mut self) -> BackendHealth;
}

pub fn run_contract_suite(factory: impl Fn() -> Box<dyn MemoryBackend>) {
    let personal = MemoryScope::new("account-a", "personal").unwrap();
    let work = MemoryScope::new("account-a", "work").unwrap();
    let other_account = MemoryScope::new("account-b", "personal").unwrap();
    let mut backend = factory();

    assert_ne!(backend.health().status(), crate::BackendStatus::Unavailable);
    assert!(matches!(
        backend.retrieve(&personal, "", 5),
        Err(MemoryError::Malformed { .. })
    ));
    assert!(matches!(
        backend.retrieve(&personal, "scarlet", 0),
        Err(MemoryError::Malformed { .. })
    ));

    let written = backend
        .write(
            &personal,
            MemoryWrite::new("owner prefers scarlet tea").unwrap(),
        )
        .unwrap();
    assert_eq!(written.scope(), &personal);
    assert_eq!(written.source().scope(), &personal);
    assert_eq!(written.source().version(), 1);

    let hits = backend.retrieve(&personal, "scarlet", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id(), written.id());
    assert!(backend.retrieve(&work, "scarlet", 5).unwrap().is_empty());
    assert!(backend
        .retrieve(&other_account, "scarlet", 5)
        .unwrap()
        .is_empty());

    for wrong_scope in [&work, &other_account] {
        assert!(matches!(
            backend.source(wrong_scope, written.id()),
            Err(MemoryError::Refused { .. })
        ));
        assert!(matches!(
            backend.correct(
                wrong_scope,
                Correction::new(written.id().clone(), 1, "owner prefers amber tea").unwrap()
            ),
            Err(MemoryError::Refused { .. })
        ));
        assert!(matches!(
            backend.forget(wrong_scope, written.id(), 1),
            Err(MemoryError::Refused { .. })
        ));
    }

    let source = backend.source(&personal, written.id()).unwrap().unwrap();
    assert_eq!(source, *written.source());
    let correction = backend
        .correct(
            &personal,
            Correction::new(written.id().clone(), 1, "owner prefers amber tea").unwrap(),
        )
        .unwrap();
    assert!(!correction.is_partial());
    assert_eq!(correction.current().scope(), &personal);
    assert_eq!(correction.current().version(), 2);
    assert!(backend
        .retrieve(&personal, "scarlet", 5)
        .unwrap()
        .is_empty());
    assert_eq!(backend.retrieve(&personal, "amber", 5).unwrap().len(), 1);

    let forgotten = backend
        .forget(&personal, written.id(), correction.current().version())
        .unwrap();
    assert!(!forgotten.is_partial());
    assert_eq!(forgotten.tombstone().scope(), &personal);
    assert!(backend.retrieve(&personal, "amber", 5).unwrap().is_empty());
}
