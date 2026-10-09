# Reuse: scripted-model testing

What mote reuses so every lane is tested with no network and no paid turn. Covers MOTE-03 and
the fixture work in CRATE-09, CRATE-10, CRATE-11 and CRATE-17. Researched 2026-10-08.

## Key points

1. Three fakes cover every lane: `scripted-model` speaks Ollama's `/api/chat`, `fake-claude` speaks Claude Code's stream-json with the control protocol, and a test `Handler` in `mote-ipc` stands in for overseerd. All three are Rust binaries in the workspace and load no weights.
2. The owner's `scripted-model.mjs` is the pattern: decide the next reply from the conversation so far (which tool calls happened, which turn this is), stream it, estimate usage, serve `/health`. mote ports the idea to Rust and to Ollama's newline JSON shape, since the Node server speaks OpenAI's SSE shape.
3. `fake-claude` keeps `claude-fixture.js`'s mode names and env vars (`FIXTURE_MODE`, `CLAUDE_FIXTURE_MODE_FILE`) so Overseer's ported tests run unchanged, and adds a `partial` mode with `stream_event` deltas.
4. Real transcripts are recorded once with `MOTE_RECORD=<file>` through `mote-redact`, stored under `fixtures/transcripts/`, and replayed into golden `Norm` snapshots with `insta`.
5. `proptest` guards the fail-closed parsers: router verdict, UI tree validator, redaction. `wiremock` mocks Slack and GitHub HTTP. `#[tokio::test(start_paused = true)]` makes watcher tests instant.
6. Overseer's `daemon/tests/common/mod.rs` helper (spawn the daemon with a temp home, `call`, `raw`, `events`, `wait_status`) is ported as `mote-testkit::Daemon`. The named `protocol.rs` tests move with their crates.
7. No network is enforced, not assumed. Tests point `MOTE_CLAUDE_PATH` and `MOTE_OLLAMA_URL` at the fakes, and the daemon's HTTP client refuses non-loopback hosts when `MOTE_TEST_LOOPBACK_ONLY=1`.
8. Live checks exist but are `#[ignore]` behind `MOTE_LIVE=1`, and their output goes to `docs/verification/`.

## What the concept needs technically

Scripted model server. A `[[bin]] scripted-model` in `crates/mote-testkit` listening on `127.0.0.1:<port>` (port 0 then printed, or `MOTE_SCRIPTED_PORT`). Routes:

| route | behaviour |
| --- | --- |
| `GET /health` | `{"ok":true,"script":"<name>","turns":n}` |
| `GET /api/tags` | the models the script declares, with `size` so the memory guard has a number |
| `GET /api/ps` | loaded models, driven by the script |
| `POST /api/show` | `model_info` with `llama.context_length` and `capabilities` |
| `POST /api/chat` | the scripted reply as newline JSON chunks, each `{"model","created_at","message":{"role":"assistant","content":"..."},"done":false}`, then `{"done":true,"done_reason":"stop","total_duration","load_duration","prompt_eval_count","eval_count","eval_duration"}` |

With `stream: false` it returns one object. With `format` set to `"json"` or a schema, the scripted object is sent as one chunk of `message.content`. Tool calls stream as `message.tool_calls`. A `delay_ms` per step and a `chunk_ms` between chunks make latency tests deterministic.

Script format. A JSON file with ordered steps keyed by exact match on the last user message, on the turn index, or on the set of tools offered:

```json
{"name": "router-basic", "models": [{"name": "qwen2.5:0.5b", "size": 397821319, "context": 32768}],
 "steps": [
  {"when": {"user": "morning"}, "reply": {"text": "{\"route\":\"talk\",\"reason\":\"greeting\"}"}},
  {"when": {"turn": 2}, "reply": {"text": "not json at all"}},
  {"when": {"tools_include": "delegate"}, "reply": {"tool_call": {"name": "delegate", "arguments": {"repo": "/tmp/r"}}}},
  {"when": {"any": true}, "reply": {"text": "{\"route\":\"think\",\"reason\":\"default\"}", "delay_ms": 400}}
 ]}
```

Matching is exact or structural. No regex over the prompt, so the fake cannot drift into judgment.

Fake Claude. A `[[bin]] fake-claude` that answers `--version` and `auth status`, reads the first stdin `user` line, and emits the mode's lines. Modes carried from `claude-fixture.js`: `nested`, `permission`, `background`, `background-early`, `slow`, `limits`, `limits-low`, `echo`, `ratelimit`, `quota`, `auth`, `stop-live`, `unparsed`, `failed-reason`, `prose`. New: `partial` (an `init`, then `stream_event` `content_block_delta` text deltas every `FIXTURE_CHUNK_MS`, then the `assistant` text, then `result` with `total_cost_usd`, `usage`, `modelUsage`), and `mcp` (a `tool_use` on `mcp__mote__memory_write` and its `tool_result`). It exits 130 on an interrupt control request, as the Node fixture does.

