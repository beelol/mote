//! The talk lane and the router. See docs/rfcs/v0.md sections 6 and 7.
//!
//! The talk lane builds a five-part pack within budget, streams the talk model as `talk.delta`
//! events and ends with one `talk.done` carrying the ledger row id. If the talk model cannot
//! load it emits `talk.unavailable` and uses no other model. The router runs on every turn in
//! parallel; any invalid verdict is fail-closed to `think`. On a `think` verdict the talk lane
//! finishes its sentence, emits `talk.handoff` and gives no second answer.

use mote_context::{count_tokens, ContextStore, LedgerEntry, Pack, PackBuilder, PartKind};
use mote_eventlog::EventLog;
use mote_harness::ThinkEvent;
use mote_localmodel::{ChatOptions, Ollama};
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Default talk pack budget in tokens.
pub const TALK_BUDGET: usize = 600;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy)]
struct CallTiming {
    total_ms: i64,
    first_token_ms: Option<i64>,
}

struct ModelRequest<'a> {
    log: &'a EventLog,
    request_id: String,
    lane: &'a str,
    model: &'a str,
    started: Instant,
}

impl<'a> ModelRequest<'a> {
    fn begin(
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

    fn elapsed_ms(&self) -> i64 {
        self.started.elapsed().as_millis() as i64
    }

    fn finish(
        self,
        status: &str,
        first_token: Option<Duration>,
    ) -> CallTiming {
        let timing = CallTiming {
            total_ms: self.elapsed_ms(),
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

fn ledger_entry(
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

/// The fixed few-shot prompt for the router, stored in the repo.
pub const ROUTER_PROMPT: &str = "\
You are a strict router for a two-lane agent. Output ONLY a JSON object \
{\"route\": \"think\" | \"talk\", \"reason\": string}. Never answer or do the request yourself.\n\
\n\
Key fact: the TALK lane is a tiny model with NO tools and NO memory. It can only make social \
small talk. It CANNOT read, open, list, search, summarise, compute, recall, look up, draft, or \
touch any file, note, email, log, or past conversation. The THINK lane has the tools.\n\
\n\
Decision rule, in order:\n\
1. If the message asks to read, open, write, rename, find, search, grep, list, show, summarise, \
count, compute, explain, fix, debug, refactor, convert, compare, analyse, plan, draft, remember, \
recall, look up, trace, extract or check ANYTHING -> think.\n\
2. If it mentions a file, code, logs, notes, emails, a ticket, a document, or the past (\"what \
did I/you\", \"last week\", \"the decision we made\") -> think.\n\
3. If it is a factual question that needs an answer -> think.\n\
4. ONLY if the whole message is a greeting, closing, thanks, acknowledgement, compliment, or \
pure small talk about the agent/user with no task -> talk.\n\
When torn between the two, choose think.\n\
\n\
Examples:\n\
hey there -> {\"route\":\"talk\",\"reason\":\"greeting\"}\n\
morning! -> {\"route\":\"talk\",\"reason\":\"greeting\"}\n\
how are you today -> {\"route\":\"talk\",\"reason\":\"small talk\"}\n\
what's your name -> {\"route\":\"talk\",\"reason\":\"small talk about the agent\"}\n\
thanks, appreciate it -> {\"route\":\"talk\",\"reason\":\"acknowledgement\"}\n\
read my config file and summarise it -> {\"route\":\"think\",\"reason\":\"needs file tools\"}\n\
summarise the last ten emails -> {\"route\":\"think\",\"reason\":\"needs to read emails\"}\n\
list the files in my downloads folder -> {\"route\":\"think\",\"reason\":\"needs to list files\"}\n\
look up when I last changed my password -> {\"route\":\"think\",\"reason\":\"needs to look up a fact\"}\n\
what did I tell you about my sister last week -> {\"route\":\"think\",\"reason\":\"needs memory\"}\n\
what's the capital of France -> {\"route\":\"think\",\"reason\":\"a factual question to answer\"}";

/// The route a turn takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Talk,
    Think,
}

impl Route {
    pub fn as_str(&self) -> &'static str {
        match self {
            Route::Talk => "talk",
            Route::Think => "think",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RouterVerdict {
    route: String,
    reason: String,
}

/// JSON schema for the router verdict.
pub fn router_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "route": { "type": "string", "enum": ["think", "talk"] },
            "reason": { "type": "string" }
        },
        "required": ["route", "reason"]
    })
}

/// Run the router. Any invalid or unexpected verdict is fail-closed to `think`.
pub fn route(client: &Ollama, model: &str, num_ctx: u32, message: &str) -> (Route, String) {
    let messages = json!([
        { "role": "system", "content": ROUTER_PROMPT },
        { "role": "user", "content": message }
    ]);
    match client.chat_structured::<RouterVerdict>(model, messages, router_schema(), num_ctx) {
        Ok(v) if v.route == "talk" => (Route::Talk, v.reason),
        Ok(v) if v.route == "think" => (Route::Think, v.reason),
        _ => (Route::Think, "invalid or missing verdict; fail-closed to think".to_string()),
    }
}

struct RouterOutcome {
    route: Route,
    reason: String,
    pack: Pack,
    timing: CallTiming,
    status: &'static str,
}

fn run_router(
    log: &EventLog,
    client: &Ollama,
    model: &str,
    num_ctx: u32,
    message: &str,
) -> Option<RouterOutcome> {
    let (pack, _) = PackBuilder::new("router")
        .part(PartKind::Charter, "router_prompt", ROUTER_PROMPT)
        .part(PartKind::Profile, "profile", "")
        .part(PartKind::Memory, "memory", "")
        .part(PartKind::WorkingSet, "working_set", "")
        .part(PartKind::Turn, "turn", message)
        .build_with_texts(num_ctx as usize);
    let messages = json!([
        { "role": "system", "content": ROUTER_PROMPT },
        { "role": "user", "content": message }
    ]);
    let request = ModelRequest::begin(
        log,
        "router",
        model,
        json!({ "num_ctx": num_ctx, "messages": messages }),
    )
    .ok()?;
    let verdict = client.chat_structured::<RouterVerdict>(
        model,
        messages,
        router_schema(),
        num_ctx,
    );
    let (route, reason, status) = match verdict {
        Ok(v) if v.route == "talk" => (Route::Talk, v.reason, "ok"),
        Ok(v) if v.route == "think" => (Route::Think, v.reason, "ok"),
        _ => (
            Route::Think,
            "invalid or missing verdict; fail-closed to think".to_string(),
            "error",
        ),
    };
    let timing = request.finish(status, None);
    Some(RouterOutcome { route, reason, pack, timing, status })
}

/// The five texts of a talk pack.
pub struct TalkInputs<'a> {
    pub charter: &'a str,
    pub profile: &'a str,
    pub memories: &'a str,
    pub working_set: &'a str,
    pub turn: &'a str,
}

/// Build the talk pack and the system/user prompt from the trimmed texts.
pub fn build_talk_pack(inputs: &TalkInputs, budget: usize) -> (Pack, String, String) {
    let (pack, texts) = PackBuilder::new("talk")
        .part(PartKind::Charter, "charter", inputs.charter)
        .part(PartKind::Profile, "profile", inputs.profile)
        .part(PartKind::Memory, "memory", inputs.memories)
        .part(PartKind::WorkingSet, "working_set", inputs.working_set)
        .part(PartKind::Turn, "turn", inputs.turn)
        .build_with_texts(budget);

    let mut system = String::new();
    let mut user = String::new();
    for (kind, text) in &texts {
        match kind {
            PartKind::Turn => user = text.clone(),
            _ => {
                if !text.is_empty() {
                    system.push_str(text);
                    system.push('\n');
                }
            }
        }
    }
    (pack, system, user)
}

/// The result of a talk-lane run.
#[derive(Debug)]
pub enum TalkOutcome {
    Done { ledger_id: i64, pack_id: i64, answer: String },
    Handoff,
    Unavailable(String),
}

/// Append one turn to a thread's working set so the next turn has continuity: the owner's message
/// and the lane's answer become two lines the next pack carries. `ts` orders the pair; the answer
/// sits just after the message. Kept small and pure so it is unit-tested without the daemon.
pub fn record_turn(
    store: &ContextStore,
    thread_id: &str,
    ts: i64,
    message: &str,
    answer: &str,
) {
    let _ = store.append_working_set(thread_id, ts, &format!("you: {message}"));
    if !answer.trim().is_empty() {
        let _ = store.append_working_set(thread_id, ts + 1, &format!("mote: {answer}"));
    }
}

/// Run the talk lane. Emits `talk.delta` for each chunk and ends with `talk.done`, or
/// `talk.handoff` if the router hands off, or `talk.unavailable` if the model cannot load.
#[allow(clippy::too_many_arguments)]
pub fn run_talk(
    log: &EventLog,
    store: &ContextStore,
    client: &Ollama,
    thread_id: &str,
    model: &str,
    num_ctx: u32,
    inputs: &TalkInputs,
    budget: usize,
    handoff: &AtomicBool,
) -> TalkOutcome {
    let (pack, system, user) = build_talk_pack(inputs, budget);
    let messages = json!([
        { "role": "system", "content": system },
        { "role": "user", "content": user }
    ]);
    let opts = ChatOptions::new(num_ctx).no_think();

    let request = match ModelRequest::begin(
        log,
        "talk",
        model,
        json!({ "num_ctx": num_ctx, "messages": messages }),
    ) {
        Ok(request) => request,
        Err(error) => {
            let reason = format!("request log failed: {error}");
            let _ = log.append(thread_id, "talk.unavailable", &json!({ "reason": reason }));
            return TalkOutcome::Unavailable(reason);
        }
    };

    let mut answer = String::new();
    let mut handed_off = false;
    let mut first_token = None;
    let res = client.chat_stream(model, messages, &opts, |chunk| {
        if handed_off {
            return;
        }
        if !chunk.content.is_empty() {
            if first_token.is_none() {
                first_token = Some(request.started.elapsed());
            }
            answer.push_str(&chunk.content);
            let _ = log.append(thread_id, "talk.delta", &json!({ "text": chunk.content }));
        }
        // Finish the current sentence, then hand off.
        if handoff.load(Ordering::Acquire) {
            let _ = log.append(thread_id, "talk.handoff", &json!({}));
            handed_off = true;
        }
    });

    let response_status = if res.is_err() {
        "error"
    } else if handed_off {
        "handoff"
    } else {
        "ok"
    };
    let timing = request.finish(response_status, first_token);
    let entry = ledger_entry(
        "talk",
        model,
        &pack,
        &answer,
        timing,
        true,
        response_status,
    );
    let recorded = store.record(&pack, &entry);

    match res {
        Err(e) => {
            let reason = e.to_string();
            let _ = log.append(thread_id, "talk.unavailable", &json!({ "reason": reason.clone() }));
            TalkOutcome::Unavailable(reason)
        }
        Ok(()) => {
            if handed_off {
                return match recorded {
                    Ok(_) => TalkOutcome::Handoff,
                    Err(error) => TalkOutcome::Unavailable(format!("ledger write failed: {error}")),
                };
            }
            match recorded {
                Ok((pack_id, ledger_id)) => {
                    let _ = log.append(
                        thread_id,
                        "talk.done",
                        &json!({ "ledger_id": ledger_id, "pack_id": pack_id }),
                    );
                    TalkOutcome::Done { ledger_id, pack_id, answer }
                }
                Err(e) => {
                    let reason = format!("ledger write failed: {e}");
                    let _ =
                        log.append(thread_id, "talk.unavailable", &json!({ "reason": reason.clone() }));
                    TalkOutcome::Unavailable(reason)
                }
            }
        }
    }
}

/// Run a full turn: the router in parallel with the talk lane, handing off on a `think` verdict.
#[allow(clippy::too_many_arguments)]
pub fn run_turn(
    log: &EventLog,
    store: &ContextStore,
    client: &Ollama,
    thread_id: &str,
    talk_model: &str,
    talk_ctx: u32,
    router_model: &str,
    router_ctx: u32,
    inputs: &TalkInputs,
    budget: usize,
) -> TalkOutcome {
    let handoff = std::sync::Arc::new(AtomicBool::new(false));

    let rclient = client.clone();
    let rmodel = router_model.to_string();
    let rmsg = inputs.turn.to_string();
    let rlog = log.clone();
    let rthread = thread_id.to_string();
    let rhandoff = handoff.clone();
    let router_handle = std::thread::spawn(move || {
        let outcome = run_router(&rlog, &rclient, &rmodel, router_ctx, &rmsg);
        let (r, reason) = outcome
            .as_ref()
            .map(|outcome| (outcome.route, outcome.reason.clone()))
            .unwrap_or((Route::Think, "request log failed; fail-closed to think".into()));
        let _ = rlog.append(
            &rthread,
            "router.verdict",
            &json!({ "route": r.as_str(), "reason": reason }),
        );
        if r == Route::Think {
            rhandoff.store(true, Ordering::Release);
        }
        outcome
    });

    let outcome =
        run_talk(log, store, client, thread_id, talk_model, talk_ctx, inputs, budget, &handoff);
    if let Ok(Some(router)) = router_handle.join() {
        let entry = ledger_entry(
            "router",
            router_model,
            &router.pack,
            "",
            router.timing,
            false,
            router.status,
        );
        let _ = store.record(&router.pack, &entry);
    }
    outcome
}

/// The five texts of a think pack. Same parts and order as the talk pack, a larger budget.
pub struct ThinkInputs<'a> {
    pub charter: &'a str,
    pub profile: &'a str,
    pub memories: &'a str,
    pub working_set: &'a str,
    pub turn: &'a str,
}

