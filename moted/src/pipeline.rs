use mote_context::{ContextStore, LedgerRow};
use mote_eventlog::{Event, EventLog, RETENTION_LIMIT};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const PIPELINE_EVENT_LIMIT: usize = 200;
pub const PIPELINE_REQUEST_LIMIT: usize = RETENTION_LIMIT as usize;
pub const CONFIG_KEYS: [&str; 9] = [
    "models.local_only",
    "models.talk",
    "models.router",
    "models.think",
    "budgets.talk_first_token",
    "budgets.router",
    "budgets.think_first_token",
    "packs.talk",
    "packs.think",
];
const HOUR_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Serialize)]
pub struct PipelineSnapshot {
    pub generated_at: i64,
    pub event_limit: usize,
    pub request_limit: usize,
    pub window_ms: i64,
    pub config: BTreeMap<String, Value>,
    pub events: Vec<Event>,
    pub numbers: Vec<StageNumbers>,
    pub requests: Vec<RequestRow>,
}

#[derive(Debug, Serialize)]
pub struct StageNumbers {
    pub stage: String,
    pub calls: usize,
    pub latency_metric: String,
    pub latency_samples: usize,
    pub latency_p50_ms: Option<i64>,
    pub latency_p95_ms: Option<i64>,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub cost_usd: f64,
    pub budget_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RequestRow {
    pub seq: i64,
    pub ts: i64,
    pub lane: String,
    pub payload: Value,
    pub total_latency_ms: Option<i64>,
    pub first_token_ms: Option<i64>,
    pub status: Option<String>,
}

pub fn build_snapshot(
    log: &EventLog,
    store: &ContextStore,
    config: BTreeMap<String, Value>,
    now: i64,
) -> rusqlite::Result<PipelineSnapshot> {
    let cutoff = now.saturating_sub(HOUR_MS);
    let events = log.recent_since(cutoff, PIPELINE_EVENT_LIMIT)?;
    let ledger = store.ledger_since(cutoff)?;
    let request_events =
        log.recent_kind_since("model.request", i64::MIN, PIPELINE_REQUEST_LIMIT)?;
    let response_events =
        log.recent_kind_since("model.response", i64::MIN, PIPELINE_REQUEST_LIMIT)?;
    let responses: BTreeMap<&str, &Value> = response_events
        .iter()
        .filter_map(|event| {
            event.payload["request_id"].as_str().map(|id| (id, &event.payload))
        })
        .collect();
    let requests = request_events
        .into_iter()
        .map(|event| {
            let response = event.payload["request_id"]
                .as_str()
                .and_then(|id| responses.get(id).copied());
            RequestRow {
                seq: event.seq,
                ts: event.ts,
                lane: event.payload["lane"].as_str().unwrap_or("unknown").to_string(),
                total_latency_ms: response.and_then(|payload| payload["total_ms"].as_i64()),
                first_token_ms: response
                    .and_then(|payload| payload["first_token_ms"].as_i64()),
                status: response
                    .and_then(|payload| payload["status"].as_str())
                    .map(str::to_string),
                payload: event.payload,
            }
        })
        .collect();
    let numbers = ["talk", "router", "think", "memory", "ledger"]
        .into_iter()
        .filter_map(|stage| aggregate(stage, &ledger, &config))
        .collect();
    Ok(PipelineSnapshot {
        generated_at: now,
        event_limit: PIPELINE_EVENT_LIMIT,
        request_limit: PIPELINE_REQUEST_LIMIT,
        window_ms: HOUR_MS,
        config,
        events,
        numbers,
        requests,
    })
}

fn aggregate(
    stage: &str,
    ledger: &[LedgerRow],
    config: &BTreeMap<String, Value>,
) -> Option<StageNumbers> {
    let rows: Vec<&LedgerRow> = ledger.iter().filter(|row| row.lane == stage).collect();
    if rows.is_empty() {
        return None;
    }
    let expected_metric = match stage {
        "talk" | "think" => Some("first_token"),
        "router" => Some("total"),
        _ => None,
    };
    let mut latencies: Vec<i64> = rows
        .iter()
        .filter(|row| Some(row.latency_kind.as_str()) == expected_metric)
        .map(|row| row.latency_ms)
        .collect();
    latencies.sort_unstable();
    let budget_key = match stage {
        "talk" => Some("budgets.talk_first_token"),
        "router" => Some("budgets.router"),
        "think" => Some("budgets.think_first_token"),
        _ => None,
    };
    Some(StageNumbers {
        stage: stage.to_string(),
        calls: rows.len(),
        latency_metric: expected_metric.unwrap_or("unavailable").to_string(),
        latency_samples: latencies.len(),
        latency_p50_ms: (!latencies.is_empty()).then(|| percentile(&latencies, 50)),
        latency_p95_ms: (!latencies.is_empty()).then(|| percentile(&latencies, 95)),
        prompt_tokens: rows.iter().map(|row| row.prompt_tokens).sum(),
        completion_tokens: rows.iter().map(|row| row.completion_tokens).sum(),
        cost_usd: rows.iter().map(|row| row.cost_usd).sum(),
        budget_ms: budget_key.and_then(|key| config.get(key)).and_then(Value::as_i64),
    })
}

fn percentile(sorted: &[i64], percentile: usize) -> i64 {
    let rank = (percentile * sorted.len()).div_ceil(100);
    sorted[rank.saturating_sub(1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_context::{ContextStore, LedgerEntry, PackBuilder, PartKind};
    use mote_eventlog::EventLog;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    fn config() -> BTreeMap<String, serde_json::Value> {
        [
            ("models.local_only", serde_json::json!(true)),
            ("models.talk", serde_json::json!("qwen3:4b")),
            ("models.router", serde_json::json!("qwen3:0.6b")),
            ("models.think", serde_json::json!("qwen3-coder:30b-64k")),
            ("budgets.talk_first_token", serde_json::json!(300)),
            ("budgets.router", serde_json::json!(150)),
            ("budgets.think_first_token", serde_json::json!(3000)),
            ("packs.talk", serde_json::json!(600)),
            ("packs.think", serde_json::json!(8000)),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
    }

    fn pack(lane: &str) -> mote_context::Pack {
        PackBuilder::new(lane)
            .part(PartKind::Charter, "house.md", "be kind")
            .part(PartKind::Turn, "turn", "hello")
            .build(600)
    }

    fn record(store: &ContextStore, latency_ms: i64, prompt: i64, completion: i64, cost: f64) {
        store
            .record(
                &pack("talk"),
                &LedgerEntry {
                    lane: "talk".into(),
                    model: "qwen3:4b".into(),
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                    latency_ms,
                    latency_kind: "first_token".into(),
                    cost_usd: cost,
                    status: "ok".into(),
                },
            )
            .unwrap();
    }

    #[test]
    fn empty_snapshot_keeps_the_complete_config_and_no_invented_rows() {
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();

        let snapshot = build_snapshot(&log, &store, config(), 10_000).unwrap();

        assert_eq!(snapshot.event_limit, PIPELINE_EVENT_LIMIT);
        assert_eq!(snapshot.request_limit, PIPELINE_REQUEST_LIMIT);
        assert_eq!(snapshot.config.len(), 9);
        assert!(snapshot.events.is_empty());
        assert!(snapshot.numbers.is_empty());
        assert!(snapshot.requests.is_empty());
    }

    #[test]
    fn populated_snapshot_is_bounded_redacted_and_aggregates_last_hour() {
        let log = EventLog::open_in_memory().unwrap();
        for index in 0..(PIPELINE_EVENT_LIMIT + 5) {
            log.append("main", "talk.delta", &serde_json::json!({"index": index}))
                .unwrap();
        }
        let request = log
            .append(
                "pipeline",
                "model.request",
                &serde_json::json!({
                    "request_id": "talk-1",
                    "lane": "talk",
                    "payload": {"prompt": "sk-abcdefghijklmnopqrstuvwxyz"}
                }),
            )
            .unwrap();
        log.append(
            "pipeline",
            "model.response",
            &serde_json::json!({
                "request_id": "talk-1", "lane": "talk", "status": "ok",
                "total_ms": 37, "first_token_ms": 11
            }),
        )
        .unwrap();
        log.append(
            "pipeline",
            "model.request",
            &serde_json::json!({"request_id": "router-1", "lane": "router",
                "payload": {"prompt": "hello"}}),
        )
        .unwrap();

        let mut store = ContextStore::open_in_memory().unwrap();
        let clock = Arc::new(Mutex::new(request.ts));
        let c = clock.clone();
        store.set_clock(Arc::new(move || *c.lock().unwrap()));
        for (latency, prompt, completion, cost) in [
            (10, 10, 1, 0.10),
            (20, 20, 2, 0.20),
            (30, 30, 3, 0.30),
            (40, 40, 4, 0.40),
        ] {
            record(&store, latency, prompt, completion, cost);
        }

        let snapshot = build_snapshot(&log, &store, config(), request.ts).unwrap();

        assert_eq!(snapshot.events.len(), PIPELINE_EVENT_LIMIT);
        assert!(snapshot.events.windows(2).all(|pair| pair[0].seq > pair[1].seq));
        assert_eq!(snapshot.requests.len(), 2);
        assert_eq!(snapshot.requests[0].lane, "router");
        assert_eq!(snapshot.requests[0].total_latency_ms, None);
        assert_eq!(snapshot.requests[1].seq, request.seq);
        assert_eq!(snapshot.requests[1].total_latency_ms, Some(37));
        assert_eq!(snapshot.requests[1].first_token_ms, Some(11));
        assert_eq!(snapshot.requests[1].status.as_deref(), Some("ok"));
        assert_eq!(snapshot.requests[1].payload["payload"]["prompt"], "[redacted]");

        let talk = snapshot.numbers.iter().find(|row| row.stage == "talk").unwrap();
        assert_eq!(talk.calls, 4);
        assert_eq!(talk.latency_metric, "first_token");
        assert_eq!(talk.latency_samples, 4);
        assert_eq!(talk.latency_p50_ms, Some(20));
        assert_eq!(talk.latency_p95_ms, Some(40));
        assert_eq!(talk.prompt_tokens, 100);
        assert_eq!(talk.completion_tokens, 10);
        assert!((talk.cost_usd - 1.0).abs() < f64::EPSILON);
        assert_eq!(talk.budget_ms, Some(300));
        assert!(!snapshot.numbers.iter().any(|row| row.stage == "router"));
    }

    #[test]
    fn numbers_exclude_rows_older_than_the_declared_hour_window() {
        let log = EventLog::open_in_memory().unwrap();
        let mut store = ContextStore::open_in_memory().unwrap();
        let clock = Arc::new(Mutex::new(10_000i64));
        let c = clock.clone();
        store.set_clock(Arc::new(move || *c.lock().unwrap()));
        record(&store, 999, 100, 10, 1.0);
        let now = 10_000 + HOUR_MS + 1;
        *clock.lock().unwrap() = now;
        record(&store, 25, 20, 2, 0.2);

        let snapshot = build_snapshot(&log, &store, config(), now).unwrap();

        assert_eq!(snapshot.window_ms, HOUR_MS);
        let talk = snapshot.numbers.iter().find(|row| row.stage == "talk").unwrap();
        assert_eq!(talk.calls, 1);
        assert_eq!(talk.latency_p50_ms, Some(25));
        assert_eq!(talk.prompt_tokens, 20);
        assert_eq!(talk.completion_tokens, 2);
        assert!((talk.cost_usd - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn numbers_exclude_non_comparable_latency_but_keep_call_totals() {
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        store
            .record(
                &pack("talk"),
                &LedgerEntry {
                    lane: "talk".into(),
                    model: "qwen3:4b".into(),
                    prompt_tokens: 7,
                    completion_tokens: 2,
                    latency_ms: 900,
                    latency_kind: "total".into(),
                    cost_usd: 0.0,
                    status: "error".into(),
                },
            )
            .unwrap();

        let snapshot = build_snapshot(&log, &store, config(), i64::MAX / 2).unwrap();

        assert!(snapshot.numbers.is_empty(), "the synthetic row is outside the hour");
        let snapshot = build_snapshot(&log, &store, config(),
            store.ledger_since(0).unwrap()[0].ts).unwrap();
        let talk = snapshot.numbers.iter().find(|row| row.stage == "talk").unwrap();
        assert_eq!(talk.calls, 1);
        assert_eq!(talk.latency_samples, 0);
        assert_eq!(talk.latency_p50_ms, None);
        assert_eq!(talk.latency_p95_ms, None);
        assert_eq!(talk.prompt_tokens, 7);
        assert_eq!(talk.budget_ms, Some(300));
    }

    #[test]
    fn requests_include_retained_calls_older_than_the_numbers_window() {
        let log = EventLog::open_in_memory().unwrap();
        let request = log
            .append(
                "pipeline",
                "model.request",
                &serde_json::json!({"request_id": "old", "lane": "talk", "payload": {}}),
            )
            .unwrap();
        let store = ContextStore::open_in_memory().unwrap();

        let snapshot = build_snapshot(&log, &store, config(), request.ts + HOUR_MS + 1).unwrap();

        assert_eq!(snapshot.requests.len(), 1);
        assert_eq!(snapshot.requests[0].payload["request_id"], "old");
    }
}