Fake overseerd. `mote-ipc` ships a test `Handler` that answers `hello`, `task.create`, `run.permission`, `run.interrupt`, `events.list` and serves `events.subscribe` from an in-memory `EventLog`. Work-lane tests that need a real overseerd use `mote-testkit::Overseer::start` with a temp `OVERSEER_HOME`, ported from Overseer's helper.

Record and replay. `MOTE_RECORD=fixtures/transcripts/claude-2.1.289-<name>.jsonl moted think --thread t` writes every raw line after `mote-redact`. A golden test replays each file through `mote-harness::parse` and compares with `tests/golden/<name>.norm.json` through `insta::assert_json_snapshot!` with redactions for `ts`, `seq`, `uuid` and `session_id`.

Daemon helper. `Daemon::start(env)` spawns `moted serve` with `MOTE_HOME=<tempdir>`, waits for `hello`, exposes `call`, `try_call`, `raw`, `events(scope)`, `wait_status`, `kill9`, `shutdown`. Its `Drop` interrupts active runs and kills the process. Fixture paths default to `/nonexistent/harness-disabled-in-tests` unless a test sets them.

Network guard. `MOTE_TEST_LOOPBACK_ONLY=1` makes `reqwest::ClientBuilder` in `mote-localmodel` and the watchers reject any host that is not `127.0.0.1`, `::1` or `localhost`, and makes `mote-harness::resolve_program` refuse a `claude` outside the workspace `target/` directory. `cargo test` sets it in `.cargo/config.toml` `[env]`.

## Candidates

| name | repo | language | license | last release | verdict | reason | pin |
| --- | --- | --- | --- | --- | --- | --- | --- |
| scripted-model.mjs | ~/projects/evalsagent/bin | Node | owner's | n/a | port the idea | Decision from conversation state, streaming, usage estimate, `/health`. OpenAI shape, not Ollama. | n/a |
| claude-fixture.js, replay.js, codex-app-fixture.js, account-cli.js | ~/projects/overseer/fixtures/fake-harness | Node | MIT | a7a5cac0 | port to Rust, keep copies | Same modes and env vars. Node copies stay under `fixtures/` for parity diffs and for CRATE-10. | a7a5cac0 |
| transcripts `*.jsonl` | ~/projects/overseer/fixtures/transcripts | JSONL | MIT | a7a5cac0 | embed | Four live and mock transcripts for golden tests. | a7a5cac0 |
| `protocol.rs`, `common/mod.rs` | ~/projects/overseer/daemon/tests | Rust | MIT | a7a5cac0 | port | The daemon helper and the named tests. | a7a5cac0 |
| insta | github.com/mitsuhiko/insta | Rust | Apache-2.0 | 1.49.0, 2026-10-03 | embed | `assert_json_snapshot!`, redactions, `cargo insta review`. | 1.49.0 |
| expect-test | github.com/rust-analyzer/expect-test | Rust | MIT or Apache-2.0 | 1.5.1, 2024-12-21 | skip | Inline literals. insta's redactions matter more. | none |
| proptest | github.com/proptest-rs/proptest | Rust | MIT or Apache-2.0 | 1.11.0, 2026-03-24 | embed | Shrinking, passive but current maintenance. | 1.11.0 |
| quickcheck | github.com/BurntSushi/quickcheck | Rust | Unlicense or MIT | 1.1.0, 2026-02-10 | skip | Weaker shrinking. | none |
| cargo-fuzz | github.com/rust-fuzz/cargo-fuzz | Rust | MIT or Apache-2.0 | 0.13.2, 2026-06-09 | skip in v0 | Needs nightly. The parsers are small. | none |
| wiremock | github.com/LukeMathWalker/wiremock-rs | Rust | MIT or Apache-2.0 | 0.6.5, 2025-08-24 | embed | Async, matchers, `set_delay`, call counts. No streaming bodies, which Slack and GitHub do not need. | 0.6.5 |
| httpmock | github.com/httpmock/httpmock | Rust | MIT | 0.8.3, 2026-02-04 | skip | Record and playback through a proxy. Heavier than needed. | none |
| mockito | github.com/lipanski/mockito | Rust | MIT | 1.7.2, 2026-02-02 | skip | Chunked bodies, but the Ollama fake is a real server anyway. | none |
| llmposter | github.com/SkillDoAI/llmposter | Rust | AGPL-3.0 | 0.5.0, 2026-07-07 | skip | AGPL, and no native Ollama `/api/chat`. | none |
| aimock | github.com/CopilotKit/aimock | Node | MIT | 1.44.0 | skip | Node runtime. Ollama framing undocumented. | none |
| rvcr | github.com/ChorusOne/rvcr | Rust | Apache-2.0 | 0.2.1, 2024-03-05 | skip | reqwest-middleware only. mote records at the harness line level. | none |
| assert_cmd | github.com/assert-rs/assert_cmd | Rust | MIT or Apache-2.0 | 2.2.2, 2026-05-11 | embed | `cargo_bin!` finds the fake binaries. | 2.2.2 |
| tempfile | github.com/Stebalien/tempfile | Rust | MIT or Apache-2.0 | 3.27.0 | embed | Temp homes per test. | 3.27.0 |
| cargo-nextest | github.com/nextest-rs/nextest | Rust | MIT or Apache-2.0 | 0.9.148, 2026-10-08 | embed as dev tool | Per-test process, `slow-timeout`, retries marked flaky. | 0.9.148 |
| tokio `test-util` | github.com/tokio-rs/tokio | Rust | MIT | 1.53.2 | embed | `start_paused`, `advance`. | 1.53 |
| turmoil | github.com/tokio-rs/turmoil | Rust | MIT | 0.7.2, 2026-04-24 | skip | Simulated networks. mote's network is one Unix socket. | none |

