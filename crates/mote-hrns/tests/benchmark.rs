use mote_hrns::{
    score_comparison, ApprovalResult, BenchmarkCaseResult, BenchmarkHarness, BenchmarkReport,
    FixtureSuite, JourneyCategory, MemoryResult, RecoveryResult, SourceResult, ViolationKind,
    BENCHMARK_SCHEMA_VERSION, FIXTURE_SCHEMA_VERSION,
};

fn fixtures() -> FixtureSuite {
    serde_json::from_str(include_str!("fixtures/personal_tasks.json")).unwrap()
}

fn passing_cases(fixtures: &FixtureSuite) -> Vec<BenchmarkCaseResult> {
    fixtures
        .fixtures
        .iter()
        .enumerate()
        .map(|(index, fixture)| BenchmarkCaseResult {
            fixture_id: fixture.id.clone(),
            category: fixture.category,
            task_success: true,
            sources: SourceResult {
                required: fixture.expected.required_sources.len() as u32,
                correct: fixture.expected.required_sources.len() as u32,
                cited: fixture.expected.required_sources.clone(),
                missing: Vec::new(),
            },
            approval: ApprovalResult {
                required: fixture.expected.approval_required,
                requested: fixture.expected.approval_required,
                effects_before_approval: 0,
                effects_after_denial: 0,
            },
            recovery: RecoveryResult {
                required: fixture.expected.restart_required,
                succeeded: fixture.expected.restart_required,
            },
            memory: MemoryResult {
                expected: fixture.expected.expected_memory_facts,
                recalled: fixture.expected.expected_memory_facts,
                cross_realm_hits: 0,
            },
            safety_violations: Vec::new(),
            isolation_violations: Vec::new(),
            duplicate_effects: 0,
            effects: fixture.expected.max_effects,
            queued_triggers: fixture.expected.queued_triggers,
            latency_ms: (index as u64 + 1) * 10,
            model_calls: fixture.bounds.max_model_calls.min(2),
            prompt_tokens: 512,
            completion_tokens: 128,
        })
        .collect()
}

fn passing_report(harness: BenchmarkHarness) -> BenchmarkReport {
    let fixtures = fixtures();
    BenchmarkReport::new(harness, true, passing_cases(&fixtures))
}

#[test]
fn fixture_suite_is_strict_bounded_and_covers_every_journey() {
    let suite = fixtures();
    suite.validate().unwrap();
    assert_eq!(suite.schema_version, FIXTURE_SCHEMA_VERSION);
    assert_eq!(suite.fixtures.len(), JourneyCategory::ALL.len());

    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/personal_tasks.json")).unwrap();
    value["fixtures"][0]["unknown"] = serde_json::json!(true);
    assert!(serde_json::from_value::<FixtureSuite>(value).is_err());
}

#[test]
fn report_schema_is_strict_and_summary_is_recomputed() {
    let report = passing_report(BenchmarkHarness::Opencode);
    assert_eq!(report.schema_version, BENCHMARK_SCHEMA_VERSION);
    assert_eq!(report.summary.cases, 8);
    assert_eq!(report.summary.latency_p50_ms, 40);
    assert_eq!(report.summary.latency_p95_ms, 80);

    let encoded = serde_json::to_value(&report).unwrap();
    assert_eq!(
        serde_json::from_value::<BenchmarkReport>(encoded.clone()).unwrap(),
        report
    );
    let mut extra = encoded;
    extra["untrusted"] = serde_json::json!(true);
    assert!(serde_json::from_value::<BenchmarkReport>(extra).is_err());

    let schema = mote_hrns::benchmark_report_schema();
    assert!(serde_json::to_value(schema).unwrap().is_object());
}

#[test]
fn both_harnesses_pass_the_same_complete_deterministic_suite() {
    let suite = fixtures();
    let opencode = passing_report(BenchmarkHarness::Opencode);
    let mote = passing_report(BenchmarkHarness::Mote);

    assert!(opencode.score(&suite).passed);
    assert!(mote.score(&suite).passed);
    let comparison = score_comparison(&[opencode, mote], &suite);
    assert!(comparison.passed, "{:?}", comparison.violations);
    assert_eq!(comparison.reports.len(), 2);
}

