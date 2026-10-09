# Reuse: context packs and the ledger

Concept 8 in `docs/concepts.md`. Criteria MOTE-17 to MOTE-19 in `docs/rfcs/mote.md`. This
document covers how a call's context is assembled, budgeted, counted, cached, compacted and
recorded. The memory store itself is in `reuse/memory.md`. Research date: 2026-10-08.

## Key points

1. A context pack is an ordered list of parts. Each part has a kind, a source reference, a hash of its redacted bytes and a token estimate. The ledger row for a call points at its pack.
2. Parts are ordered from most stable to most volatile. Claude Code and Ollama both cache by exact prefix, so one changed byte early in the prompt recomputes everything after it.
3. Token counts are exact only after the call. `claude -p` reports them in its `result` message and Ollama reports `prompt_eval_count` and `eval_count`. Before the call mote estimates with `tiktoken-rs` and a per-model correction. Claude's tokenizer is not public.
4. Compaction is staged. Old tool results become stubs with a pointer into the event log first. A summary is written only when stubs are not enough, by a structured verdict parsed fail-closed.
5. Ledger columns use the OpenTelemetry GenAI attribute names. The conventions are still marked Development. The ledger stays in SQLite with no exporter in v0.
6. Dollar cost is an estimate. Subscription turns have no bill. Tokens are the primary unit and USD is a derived column.
7. Connector snapshots carry `fetched_ms`, a validator and a TTL. GitHub conditional requests are free on a 304. Slack history for a non-Marketplace app is one request per minute with 15 items.
8. "What did you know" is a query over the pack and ledger tables and renders as a card.
9. Everything is embedded Rust. Langfuse and Helicone are read for their data models only.

## What the concept needs technically

**Pack model.** `ContextPack { id, lane, model, budget_tokens, estimate_tokens, parts }` and
`Part { position, kind, source_ref, content_hash, tokens_est, cut: bool }`. Kinds, in prefix
order: `system`, `profile`, `tool_defs`, `memory`, `snapshot`, `summary`, `working_set`, `turn`.
`source_ref` is an event log `seq`, a snapshot id or a memory id. Serialization of anything that
lands in a prompt is deterministic. `serde_json` with `preserve_order` and one formatter.

**Caching facts that shape the pack.**
- Claude Code caches server side by prefix. On a subscription the main conversation gets a one hour TTL. Usage credits and API keys get five minutes. The cache is per model and per effort level on most models.
- Cache breakers: a model switch, a tool set change, compaction, an effort change on older models, a Claude Code upgrade, and any change to `--append-system-prompt` text within a session. `--resume` reuses the cache when the prefix is unchanged and the TTL has not passed.
- The `result` message of `claude -p --output-format json` carries `session_id`, `total_cost_usd`, `usage` with `cache_read_input_tokens`, `cache_creation_input_tokens` and `cache_creation.ephemeral_1h_input_tokens`, plus a per-model breakdown. With `--resume` the cost is the conversation total, so the ledger stores the delta.
- `--bare` skips the subscription login and needs `ANTHROPIC_API_KEY`. The docs say it will become the default for `-p`. mote must not pass `--bare`. See open decision 1.
- Ollama reuses the KV prefix inside one server process per slot. `OLLAMA_NUM_PARALLEL` defaults to 1. `keep_alive` defaults to 5 minutes. `num_ctx` defaults to 4096 and must be set per request. Flash attention enables `OLLAMA_KV_CACHE_TYPE` (`f16`, `q8_0`, `q4_0`). `prompt_eval_duration` covers uncached tokens only. Whether `prompt_eval_count` excludes cached tokens is unknown and must be measured.
- Manus's rule holds for both: no timestamps at the top, append only, never edit earlier parts in place, mask tools instead of removing them.

**Compaction and its failure modes.**
- Anthropic (2025-09-29): aim for the smallest set of high-signal tokens. Clearing old tool results is the lightest compaction. Summaries are tuned for recall first and trimmed for precision later. Notes outside the window and sub-agents are the other two tools.
- Chroma (2025-07-14, 18 models): accuracy falls with input length even on copy tasks. One similar distractor hurts and four hurt more. Mid-context positions are weakest.
- Known traps: a summary that drops an error makes the agent retry the failed action. Type-blind summarization fails abruptly (the "compaction cliff" preprint). Sliding windows split a tool call from its result. Compaction after a long break reprocesses the whole prefix uncached.
- Claude Code compacts the think lane on its own at the model's window (about 967K on 1M models, 200K otherwise). The compaction call shares the parent prefix, so it costs a cache read when warm. Summaries are about 20K tokens. mote cannot see inside that summary. It records the event and the token delta.

