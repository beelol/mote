#![allow(dead_code)]

use crate::model_call::{ledger_entry, ModelRequest};
use mote_context::{count_tokens, ContextStore, PackBuilder, PartKind};
use mote_eventlog::EventLog;
use mote_localmodel::Ollama;
use mote_redact::redact;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::time::{Duration, Instant};

pub(crate) const SURFACE_MODEL: &str = "qwen3:4b";
pub(crate) const SURFACE_CONTEXT: u32 = 32_768;
pub(crate) const MAX_SOURCE_BYTES: usize = 262_144;
const RETRY_WINDOW: Duration = Duration::from_secs(60);
const RETRIES_PER_WINDOW: usize = 3;

const SURFACE_PROMPT: &str = r#"You generate one compact Matrix interface for the request JSON in this conversation.
Read request as the task. Prose is the completed answer, not a replacement task.
Previous_surface and diagnostics are context for repairs, not requests.
An explicit request to show a panel, form, list, progress, row, selection control, slider, stack, table, text surface or toggle requires kind matrix.
Use kind none for a plain explanation that does not request an interface.
Return only one JSON object. A matrix object has kind and source. A none object has only kind.
Matrix is a declaration language, not JSON, HTML, JavaScript or a function call.
The complete source starts with screen { and ends with }. Strings in source use double quotes escaped inside the JSON string.
Use the following exact syntax examples. Adapt literal labels to the request. Keep host paths and action names exactly as shown.
Plain explanation:
{"kind":"none"}
Panel with refresh button:
{"kind":"matrix","source":"screen { text \"Panel\" button \"Refresh\" -> refresh }"}
Form with local-only boolean field (boolean fields use toggle):
{"kind":"matrix","source":"screen { col { toggle \"Local only\" models.local_only } }"}
List of available host items:
{"kind":"matrix","source":"screen { list from host.items { item { text .label } } }"}
Progress:
{"kind":"matrix","source":"screen { progress value host.progress }"}
Two labels side by side:
{"kind":"matrix","source":"screen { row { text \"First\" text \"Second\" } }"}
Talk model selection:
{"kind":"matrix","source":"screen { select \"Talk model\" models.talk options model_options }"}
Vertical stack of three labels:
{"kind":"matrix","source":"screen { col { text \"First\" text \"Second\" text \"Third\" } }"}
Pipeline stage table (columns go directly inside table):
{"kind":"matrix","source":"screen { table from pipeline.stages { column \"Stage\" .stage column \"Calls\" .calls } }"}
Literal text surface:
{"kind":"matrix","source":"screen { text \"Requested text\" }"}
Local-only toggle:
{"kind":"matrix","source":"screen { toggle \"Local only\" models.local_only }"}
Router latency budget:
{"kind":"matrix","source":"screen { slider \"Router budget\" budgets.router min 50 max 1000 step 10 }"}
Slider min, max and step are unquoted numeric literals. Never quote 50, 1000 or 10.
Host-owned manifest: fields models.local_only:boolean, models.talk:Model, models.router:Model, models.think:Model, budgets.router:integer.
Values model_options:list Model, pipeline.stages:list PipelineStage(stage,calls,latency), host.progress:number, host.items:list HostItem(label).
Only action refresh exists and takes no arguments. Do not create bindings or actions.
Use only screen, col, row, text, progress value, toggle, select options, slider min max step, button ->, list from with item, and table from with column.
Do not add selection, keys, overlays, pending states, failure states or action arguments.
Now return the decision for request, even when prose is already complete or diagnostics is empty."#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SurfaceDecision {
    None,
    Matrix { source: String },
}