/// Default think pack budget in tokens (v0.md section 9).
pub const THINK_BUDGET: usize = 8000;

/// Build the think pack and the system/turn texts. The system text is every part but the turn,
/// in order; the turn is the user message. The think lane sends the system text as the harness
/// `system` field so the charter leads every call.
pub fn build_think_pack(inputs: &ThinkInputs, budget: usize) -> (Pack, String, String) {
    let (pack, texts) = PackBuilder::new("think")
        .part(PartKind::Charter, "charter", inputs.charter)
        .part(PartKind::Profile, "profile", inputs.profile)
        .part(PartKind::Memory, "memory", inputs.memories)
        .part(PartKind::WorkingSet, "working_set", inputs.working_set)
        .part(PartKind::Turn, "turn", inputs.turn)
        .build_with_texts(budget);

    let mut system = String::new();
    let mut turn = String::new();
    for (kind, text) in &texts {
        match kind {
            PartKind::Turn => turn = text.clone(),
            _ => {
                if !text.is_empty() {
                    system.push_str(text);
                    system.push('\n');
                }
            }
        }
    }
    (pack, system, turn)
}

/// The result of a think-lane run.
#[derive(Debug)]
pub enum ThinkOutcome {
    Done { ledger_id: i64, pack_id: i64, answer: String },
    Failed(String),
}

