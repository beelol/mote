//! Benchmarks: `moted bench talk|router|memory|surface|compiler`. The latency benchmarks report
//! p50 and p95.
//! The surface benchmark reports its fixed-corpus validity gate. Each appends one JSON line to
//! `<data dir>/bench.jsonl`. See docs/rfcs/v0.md section 17.

use crate::{inferred_ui, lanes, matrix, pipeline};
use matrix_lang::{lower_v0, lower_v1, parse_surface, resolve};
use mote_context::{hash_text, ContextStore};
use mote_eventlog::EventLog;
use mote_localmodel::Ollama;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

/// Default run count.
pub const RUNS: usize = 20;

const COMPILER_SOURCE: &str = include_str!("../../matrix/examples/mote-dashboard.matrix");
const COMPILER_WARMUPS: usize = 200;
const COMPILER_SAMPLES: usize = 2_000;

struct CompilerFixture {
    source: &'static str,
    source_hash: String,
    combined_hash: String,
    config: BTreeMap<String, Value>,
    pipeline: pipeline::PipelineSnapshot,
    host: matrix::HostSnapshot,
}

impl CompilerFixture {
    fn bindings(&self) -> Result<matrix::HostBindings, Vec<matrix_lang::Diagnostic>> {
        matrix::host_bindings(&self.config, &self.pipeline, &self.host)
    }
}

fn compiler_fixture() -> Result<CompilerFixture, String> {
    let config = BTreeMap::from([
        ("models.local_only".to_string(), json!(true)),
        ("models.talk".to_string(), json!("qwen3:4b")),
        ("models.router".to_string(), json!("qwen3:0.6b")),
        ("models.think".to_string(), json!("qwen3-coder:30b-64k")),
        ("budgets.talk_first_token".to_string(), json!(300)),
        ("budgets.router".to_string(), json!(150)),
        ("budgets.think_first_token".to_string(), json!(3000)),
        ("packs.talk".to_string(), json!(600)),
        ("packs.think".to_string(), json!(8000)),
    ]);
    let numbers = [
        (
            "talk",
            11,
            "first_token",
            10,
            Some(120),
            Some(210),
            730,
            190,
            Some(300),
        ),
        (
            "router",
            13,
            "total",
            13,
            Some(21),
            Some(45),
            320,
            80,
            Some(150),
        ),
        (
            "think",
            3,
            "first_token",
            3,
            Some(940),
            Some(1700),
            2400,
            510,
            Some(3000),
        ),
        ("memory", 7, "unavailable", 0, None, None, 0, 0, None),
        ("ledger", 34, "unavailable", 0, None, None, 0, 0, None),
    ]
    .into_iter()
    .map(
        |(stage, calls, latency_metric, latency_samples, p50, p95, prompt, completion, budget)| {
            pipeline::StageNumbers {
                stage: stage.to_string(),
                calls,
                latency_metric: latency_metric.to_string(),
                latency_samples,
                latency_p50_ms: p50,
                latency_p95_ms: p95,
                prompt_tokens: prompt,
                completion_tokens: completion,
                cost_usd: 0.0,
                budget_ms: budget,
            }
        },
    )
    .collect();
    let pipeline = pipeline::PipelineSnapshot {
        generated_at: 0,
        event_limit: pipeline::PIPELINE_EVENT_LIMIT,
        request_limit: pipeline::PIPELINE_REQUEST_LIMIT,
        window_ms: 3_600_000,
        config: config.clone(),
        events: Vec::new(),
        numbers,
        requests: Vec::new(),
    };
    let canonical = serde_json::to_string(&json!({
        "source": COMPILER_SOURCE,
        "config": config,
        "pipeline": pipeline,
        "host": {"progress": 0.5, "items": ["first", "second"]},
    }))
    .map_err(|error| error.to_string())?;
    Ok(CompilerFixture {
        source: COMPILER_SOURCE,
        source_hash: hash_text(COMPILER_SOURCE),
        combined_hash: hash_text(&canonical),
        config,
        pipeline,
        host: matrix::HostSnapshot::ready(0.5, ["first", "second"]),
    })
}

