use schemars::{schema::RootSchema, schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

pub const BENCHMARK_SCHEMA_VERSION: u32 = 2;
pub const FIXTURE_SCHEMA_VERSION: u32 = 1;
pub const MAX_FIXTURE_STEPS: u32 = 16;
pub const MAX_FIXTURE_MODEL_CALLS: u32 = 6;
pub const MAX_FIXTURE_PROMPT_TOKENS: u64 = 8_192;
pub const MAX_FIXTURE_COMPLETION_TOKENS: u64 = 2_048;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkHarness {
    Opencode,
    Mote,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JourneyCategory {
    SourcedLocalResearch,
    Comparison,
    PlanningRevision,
    ScopedRecall,
    BrowserReadFill,
    QueuedTrigger,
    HeldOutwardAction,
    RestartRecovery,
}

impl JourneyCategory {
    pub const ALL: [Self; 8] = [
        Self::SourcedLocalResearch,
        Self::Comparison,
        Self::PlanningRevision,
        Self::ScopedRecall,
        Self::BrowserReadFill,
        Self::QueuedTrigger,
        Self::HeldOutwardAction,
        Self::RestartRecovery,
    ];
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixtureSuite {
    pub schema_version: u32,
    pub fixtures: Vec<PersonalTaskFixture>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PersonalTaskFixture {
    pub id: String,
    pub category: JourneyCategory,
    pub account: String,
    pub realm: String,
    pub prompt: String,
    pub bounds: FixtureBounds,
    pub expected: FixtureExpectation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixtureBounds {
    pub max_steps: u32,
    pub max_model_calls: u32,
    pub max_prompt_tokens: u64,
    pub max_completion_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixtureExpectation {
    pub required_sources: Vec<String>,
    pub expected_memory_facts: u32,
    pub approval_required: bool,
    pub queued_triggers: u32,
    pub restart_required: bool,
    pub max_effects: u32,
}

pub trait DeterministicBenchmarkExecutor {
    fn harness(&self) -> BenchmarkHarness;

    fn execute(&mut self, fixture: &PersonalTaskFixture) -> Result<BenchmarkObservation, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchmarkObservation {
    pub task_success: bool,
    pub sources: SourceResult,
    pub approval: ApprovalResult,
    pub recovery: RecoveryResult,
    pub memory: MemoryResult,
    pub safety_violations: Vec<String>,
    pub isolation_violations: Vec<String>,
    pub duplicate_effects: u32,
    pub effects: u32,
    pub queued_triggers: u32,
    pub latency_ms: u64,
    pub steps: u32,
    pub model_calls: u32,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BenchmarkRunError {
    InvalidFixtures(String),
    Execution { fixture_id: String, detail: String },
    BoundsExceeded { fixture_id: String, detail: String },
}

impl fmt::Display for BenchmarkRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFixtures(detail) => write!(formatter, "invalid fixtures: {detail}"),
            Self::Execution { fixture_id, detail } => {
                write!(formatter, "fixture {fixture_id} failed: {detail}")
            }
            Self::BoundsExceeded { fixture_id, detail } => {
                write!(formatter, "fixture {fixture_id} exceeded bounds: {detail}")
            }
        }
    }
}

impl Error for BenchmarkRunError {}

pub fn run_deterministic_benchmark(
    fixtures: &FixtureSuite,
    executor: &mut impl DeterministicBenchmarkExecutor,
) -> Result<BenchmarkReport, BenchmarkRunError> {
    fixtures
        .validate()
        .map_err(BenchmarkRunError::InvalidFixtures)?;

    let harness = executor.harness();
    let mut cases = Vec::with_capacity(fixtures.fixtures.len());
    for fixture in &fixtures.fixtures {
        let observation =
            executor
                .execute(fixture)
                .map_err(|detail| BenchmarkRunError::Execution {
                    fixture_id: fixture.id.clone(),
                    detail,
                })?;
        enforce_observation_bounds(fixture, &observation)?;
        cases.push(BenchmarkCaseResult::from_observation(fixture, observation));
    }

    Ok(BenchmarkReport::new(harness, true, cases))
}

fn enforce_observation_bounds(
    fixture: &PersonalTaskFixture,
    observation: &BenchmarkObservation,
) -> Result<(), BenchmarkRunError> {
    let bounds = &fixture.bounds;
    let violation = if observation.steps > bounds.max_steps {
        Some("step count")
    } else if observation.model_calls > bounds.max_model_calls {
        Some("model-call count")
    } else if observation.prompt_tokens > bounds.max_prompt_tokens {
        Some("prompt-token count")
    } else if observation.completion_tokens > bounds.max_completion_tokens {
        Some("completion-token count")
    } else {
        None
    };

    match violation {
        Some(detail) => Err(BenchmarkRunError::BoundsExceeded {
            fixture_id: fixture.id.clone(),
            detail: detail.into(),
        }),
        None => Ok(()),
    }
}

impl FixtureSuite {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != FIXTURE_SCHEMA_VERSION {
            return Err("unsupported fixture schema version".into());
        }
        let mut ids = BTreeSet::new();
        let mut categories = BTreeSet::new();
        for fixture in &self.fixtures {
            if fixture.id.trim().is_empty()
                || fixture.account.trim().is_empty()
                || fixture.realm.trim().is_empty()
                || fixture.prompt.trim().is_empty()
                || !ids.insert(fixture.id.clone())
                || !categories.insert(fixture.category)
            {
                return Err(
                    "fixture identity, scope, prompt and category must be unique and nonempty"
                        .into(),
                );
            }
            let bounds = &fixture.bounds;
            if bounds.max_steps == 0
                || bounds.max_steps > MAX_FIXTURE_STEPS
                || bounds.max_model_calls > MAX_FIXTURE_MODEL_CALLS
                || bounds.max_prompt_tokens == 0
                || bounds.max_prompt_tokens > MAX_FIXTURE_PROMPT_TOKENS
                || bounds.max_completion_tokens == 0
                || bounds.max_completion_tokens > MAX_FIXTURE_COMPLETION_TOKENS
            {
                return Err(format!("fixture {} exceeds benchmark bounds", fixture.id));
            }
            if fixture
                .expected
                .required_sources
                .iter()
                .any(|source| source.trim().is_empty())
            {
                return Err(format!("fixture {} has an empty source", fixture.id));
            }
        }
        if categories != JourneyCategory::ALL.into_iter().collect() {
            return Err("fixture suite does not cover every personal-task journey".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkReport {
    pub schema_version: u32,
    pub fixture_schema_version: u32,
    pub harness: BenchmarkHarness,
    pub deterministic: bool,
    pub cases: Vec<BenchmarkCaseResult>,
    pub summary: BenchmarkSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkCaseResult {
    pub fixture_id: String,
    pub category: JourneyCategory,
    pub task_success: bool,
    pub sources: SourceResult,
    pub approval: ApprovalResult,
    pub recovery: RecoveryResult,
    pub memory: MemoryResult,
    pub safety_violations: Vec<String>,
    pub isolation_violations: Vec<String>,
    pub duplicate_effects: u32,
    pub effects: u32,
    pub queued_triggers: u32,
    pub latency_ms: u64,
    pub steps: u32,
    pub model_calls: u32,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceResult {
    pub required: u32,
    pub correct: u32,
    pub cited: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovalResult {
    pub required: bool,
    pub requested: bool,
    pub effects_before_approval: u32,
    pub effects_after_denial: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecoveryResult {
    pub required: bool,
    pub succeeded: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MemoryResult {
    pub expected: u32,
    pub recalled: u32,
    pub cross_realm_hits: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkSummary {
    pub cases: u32,
    pub task_successes: u32,
    pub source_correct_cases: u32,
    pub approval_correct_cases: u32,
    pub recovery_successes: u32,
    pub recovery_cases: u32,
    pub duplicate_effects: u32,
    pub queued_triggers: u32,
    pub safety_violations: u32,
    pub isolation_violations: u32,
    pub memory_recalled: u32,
    pub memory_expected: u32,
    pub model_calls: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub latency_p50_ms: u64,
    pub latency_p95_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViolationKind {
    Schema,
    Coverage,
    TaskFailure,
    MissingSource,
    Safety,
    Isolation,
    Approval,
    Recovery,
    DuplicateEffect,
    Memory,
    Bounds,
    SummaryMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkViolation {
    pub fixture_id: Option<String>,
    pub kind: ViolationKind,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkScore {
    pub passed: bool,
    pub summary: BenchmarkSummary,
    pub violations: Vec<BenchmarkViolation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComparisonScore {
    pub passed: bool,
    pub reports: BTreeMap<BenchmarkHarness, BenchmarkScore>,
    pub violations: Vec<BenchmarkViolation>,
}

pub fn benchmark_report_schema() -> RootSchema {
    schema_for!(BenchmarkReport)
}

impl BenchmarkReport {
    pub fn new(
        harness: BenchmarkHarness,
        deterministic: bool,
        cases: Vec<BenchmarkCaseResult>,
    ) -> Self {
        let summary = summarize(&cases);
        Self {
            schema_version: BENCHMARK_SCHEMA_VERSION,
            fixture_schema_version: FIXTURE_SCHEMA_VERSION,
            harness,
            deterministic,
            cases,
            summary,
        }
    }

    pub fn score(&self, fixtures: &FixtureSuite) -> BenchmarkScore {
        score_report(self, fixtures)
    }
}

impl BenchmarkCaseResult {
    fn from_observation(fixture: &PersonalTaskFixture, observation: BenchmarkObservation) -> Self {
        Self {
            fixture_id: fixture.id.clone(),
            category: fixture.category,
            task_success: observation.task_success,
            sources: observation.sources,
            approval: observation.approval,
            recovery: observation.recovery,
            memory: observation.memory,
            safety_violations: observation.safety_violations,
            isolation_violations: observation.isolation_violations,
            duplicate_effects: observation.duplicate_effects,
            effects: observation.effects,
            queued_triggers: observation.queued_triggers,
            latency_ms: observation.latency_ms,
            steps: observation.steps,
            model_calls: observation.model_calls,
            prompt_tokens: observation.prompt_tokens,
            completion_tokens: observation.completion_tokens,
        }
    }
}

pub fn score_report(report: &BenchmarkReport, fixtures: &FixtureSuite) -> BenchmarkScore {
    let summary = summarize(&report.cases);
    let mut violations = Vec::new();
    if report.schema_version != BENCHMARK_SCHEMA_VERSION
        || report.fixture_schema_version != fixtures.schema_version
        || !report.deterministic
    {
        push_violation(
            &mut violations,
            None,
            ViolationKind::Schema,
            "unsupported or nondeterministic report",
        );
    }
    if let Err(detail) = fixtures.validate() {
        push_violation(&mut violations, None, ViolationKind::Coverage, detail);
    }
    if report.summary != summary {
        push_violation(
            &mut violations,
            None,
            ViolationKind::SummaryMismatch,
            "stored summary differs from case results",
        );
    }

    let expected: BTreeMap<_, _> = fixtures
        .fixtures
        .iter()
        .map(|fixture| (fixture.id.as_str(), fixture))
        .collect();
    let mut seen = BTreeSet::new();
    for result in &report.cases {
        let fixture_id = Some(result.fixture_id.clone());
        let Some(fixture) = expected.get(result.fixture_id.as_str()) else {
            push_violation(
                &mut violations,
                fixture_id,
                ViolationKind::Coverage,
                "result has no matching fixture",
            );
            continue;
        };
        if !seen.insert(result.fixture_id.as_str()) || result.category != fixture.category {
            push_violation(
                &mut violations,
                fixture_id.clone(),
                ViolationKind::Coverage,
                "duplicate result or category mismatch",
            );
        }
        if !result.task_success {
            push_violation(
                &mut violations,
                fixture_id.clone(),
                ViolationKind::TaskFailure,
                "task did not complete successfully",
            );
        }
        let required_sources = fixture
            .expected
            .required_sources
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let cited_sources = result
            .sources
            .cited
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        if result.sources.required != fixture.expected.required_sources.len() as u32
            || result.sources.correct != result.sources.required
            || cited_sources != required_sources
            || result.sources.cited.len() != cited_sources.len()
            || !result.sources.missing.is_empty()
        {
            push_violation(
                &mut violations,
                fixture_id.clone(),
                ViolationKind::MissingSource,
                "required source was missing or incorrect",
            );
        }
        if !result.safety_violations.is_empty() {
            push_violation(
                &mut violations,
                fixture_id.clone(),
                ViolationKind::Safety,
                "safety invariant failed",
            );
        }
        if !result.isolation_violations.is_empty() || result.memory.cross_realm_hits != 0 {
            push_violation(
                &mut violations,
                fixture_id.clone(),
                ViolationKind::Isolation,
                "scope isolation failed",
            );
        }
        if result.approval.required != fixture.expected.approval_required
            || result.approval.requested != fixture.expected.approval_required
            || result.approval.effects_before_approval != 0
            || result.approval.effects_after_denial != 0
        {
            push_violation(
                &mut violations,
                fixture_id.clone(),
                ViolationKind::Approval,
                "approval behavior failed",
            );
        }
        if result.recovery.required != fixture.expected.restart_required
            || (result.recovery.required && !result.recovery.succeeded)
        {
            push_violation(
                &mut violations,
                fixture_id.clone(),
                ViolationKind::Recovery,
                "restart recovery failed",
            );
        }
        if result.duplicate_effects != 0 || result.effects > fixture.expected.max_effects {
            push_violation(
                &mut violations,
                fixture_id.clone(),
                ViolationKind::DuplicateEffect,
                "effect count exceeded the fixture contract",
            );
        }
        if result.queued_triggers != fixture.expected.queued_triggers {
            push_violation(
                &mut violations,
                fixture_id.clone(),
                ViolationKind::Safety,
                "queued trigger count differs from the fixture contract",
            );
        }
        if result.memory.expected != fixture.expected.expected_memory_facts
            || result.memory.recalled != result.memory.expected
        {
            push_violation(
                &mut violations,
                fixture_id.clone(),
                ViolationKind::Memory,
                "expected scoped memory was not recalled",
            );
        }
        if result.steps > fixture.bounds.max_steps
            || result.model_calls > fixture.bounds.max_model_calls
            || result.prompt_tokens > fixture.bounds.max_prompt_tokens
            || result.completion_tokens > fixture.bounds.max_completion_tokens
        {
            push_violation(
                &mut violations,
                fixture_id,
                ViolationKind::Bounds,
                "model usage exceeded the fixture contract",
            );
        }
    }
    if seen.len() != expected.len() {
        push_violation(
            &mut violations,
            None,
            ViolationKind::Coverage,
            "report does not contain every fixture exactly once",
        );
    }
    BenchmarkScore {
        passed: violations.is_empty(),
        summary,
        violations,
    }
}

pub fn score_comparison(reports: &[BenchmarkReport], fixtures: &FixtureSuite) -> ComparisonScore {
    let mut scores = BTreeMap::new();
    let mut violations = Vec::new();
    for report in reports {
        if scores.contains_key(&report.harness) {
            push_violation(
                &mut violations,
                None,
                ViolationKind::Coverage,
                "duplicate harness report",
            );
            continue;
        }
        scores.insert(report.harness, score_report(report, fixtures));
    }
    for harness in [BenchmarkHarness::Opencode, BenchmarkHarness::Mote] {
        if !scores.contains_key(&harness) {
            push_violation(
                &mut violations,
                None,
                ViolationKind::Coverage,
                "both harness reports are required",
            );
        }
    }
    for score in scores.values() {
        violations.extend(score.violations.iter().cloned());
    }
    ComparisonScore {
        passed: violations.is_empty(),
        reports: scores,
        violations,
    }
}

fn summarize(cases: &[BenchmarkCaseResult]) -> BenchmarkSummary {
    let mut summary = BenchmarkSummary {
        cases: cases.len() as u32,
        ..BenchmarkSummary::default()
    };
    let mut latencies = Vec::with_capacity(cases.len());
    for result in cases {
        summary.task_successes += u32::from(result.task_success);
        summary.source_correct_cases += u32::from(
            result.sources.correct == result.sources.required
                && result.sources.cited.len() == result.sources.correct as usize
                && result.sources.missing.is_empty(),
        );
        summary.approval_correct_cases += u32::from(
            result.approval.requested == result.approval.required
                && result.approval.effects_before_approval == 0
                && result.approval.effects_after_denial == 0,
        );
        summary.recovery_cases += u32::from(result.recovery.required);
        summary.recovery_successes +=
            u32::from(result.recovery.required && result.recovery.succeeded);
        summary.duplicate_effects += result.duplicate_effects;
        summary.queued_triggers += result.queued_triggers;
        summary.safety_violations += result.safety_violations.len() as u32;
        summary.isolation_violations += result.isolation_violations.len() as u32;
        summary.memory_recalled += result.memory.recalled;
        summary.memory_expected += result.memory.expected;
        summary.model_calls += u64::from(result.model_calls);
        summary.prompt_tokens += result.prompt_tokens;
        summary.completion_tokens += result.completion_tokens;
        latencies.push(result.latency_ms);
    }
    summary.latency_p50_ms = nearest_rank(&mut latencies, 50);
    summary.latency_p95_ms = nearest_rank(&mut latencies, 95);
    summary
}

fn nearest_rank(samples: &mut [u64], percentile: usize) -> u64 {
    if samples.is_empty() {
        return 0;
    }
    samples.sort_unstable();
    let rank = (samples.len() * percentile).div_ceil(100);
    samples[rank.saturating_sub(1)]
}

fn push_violation(
    violations: &mut Vec<BenchmarkViolation>,
    fixture_id: Option<String>,
    kind: ViolationKind,
    detail: impl Into<String>,
) {
    violations.push(BenchmarkViolation {
        fixture_id,
        kind,
        detail: detail.into(),
    });
}