#[test]
fn safety_isolation_approval_duplicates_and_sources_fail_closed() {
    let suite = fixtures();
    type CaseMutation = Box<dyn Fn(&mut BenchmarkCaseResult)>;
    let mutations: Vec<(ViolationKind, CaseMutation)> = vec![
        (
            ViolationKind::Safety,
            Box::new(|case| case.safety_violations.push("unsafe dispatch".into())),
        ),
        (
            ViolationKind::Isolation,
            Box::new(|case| case.memory.cross_realm_hits = 1),
        ),
        (
            ViolationKind::Approval,
            Box::new(|case| case.approval.effects_before_approval = 1),
        ),
        (
            ViolationKind::DuplicateEffect,
            Box::new(|case| case.duplicate_effects = 1),
        ),
        (
            ViolationKind::MissingSource,
            Box::new(|case| {
                case.sources.correct = case.sources.correct.saturating_sub(1);
                case.sources.cited.clear();
                case.sources.missing.push("local://missing".into());
            }),
        ),
    ];

    for (expected, mutate) in mutations {
        let mut report = passing_report(BenchmarkHarness::Mote);
        let index = if expected == ViolationKind::MissingSource {
            0
        } else {
            6
        };
        mutate(&mut report.cases[index]);
        report.summary =
            BenchmarkReport::new(report.harness, report.deterministic, report.cases.clone())
                .summary;
        let score = report.score(&suite);
        assert!(!score.passed);
        assert!(score
            .violations
            .iter()
            .any(|violation| violation.kind == expected));
    }
}

#[test]
fn recovery_memory_bounds_coverage_and_summary_tampering_fail() {
    let suite = fixtures();

    let mut recovery = passing_report(BenchmarkHarness::Mote);
    let case = recovery
        .cases
        .iter_mut()
        .find(|case| case.category == JourneyCategory::RestartRecovery)
        .unwrap();
    case.recovery.succeeded = false;
    recovery.summary = BenchmarkReport::new(recovery.harness, true, recovery.cases.clone()).summary;
    assert!(recovery
        .score(&suite)
        .violations
        .iter()
        .any(|violation| violation.kind == ViolationKind::Recovery));

    let mut memory = passing_report(BenchmarkHarness::Mote);
    memory
        .cases
        .iter_mut()
        .find(|case| case.category == JourneyCategory::ScopedRecall)
        .unwrap()
        .memory
        .recalled = 0;
    memory.summary = BenchmarkReport::new(memory.harness, true, memory.cases.clone()).summary;
    assert!(memory
        .score(&suite)
        .violations
        .iter()
        .any(|violation| violation.kind == ViolationKind::Memory));

    let mut bounds = passing_report(BenchmarkHarness::Mote);
    bounds.cases[0].model_calls = 99;
    bounds.summary = BenchmarkReport::new(bounds.harness, true, bounds.cases.clone()).summary;
    assert!(bounds
        .score(&suite)
        .violations
        .iter()
        .any(|violation| violation.kind == ViolationKind::Bounds));

    let mut missing = passing_report(BenchmarkHarness::Mote);
    missing.cases.pop();
    missing.summary = BenchmarkReport::new(missing.harness, true, missing.cases.clone()).summary;
    assert!(missing
        .score(&suite)
        .violations
        .iter()
        .any(|violation| violation.kind == ViolationKind::Coverage));

    let mut tampered = passing_report(BenchmarkHarness::Mote);
    tampered.summary.task_successes = 0;
    assert!(tampered
        .score(&suite)
        .violations
        .iter()
        .any(|violation| violation.kind == ViolationKind::SummaryMismatch));
}

#[test]
fn comparison_requires_one_report_from_each_harness() {
    let suite = fixtures();
    let report = passing_report(BenchmarkHarness::Opencode);
    assert!(!score_comparison(std::slice::from_ref(&report), &suite).passed);
    assert!(!score_comparison(&[report.clone(), report], &suite).passed);
}
