use mote_eventlog::Event;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
pub struct RequestRow {
    pub seq: i64,
    pub ts: i64,
    pub lane: String,
    pub payload: Value,
    pub latency_ms: Option<i64>,
}

impl PipelineSnapshot {
    pub fn from_value(value: Value) -> Result<Self, String> {
        serde_json::from_value(value).map_err(|error| format!("invalid pipeline snapshot: {error}"))
    }

    pub fn events_for(&self, stage: &str) -> Vec<&Event> {
        self.events.iter().filter(|event| event_matches_stage(event, stage)).collect()
    }

    pub fn requests_for(&self, stage: &str) -> Vec<&RequestRow> {
        self.requests.iter().filter(|request| request.lane == stage).collect()
    }

    pub fn numbers_for(&self, stage: &str) -> Option<&StageNumbers> {
        self.numbers.iter().find(|numbers| numbers.stage == stage)
    }

    pub fn config_edit_value(&self, key: &str) -> Option<String> {
        self.config.get(key).map(|value| match value {
            Value::String(value) => value.clone(),
            other => other.to_string(),
        })
    }

    pub fn ingest_live(&mut self, event: Event) {
        if self.events.iter().any(|existing| existing.seq == event.seq) {
            return;
        }
        let latest = self
            .events
            .iter()
            .map(|existing| existing.ts)
            .max()
            .unwrap_or(self.generated_at)
            .max(self.generated_at)
            .max(event.ts);
        let cutoff = latest.saturating_sub(self.window_ms);
        if event.ts < cutoff {
            return;
        }
        self.events.push(event);
        self.events.retain(|event| event.ts >= cutoff);
        self.events.sort_unstable_by(|left, right| right.seq.cmp(&left.seq));
        self.events.truncate(self.event_limit);
    }
}

fn event_matches_stage(event: &Event, stage: &str) -> bool {
    if stage == "ledger" {
        return event.payload.get("ledger_id").is_some();
    }
    if event.kind == "model.request" {
        return event.payload["lane"].as_str() == Some(stage);
    }
    event.kind.starts_with(&format!("{stage}."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> PipelineSnapshot {
        PipelineSnapshot::from_value(serde_json::json!({
            "generated_at": 5000,
            "event_limit": 200,
            "request_limit": 200,
            "window_ms": 3600000,
            "config": {
                "models.local_only": true,
                "models.talk": "qwen3:4b",
                "models.router": "qwen3:0.6b",
                "models.think": "qwen3-coder:30b-64k",
                "budgets.talk_first_token": 300,
                "budgets.router": 150,
                "budgets.think_first_token": 3000,
                "packs.talk": 600,
                "packs.think": 8000
            },
            "events": [
                {"seq": 9, "ts": 4900, "scope_id": "main", "kind": "think.done",
                 "payload": {"ledger_id": 8}},
                {"seq": 8, "ts": 4800, "scope_id": "main", "kind": "router.verdict",
                 "payload": {"route": "think"}},
                {"seq": 7, "ts": 4700, "scope_id": "pipeline", "kind": "model.request",
                 "payload": {"lane": "talk", "prompt": "[redacted]"}},
                {"seq": 6, "ts": 4600, "scope_id": "main", "kind": "talk.delta",
                 "payload": {"text": "hello"}}
            ],
            "numbers": [{
                "stage": "talk", "calls": 2, "latency_p50_ms": 12,
                "latency_p95_ms": 20, "prompt_tokens": 30,
                "completion_tokens": 7, "cost_usd": 0.0, "budget_ms": 300
            }],
            "requests": [
                {"seq": 7, "ts": 4700, "lane": "talk",
                 "payload": {"lane": "talk", "prompt": "[redacted]"}, "latency_ms": null}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn typed_snapshot_filters_events_and_requests_by_stage() {
        let snapshot = snapshot();

        assert_eq!(
            snapshot.events_for("talk").iter().map(|event| event.seq).collect::<Vec<_>>(),
            [7, 6]
        );
        assert_eq!(
            snapshot.events_for("router").iter().map(|event| event.seq).collect::<Vec<_>>(),
            [8]
        );
        assert_eq!(
            snapshot.events_for("ledger").iter().map(|event| event.seq).collect::<Vec<_>>(),
            [9]
        );
        assert!(snapshot.events_for("memory").is_empty());
        assert_eq!(snapshot.requests_for("talk")[0].latency_ms, None);
        assert!(snapshot.requests_for("router").is_empty());
    }

    #[test]
    fn config_edit_values_do_not_quote_strings() {
        let snapshot = snapshot();

        assert_eq!(snapshot.config_edit_value("models.talk"), Some("qwen3:4b".into()));
        assert_eq!(snapshot.config_edit_value("models.local_only"), Some("true".into()));
        assert_eq!(snapshot.config_edit_value("budgets.router"), Some("150".into()));
    }

    #[test]
    fn malformed_snapshot_is_rejected_instead_of_fabricating_defaults() {
        let error = PipelineSnapshot::from_value(serde_json::json!({"events": []})).unwrap_err();
        assert!(!error.is_empty());
    }

    #[test]
    fn live_events_merge_newest_first_without_duplicates_and_keep_the_cap() {
        let mut snapshot = snapshot();
        snapshot.event_limit = 4;

        snapshot.ingest_live(Event {
            seq: 10,
            ts: 5100,
            scope_id: "main".into(),
            kind: "talk.done".into(),
            payload: serde_json::json!({"ledger_id": 9}),
        });
        snapshot.ingest_live(Event {
            seq: 10,
            ts: 5100,
            scope_id: "main".into(),
            kind: "talk.done".into(),
            payload: serde_json::json!({"ledger_id": 9}),
        });

        assert_eq!(snapshot.generated_at, 5000);
        assert_eq!(snapshot.events.len(), 4);
        assert_eq!(snapshot.events.iter().map(|event| event.seq).collect::<Vec<_>>(), [10, 9, 8, 7]);
    }
}
