use mote_context::{ContextStore, LedgerRow};
use mote_eventlog::{Event, EventLog};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const PIPELINE_EVENT_LIMIT: usize = 200;
pub const PIPELINE_REQUEST_LIMIT: usize = 200;
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
    pub latency_p50_ms: i64,
    pub latency_p95_ms: i64,
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
    pub latency_ms: Option<i64>,
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
        log.recent_kind_since("model.request", cutoff, PIPELINE_REQUEST_LIMIT)?;
    let requests = request_events
        .into_iter()
        .map(|event| RequestRow {
            seq: event.seq,
            ts: event.ts,
            lane: event.payload["lane"].as_str().unwrap_or("unknown").to_string(),
            latency_ms: event.payload["latency_ms"].as_i64(),
            payload: event.payload,
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
    let mut latencies: Vec<i64> = rows.iter().map(|row| row.latency_ms).collect();
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
        latency_p50_ms: percentile(&latencies, 50),
        latency_p95_ms: percentile(&latencies, 95),
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
                    "lane": "talk",
                    "prompt": "sk-abcdefghijklmnopqrstuvwxyz",
                    "latency_ms": 37
                }),
            )
            .unwrap();
        log.append(
            "pipeline",
            "model.request",
            &serde_json::json!({"lane": "router", "prompt": "hello"}),
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
        assert_eq!(snapshot.requests[0].latency_ms, None);
        assert_eq!(snapshot.requests[1].seq, request.seq);
        assert_eq!(snapshot.requests[1].latency_ms, Some(37));
        assert_eq!(snapshot.requests[1].payload["prompt"], "[redacted]");

        let talk = snapshot.numbers.iter().find(|row| row.stage == "talk").unwrap();
        assert_eq!(talk.calls, 4);
        assert_eq!(talk.latency_p50_ms, 20);
        assert_eq!(talk.latency_p95_ms, 40);
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
        assert_eq!(talk.latency_p50_ms, 25);
        assert_eq!(talk.prompt_tokens, 20);
        assert_eq!(talk.completion_tokens, 2);
        assert!((talk.cost_usd - 0.2).abs() < f64::EPSILON);
    }
}