fn nearest_rank(samples: &[u64], percentile: usize) -> u64 {
    assert!(!samples.is_empty(), "percentile needs at least one sample");
    assert!(
        (1..=100).contains(&percentile),
        "percentile must be 1 through 100"
    );
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = (percentile * sorted.len()).div_ceil(100);
    sorted[rank - 1]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum CompilerBudget {
    Passed { limit_ns: u64 },
    Failed { limit_ns: u64 },
    Unsupported,
}

fn compiler_budget(os: &str, arch: &str, hardware: &str, p95_ns: u64) -> CompilerBudget {
    let apple_m_series = hardware
        .strip_prefix("Apple M")
        .and_then(|suffix| suffix.chars().next())
        .is_some_and(|generation| generation.is_ascii_digit());
    let limit_ns = if os == "macos" && arch == "aarch64" && apple_m_series {
        Some(2_000_000)
    } else if os == "linux"
        && arch == "aarch64"
        && hardware.to_ascii_lowercase().contains("raspberry pi 5")
    {
        Some(10_000_000)
    } else {
        None
    };
    match limit_ns {
        Some(limit_ns) if p95_ns < limit_ns => CompilerBudget::Passed { limit_ns },
        Some(limit_ns) => CompilerBudget::Failed { limit_ns },
        None => CompilerBudget::Unsupported,
    }
}

#[derive(Debug, Serialize)]
pub struct CompilerReport {
    name: &'static str,
    revision: String,
    profile: &'static str,
    rustc_version: String,
    cargo_version: String,
    os: &'static str,
    arch: &'static str,
    hardware: String,
    source_hash: String,
    combined_hash: String,
    manifest_version: u64,
    warmups: usize,
    samples: usize,
    percentile_method: &'static str,
    parse_p95_ns: u64,
    parse_p95_ms: f64,
    resolve_check_p95_ns: u64,
    resolve_check_p95_ms: f64,
    consumer_contract: u32,
    lower_v1_p95_ns: u64,
    lower_v1_p95_ms: f64,
    end_to_end_p95_ns: u64,
    end_to_end_p95_ms: f64,
    budget: CompilerBudget,
}

impl CompilerReport {
    pub fn line(&self) -> String {
        serde_json::to_string(self).expect("compiler report serializes")
    }

    pub fn target_failed(&self) -> bool {
        matches!(self.budget, CompilerBudget::Failed { .. })
    }
}

fn command_version(program: &str) -> String {
    Command::new(program)
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn hardware_name() -> String {
    #[cfg(target_os = "macos")]
    {
        return Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "unknown macOS arm64 hardware".to_string());
    }
    #[cfg(target_os = "linux")]
    {
        return std::fs::read("/proc/device-tree/model")
            .ok()
            .map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .trim_matches(char::from(0))
                    .trim()
                    .to_string()
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "unknown Linux hardware".to_string());
    }
    #[allow(unreachable_code)]
    "unknown hardware".to_string()
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn compiler_pass(fixture: &CompilerFixture, bindings: &matrix::HostBindings) -> Result<(), String> {
    let parsed = parse_surface(std::hint::black_box(fixture.source))
        .map_err(|errors| matrix::diagnostics(&errors).to_string())?;
    let resolved = resolve(
        std::hint::black_box(&parsed),
        std::hint::black_box(&bindings.manifest),
        std::hint::black_box(&bindings.snapshot),
    )
    .map_err(|errors| matrix::diagnostics(&errors).to_string())?;
    let lowered = lower_v1(std::hint::black_box(&resolved))
        .map_err(|errors| matrix::diagnostics(&errors).to_string())?;
    std::hint::black_box(lowered);
    Ok(())
}

pub fn run_compiler(data_dir: &Path) -> Result<CompilerReport, String> {
    if cfg!(debug_assertions) {
        return Err("moted bench compiler requires a release build".to_string());
    }
    let fixture = compiler_fixture()?;
    let bindings = fixture
        .bindings()
        .map_err(|errors| matrix::diagnostics(&errors).to_string())?;
    for _ in 0..COMPILER_WARMUPS {
        compiler_pass(&fixture, &bindings)?;
        std::hint::black_box(
            matrix::compile_contract(
                std::hint::black_box(fixture.source),
                std::hint::black_box(&bindings),
            )
            .map_err(|errors| matrix::diagnostics(&errors).to_string())?,
        );
    }

    let parsed =
        parse_surface(fixture.source).map_err(|errors| matrix::diagnostics(&errors).to_string())?;
    let resolved = resolve(&parsed, &bindings.manifest, &bindings.snapshot)
        .map_err(|errors| matrix::diagnostics(&errors).to_string())?;
    let mut parse_samples = Vec::with_capacity(COMPILER_SAMPLES);
    let mut resolve_samples = Vec::with_capacity(COMPILER_SAMPLES);
    let mut lower_samples = Vec::with_capacity(COMPILER_SAMPLES);
    let mut end_to_end_samples = Vec::with_capacity(COMPILER_SAMPLES);
    for _ in 0..COMPILER_SAMPLES {
        let start = Instant::now();
        std::hint::black_box(
            parse_surface(std::hint::black_box(fixture.source))
                .map_err(|errors| matrix::diagnostics(&errors).to_string())?,
        );
        parse_samples.push(elapsed_ns(start));

        let start = Instant::now();
        std::hint::black_box(
            resolve(
                std::hint::black_box(&parsed),
                std::hint::black_box(&bindings.manifest),
                std::hint::black_box(&bindings.snapshot),
            )
            .map_err(|errors| matrix::diagnostics(&errors).to_string())?,
        );
        resolve_samples.push(elapsed_ns(start));

        let start = Instant::now();
        std::hint::black_box(
            lower_v1(std::hint::black_box(&resolved))
                .map_err(|errors| matrix::diagnostics(&errors).to_string())?,
        );
        lower_samples.push(elapsed_ns(start));

        let start = Instant::now();
        std::hint::black_box(
            matrix::compile_contract(
                std::hint::black_box(fixture.source),
                std::hint::black_box(&bindings),
            )
            .map_err(|errors| matrix::diagnostics(&errors).to_string())?,
        );
        end_to_end_samples.push(elapsed_ns(start));
    }

    let parse_p95_ns = nearest_rank(&parse_samples, 95);
    let resolve_check_p95_ns = nearest_rank(&resolve_samples, 95);
    let lower_v1_p95_ns = nearest_rank(&lower_samples, 95);
    let end_to_end_p95_ns = nearest_rank(&end_to_end_samples, 95);
    let hardware = hardware_name();
    let report = CompilerReport {
        name: "compiler",
        revision: revision(),
        profile: "release",
        rustc_version: command_version("rustc"),
        cargo_version: command_version("cargo"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        hardware: hardware.clone(),
        source_hash: fixture.source_hash,
        combined_hash: fixture.combined_hash,
        manifest_version: u64::from(bindings.manifest.version),
        warmups: COMPILER_WARMUPS,
        samples: COMPILER_SAMPLES,
        percentile_method: "nearest_rank",
        parse_p95_ns,
        parse_p95_ms: parse_p95_ns as f64 / 1_000_000.0,
        resolve_check_p95_ns,
        resolve_check_p95_ms: resolve_check_p95_ns as f64 / 1_000_000.0,
        consumer_contract: 1,
        lower_v1_p95_ns,
        lower_v1_p95_ms: lower_v1_p95_ns as f64 / 1_000_000.0,
        end_to_end_p95_ns,
        end_to_end_p95_ms: end_to_end_p95_ns as f64 / 1_000_000.0,
        budget: compiler_budget(
            std::env::consts::OS,
            std::env::consts::ARCH,
            &hardware,
            end_to_end_p95_ns,
        ),
    };
    let path = data_dir.join("bench.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("could not append {}: {error}", path.display()))?;
    writeln!(file, "{}", report.line())
        .map_err(|error| format!("could not append {}: {error}", path.display()))?;
    Ok(report)
}

#[derive(Clone, Debug)]
struct SurfaceCase {
    category: String,
    request: String,
    expects_surface: bool,
}

impl SurfaceCase {
    fn new(category: &str, request: &str, expects_surface: bool) -> Self {
        Self {
            category: category.to_string(),
            request: request.to_string(),
            expects_surface,
        }
    }
}

fn surface_corpus() -> Vec<SurfaceCase> {
    const SUBJECTS: [&str; 20] = [
        "agent runtime",
        "local models",
        "request pipeline",
        "memory index",
        "tool policy",
        "daily plan",
        "build status",
        "test suite",
        "privacy settings",
        "context budget",
        "task queue",
        "release checks",
        "conversation state",
        "latency report",
        "local services",
        "project notes",
        "workspace health",
        "model routing",
        "event history",
        "approval queue",
    ];
    let mut cases = Vec::with_capacity(200);
    for (index, subject) in SUBJECTS.into_iter().enumerate() {
        cases.push(SurfaceCase::new(
            "none",
            &format!("Explain the {subject} in two plain sentences."),
            false,
        ));
        if index >= 18 {
            continue;
        }
        cases.push(SurfaceCase::new(
            "action",
            &format!("Show a compact {subject} panel with a refresh button."),
            true,
        ));
        cases.push(SurfaceCase::new(
            "field",
            &format!("Show a compact {subject} form with the local-only field."),
            true,
        ));
        cases.push(SurfaceCase::new(
            "list",
            &format!("Show a compact {subject} list using the available host items."),
            true,
        ));
        cases.push(SurfaceCase::new(
            "progress",
            &format!("Show compact progress for the {subject} using host progress."),
            true,
        ));
        cases.push(SurfaceCase::new(
            "row",
            &format!("Show two compact {subject} labels side by side in one row."),
            true,
        ));
        cases.push(SurfaceCase::new(
            "select",
            &format!("Show a compact {subject} control for selecting the talk model."),
            true,
        ));
        cases.push(SurfaceCase::new(
            "stack",
            &format!("Show a compact vertical stack of three {subject} labels."),
            true,
        ));
        cases.push(SurfaceCase::new(
            "table",
            &format!("Show a compact table of pipeline stages for the {subject}."),
            true,
        ));
        cases.push(SurfaceCase::new(
            "text",
            &format!("Show a compact surface with the literal text {subject}."),
            true,
        ));
        cases.push(SurfaceCase::new(
            "toggle",
            &format!("Show a compact {subject} toggle for local-only models."),
            true,
        ));
    }
    cases
}

fn surface_corpus_hash(cases: &[SurfaceCase]) -> String {
    let canonical = cases
        .iter()
        .map(|case| {
            format!(
                "{}\t{}\t{}",
                case.category, case.expects_surface, case.request
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    hash_text(&canonical)
}

pub(crate) fn current_surface_corpus_hash() -> String {
    surface_corpus_hash(&surface_corpus())
}

fn surface_category_counts(cases: &[SurfaceCase]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for case in cases {
        *counts.entry(case.category.clone()).or_default() += 1;
    }
    counts
}

#[derive(Clone, Debug)]
struct AttemptTrace {
    envelope_valid: bool,
    matrix_decision: bool,
    matrix_parsed: bool,
    host_resolved: bool,
    v0_lowered: bool,
    none: bool,
    diagnostics: String,
    rejected_source: String,
}

impl AttemptTrace {
    fn envelope_failure() -> Self {
        Self {
            envelope_valid: false,
            matrix_decision: false,
            matrix_parsed: false,
            host_resolved: false,
            v0_lowered: false,
            none: false,
            diagnostics: String::new(),
            rejected_source: String::new(),
        }
    }

    fn none() -> Self {
        Self {
            envelope_valid: true,
            none: true,
            ..Self::envelope_failure()
        }
    }

    #[cfg(test)]
    fn parsed_but_unresolved() -> Self {
        Self {
            envelope_valid: true,
            matrix_decision: true,
            matrix_parsed: true,
            host_resolved: false,
            v0_lowered: false,
            none: false,
            diagnostics: "host resolution failed".to_string(),
            rejected_source: "screen { text absent }".to_string(),
        }
    }

    fn valid_matrix() -> Self {
        Self {
            envelope_valid: true,
            matrix_decision: true,
            matrix_parsed: true,
            host_resolved: true,
            v0_lowered: true,
            none: false,
            diagnostics: String::new(),
            rejected_source: String::new(),
        }
    }

    fn valid_for(&self, case: &SurfaceCase) -> bool {
        if case.expects_surface {
            self.v0_lowered
        } else {
            self.none
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
struct SurfaceMetrics {
    samples: usize,
    attempts: usize,
    envelope_valid: usize,
    matrix_parsed: usize,
    host_resolved: usize,
    v0_lowered: usize,
    first_attempt_valid: usize,
    after_retry_valid: usize,
    retries: usize,
    first_attempt_rate: f64,
    after_retry_rate: f64,
    first_try_by_category: BTreeMap<String, usize>,
    terminal_failures: BTreeMap<String, usize>,
    rejected_sources: BTreeMap<String, String>,
}

impl SurfaceMetrics {
    fn from_traces(cases: &[SurfaceCase], traces: &[Vec<AttemptTrace>]) -> Self {
        assert_eq!(cases.len(), traces.len(), "one attempt trace per corpus case");
        let mut metrics = Self {
            samples: cases.len(),
            ..Self::default()
        };
        for (case, attempts) in cases.iter().zip(traces) {
            let valid = attempts.first().is_some_and(|attempt| attempt.valid_for(case));
            *metrics.first_try_by_category.entry(case.category.clone()).or_default() +=
                usize::from(valid);
            if let Some(last) = attempts.last().filter(|attempt| !attempt.valid_for(case)) {
                if !last.rejected_source.is_empty() {
                    metrics.rejected_sources.entry(case.category.clone())
                        .or_insert_with(|| last.rejected_source.clone());
                }
                let failure = if !last.envelope_valid {
                    "invalid_envelope".to_string()
                } else if last.none {
                    format!("{}: unexpected_none", case.category)
                } else if !case.expects_surface && last.v0_lowered {
                    "unexpected_matrix".to_string()
                } else {
                    format!("{}: {}", case.category, mote_redact::redact(&last.diagnostics))
                };
                *metrics.terminal_failures.entry(failure).or_default() += 1;
            }
            metrics.attempts += attempts.len();
            metrics.retries += attempts.len().saturating_sub(1);
            for attempt in attempts {
                metrics.envelope_valid += usize::from(attempt.envelope_valid);
                metrics.matrix_parsed += usize::from(attempt.matrix_parsed);
                metrics.host_resolved += usize::from(attempt.host_resolved);
                metrics.v0_lowered += usize::from(attempt.v0_lowered);
            }
            metrics.first_attempt_valid += usize::from(
                attempts
                    .first()
                    .is_some_and(|attempt| attempt.valid_for(case)),
            );
            metrics.after_retry_valid += usize::from(
                attempts
                    .last()
                    .is_some_and(|attempt| attempt.valid_for(case)),
            );
        }
        if metrics.samples > 0 {
            metrics.first_attempt_rate =
                metrics.first_attempt_valid as f64 / metrics.samples as f64;
            metrics.after_retry_rate = metrics.after_retry_valid as f64 / metrics.samples as f64;
        }
        metrics
    }
}

fn surface_activation(evidence: Option<(usize, usize)>) -> bool {
    evidence.is_some_and(|(valid, samples)| samples > 0 && valid * 100 >= samples * 95)
}

fn default_surface_config() -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("models.local_only".to_string(), json!(true)),
        ("models.talk".to_string(), json!("qwen3:4b")),
        ("models.router".to_string(), json!("qwen3:0.6b")),
        ("models.think".to_string(), json!("qwen3-coder:30b-64k")),
    ])
}

fn surface_bindings() -> Result<matrix::HostBindings, String> {
    let config = default_surface_config();
    let snapshot = pipeline::PipelineSnapshot {
        generated_at: 0,
        event_limit: pipeline::PIPELINE_EVENT_LIMIT,
        request_limit: pipeline::PIPELINE_REQUEST_LIMIT,
        window_ms: 3_600_000,
        config: config.clone(),
        events: Vec::new(),
        numbers: Vec::new(),
        requests: Vec::new(),
    };
    matrix::host_bindings(
        &config,
        &snapshot,
        &matrix::HostSnapshot::ready(0.5, ["first", "second"]),
    )
    .map_err(|errors| matrix::diagnostics(&errors).to_string())
}

fn evaluate_decision(
    decision: Result<inferred_ui::SurfaceDecision, String>,
    bindings: &matrix::HostBindings,
) -> AttemptTrace {
    let Ok(decision) = decision else {
        return AttemptTrace::envelope_failure();
    };
    let inferred_ui::SurfaceDecision::Matrix { source } = decision else {
        return AttemptTrace::none();
    };
    let parsed = match parse_surface(&source) {
        Ok(parsed) => parsed,
        Err(errors) => {
            return AttemptTrace {
                envelope_valid: true,
                matrix_decision: true,
                diagnostics: matrix::diagnostics(&errors).to_string(),
                rejected_source: mote_redact::redact(&source),
                ..AttemptTrace::envelope_failure()
            };
        }
    };
    let resolved = match resolve(&parsed, &bindings.manifest, &bindings.snapshot) {
        Ok(resolved) => resolved,
        Err(errors) => {
            return AttemptTrace {
                envelope_valid: true,
                matrix_decision: true,
                matrix_parsed: true,
                diagnostics: matrix::diagnostics(&errors).to_string(),
                rejected_source: mote_redact::redact(&source),
                ..AttemptTrace::envelope_failure()
            };
        }
    };
    match lower_v0(&resolved) {
        Ok(_) => AttemptTrace::valid_matrix(),
        Err(errors) => AttemptTrace {
            envelope_valid: true,
            matrix_decision: true,
            matrix_parsed: true,
            host_resolved: true,
            diagnostics: matrix::diagnostics(&errors).to_string(),
            rejected_source: mote_redact::redact(&source),
            ..AttemptTrace::envelope_failure()
        },
    }
}

fn bench_surface_cases(
    cases: &[SurfaceCase],
    log: &EventLog,
    store: &ContextStore,
    client: &Ollama,
) -> Result<SurfaceMetrics, String> {
    let bindings = surface_bindings()?;
    let mut traces = Vec::with_capacity(cases.len());
    for case in cases {
        let first_inputs = inferred_ui::SurfaceInputs {
            request: &case.request,
            prose: "The requested response is complete.",
            previous_source: "",
            diagnostics: "",
        };
        let first = evaluate_decision(
            inferred_ui::generate_surface(log, store, client, &first_inputs),
            &bindings,
        );
        let retry = first.matrix_decision && !first.v0_lowered;
        let mut attempts = vec![first];
        if retry {
            let retry_inputs = inferred_ui::SurfaceInputs {
                request: &case.request,
                prose: "The requested response is complete.",
                previous_source: "",
                diagnostics: &attempts[0].diagnostics,
            };
            attempts.push(evaluate_decision(
                inferred_ui::generate_surface(log, store, client, &retry_inputs),
                &bindings,
            ));
        }
        traces.push(attempts);
    }
    Ok(SurfaceMetrics::from_traces(cases, &traces))
}

#[derive(Debug, Serialize)]
struct SurfaceReport {
    name: &'static str,
    revision: String,
    model: &'static str,
    model_digest: Option<String>,
    compiler_contract_version: u64,
    corpus_hash: String,
    category_counts: BTreeMap<String, usize>,
    status: &'static str,
    reason: Option<String>,
    activation_passed: bool,
    metrics: SurfaceMetrics,
}

fn revision() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|revision| revision.trim().to_string())
        .filter(|revision| !revision.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn unavailable_surface_report(cases: &[SurfaceCase], reason: String) -> SurfaceReport {
    SurfaceReport {
        name: "surface",
        revision: revision(),
        model: inferred_ui::SURFACE_MODEL,
        model_digest: None,
        compiler_contract_version: super::surface_activation::COMPILER_CONTRACT_VERSION,
        corpus_hash: surface_corpus_hash(cases),
        category_counts: surface_category_counts(cases),
        status: "unavailable",
        reason: Some(reason),
        activation_passed: false,
        metrics: SurfaceMetrics {
            samples: cases.len(),
            ..SurfaceMetrics::default()
        },
    }
}

fn persist_surface_gate(data_dir: &Path, report: &mut SurfaceReport) -> std::io::Result<()> {
    let valid = if report.status == "complete" && report.reason.is_none() {
        report.metrics.first_attempt_valid
    } else {
        0
    };
    report.activation_passed = super::surface_activation::SurfaceActivationStore::new(data_dir)
        .record(
            report.model_digest.as_deref().unwrap_or(""),
            &report.corpus_hash,
            valid as u64,
            report.metrics.samples as u64,
        )?;
    Ok(())
}

fn run_surface(log: &EventLog, store: &ContextStore, client: &Ollama) -> SurfaceReport {
    let cases = surface_corpus();
    if std::env::var("MOTE_LIVE").as_deref() != Ok("1") {
        return unavailable_surface_report(&cases, "MOTE_LIVE=1 is required".to_string());
    }
    let models = match client.list_models() {
        Ok(models) => models,
        Err(error) => {
            return unavailable_surface_report(
                &cases,
                format!("could not list local Ollama models: {error}"),
            );
        }
    };
    if !models
        .iter()
        .any(|(name, _)| name == inferred_ui::SURFACE_MODEL)
    {
        return unavailable_surface_report(
            &cases,
            format!(
                "approved local model {} is not installed",
                inferred_ui::SURFACE_MODEL
            ),
        );
    }
    let digest = match client.model_digest(inferred_ui::SURFACE_MODEL) {
        Ok(Some(digest)) if !digest.is_empty() => digest,
        _ => return unavailable_surface_report(&cases, "model digest unavailable".to_string()),
    };
    match bench_surface_cases(&cases, log, store, client) {
        Ok(metrics) => {
            let stable_digest = matches!(client.model_digest(inferred_ui::SURFACE_MODEL), Ok(Some(current)) if current == digest);
            SurfaceReport {
                name: "surface",
                revision: revision(),
                model: inferred_ui::SURFACE_MODEL,
                model_digest: Some(digest),
                compiler_contract_version: super::surface_activation::COMPILER_CONTRACT_VERSION,
                corpus_hash: surface_corpus_hash(&cases),
                category_counts: surface_category_counts(&cases),
                status: "complete",
                reason: if stable_digest {
                    None
                } else {
                    Some("model digest changed during benchmark".to_string())
                },
                activation_passed: stable_digest && surface_activation(Some((
                    metrics.first_attempt_valid,
                    metrics.samples,
                ))),
                metrics,
            }
        },
        Err(error) => unavailable_surface_report(&cases, error),
    }
}

fn append_surface_report(path: &Path, report: &SurfaceReport) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(
        file,
        "{}",
        serde_json::to_string(report).expect("surface report serializes")
    )
}

/// A benchmark result: p50/p95 of the primary metric, plus any extra metrics.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BenchResult {
    pub name: String,
    pub runs: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub extra: Value,
}

impl BenchResult {
    pub fn to_line(&self) -> Value {
        json!({
            "name": self.name,
            "runs": self.runs,
            "p50_ms": self.p50_ms,
            "p95_ms": self.p95_ms,
            "extra": self.extra,
        })
    }
}

fn percentile(samples: &[f64], p: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut s = samples.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let rank = (p / 100.0) * (s.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        s[lo]
    } else {
        s[lo] + (rank - lo as f64) * (s[hi] - s[lo])
    }
}

/// Append one benchmark line to `bench.jsonl`.
pub fn append_bench_line(path: &Path, result: &BenchResult) -> std::io::Result<()> {
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", result.to_line())
}

/// Benchmark talk-lane first-token latency.
pub fn bench_talk(
    log: &EventLog,
    store: &ContextStore,
    client: &Ollama,
    model: &str,
    num_ctx: u32,
    runs: usize,
) -> Result<BenchResult, String> {
    let mut firsts = Vec::new();
    for _ in 0..runs {
        let inputs = lanes::TalkInputs {
            charter: "Reply in one short sentence.",
            profile: "",
            memories: "",
            working_set: "",
            turn: "say hello",
        };
        let outcome = lanes::run_talk(
            log, store, client, "bench.talk", model, num_ctx, &inputs, lanes::TALK_BUDGET,
            &std::sync::atomic::AtomicBool::new(false),
        );
        let lanes::TalkOutcome::Done { ledger_id, .. } = outcome else {
            return Err(format!("talk benchmark sample failed: {outcome:?}"));
        };
        let row = store.ledger_since(0).map_err(|error| error.to_string())?
            .into_iter().find(|row| row.id == ledger_id)
            .ok_or_else(|| "talk benchmark ledger row missing".to_string())?;
        if row.latency_kind != "first_token" || row.status != "ok" {
            return Err("talk benchmark sample has no successful first token".into());
        }
        firsts.push(row.latency_ms as f64);
    }
    Ok(BenchResult {
        name: "talk".into(),
        runs: firsts.len(),
        p50_ms: percentile(&firsts, 50.0),
        p95_ms: percentile(&firsts, 95.0),
        extra: json!({ "metric": "first_token_ms", "model": model,
            "samples_ms": firsts, "measurement": "live_talk_lane",
            "resolution_ms": 1, "num_ctx": num_ctx }),
    })
}

/// Benchmark router latency (full structured verdict).
pub fn bench_router(client: &Ollama, model: &str, num_ctx: u32, runs: usize) -> BenchResult {
    let mut times = Vec::new();
    for _ in 0..runs {
        let start = Instant::now();
        let _ = lanes::route(client, model, num_ctx, "hey there");
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    BenchResult {
        name: "router".into(),
        runs: times.len(),
        p50_ms: percentile(&times, 50.0),
        p95_ms: percentile(&times, 95.0),
        extra: json!({ "metric": "verdict_ms", "model": model }),
    }
}

/// Benchmark memory write and retrieve at a given node scale. Reports retrieve p50/p95 as the
/// primary metric and write p50/p95 plus the scale in `extra`.
pub fn bench_memory_scale(scale: usize, probes: usize) -> BenchResult {
    let store = ContextStore::open_in_memory().expect("store");
    let words = ["alpha", "beta", "gamma", "cat", "mat", "sky", "blue", "plan", "note", "owner"];
    let mut writes = Vec::with_capacity(scale);
    for i in 0..scale {
        let text = format!("memory {i} about {} and {}", words[i % 10], words[(i / 10) % 10]);
        let start = Instant::now();
        store.memory_write("event", &text, None).ok();
        writes.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let mut retrieves = Vec::with_capacity(probes);
    for i in 0..probes {
        let q = format!("{} and {}", words[i % 10], words[(i + 3) % 10]);
        let start = Instant::now();
        let _ = store.memory_retrieve(&q, 8);
        retrieves.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    BenchResult {
        name: "memory".into(),
        runs: scale,
        p50_ms: percentile(&retrieves, 50.0),
        p95_ms: percentile(&retrieves, 95.0),
        extra: json!({
            "metric": "retrieve_ms",
            "scale": scale,
            "write_p50_ms": percentile(&writes, 50.0),
            "write_p95_ms": percentile(&writes, 95.0),
        }),
    }
}

/// Run a named benchmark with live dependencies and append the result line.
pub fn run_named(
    name: &str,
    data_dir: &Path,
    client: &Ollama,
    config: &crate::config::ConfigStore,
    log: &EventLog,
    store: &ContextStore,
) {
    let bench_path = data_dir.join("bench.jsonl");
    let result = match name {
        "surface" => {
            let mut report = run_surface(log, store, client);
            if let Err(error) = persist_surface_gate(data_dir, &mut report) {
                report.activation_passed = false;
                report.reason = Some(format!("could not persist activation gate: {error}"));
            }
            let line = serde_json::to_string(&report).expect("surface report serializes");
            if let Err(error) = append_surface_report(&bench_path, &report) {
                eprintln!("moted bench surface: could not append result: {error}");
            }
            println!("{line}");
            return;
        }
        "talk" => {
            let model = config.get_json("models.talk").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            match bench_talk(log, store, client, &model, 32768, RUNS) {
                Ok(result) => result,
                Err(error) => {
                    eprintln!("moted bench talk: {error}");
                    return;
                }
            }
        }
        "router" => {
            let model = config.get_json("models.router").and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            bench_router(client, &model, 32768, RUNS)
        }
        "memory" => {
            // Scale bench at 10k and 100k nodes; one line per scale.
            for scale in [10_000usize, 100_000] {
                let r = bench_memory_scale(scale, 30);
                let _ = append_bench_line(&bench_path, &r);
                println!(
                    "bench memory @ {} nodes: retrieve p50 {:.2} ms, p95 {:.2} ms; write p50 {:.3} ms",
                    scale,
                    r.p50_ms,
                    r.p95_ms,
                    r.extra["write_p50_ms"].as_f64().unwrap_or(0.0)
                );
            }
            return;
        }
        other => {
            eprintln!("moted bench: unknown benchmark '{other}' (talk|router|memory|surface)");
            return;
        }
    };
    let _ = append_bench_line(&bench_path, &result);
    println!(
        "bench {}: p50 {:.1} ms, p95 {:.1} ms over {} runs",
        result.name, result.p50_ms, result.p95_ms, result.runs
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_context::ContextStore;
    use mote_eventlog::EventLog;
    use mote_testkit::ScriptedServer;

    #[test]
    fn compiler_benchmark_fixture_is_fixed_and_exercises_v1_bindings() {
        let fixture = compiler_fixture().unwrap();
        assert_eq!(fixture.source_hash, "6b35bde2eb18417d9b147ce0dd01fa27c0da7cd8372491829c321003ddde8293");
        assert_eq!(fixture.combined_hash, "d4cdd34802ef22315a3b47e516f8d8eb3de8533ea0cbf0da5c471543073df86f");
        assert_eq!(fixture.config.len(), 9);
        assert_eq!(fixture.pipeline.numbers.len(), 5);

        let parsed = parse_surface(fixture.source).unwrap();
        let bindings = fixture.bindings().unwrap();
        assert_eq!(
            bindings.snapshot.values["host.items"],
            json!([{"label": "first"}, {"label": "second"}])
        );
        assert_eq!(bindings.snapshot.values["host.progress"], json!(0.5));
        let resolved = resolve(&parsed, &bindings.manifest, &bindings.snapshot).unwrap();
        let lowered = matrix_lang::lower_v1(&resolved).unwrap();
        assert_eq!(lowered.surface.version, 1);
        lowered.surface.validate().unwrap();
        compiler_pass(&fixture, &bindings).unwrap();
        let compiled = matrix::compile_contract(fixture.source, &bindings).unwrap();
        let (payload, _) = compiled.prepare(&bindings.manifest).unwrap();
        mote_ui::v1::validate_envelope(&serde_json::to_vec(&payload).unwrap()).unwrap();
        let text = payload["matrix_surface"].to_string();

        assert!(text.contains("talk"));
        assert!(text.contains("router"));
        assert!(text.contains("think"));
        assert!(text.contains("models.local_only"));
        assert!(text.contains("models.talk"));
        assert!(text.contains("refresh"));
    }

    #[test]
    fn compiler_benchmark_uses_nearest_rank_p95() {
        let samples = (1_u64..=20).collect::<Vec<_>>();

        assert_eq!(nearest_rank(&samples, 95), 19);
        assert_eq!(nearest_rank(&[7], 95), 7);
    }

    #[test]
    fn compiler_benchmark_budget_only_passes_on_named_target_hardware() {
        assert_eq!(
            compiler_budget("macos", "aarch64", "Apple M4", 1_999_999),
            CompilerBudget::Passed {
                limit_ns: 2_000_000
            }
        );
        assert_eq!(
            compiler_budget("macos", "aarch64", "Apple M4", 2_000_000),
            CompilerBudget::Failed {
                limit_ns: 2_000_000
            }
        );
        assert_eq!(
            compiler_budget("macos", "aarch64", "unknown Apple Silicon", 100),
            CompilerBudget::Unsupported
        );
        assert_eq!(
            compiler_budget("macos", "aarch64", "Apple A14", 100),
            CompilerBudget::Unsupported
        );
        assert_eq!(
            compiler_budget(
                "linux",
                "aarch64",
                "Raspberry Pi 5 Model B Rev 1.0",
                9_999_999
            ),
            CompilerBudget::Passed {
                limit_ns: 10_000_000
            }
        );
        assert_eq!(
            compiler_budget("linux", "aarch64", "generic arm64", 100),
            CompilerBudget::Unsupported
        );
    }

    #[test]
    fn compiler_benchmark_refuses_debug_builds() {
        if cfg!(debug_assertions) {
            assert_eq!(
                run_compiler(Path::new("unused")).unwrap_err(),
                "moted bench compiler requires a release build"
            );
        }
    }

    #[test]
    fn three_benches_each_write_one_line() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(vec![
            json!({"message":{"role":"assistant","content":"hello"},"done":false}).to_string(),
            json!({"message":{"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"hi\"}"},"done":true}).to_string(),
        ]);
        let client = Ollama::new(srv.base_url());
        let dir = std::env::temp_dir().join(format!("mote-bench-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bench.jsonl");

        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        append_bench_line(&path, &bench_talk(&log, &store, &client, "qwen3:4b", 32768, 3).unwrap()).unwrap();
        append_bench_line(&path, &bench_router(&client, "qwen3:0.6b", 32768, 3)).unwrap();
        append_bench_line(&path, &bench_memory_scale(50, 3)).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 3, "one line per bench");
        let names: Vec<String> = lines
            .iter()
            .map(|l| serde_json::from_str::<Value>(l).unwrap()["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["talk", "router", "memory"]);
        for l in &lines {
            let v: Value = serde_json::from_str(l).unwrap();
            assert!(v["p50_ms"].is_number());
            assert!(v["p95_ms"].is_number());
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    #[ignore = "requires MOTE_LIVE=1 and isolated MOTE_HOME for a ledgered live diagnostic"]
    fn live_surface_table_diagnostic() {
        assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
        let dir = std::path::PathBuf::from(std::env::var("MOTE_HOME").unwrap());
        assert!(dir.starts_with("/private/tmp"));
        let path = dir.join("mote.db");
        let log = EventLog::open(&path).unwrap();
        let store = ContextStore::open(&path).unwrap();
        let case = surface_corpus().into_iter().find(|case| case.category == "table").unwrap();
        let metrics = bench_surface_cases(&[case], &log, &store, &Ollama::new("http://127.0.0.1:11434")).unwrap();
        println!("{}", serde_json::to_string(&metrics).unwrap());
        assert_eq!(metrics.samples, 1);
    }

    #[test]
    fn surface_rejection_keeps_redacted_source_for_compiler_diagnosis() {
        let trace = evaluate_decision(Ok(inferred_ui::SurfaceDecision::Matrix {
            source: "screen { text \"sk-abcdefghijklmnopqrstuvwxyz\"".to_string(),
        }), &surface_bindings().unwrap());
        assert!(!trace.matrix_parsed);
        assert_eq!(trace.rejected_source, "screen { text \"[redacted]\"");
    }

    #[test]
    fn surface_report_records_only_complete_exact_digest_evidence() {
        let dir = std::env::temp_dir().join(format!("mote-bench-gate-{}-{}", std::process::id(), crate::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut report = unavailable_surface_report(&surface_corpus(), "unavailable".into());
        report.model_digest = Some("sha256:good".to_string());
        report.metrics.first_attempt_valid = 200;
        persist_surface_gate(&dir, &mut report).unwrap();
        assert!(!report.activation_passed);
        assert!(!dir.join("surface-activation.json").exists());
        report.status = "complete";
        report.reason = None;
        persist_surface_gate(&dir, &mut report).unwrap();
        assert!(report.activation_passed);
        let attestation: Value = serde_json::from_slice(&std::fs::read(dir.join("surface-activation.json")).unwrap()).unwrap();
        assert_eq!(attestation["model_digest"], "sha256:good");
        report.reason = Some("model digest changed during benchmark".into());
        persist_surface_gate(&dir, &mut report).unwrap();
        assert!(!report.activation_passed);
        assert!(!dir.join("surface-activation.json").exists());
        report.reason = None;
        report.metrics.first_attempt_valid = 189;
        persist_surface_gate(&dir, &mut report).unwrap();
        assert!(!report.activation_passed);
        assert!(!dir.join("surface-activation.json").exists());
    }

    #[test]
    fn surface_table_example_lowers_a_compact_two_column_host_table() {
        let bindings = surface_bindings().unwrap();
        let prompt = inferred_ui::surface_message_contents(&inferred_ui::SurfaceInputs {
            request: "show a table", prose: "done", previous_source: "", diagnostics: "",
        });
        let example = prompt[0].lines().filter(|line| line.starts_with('{')).nth(8).unwrap();
        let inferred_ui::SurfaceDecision::Matrix { source } = mote_localmodel::parse_verdict(example).unwrap() else {
            panic!("table example must generate Matrix");
        };
        let parsed = parse_surface(&source).unwrap();
        let resolved = resolve(&parsed, &bindings.manifest, &bindings.snapshot).unwrap();
        let mote_ui::Node::Table { columns, .. } = lower_v0(&resolved).unwrap() else {
            panic!("table example must lower to a table");
        };
        assert_eq!(columns, vec!["Stage", "Calls"]);
    }

    #[test]
    fn surface_prompt_examples_compile_with_the_real_host_contract() {
        let bindings = surface_bindings().unwrap();
        let prompt = inferred_ui::surface_message_contents(&inferred_ui::SurfaceInputs {
            request: "show controls",
            prose: "done",
            previous_source: "",
            diagnostics: "",
        });
        let examples: Vec<_> = prompt[0].lines().filter(|line| line.starts_with('{')).collect();
        assert_eq!(examples.len(), 11, "one abstention and ten supported shapes");
        for example in examples {
            let decision = mote_localmodel::parse_verdict::<inferred_ui::SurfaceDecision>(example)
                .unwrap();
            let trace = evaluate_decision(Ok(decision), &bindings);
            assert!(trace.none || trace.v0_lowered, "{}", trace.diagnostics);
        }
    }

    #[test]
    fn surface_corpus_is_fixed_and_represents_every_v0_category() {
        let corpus = surface_corpus();

        assert_eq!(corpus.len(), 200);
        assert_eq!(
            surface_corpus_hash(&corpus),
            "447a7a73f4669a6450bf88a9e1ab5bc03f914c2ab70d213f3d398e49980a5d2d"
        );
        assert_eq!(
            surface_category_counts(&corpus),
            [
                ("action", 18),
                ("field", 18),
                ("list", 18),
                ("none", 20),
                ("progress", 18),
                ("row", 18),
                ("select", 18),
                ("stack", 18),
                ("table", 18),
                ("text", 18),
                ("toggle", 18),
            ]
            .into_iter()
            .map(|(name, count)| (name.to_string(), count))
            .collect()
        );
    }

    #[test]
    fn surface_metrics_count_each_validation_boundary_and_retry_outcome() {
        let cases = vec![
            SurfaceCase::new("none", "say hello", false),
            SurfaceCase::new("toggle", "show a toggle", true),
            SurfaceCase::new("table", "show a table", true),
            SurfaceCase::new("list", "show a list", true),
        ];
        let attempts = vec![
            vec![AttemptTrace::none()],
            vec![AttemptTrace::valid_matrix()],
            vec![
                AttemptTrace::parsed_but_unresolved(),
                AttemptTrace::valid_matrix(),
            ],
            vec![AttemptTrace::envelope_failure()],
        ];

        let metrics = SurfaceMetrics::from_traces(&cases, &attempts);

        assert_eq!(metrics.samples, 4);
        assert_eq!(metrics.attempts, 5);
        assert_eq!(metrics.envelope_valid, 4);
        assert_eq!(metrics.matrix_parsed, 3);
        assert_eq!(metrics.host_resolved, 2);
        assert_eq!(metrics.v0_lowered, 2);
        assert_eq!(metrics.first_attempt_valid, 2);
        assert_eq!(metrics.after_retry_valid, 3);
        assert_eq!(metrics.retries, 1);
        assert_eq!(metrics.first_attempt_rate, 0.5);
        assert_eq!(metrics.after_retry_rate, 0.75);
        assert_eq!(metrics.first_try_by_category["none"], 1);
        assert_eq!(metrics.first_try_by_category["toggle"], 1);
        assert_eq!(metrics.first_try_by_category["table"], 0);
        assert_eq!(metrics.terminal_failures["invalid_envelope"], 1);
    }

    #[test]
    #[should_panic(expected = "one attempt trace per corpus case")]
    fn surface_metrics_never_silently_omit_a_corpus_case() {
        let cases = vec![
            SurfaceCase::new("none", "say hello", false),
            SurfaceCase::new("toggle", "show a toggle", true),
        ];

        let _ = SurfaceMetrics::from_traces(&cases, &[vec![AttemptTrace::none()]]);
    }

    #[test]
    fn production_activation_requires_available_first_attempt_rate_at_least_ninety_five_percent() {
        assert!(!surface_activation(None));
        assert!(!surface_activation(Some((189, 200))));
        assert!(surface_activation(Some((190, 200))));
        assert!(surface_activation(Some((200, 200))));
        assert!(!surface_activation(Some((0, 0))));
    }

    #[test]
    fn surface_benchmark_uses_the_ledgered_model_boundary_for_every_attempt() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(vec![
            json!({
                "model":"qwen3:4b",
                "message":{"role":"assistant","content":"{\"kind\":\"matrix\",\"source\":\"not matrix\"}"},
                "done":true
            }).to_string(),
        ]);
        let client = Ollama::new(srv.base_url());
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        let cases = vec![SurfaceCase::new("toggle", "show a toggle", true)];

        let metrics = bench_surface_cases(&cases, &log, &store, &client).unwrap();

        assert_eq!(metrics.attempts, 2);
        assert_eq!(metrics.retries, 1);
        assert_eq!(store.count_ledger().unwrap(), 2);
        assert_eq!(
            store
                .ledger_since(0)
                .unwrap()
                .iter()
                .filter(|row| row.lane == "surface")
                .count(),
            2
        );
        assert_eq!(
            srv.recorded()
                .iter()
                .filter(|request| request.path == "/api/chat")
                .count(),
            2
        );
    }
}
