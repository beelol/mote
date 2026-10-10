use mote_context::{count_tokens, LedgerEntry, Pack};
use mote_eventlog::EventLog;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
pub(crate) struct CallTiming {
    pub(crate) total_ms: i64,
    pub(crate) first_token_ms: Option<i64>,
}

pub(crate) struct ModelRequest<'a> {
    log: &'a EventLog,
    request_id: String,
    lane: &'a str,
    model: &'a str,
    started: Instant,
}

impl<'a> ModelRequest<'a> {
    pub(crate) fn begin(
        log: &'a EventLog,
        lane: &'a str,
        model: &'a str,
        payload: serde_json::Value,
    ) -> rusqlite::Result<Self> {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let counter = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let request_id = format!("{}-{epoch}-{counter}", std::process::id());
        log.append(
            "pipeline",
            "model.request",
            &json!({
                "request_id": request_id,
                "lane": lane,
                "model": model,
                "payload": payload
            }),
        )?;
        Ok(Self { log, request_id, lane, model, started: Instant::now() })
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    pub(crate) fn finish(
        self,
        status: &str,
        first_token: Option<Duration>,
    ) -> CallTiming {
        let timing = CallTiming {
            total_ms: self.started.elapsed().as_millis() as i64,
            first_token_ms: first_token.map(|duration| duration.as_millis() as i64),
        };
        let _ = self.log.append(
            "pipeline",
            "model.response",
            &json!({
                "request_id": self.request_id,
                "lane": self.lane,
                "model": self.model,
                "status": status,
                "total_ms": timing.total_ms,
                "first_token_ms": timing.first_token_ms
            }),
        );
        timing
    }
}

fn latency_for(timing: CallTiming, expected_first_token: bool) -> (i64, String) {
    if expected_first_token {
        timing
            .first_token_ms
            .map(|latency| (latency, "first_token".to_string()))
            .unwrap_or_else(|| (timing.total_ms, "total".to_string()))
    } else {
        (timing.total_ms, "total".to_string())
    }
}

pub(crate) fn ledger_entry(
    lane: &str,
    model: &str,
    pack: &Pack,
    completion: &str,
    timing: CallTiming,
    expected_first_token: bool,
    status: &str,
) -> LedgerEntry {
    let (latency_ms, latency_kind) = latency_for(timing, expected_first_token);
    LedgerEntry {
        lane: lane.into(),
        model: model.into(),
        prompt_tokens: pack.token_estimate() as i64,
        completion_tokens: count_tokens(completion) as i64,
        latency_ms,
        latency_kind,
        cost_usd: 0.0,
        status: status.into(),
    }
}
