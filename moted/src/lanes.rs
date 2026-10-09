//! The talk lane and the router. See docs/rfcs/v0.md sections 6 and 7.
//!
//! The talk lane builds a five-part pack within budget, streams the talk model as `talk.delta`
//! events and ends with one `talk.done` carrying the ledger row id. If the talk model cannot
//! load it emits `talk.unavailable` and uses no other model. The router runs on every turn in
//! parallel; any invalid verdict is fail-closed to `think`. On a `think` verdict the talk lane
//! finishes its sentence, emits `talk.handoff` and gives no second answer.

use mote_context::{count_tokens, ContextStore, LedgerEntry, Pack, PackBuilder, PartKind};
use mote_eventlog::EventLog;
use mote_localmodel::{ChatOptions, Ollama};
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

/// Default talk pack budget in tokens.
pub const TALK_BUDGET: usize = 600;

/// The fixed few-shot prompt for the router, stored in the repo.
pub const ROUTER_PROMPT: &str = "\
You are a router. Classify the user's latest message into one of two lanes and reply with only a \
JSON object {\"route\": \"talk\" | \"think\", \"reason\": string}. Use \"talk\" for chit-chat, \
greetings, acknowledgements and simple questions a small model answers instantly. Use \"think\" \
for anything needing tools, files, code, multi-step reasoning or memory lookup.\n\
Examples:\n\
user: hey there -> {\"route\":\"talk\",\"reason\":\"greeting\"}\n\
user: thanks! -> {\"route\":\"talk\",\"reason\":\"acknowledgement\"}\n\
user: read my config file and summarise it -> {\"route\":\"think\",\"reason\":\"needs file tools\"}\n\
user: refactor this function -> {\"route\":\"think\",\"reason\":\"needs code reasoning\"}";

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
            "route": { "type": "string", "enum": ["talk", "think"] },
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
    Done { ledger_id: i64, pack_id: i64 },
    Handoff,
    Unavailable(String),
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
    let opts = ChatOptions::new(num_ctx);

    let mut answer = String::new();
    let mut handed_off = false;
    let res = client.chat_stream(model, messages, &opts, |chunk| {
        if handed_off {
            return;
        }
        if !chunk.content.is_empty() {
            answer.push_str(&chunk.content);
            let _ = log.append(thread_id, "talk.delta", &json!({ "text": chunk.content }));
        }
        // Finish the current sentence, then hand off.
        if handoff.load(Ordering::Acquire) {
            let _ = log.append(thread_id, "talk.handoff", &json!({}));
            handed_off = true;
        }
    });

    match res {
        Err(e) => {
            let reason = e.to_string();
            let _ = log.append(thread_id, "talk.unavailable", &json!({ "reason": reason.clone() }));
            TalkOutcome::Unavailable(reason)
        }
        Ok(()) => {
            if handed_off {
                return TalkOutcome::Handoff;
            }
            let entry = LedgerEntry {
                lane: "talk".into(),
                model: model.into(),
                prompt_tokens: pack.token_estimate() as i64,
                completion_tokens: count_tokens(&answer) as i64,
                latency_ms: 0,
                cost_usd: 0.0,
                status: "ok".into(),
            };
            match store.record(&pack, &entry) {
                Ok((pack_id, ledger_id)) => {
                    let _ = log.append(
                        thread_id,
                        "talk.done",
                        &json!({ "ledger_id": ledger_id, "pack_id": pack_id }),
                    );
                    TalkOutcome::Done { ledger_id, pack_id }
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
        let (r, reason) = route(&rclient, &rmodel, router_ctx, &rmsg);
        let _ = rlog.append(
            &rthread,
            "router.verdict",
            &json!({ "route": r.as_str(), "reason": reason }),
        );
        if r == Route::Think {
            rhandoff.store(true, Ordering::Release);
        }
    });

    let outcome =
        run_talk(log, store, client, thread_id, talk_model, talk_ctx, inputs, budget, &handoff);
    let _ = router_handle.join();
    outcome
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
        // Only the talk model was ever requested; no fallback model.
        for r in srv.recorded() {
            if r.path == "/api/chat" {
                assert_eq!(r.body["model"], "qwen3:4b");
            }
        }
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
    }
}