**Ledger.** One row per model call and one per tool call. Column names follow the OTel
registry: `gen_ai.provider.name`, `gen_ai.request.model`, `gen_ai.response.model`,
`gen_ai.conversation.id`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`,
`gen_ai.usage.cache_read.input_tokens`, `gen_ai.usage.cache_write.input_tokens`,
`gen_ai.usage.reasoning.output_tokens`, `gen_ai.tool.name`, `gen_ai.tool.call.id`. Langfuse
adds two rules worth copying: usage types are open-ended, and a price is fixed at ingestion.
Plan limits are not dollars. `mote-usage` already parses Claude rate-limit events (CRATE-13), so
the ledger stores the last known window state beside the token rows.

**Connector snapshots.** `Snapshot { id, connector, key, fetched_ms, validator, ttl_ms,
body_hash, body }`. GitHub: send `If-None-Match` or `If-Modified-Since`, a 304 is not counted
against the primary limit, and notifications polling must obey `X-Poll-Interval`. Slack: since
2025-05-29 `conversations.history` and `conversations.replies` are Tier 1 for unlisted apps, one
request per minute and 15 objects. Internal custom apps keep Tier 3. A pack part references a
snapshot id, so a card can show the age of what the model saw.

**Provenance.** Redaction runs before hashing. The hash is over redacted bytes. A card action
`what_did_you_know(call_id)` joins ledger, pack and parts and returns kind, source, age and
tokens per part.

## Candidates

| Name | Repo or product | Lang | License | Last release or availability | Verdict | Reason | Pin |
| --- | --- | --- | --- | --- | --- | --- | --- |
| tiktoken-rs | github.com/zurawiki/tiktoken-rs | Rust | MIT | 0.12.1, 2026-09-24 | embed | `o200k_base` estimator, MSRV 1.85 | `0.12.1` |
| tokenizers | github.com/huggingface/tokenizers | Rust | Apache-2.0 | 0.23.2, 2026-09-03 | embed, optional feature | exact counts for local Qwen models from `tokenizer.json` | `0.23.2` |
| Claude Code `result` usage | code.claude.com/docs/en/headless | CLI JSON | n/a | 2.1.289 installed | embed (parse) | exact tokens and cache fields after the call | `2.1.289` |
| Anthropic `count_tokens` | platform.claude.com | HTTP | n/a | available | skip | needs an API key | n/a |
| Ollama `/api/chat` counters | github.com/ollama/ollama | HTTP | MIT | installed | embed (parse) | `prompt_eval_count`, `eval_count`, durations | installed |
| OTel GenAI semconv | github.com/open-telemetry/semantic-conventions-genai | spec | Apache-2.0 | Development, no tagged release found | port the idea | column names only | `main`, read 2026-10-08 |
| opentelemetry + opentelemetry_sdk | opentelemetry-rust | Rust | Apache-2.0 | 0.33.0, 2026-09-18 | skip in v0 | no collector to send to | `0.33.0` later |
| Langfuse | github.com/langfuse/langfuse | TS | MIT core, `/ee` commercial | active, acquired by ClickHouse 2026-01 | port the idea | trace, observation, generation, open usage types, price at ingestion | n/a |
| Helicone | github.com/Helicone/helicone | TS, Rust gateway | Apache-2.0, gateway GPL-3.0 | maintenance mode since 2026-03 | skip | proxy model, no new features | n/a |
| rusqlite | github.com/rusqlite/rusqlite | Rust | MIT | 0.40.2 crate, RFC pins 0.32 | embed | ledger tables beside the event log | `0.32` |
| serde_json | github.com/serde-rs/json | Rust | MIT or Apache-2.0 | 1.0.151 | embed | deterministic pack bytes with `preserve_order` | `1` |
| jsonschema | github.com/Stranger6667/jsonschema | Rust | MIT | 0.58.6, 2026-10-06 | embed | fail-closed parse of summary verdicts | `0.58.6` |
| schemars | github.com/GREsau/schemars | Rust | MIT | 1.2.2 | embed | derive the schemas above | `1.2.2` |
| sha2 | RustCrypto | Rust | MIT or Apache-2.0 | 0.10 (Overseer uses it) | embed | part hashes | `0.10` |
| GitHub conditional requests | docs.github.com | HTTP | n/a | available | port the idea | free 304, `X-Poll-Interval` | n/a |
| Slack `conversations.history` | docs.slack.dev | HTTP | n/a | Tier 1 for unlisted apps since 2025-05-29 | port the idea with limits | 1 request per minute, 15 items | n/a |
| Manus context essay | manus.im | essay | n/a | 2025-07-18 | port the idea | KV hit rate, append only, restorable compression, keep errors | n/a |
| Anthropic context essay | anthropic.com/engineering | essay | n/a | 2025-09-29 | port the idea | smallest high-signal set, staged compaction, notes, sub-agents | n/a |
| Claude Code caching post | claude.dev/blog | essay | n/a | 2026-04-30 | port the idea | four-layer prefix, pass updates as messages, never swap tools | n/a |

## Recommendation

Crate `crates/mote-context`. It owns packs, parts, snapshots and the ledger tables. Lanes call it.

```toml
[package]
name = "mote-context"
edition = "2021"