## Recommendation

```toml
# crates/mote-testkit/Cargo.toml
[package]
name = "mote-testkit"

[[bin]]
name = "scripted-model"
path = "src/bin/scripted_model.rs"

[[bin]]
name = "fake-claude"
path = "src/bin/fake_claude.rs"

[dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1.53", features = ["full", "test-util"] }
hyper = { version = "1", features = ["server", "http1"] }
hyper-util = { version = "0.1", features = ["tokio"] }
tempfile = "3.27.0"
mote-eventlog = { path = "../mote-eventlog" }
mote-ipc = { path = "../mote-ipc" }

# workspace [workspace.dependencies] for dev use
insta = { version = "1.49.0", features = ["json", "redactions"] }
proptest = "1.11.0"
wiremock = "0.6.5"
assert_cmd = "2.2.2"
```

Layout and commands:

```text
crates/mote-testkit/src/{lib.rs, bin/scripted_model.rs, bin/fake_claude.rs}
fixtures/scripts/*.json           # scripted model scripts
fixtures/transcripts/*.jsonl      # recorded, redacted
fixtures/fake-harness/*.js        # Overseer's Node fixtures, unchanged
crates/mote-harness/tests/golden/*.norm.json
```

```bash
cargo nextest run --workspace                       # default runner
cargo test --workspace                              # must also pass
INSTA_UPDATE=new cargo test -p mote-harness golden && cargo insta review
MOTE_RECORD=fixtures/transcripts/claude-2.1.289-partial.jsonl MOTE_LIVE=1 cargo test -p mote-harness live_partial -- --ignored
```

Tests find the fakes with `env!("CARGO_BIN_EXE_fake-claude")` from integration tests in `mote-testkit`, or with `assert_cmd::cargo_bin!` elsewhere, and pass the path through `MOTE_CLAUDE_PATH`.

## Efficiency targets and what to measure

- The whole workspace suite finishes under 90 s with nextest on the owner's Mac. No single test takes over 10 s except the 12,000 burst, which is tagged `slow`.
- `scripted-model` starts under 100 ms and stays under 20 MB RSS.
- `fake-claude` starts under 50 ms.
- Paused-time watcher tests finish under 1 s each.
- Zero network: a `lsof -i -P` sample during the suite shows only loopback sockets. Record it once in the evidence file.
- Zero paid turns: the ledger in every test home has no row with `lane != 'test'` after the suite.

## Open decisions with defaults

1. Fakes in Rust or Node. Default: Rust binaries. The Node fixtures stay for parity and for the supervisor test that spawns `/bin/sh` scripts.
2. Scripted model shape. Default: Ollama native only. OpenAI `/v1/chat/completions` comes if a lane needs it.
3. Script matching. Default: exact or structural matches, never regex over prompt text.
4. Record mode. Default: `MOTE_RECORD` writes after redaction. CRATE-17's `fixtures_are_redacted` test guards the directory.
5. Snapshot format. Default: JSON through insta with redactions for `ts`, `seq`, `uuid`, `session_id`, `duration_ms`.
6. Runner. Default: nextest for developers and CI, `cargo test` as the fallback that must also pass.
7. Network guard. Default: `MOTE_TEST_LOOPBACK_ONLY=1` in `.cargo/config.toml` `[env]`, enforced in the HTTP client builder and in `resolve_program`.
8. Live tests. Default: `#[ignore]` plus `MOTE_LIVE=1`. They never run in `cargo test --workspace`.
9. Flaky retries. Default: none. A flaky test is a bug.
10. Fuzzing. Default: later, behind nightly, for the UI tree parser.