impl<'de> Deserialize<'de> for SurfaceDecision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DecisionVisitor;

        impl<'de> Visitor<'de> for DecisionVisitor {
            type Value = SurfaceDecision;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a strict surface decision object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut kind = None;
                let mut source = None;
                let mut source_seen = false;
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "kind" => {
                            if kind.is_some() {
                                return Err(serde::de::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value::<String>()?);
                        }
                        "source" => {
                            if source_seen {
                                return Err(serde::de::Error::duplicate_field("source"));
                            }
                            source_seen = true;
                            source = Some(map.next_value::<String>()?);
                        }
                        _ => return Err(serde::de::Error::unknown_field(&key, &["kind", "source"])),
                    }
                }
                match kind.as_deref() {
                    Some("none") if !source_seen => Ok(SurfaceDecision::None),
                    Some("matrix") if source_seen => {
                        let source = source.unwrap_or_default();
                        if source.is_empty() || source.len() > MAX_SOURCE_BYTES {
                            return Err(serde::de::Error::custom(
                                "matrix source must contain 1 through 262144 bytes",
                            ));
                        }
                        Ok(SurfaceDecision::Matrix { source })
                    }
                    _ => Err(serde::de::Error::custom("unknown or malformed surface decision")),
                }
            }
        }

        deserializer.deserialize_map(DecisionVisitor)
    }
}

pub(crate) fn surface_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["matrix"] },
                    "source": { "type": "string", "minLength": 1, "maxLength": MAX_SOURCE_BYTES }
                },
                "required": ["kind", "source"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": { "kind": { "type": "string", "enum": ["none"] } },
                "required": ["kind"],
                "additionalProperties": false
            }
        ]
    })
}

pub(crate) struct SurfaceInputs<'a> {
    pub(crate) request: &'a str,
    pub(crate) prose: &'a str,
    pub(crate) previous_source: &'a str,
    pub(crate) diagnostics: &'a str,
}

#[derive(Default)]
pub(crate) struct RetryBudget {
    attempts: HashMap<(String, i64), VecDeque<Instant>>,
}

impl RetryBudget {
    pub(crate) fn acquire(&mut self, thread: &str, surface_seq: i64, now: Instant) -> bool {
        let attempts = self.attempts.entry((thread.to_owned(), surface_seq)).or_default();
        while attempts
            .front()
            .is_some_and(|attempt| now.saturating_duration_since(*attempt) >= RETRY_WINDOW)
        {
            attempts.pop_front();
        }
        if attempts.len() >= RETRIES_PER_WINDOW {
            return false;
        }
        attempts.push_back(now);
        true
    }
}

fn labeled_content(label: &str, value: &str) -> String {
    let mut object = serde_json::Map::new();
    object.insert(label.to_string(), json!(value));
    serde_json::Value::Object(object).to_string()
}

fn shrink(text: &mut String) {
    if text.is_empty() {
        return;
    }
    let mut end = if text.len() == 1 { 0 } else { text.len() * 7 / 8 };
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}

pub(crate) fn surface_message_contents(inputs: &SurfaceInputs<'_>) -> [String; 5] {
    let mut dynamic = [
        redact(inputs.request),
        redact(inputs.prose),
        redact(inputs.previous_source),
        redact(inputs.diagnostics),
    ];
    loop {
        let contents = [
            SURFACE_PROMPT.to_string(),
            labeled_content("request", &dynamic[0]),
            labeled_content("prose", &dynamic[1]),
            labeled_content("previous_surface", &dynamic[2]),
            labeled_content("diagnostics", &dynamic[3]),
        ];
        let tokens: usize = contents.iter().map(|content| count_tokens(content)).sum();
        if tokens <= SURFACE_CONTEXT as usize - 512 {
            return contents;
        }
        let Some(index) = [2, 1, 0, 3].into_iter().find(|index| !dynamic[*index].is_empty()) else {
            return contents;
        };
        shrink(&mut dynamic[index]);
    }
}