[features]
exact-local = ["dep:tokenizers"]

[dependencies]
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["preserve_order"] }
tiktoken-rs = "0.12.1"
tokenizers = { version = "0.23.2", default-features = false, features = ["onig"], optional = true }
jsonschema = "0.58.6"
schemars = "1.2.2"
sha2 = "0.10"
anyhow = "1"
mote-redact = { path = "../mote-redact" }
mote-eventlog = { path = "../mote-eventlog" }
mote-usage = { path = "../mote-usage" }
```

```sql
CREATE TABLE context_pack (id TEXT PRIMARY KEY, lane TEXT NOT NULL, model TEXT NOT NULL,
  budget_tokens INTEGER NOT NULL, estimate_tokens INTEGER NOT NULL, created_ms INTEGER NOT NULL);
CREATE TABLE context_part (pack_id TEXT NOT NULL, position INTEGER NOT NULL, kind TEXT NOT NULL,
  source_ref TEXT NOT NULL, content_hash TEXT NOT NULL, tokens_est INTEGER NOT NULL,
  cut INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (pack_id, position));
CREATE TABLE ledger (id INTEGER PRIMARY KEY, created_ms INTEGER NOT NULL, lane TEXT NOT NULL,
  pack_id TEXT, gen_ai_provider_name TEXT, gen_ai_request_model TEXT, gen_ai_response_model TEXT,
  gen_ai_conversation_id TEXT, gen_ai_usage_input_tokens INTEGER, gen_ai_usage_output_tokens INTEGER,
  gen_ai_usage_cache_read_input_tokens INTEGER, gen_ai_usage_cache_write_input_tokens INTEGER,
  gen_ai_usage_reasoning_output_tokens INTEGER, gen_ai_tool_name TEXT, gen_ai_tool_call_id TEXT,
  usd_estimate REAL, first_token_ms INTEGER, duration_ms INTEGER, cache_break_expected INTEGER);
CREATE TABLE snapshot (id TEXT PRIMARY KEY, connector TEXT NOT NULL, key TEXT NOT NULL,
  fetched_ms INTEGER NOT NULL, validator TEXT, ttl_ms INTEGER NOT NULL, body_hash TEXT NOT NULL,
  body BLOB NOT NULL);
CREATE TABLE token_ratio (model TEXT PRIMARY KEY, exact_over_estimate REAL NOT NULL,
  samples INTEGER NOT NULL, updated_ms INTEGER NOT NULL);