## Acceptance criteria

- **TEST-01** `scripted-model` serves `/api/chat` as newline JSON chunks ending in `done: true` with eval counts, plus `/api/tags`, `/api/ps`, `/api/show` and `/health`, from a JSON script, under 20 MB RSS. **Verify:** `cargo test -p mote-testkit scripted` and a `ps -o rss` sample.
- **TEST-02** A `format` request returns the scripted object as `message.content`, and `stream: false` returns one object. **Verify:** unit tests in `mote-testkit`.
- **TEST-03** `fake-claude` reproduces every `claude-fixture.js` mode byte for byte on stdout for the shared modes and adds `partial` and `mcp`. **Verify:** a parity test runs both binaries per mode with `FIXTURE_SLOW_MS=10` and diffs the output after timestamp redaction.
- **TEST-04** `cargo test --workspace` passes with networking disabled. **Verify:** run with Wi-Fi off and `MOTE_TEST_LOOPBACK_ONLY=1`, record the output (MOTE-03).
- **TEST-05** `MOTE_RECORD` writes redacted transcripts and the golden test replays them into `Norm` snapshots. **Verify:** `cargo test -p mote-harness golden` green and `fixtures_are_redacted` green.
- **TEST-06** Overseer's four transcripts replay to the same `Norm` sequence at parser version `2026-09-24.1`. **Verify:** `cargo test -p mote-harness golden_overseer` (CRATE-09).
- **TEST-07** `mote-testkit::Daemon::start` spawns `moted serve` in a temp home, and `call`, `raw`, `events`, `wait_status` and `Drop` behave as Overseer's helper. **Verify:** `cargo test -p moted core::` uses it for MOTE-01.
- **TEST-08** Property tests show the router parser never returns `talk` for arbitrary bytes, redaction is idempotent, and the tree validator rejects unknown nodes, 1,000 cases each. **Verify:** `cargo test -p moted prop_` green with a fixed seed in CI.
- **TEST-09** Watcher tests run under paused time and finish under 1 s each. **Verify:** nextest timing report.
- **TEST-10** Slack and GitHub tests use wiremock and assert no other host is contacted. **Verify:** mock expectations plus the loopback guard.
- **TEST-11** The suite finishes under 90 s and no test except `slow` tagged ones exceeds 10 s. **Verify:** `cargo nextest run --workspace` summary in the evidence record.
- **TEST-12** Live checks are `#[ignore]` behind `MOTE_LIVE=1` and write their output to `docs/verification/`. **Verify:** `cargo test --workspace -- --list` shows them ignored.

## Sources

- Ollama API: https://github.com/ollama/ollama/blob/main/docs/api.md
- insta: https://github.com/mitsuhiko/insta and https://insta.rs/docs/redactions/
- cargo-insta: https://insta.rs/docs/cli/
- expect-test: https://crates.io/crates/expect-test
- proptest: https://github.com/proptest-rs/proptest
- quickcheck: https://github.com/BurntSushi/quickcheck
- cargo-fuzz: https://github.com/rust-fuzz/cargo-fuzz
- wiremock: https://github.com/LukeMathWalker/wiremock-rs and https://docs.rs/wiremock/latest/wiremock/struct.ResponseTemplate.html
- httpmock: https://docs.rs/httpmock/latest/httpmock/struct.MockServer.html
- mockito: https://docs.rs/mockito/latest/mockito/struct.Mock.html
- llmposter: https://github.com/SkillDoAI/llmposter
- aimock: https://github.com/CopilotKit/aimock
- rvcr: https://github.com/ChorusOne/rvcr
- vcrpy record modes and filters: https://vcrpy.readthedocs.io/en/latest/advanced.html
- nock back: https://github.com/nock/nock#nock-back
- assert_cmd: https://docs.rs/assert_cmd/latest/assert_cmd/
- Cargo `CARGO_BIN_EXE_<name>`: https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-crates
- tokio pause and advance: https://docs.rs/tokio/latest/tokio/time/fn.pause.html and https://docs.rs/tokio/latest/tokio/attr.test.html
- turmoil: https://github.com/tokio-rs/turmoil
- nextest: https://nexte.st/docs/design/how-it-works/ and https://nexte.st/docs/features/slow-tests/
- Owner's scripted server: `~/projects/evalsagent/bin/scripted-model.mjs` (local)
- Overseer `fixtures/fake-harness/*.js`, `fixtures/transcripts/*.jsonl`, `daemon/tests/common/mod.rs`, `daemon/tests/protocol.rs` at `a7a5cac0` (local, read-only)