pub(crate) fn generate_surface(
    log: &EventLog,
    store: &ContextStore,
    client: &Ollama,
    inputs: &SurfaceInputs<'_>,
) -> Result<SurfaceDecision, String> {
    let contents = surface_message_contents(inputs);
    let pack = PackBuilder::new("surface")
        .part(PartKind::Charter, "surface_prompt", &contents[0])
        .part(PartKind::Profile, "request", &contents[1])
        .part(PartKind::Memory, "prose", &contents[2])
        .part(PartKind::WorkingSet, "previous_surface", &contents[3])
        .part(PartKind::Turn, "diagnostics", &contents[4])
        .build(SURFACE_CONTEXT as usize);
    let messages = serde_json::Value::Array(
        contents
            .iter()
            .enumerate()
            .map(|(index, content)| {
                json!({
                    "role": if index == 0 { "system" } else { "user" },
                    "content": content
                })
            })
            .collect(),
    );
    let schema = surface_schema();
    let request = ModelRequest::begin(
        log,
        "surface",
        SURFACE_MODEL,
        json!({
            "num_ctx": SURFACE_CONTEXT,
            "think": false,
            "temperature": 0.0,
            "format": schema,
            "messages": messages
        }),
    )
    .map_err(|error| format!("request log failed: {error}"))?;

    let result = client.chat_structured_with_raw::<SurfaceDecision>(
        SURFACE_MODEL,
        messages,
        schema,
        SURFACE_CONTEXT,
    );
    match result {
        Ok(reply) => {
            let timing = request.finish("ok", None);
            let entry = ledger_entry(
                "surface",
                SURFACE_MODEL,
                &pack,
                &reply.content,
                timing,
                false,
                "ok",
            );
            store.record(&pack, &entry).map_err(|error| format!("ledger write failed: {error}"))?;
            Ok(reply.value)
        }
        Err(error) => {
            let timing = request.finish("error", None);
            let completion = error.content.as_deref().unwrap_or("");
            let entry = ledger_entry(
                "surface",
                SURFACE_MODEL,
                &pack,
                completion,
                timing,
                false,
                "error",
            );
            store.record(&pack, &entry).map_err(|record_error| {
                format!("surface call failed: {error}; ledger write failed: {record_error}")
            })?;
            Err(error.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_context::{count_tokens, hash_text, ContextStore, PartKind};
    use mote_eventlog::EventLog;
    use mote_localmodel::{parse_verdict, Ollama};
    use mote_redact::redact;
    use mote_testkit::ScriptedServer;
    use serde_json::json;

    const SECRET: &str = "sk-abcdefghijklmnopqrstuvwxyz";

    fn inputs<'a>() -> SurfaceInputs<'a> {
        SurfaceInputs {
            request: "show controls sk-abcdefghijklmnopqrstuvwxyz",
            prose: "done ghp_0123456789abcdefghijklmnopqrstuvwx",
            previous_source: "Text(\"xoxb-1234567890-abcdefghij\")",
            diagnostics: "Bearer abcdefghijklmnopqrstuvwxyz",
        }
    }

    #[test]
    fn retry_budget_expires_attempts_at_the_rolling_minute_boundary() {
        let mut budget = RetryBudget::default();
        let start = Instant::now();
        assert!(budget.acquire("main", 7, start));
        assert!(budget.acquire("main", 7, start + Duration::from_secs(1)));
        assert!(budget.acquire("main", 7, start + Duration::from_secs(2)));
        assert!(!budget.acquire("main", 7, start + Duration::from_secs(59)));
        assert!(budget.acquire("main", 7, start + Duration::from_secs(60)));
    }

    fn reply(content: &str) -> Vec<String> {
        vec![json!({
            "message": {"role":"assistant","content": content},
            "done": true
        })
        .to_string()]
    }

    #[test]
    fn surface_decision_rejects_malformed_and_duplicate_envelopes() {
        for malformed in [
            r#"{"kind":"matrix"}"#,
            r#"{"kind":"matrix","source":4}"#,
            r#"{"kind":"none","extra":true}"#,
            r#"{"kind":"none"} trailing"#,
            r#"the answer is none"#,
            r#"{"kind":"none","kind":"matrix"}"#,
            r#"{"kind":"matrix","source":"a","source":"b"}"#,
        ] {
            assert!(
                parse_verdict::<SurfaceDecision>(malformed).is_err(),
                "malformed decision accepted: {malformed}"
            );
        }
        assert!(parse_verdict::<SurfaceDecision>(r#"{"kind":"matrix","source":""}"#).is_err());
        let oversized = format!(
            "{{\"kind\":\"matrix\",\"source\":\"{}\"}}",
            "x".repeat(MAX_SOURCE_BYTES + 1)
        );
        assert!(parse_verdict::<SurfaceDecision>(&oversized).is_err());
    }

    #[test]
    fn surface_call_uses_fixed_local_options_and_strict_schema() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(reply(r#"{"kind":"none"}"#));
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();

        let decision = generate_surface(&log, &store, &Ollama::new(srv.base_url()), &inputs())
            .unwrap();

        assert_eq!(decision, SurfaceDecision::None);
        let request = srv.recorded().into_iter().find(|r| r.path == "/api/chat").unwrap();
        assert_eq!(request.body["model"], SURFACE_MODEL);
        assert_eq!(request.body["options"]["num_ctx"], SURFACE_CONTEXT);
        assert_eq!(request.body["think"], false);
        assert_eq!(request.body["options"]["temperature"], 0.0);
        assert_eq!(request.body["stream"], false);
        assert_eq!(
            request.body["format"],
            json!({
                "type": "object",
                "oneOf": [
                    {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "enum": ["matrix"] },
                            "source": { "type": "string", "minLength": 1, "maxLength": 262144 }
                        },
                        "required": ["kind", "source"],
                        "additionalProperties": false
                    },
                    {
                        "type": "object",
                        "properties": { "kind": { "type": "string", "enum": ["none"] } },
                        "required": ["kind"],
                        "additionalProperties": false
                    }
                ]
            })
        );
    }

    #[test]
    fn surface_call_redacts_outbound_pack_and_events_before_use() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(reply(r#"{"kind":"none"}"#));
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();

        generate_surface(&log, &store, &Ollama::new(srv.base_url()), &inputs()).unwrap();

        let request = srv.recorded().into_iter().find(|r| r.path == "/api/chat").unwrap();
        let outbound = request.body.to_string();
        for secret_fragment in [SECRET, "ghp_0123456789", "xoxb-1234567890", "Bearer abcdef"] {
            assert!(!outbound.contains(secret_fragment), "outbound leaked {secret_fragment}");
        }
        assert!(outbound.contains("[redacted]"));

        let events = log.recent_kind_since("model.request", 0, 10).unwrap();
        assert_eq!(events.len(), 1);
        assert!(!events[0].payload.to_string().contains(SECRET));

        let expected = [
            (PartKind::Charter, SURFACE_PROMPT.to_string()),
            (
                PartKind::Profile,
                json!({"request": redact(inputs().request)}).to_string(),
            ),
            (
                PartKind::Memory,
                json!({"prose": redact(inputs().prose)}).to_string(),
            ),
            (
                PartKind::WorkingSet,
                json!({"previous_surface": redact(inputs().previous_source)}).to_string(),
            ),
            (
                PartKind::Turn,
                json!({"diagnostics": redact(inputs().diagnostics)}).to_string(),
            ),
        ];
        let parts_json: String = store
            .conn()
            .query_row("SELECT parts FROM packs LIMIT 1", [], |row| row.get(0))
            .unwrap();
        let parts: Vec<mote_context::Part> = serde_json::from_str(&parts_json).unwrap();
        for (kind, text) in expected {
            let part = parts.iter().find(|part| part.kind == kind).unwrap();
            assert_eq!(part.hash, hash_text(&text));
        }
    }

    #[test]
    fn surface_call_records_one_pack_row_and_event_pair_on_success_and_failure() {
        for (status, content) in [
            ("ok", r#"{"kind":"matrix","source":"Text(\"Ready\")"}"#),
            ("error", r#"{"kind":"matrix"}"#),
        ] {
            let srv = ScriptedServer::start().unwrap();
            srv.set_chat_chunks(reply(content));
            let log = EventLog::open_in_memory().unwrap();
            let store = ContextStore::open_in_memory().unwrap();

            let result = generate_surface(
                &log,
                &store,
                &Ollama::new(srv.base_url()),
                &inputs(),
            );

            assert_eq!(result.is_ok(), status == "ok");
            assert_eq!(store.count_packs().unwrap(), 1);
            assert_eq!(store.count_ledger().unwrap(), 1);
            let rows = store.ledger_since(0).unwrap();
            assert_eq!(rows[0].lane, "surface");
            assert_eq!(rows[0].model, SURFACE_MODEL);
            assert_eq!(rows[0].status, status);
            assert_eq!(rows[0].completion_tokens, count_tokens(content) as i64);
            assert_eq!(log.recent_kind_since("model.request", 0, 10).unwrap().len(), 1);
            assert_eq!(log.recent_kind_since("model.response", 0, 10).unwrap().len(), 1);
        }
    }

    #[test]
    fn surface_call_sends_only_the_redacted_text_accounted_by_its_pack() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(reply(r#"{"kind":"none"}"#));
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        let oversized_request = "word ".repeat(100_000);
        let inputs = SurfaceInputs {
            request: &oversized_request,
            prose: "answer",
            previous_source: "screen { text \"prior\" }",
            diagnostics: "none",
        };

        generate_surface(&log, &store, &Ollama::new(srv.base_url()), &inputs).unwrap();

        let request = srv.recorded().into_iter().find(|r| r.path == "/api/chat").unwrap();
        let messages = request.body["messages"].as_array().unwrap();
        let sent_tokens: usize = messages
            .iter()
            .map(|message| count_tokens(message["content"].as_str().unwrap()))
            .sum();
        assert!(sent_tokens <= SURFACE_CONTEXT as usize);
        assert!(!request.body.to_string().contains(&oversized_request));
    }

    #[test]
    fn surface_pack_accounts_for_exact_escaped_message_contents() {
        let srv = ScriptedServer::start().unwrap();
        srv.set_chat_chunks(reply(r#"{"kind":"none"}"#));
        let log = EventLog::open_in_memory().unwrap();
        let store = ContextStore::open_in_memory().unwrap();
        let inputs = SurfaceInputs {
            request: "show \"quoted\" \\ path",
            prose: "line\nnext",
            previous_source: "screen { text \"\\\"quoted\\\"\" }",
            diagnostics: "bad\tpath\\",
        };

        generate_surface(&log, &store, &Ollama::new(srv.base_url()), &inputs).unwrap();

        let request = srv.recorded().into_iter().find(|r| r.path == "/api/chat").unwrap();
        let messages: Vec<&str> = request.body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|message| message["content"].as_str().unwrap())
            .collect();
        assert_eq!(
            messages,
            [
                SURFACE_PROMPT,
                r#"{"request":"show \"quoted\" \\ path"}"#,
                r#"{"prose":"line\nnext"}"#,
                r#"{"previous_surface":"screen { text \"\\\"quoted\\\"\" }"}"#,
                r#"{"diagnostics":"bad\tpath\\"}"#,
            ]
        );
        let parts_json: String = store
            .conn()
            .query_row("SELECT parts FROM packs LIMIT 1", [], |row| row.get(0))
            .unwrap();
        let parts: Vec<mote_context::Part> = serde_json::from_str(&parts_json).unwrap();
        assert_eq!(parts.len(), messages.len());
        for (part, content) in parts.iter().zip(&messages) {
            assert_eq!(part.hash, hash_text(content));
            assert_eq!(part.tokens, count_tokens(content));
        }
        let accounted_tokens: i64 = store
            .conn()
            .query_row("SELECT token_estimate FROM packs LIMIT 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            accounted_tokens,
            messages.iter().map(|text| count_tokens(text) as i64).sum::<i64>()
        );
        assert!(accounted_tokens <= SURFACE_CONTEXT as i64);
    }

    #[test]
    fn surface_prompt_requires_unquoted_numeric_slider_bounds() {
        assert!(SURFACE_PROMPT.contains(
            "Slider min, max and step are unquoted numeric literals. Never quote 50, 1000 or 10."
        ));
    }
}