```

Per-lane defaults. Talk: `num_ctx` 8192, pack budget 3000 tokens (profile 300, memories 800,
snapshots 400, working set 1200, turn the rest), volatile parts last. Router: `num_ctx` 2048,
budget 600, the turn text and one line of thread state only. Think: Claude Code owns the window.
mote injects at most 4000 tokens into the user message per turn, keeps `--append-system-prompt`
constant for the life of a session, never changes `--allowedTools` or `--mcp-config` within a
session, and opens a new session when it must.

Assembly order of operations: collect candidate parts, redact, hash, estimate with
`tiktoken-rs` times `token_ratio` for the model, cut lowest-priority parts until the budget holds
(the turn is never cut), write the pack, call the model, write the ledger row from the harness
counters, update `token_ratio`.

Commands to keep in the evidence record:

```sh
claude -p "hello" --output-format json | jq '.session_id, .total_cost_usd, .usage'
curl -s localhost:11434/api/chat -d '{"model":"qwen2.5:0.5b","messages":[{"role":"user","content":"hi"}],"stream":false,"options":{"num_ctx":2048}}' | jq '{prompt_eval_count,prompt_eval_duration,eval_count,eval_duration}'
sqlite3 mote.sqlite "select lane, count(*), sum(gen_ai_usage_input_tokens), sum(gen_ai_usage_cache_read_input_tokens) from ledger group by lane"
```

## Efficiency targets and what to measure

| Target | Number | Measure with |
| --- | --- | --- |
| Pack assembly latency | 5 ms p50, 20 ms p95 | `moted bench pack` over 200 packs |
| Estimate error, Claude models | within 10% of exact after 50 turns | `token_ratio` table and bench output |
| Estimate error, local models with `exact-local` | within 2% | same |
| Claude Code cache hit share | at least 80% of input tokens from cache across a thread within one hour | ledger `cache_read / (cache_read + cache_write + input)` |
| Ollama prefix reuse | repeated prefix `prompt_eval_duration` under 20% of the cold call | `moted bench cache` |
| Ledger write | under 1 ms per row, inside the call's transaction | bench |
| Ledger growth | about 1 KB per row, parts pruned after 90 days, hashes kept | `pragma page_count` monthly |
| GitHub polling | at least 80% of polls answered 304 | ledger of snapshot fetches |
| Slack polling | never above 1 request per minute per channel | same |
| Compaction | summary at most 10% of replaced tokens, zero repeated reads of stubbed items in tests | scripted test |
| "What did you know" | under 100 ms from click to card | window timing log |

## Open decisions with defaults

1. `--bare`. The RFC lists it. Bare mode skips the subscription login. Default: do not pass `--bare`. Watch the Claude Code changelog, because the docs say bare will become the default for `-p`, and a flag to keep the login may be needed then.
2. Estimator. Default: `tiktoken-rs` `o200k_base` with a learned ratio per model. The `exact-local` feature stays off until the ratio error exceeds 10%.
3. USD column. Default: store the delta of `total_cost_usd` per call and keep the reported total in the event. Subscription turns record the estimate and a `billed = 0` flag.
4. Compaction ladder. Default: stub tool results older than 10 turns, then summarize with the local talk model through `format` JSON schema, parsed fail-closed. A failed summary leaves stubs in place.
5. Summary content. Default: the schema has required fields `goal`, `done`, `open`, `errors`, `decisions`. `errors` must list every failed action in the span.
6. Snapshot TTLs. Default: GitHub 60 s or `X-Poll-Interval`, whichever is larger. Slack 60 s and 15 items per pull. Snapshots older than 24 h are not placed in a pack.
7. Retention. Default: parts 90 days, ledger rows forever, snapshots 7 days.
8. OTel export. Default: none. Revisit when a second machine (the Pi) needs the numbers.
9. Prompt layering for Ollama. Default: system, profile, memories, snapshots, working set, turn. The time of day goes in the turn part, never at the top.
10. Overlap with `reuse/memory.md`. MEM-01 defines `mem_pack` and MEM-13 says the working set leads every pack. Default: `mem_pack` rows become `memory` and `working_set` parts of one `context_pack` by id, and the working set sits directly before the turn, which keeps the cached prefix stable and still puts it in the strong end position. The owner resolves the order if MEM-13 must hold literally.

## Acceptance criteria

- **CTX-01** Every model call stores a pack with ordered parts, each with kind, source reference, redacted-content hash and token estimate, and its ledger row points at that pack. **Verify:** `cargo test -p mote-context pack::` green and a join of `ledger` and `context_part` after a live talk turn.
- **CTX-02** Two consecutive packs of one thread are byte-identical up to the first changed part, and only parts at or after it differ. **Verify:** unit test that diffs two packs and asserts a single changed suffix.
- **CTX-03** Budgets per lane are enforced by cutting the lowest-priority parts first, the turn part is never cut, and cut parts are recorded with `cut = 1`. **Verify:** unit test with oversized memories and a `SELECT` of cut parts.
- **CTX-04** Token estimates use `o200k_base` times a per-model ratio learned from exact counts, and after 50 live turns the estimate is within 10% of exact for each model used. **Verify:** `moted bench tokens` output saved in the evidence record.
- **CTX-05** Ledger rows carry the OTel names for input, output, cache read, cache write and reasoning tokens, first-token and total latency, and a USD estimate when the harness reports one. **Verify:** schema test plus a `SELECT` after the second think turn of a thread showing `cache_read > 0`.
- **CTX-06** The think lane never changes the system prompt text, the tool set or the model within a session, and a required change opens a new session with `cache_break_expected = 1` on the next row. **Verify:** shim test records argv per turn and asserts equality across turns.
- **CTX-07** Ollama prompts place volatile parts last, and a repeated prefix shows `prompt_eval_duration` under 20% of the cold call. **Verify:** `moted bench cache` against the real local model in the evidence record.
- **CTX-08** Compaction is staged: stubs first, then a summary from a structured verdict parsed fail-closed, and a malformed summary leaves the stubs in place. **Verify:** scripted model test with empty, truncated and prose summary outputs.
- **CTX-09** A compaction summary lists every failed action and refusal from the compacted span, and a scripted agent does not repeat a failed action after compaction. **Verify:** fixture with one failed tool call and an assertion on the summary's `errors` field and the replayed tool calls.
- **CTX-10** The "what did you know" action on any card lists every part of that call's pack with kind, source, age and tokens within 100 ms. **Verify:** window screenshot and the timing log entry.
- **CTX-11** Connector snapshots store `fetched_ms`, validator and TTL, GitHub polls send `If-None-Match` or `If-Modified-Since` and respect `X-Poll-Interval`, and Slack history polls stay at or under one request per minute with 15 items. **Verify:** scripted HTTP server test that counts headers and rates.
- **CTX-12** Every part is redacted before hashing, and a planted secret in a snapshot never appears in a pack, a ledger row or a card. **Verify:** test with a fake token in a snapshot body and a `grep` of the database file.
- **CTX-13** Daily and per-lane ledger totals are queryable and the window shows them. **Verify:** `SELECT` output and a screenshot.

## Sources

- Anthropic, Effective context engineering for AI agents (2025-09-29): https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents
- Claude Code, prompt caching: https://code.claude.com/docs/en/prompt-caching
- Claude Code, run programmatically (`-p`, `--bare`, `result` fields): https://code.claude.com/docs/en/headless
- Claude Code, costs and auto-compact: https://code.claude.com/docs/en/costs and https://code.claude.com/docs/en/model-config
- Lessons from building Claude Code: prompt caching is everything (2026-04-30): https://claude.dev/blog/lessons-from-building-claude-code-prompt-caching-is-everything/
- Claude Platform, compaction overview: https://platform.claude.com/docs/en/build-with-claude/compaction
- Manus, Context engineering for AI agents (2025-07-18): https://manus.im/en/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus
- Chroma, Context rot (2025-07-14): https://www.trychroma.com/research/context-rot
- The Compaction Cliff (preprint): https://arxiv.org/pdf/2608.22752
- Compaction traps: https://tianpan.co/blog/2026-04-19-compaction-traps-long-running-agents
- Ollama API: https://github.com/ollama/ollama/blob/main/docs/api.md and FAQ: https://docs.ollama.com/faq
- OTel GenAI attributes registry: https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/registry/attributes/gen-ai.md
- OTel GenAI client inference spans: https://github.com/open-telemetry/semantic-conventions-genai/blob/main/docs/gen-ai/client-inference.md
- tiktoken-rs: https://docs.rs/crate/tiktoken-rs/latest and tokenizers: https://github.com/huggingface/tokenizers
- Langfuse data model: https://langfuse.com/docs/observability/data-model and open source: https://langfuse.com/docs/open-source
- Helicone self-hosting: https://docs.helicone.ai/getting-started/self-host/overview
- GitHub REST best practices (conditional requests): https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api
- GitHub notifications polling: https://docs.github.com/en/rest/activity/notifications
- Slack rate limit changes for non-Marketplace apps (2025-05-29): https://docs.slack.dev/changelog/2025/05/29/rate-limit-changes-for-non-marketplace-apps
- Slack conversations.history: https://docs.slack.dev/reference/methods/conversations.history
- Claude tokenizer not public (secondary): https://blog.gopenai.com/counting-claude-tokens-without-a-tokenizer-e767f2b6e632