/// Instrument the think model boundary, then store exactly one pack and ledger row for the call.
#[allow(clippy::too_many_arguments)]
pub fn run_think_model_call<F>(
    log: &EventLog,
    store: &ContextStore,
    thread_id: &str,
    model: &str,
    pack: &Pack,
    system: &str,
    turn: &str,
    invoke: F,
) -> ThinkOutcome
where
    F: FnOnce() -> Result<(Vec<ThinkEvent>, Duration), String>,
{
    let request = match ModelRequest::begin(
        log,
        "think",
        model,
        json!({ "system": system, "turn": turn }),
    ) {
        Ok(request) => request,
        Err(error) => {
            let reason = format!("request log failed: {error}");
            let _ = log.append(thread_id, "think.failed", &json!({ "reason": reason }));
            return ThinkOutcome::Failed(reason);
        }
    };
    match invoke() {
        Ok((events, first_token)) => {
            let status = if events
                .iter()
                .rev()
                .find_map(|event| match event {
                    ThinkEvent::Done { ok } => Some(*ok),
                    _ => None,
                })
                .unwrap_or(false)
            {
                "ok"
            } else {
                "error"
            };
            request.finish(status, Some(first_token));
            emit_think(
                log,
                store,
                thread_id,
                model,
                pack,
                &events,
                first_token.as_millis() as u64,
            )
        }
        Err(reason) => {
            let timing = request.finish("error", None);
            let entry = ledger_entry("think", model, pack, "", timing, true, "error");
            let _ = store.record(pack, &entry);
            let _ = log.append(
                thread_id,
                "think.failed",
                &json!({ "reason": reason.clone() }),
            );
            ThinkOutcome::Failed(reason)
        }
    }
}

/// Turn the harness's normalized think events into log events and one ledger row. Each `Text`
/// becomes `think.text`, each tool call and result its own event; the turn ends with one
/// `think.done` that carries the ledger and pack ids. The answer text drives the completion token
/// count. The server interaction that produces `events` is tested live by V0-29; this is the
/// daemon-side mapping, tested offline with scripted events.
pub fn emit_think(
    log: &EventLog,
    store: &ContextStore,
    thread_id: &str,
    model: &str,
    pack: &Pack,
    events: &[ThinkEvent],
    latency_ms: u64,
) -> ThinkOutcome {
    let mut answer = String::new();
    let mut ok = true;
    for e in events {
        match e {
            ThinkEvent::Text { text } => {
                answer.push_str(text);
                let _ = log.append(thread_id, "think.text", &json!({ "text": text }));
            }
            ThinkEvent::ToolCall { name, id, input } => {
                let _ = log.append(
                    thread_id,
                    "think.tool_call",
                    &json!({ "name": name, "id": id, "input": input }),
                );
            }
            ThinkEvent::ToolResult { name, id, output, is_error } => {
                let _ = log.append(
                    thread_id,
                    "think.tool_result",
                    &json!({ "name": name, "id": id, "output": output, "is_error": is_error }),
                );
            }
            ThinkEvent::Done { ok: turn_ok } => ok = *turn_ok,
        }
    }

    let entry = LedgerEntry {
        lane: "think".into(),
        model: model.into(),
        prompt_tokens: pack.token_estimate() as i64,
        completion_tokens: count_tokens(&answer) as i64,
        latency_ms: latency_ms as i64,
        latency_kind: "first_token".into(),
        cost_usd: 0.0,
        status: if ok { "ok".into() } else { "error".into() },
    };
    match store.record(pack, &entry) {
        Ok((pack_id, ledger_id)) => {
            let _ = log.append(
                thread_id,
                "think.done",
                &json!({ "ledger_id": ledger_id, "pack_id": pack_id, "ok": ok }),
            );
            ThinkOutcome::Done { ledger_id, pack_id, answer }
        }
        Err(e) => {
            let reason = format!("ledger write failed: {e}");
            let _ = log.append(thread_id, "think.failed", &json!({ "reason": reason.clone() }));
            ThinkOutcome::Failed(reason)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_testkit::ScriptedServer;
    use std::time::Duration;

    fn inputs<'a>(turn: &'a str) -> TalkInputs<'a> {
        TalkInputs {
            charter: "be kind",
            profile: "name: owner",
            memories: "",
            working_set: "earlier message",
            turn,
        }
    }

    fn chat_chunks(parts: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = parts
            .iter()
            .map(|p| json!({"message":{"role":"assistant","content":p},"done":false}).to_string())
            .collect();
        v.push(json!({"message":{"role":"assistant","content":""},"done":true}).to_string());
        v
    }

    #[test]
    fn talk_emits_deltas_and_done_with_ledger_id() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(chat_chunks(&["Hello", " there"]));
        let client = Ollama::new(srv.base_url());
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        let mut sub = log.subscribe("t1", 0).unwrap();

        let out = run_talk(
            &log, &store, &client, "t1", "qwen3:4b", 32768, &inputs("hi"), TALK_BUDGET,
            &AtomicBool::new(false),
        );
        let ledger_id = match out {
            TalkOutcome::Done { ledger_id, .. } => ledger_id,
            other => panic!("expected done, got {other:?}"),
        };
        assert!(ledger_id > 0);

        let mut deltas = 0;
        let mut done_ledger = None;
        while let Some(ev) = sub.recv_timeout(Duration::from_millis(50)) {
            match ev.kind.as_str() {
                "talk.delta" => deltas += 1,
                "talk.done" => done_ledger = ev.payload["ledger_id"].as_i64(),
                _ => {}
            }
        }
        assert!(deltas >= 2, "expected talk.delta events");
        assert_eq!(done_ledger, Some(ledger_id));
        let rows = store.ledger_since(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].latency_kind, "first_token");
    }

    #[test]
    fn talk_pack_has_five_parts_in_order_within_budget() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(chat_chunks(&["ok"]));
        let client = Ollama::new(srv.base_url());
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();

        let out = run_talk(
            &log, &store, &client, "t1", "qwen3:4b", 32768, &inputs("hello"), TALK_BUDGET,
            &AtomicBool::new(false),
        );
        let pack_id = match out {
            TalkOutcome::Done { pack_id, .. } => pack_id,
            other => panic!("expected done, got {other:?}"),
        };
        let parts = store.pack_parts(pack_id).unwrap();
        let kinds: Vec<PartKind> = parts.iter().map(|p| p.kind).collect();
        assert_eq!(kinds, PartKind::ORDER.to_vec());
        let total: usize = parts.iter().map(|p| p.tokens).sum();
        assert!(total <= TALK_BUDGET, "pack over budget: {total}");
    }

    #[test]
    fn talk_unavailable_when_model_cannot_load() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_status(404);
        let client = Ollama::new(srv.base_url());
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        let mut sub = log.subscribe("t1", 0).unwrap();

        let out = run_talk(
            &log, &store, &client, "t1", "qwen3:4b", 32768, &inputs("hi"), TALK_BUDGET,
            &AtomicBool::new(false),
        );
        assert!(matches!(out, TalkOutcome::Unavailable(_)));

        let mut saw_unavailable = false;
        while let Some(ev) = sub.recv_timeout(Duration::from_millis(50)) {
            if ev.kind == "talk.unavailable" {
                saw_unavailable = true;
                assert!(ev.payload["reason"].as_str().unwrap().contains("404"));
            }
            assert_ne!(ev.kind, "talk.done", "no answer when unavailable");
        }
        assert!(saw_unavailable);
        let rows = store.ledger_since(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "error");
        assert_eq!(rows[0].latency_kind, "total");
        // Only the talk model was ever requested; no fallback model.
        for r in srv.recorded() {
            if r.path == "/api/chat" {
                assert_eq!(r.body["model"], "qwen3:4b");
            }
        }
    }

    #[test]
    fn requests_store_redacted_payloads_no_secrets() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(chat_chunks(&["ok"]));
        let client = Ollama::new(srv.base_url());
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();

        // A secret planted in the turn must not survive into any stored row.
        let secret = "sk-abcdefghijklmnopqrstuvwxyz";
        let mut sub = log.subscribe("pipeline", 0).unwrap();
        let inputs = TalkInputs {
            charter: "be kind",
            profile: "",
            memories: "",
            working_set: "",
            turn: "my key is sk-abcdefghijklmnopqrstuvwxyz please remember",
        };
        run_talk(
            &log, &store, &client, "t1", "qwen3:4b", 32768, &inputs, TALK_BUDGET,
            &AtomicBool::new(false),
        );

        // The Requests tab reads model.request events on the pipeline scope.
        let mut request_id = None;
        let mut response_id = None;
        while let Some(ev) = sub.recv_timeout(Duration::from_millis(50)) {
            if ev.kind == "model.request" {
                request_id = ev.payload["request_id"].as_str().map(str::to_string);
                let text = ev.payload.to_string();
                assert!(!text.contains(secret), "stored request must be redacted: {text}");
                assert!(text.contains("[redacted]"), "redaction marker expected: {text}");
            } else if ev.kind == "model.response" {
                response_id = ev.payload["request_id"].as_str().map(str::to_string);
                assert!(ev.payload["total_ms"].is_i64() || ev.payload["total_ms"].is_u64());
                assert!(ev.payload["first_token_ms"].is_i64()
                    || ev.payload["first_token_ms"].is_u64());
            }
        }
        assert!(request_id.is_some(), "an outbound call must be recorded");
        assert_eq!(request_id, response_id, "timing must correlate to the request");
    }

    #[test]
    fn full_turn_records_talk_and_router_calls_once() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(vec![json!({"message":{"role":"assistant",
            "content":"{\"route\":\"talk\",\"reason\":\"greeting\"}"},"done":true})
        .to_string()]);
        let client = Ollama::new(srv.base_url());
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();

        let outcome = run_turn(
            &log,
            &store,
            &client,
            "t1",
            "qwen3:4b",
            32768,
            "qwen3:0.6b",
            32768,
            &inputs("hello"),
            TALK_BUDGET,
        );

        assert!(matches!(outcome, TalkOutcome::Done { .. }));
        let rows = store.ledger_since(0).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.iter().filter(|row| row.lane == "talk").count(), 1);
        let router = rows.iter().find(|row| row.lane == "router").unwrap();
        assert_eq!(router.latency_kind, "total");
        assert_eq!(store.count_packs().unwrap(), 2);
        let requests = log.recent_kind_since("model.request", 0, 10).unwrap();
        let responses = log.recent_kind_since("model.response", 0, 10).unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(responses.len(), 2);
    }

    #[test]
    fn router_fail_closed_on_six_malformed_verdicts() {
        let srv = ScriptedServer::start().unwrap();
        let client = Ollama::new(srv.base_url());
        let malformed = [
            r#"{"route":"talk","reason":"ok"} trailing"#, // trailing text
            r#"{"route":"talk","reason":"ok","x":1}"#,     // unknown field
            r#"{"route":"talk"}"#,                          // missing field
            r#"not json at all"#,                           // prose
            r#"{"route":5,"reason":"ok"}"#,                // wrong type
            r#"{"route":"maybe","reason":"ok"}"#,          // invalid route value
        ];
        for m in malformed {
            srv.set_chat_chunks(vec![
                json!({"message":{"role":"assistant","content":m},"done":true}).to_string(),
            ]);
            let (r, _) = route(&client, "qwen3:0.6b", 32768, "hello");
            assert_eq!(r, Route::Think, "malformed verdict must route to think: {m}");
        }
        // A valid talk verdict still routes to talk.
        srv.set_chat_chunks(vec![json!({"message":{"role":"assistant",
            "content":"{\"route\":\"talk\",\"reason\":\"greeting\"}"},"done":true})
        .to_string()]);
        let (r, _) = route(&client, "qwen3:0.6b", 32768, "hey");
        assert_eq!(r, Route::Talk);
    }

    // V0-25: router macro-F1 on the 60 labelled turns against live qwen3:0.6b. Skipped unless
    // MOTE_LIVE=1.
    #[test]
    fn live_router_macro_f1() {
        if std::env::var("MOTE_LIVE").is_err() {
            return;
        }
        let client = Ollama::local();
        // Confusion counts for the "think" class as positive.
        let (mut tp, mut fp, mut tn, mut fn_) = (0i32, 0i32, 0i32, 0i32);
        for (text, label) in mote_testkit::ROUTER_TURNS {
            let (r, _) = route(&client, "qwen3:0.6b", 32768, text);
            let predicted_think = r == Route::Think;
            let actual_think = *label == "think";
            match (predicted_think, actual_think) {
                (true, true) => tp += 1,
                (true, false) => {
                    fp += 1;
                    println!("  FP (talk->think): {text}");
                }
                (false, false) => tn += 1,
                (false, true) => {
                    fn_ += 1;
                    println!("  FN (think->talk): {text}");
                }
            }
        }
        let f1 = |tp: i32, fp: i32, fn_: i32| -> f64 {
            let p = if tp + fp == 0 { 0.0 } else { tp as f64 / (tp + fp) as f64 };
            let r = if tp + fn_ == 0 { 0.0 } else { tp as f64 / (tp + fn_) as f64 };
            if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) }
        };
        // think as positive, then talk as positive (swap roles).
        let f1_think = f1(tp, fp, fn_);
        let f1_talk = f1(tn, fn_, fp);
        let macro_f1 = (f1_think + f1_talk) / 2.0;
        // The contract's gate is 0.85 (section 7). The decode is deterministic (temperature 0),
        // so this score is reproducible.
        println!(
            "LIVE router macro-F1 = {macro_f1:.3} (gate 0.85; think F1 {f1_think:.3}, talk F1 \
             {f1_talk:.3}; tp={tp} fp={fp} tn={tn} fn={fn_})"
        );
        assert!(macro_f1 >= 0.85, "router macro-F1 {macro_f1:.3} below the 0.85 gate");
    }

    #[test]
    fn record_turn_gives_the_next_turn_continuity() {
        let store = ContextStore::open_in_memory().unwrap();
        // Empty to start: no prior turns.
        assert_eq!(store.working_set_text("t1").unwrap(), "");
        record_turn(&store, "t1", 1000, "what is my name", "You are the owner.");
        record_turn(&store, "t1", 2000, "and my dog", "");
        let ws = store.working_set_text("t1").unwrap();
        // The owner's messages and the lane's answer are present, in order, for the next pack.
        assert!(ws.contains("you: what is my name"));
        assert!(ws.contains("mote: You are the owner."));
        assert!(ws.contains("you: and my dog"));
        let you1 = ws.find("what is my name").unwrap();
        let answer1 = ws.find("You are the owner").unwrap();
        let you2 = ws.find("and my dog").unwrap();
        assert!(you1 < answer1 && answer1 < you2, "chronological order");
        // An empty answer writes no mote line.
        assert_eq!(ws.matches("mote:").count(), 1);
    }

    #[test]
    fn emit_think_writes_events_and_one_ledger_row() {
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        let mut sub = log.subscribe("t1", 0).unwrap();

        let (pack, _system, _turn) = build_think_pack(
            &ThinkInputs {
                charter: "be kind",
                profile: "name: owner",
                memories: "",
                working_set: "",
                turn: "read notes.txt and tell me the passphrase",
            },
            THINK_BUDGET,
        );
        let events = vec![
            ThinkEvent::ToolCall {
                name: "moted_file_read".into(),
                id: Some("c1".into()),
                input: json!({ "path": "notes.txt" }),
            },
            ThinkEvent::ToolResult {
                name: "moted_file_read".into(),
                id: Some("c1".into()),
                output: "The secret passphrase is blue-otter-42.".into(),
                is_error: false,
            },
            ThinkEvent::Text { text: "The passphrase is blue-otter-42.".into() },
            ThinkEvent::Done { ok: true },
        ];
        let out = emit_think(&log, &store, "t1", "qwen3-coder:30b-64k", &pack, &events, 420);
        let (ledger_id, pack_id) = match out {
            ThinkOutcome::Done { ledger_id, pack_id, .. } => (ledger_id, pack_id),
            other => panic!("expected done, got {other:?}"),
        };
        assert!(ledger_id > 0 && pack_id > 0);

        let mut kinds = Vec::new();
        let mut done_ledger = None;
        while let Some(ev) = sub.recv_timeout(Duration::from_millis(50)) {
            if ev.kind == "think.done" {
                done_ledger = ev.payload["ledger_id"].as_i64();
            }
            kinds.push(ev.kind);
        }
        assert!(kinds.contains(&"think.tool_call".to_string()));
        assert!(kinds.contains(&"think.tool_result".to_string()));
        assert!(kinds.contains(&"think.text".to_string()));
        assert_eq!(kinds.last(), Some(&"think.done".to_string()), "done is last");
        assert_eq!(kinds.iter().filter(|k| *k == "think.done").count(), 1, "one ledger/done");
        assert_eq!(done_ledger, Some(ledger_id));
        assert_eq!(store.ledger_since(0).unwrap()[0].latency_kind, "first_token");
    }

    #[test]
    fn think_model_boundary_correlates_timing_and_records_failures_once() {
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        let (pack, system, turn) = build_think_pack(
            &ThinkInputs {
                charter: "be kind",
                profile: "",
                memories: "",
                working_set: "",
                turn: "inspect the file",
            },
            THINK_BUDGET,
        );

        let result = run_think_model_call(
            &log,
            &store,
            "t1",
            "qwen3-coder:30b-64k",
            &pack,
            &system,
            &turn,
            || Err("transport failed".into()),
        );

        assert!(matches!(result, ThinkOutcome::Failed(_)));
        assert_eq!(store.count_packs().unwrap(), 1);
        let rows = store.ledger_since(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "error");
        assert_eq!(rows[0].latency_kind, "total");
        let requests = log.recent_kind_since("model.request", 0, 10).unwrap();
        let responses = log.recent_kind_since("model.response", 0, 10).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(responses.len(), 1);
        assert_eq!(requests[0].payload["request_id"], responses[0].payload["request_id"]);
    }

    #[test]
    fn think_model_boundary_uses_the_harness_first_token_duration() {
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        let (pack, system, turn) = build_think_pack(
            &ThinkInputs {
                charter: "be kind",
                profile: "",
                memories: "",
                working_set: "",
                turn: "inspect the file",
            },
            THINK_BUDGET,
        );

        let result = run_think_model_call(
            &log,
            &store,
            "t1",
            "qwen3-coder:30b-64k",
            &pack,
            &system,
            &turn,
            || {
                Ok((
                    vec![
                        ThinkEvent::Text { text: "done".into() },
                        ThinkEvent::Done { ok: true },
                    ],
                    Duration::from_millis(42),
                ))
            },
        );

        assert!(matches!(result, ThinkOutcome::Done { .. }));
        let rows = store.ledger_since(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].latency_ms, 42);
        assert_eq!(rows[0].latency_kind, "first_token");
        let responses = log.recent_kind_since("model.response", 0, 10).unwrap();
        assert_eq!(responses[0].payload["first_token_ms"], 42);
    }

    #[test]
    fn think_verdict_makes_talk_finish_and_hand_off() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(chat_chunks(&["Let me", " think"]));
        let client = Ollama::new(srv.base_url());
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        let mut sub = log.subscribe("t1", 0).unwrap();

        // Router already decided think.
        let handoff = AtomicBool::new(true);
        let out = run_talk(
            &log, &store, &client, "t1", "qwen3:4b", 32768, &inputs("do a thing"), TALK_BUDGET,
            &handoff,
        );
        assert!(matches!(out, TalkOutcome::Handoff));

        let mut order = Vec::new();
        while let Some(ev) = sub.recv_timeout(Duration::from_millis(50)) {
            order.push(ev.kind);
        }
        assert!(order.contains(&"talk.delta".to_string()), "a sentence was spoken");
        assert!(order.contains(&"talk.handoff".to_string()), "handoff emitted");
        assert!(!order.contains(&"talk.done".to_string()), "no second answer");
        // handoff comes after at least one delta.
        let first_delta = order.iter().position(|k| k == "talk.delta").unwrap();
        let handoff_pos = order.iter().position(|k| k == "talk.handoff").unwrap();
        assert!(handoff_pos > first_delta);
        assert_eq!(store.count_packs().unwrap(), 1);
        assert_eq!(store.count_ledger().unwrap(), 1);
    }
}
