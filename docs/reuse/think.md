# Reuse: think lane on subscriptions

What mote reuses to run Claude Code as the think lane under the owner's login, with no API key.
Covers MOTE-09, MOTE-10 and MOTE-11. Researched 2026-10-08 against Claude Code 2.1.289,
Codex 0.158.0 and OpenCode 1.15.13 on the owner's Mac, and Overseer at `a7a5cac0`.

## Key points

1. The think lane is one `claude -p` child per turn. `mote-harness` launches and parses it. That crate is Overseer's `adapters.rs` plus five Claude fields.
2. Do not pass `--bare`. Bare mode skips keychain reads and the subscription login lives in the keychain. The product RFC's flag list must drop it.
3. Partial text arrives as `stream_event` lines. Overseer ignores them today. `mote-harness` turns `content_block_delta` into `Norm::Partial`.
4. One Claude session per thread. The first turn passes `--session-id <uuid>`, later turns pass `--resume <uuid>`. Transcripts live under `~/.claude/projects/<cwd-slug>/<session>.jsonl`.
5. The `result` line carries `total_cost_usd`, `usage`, `modelUsage`, `num_turns` and `duration_ms`. One ledger row per turn comes from it.
6. An expired login is an `assistant` line with `error: "authentication_failed"`. Overseer classifies it as `auth`. The preflight is `claude auth status`, which exits 1 when logged out.
7. Memory, delegate and Slack-read tools live in one `moted mcp` stdio server built on `rmcp`, the official Rust MCP SDK. Files use Claude's built-in read tools. GitHub uses `gh` through a Bash allowlist.
8. The Claude Agent SDK is not used. It has no Rust binding, it bundles its own CLI, and Anthropic does not allow third parties to offer claude.ai login. Codex and OpenCode stay deferred.

## What the concept needs technically

Launch. Overseer's argv plus mote's additions:

```text
claude -p --output-format stream-json --input-format stream-json --verbose \
  --include-partial-messages --permission-prompt-tool stdio \
  --session-id <uuid>                 # first turn of a thread
  --resume <uuid>                     # every later turn
  --model <name> --fallback-model haiku \
  --mcp-config <data_dir>/mcp.json --strict-mcp-config \
  --allowedTools mcp__mote Read Grep Glob "Bash(gh *)" \
  --disallowedTools Write Edit MultiEdit NotebookEdit WebFetch WebSearch
```

The prompt goes to stdin as one line, `{"type":"user","message":{"role":"user","content":"..."}}`, as Overseer does. Stdin stays open for `control_response` lines. The environment is Overseer's `base_env` with `forbidden_env` applied, so no `ANTHROPIC_API_KEY` and no `CLAUDE_CODE_OAUTH_TOKEN` ever reach the child. `MCP_TIMEOUT` (30 s default) bounds the MCP server start.

Lines to parse, one JSON object per line:

| type | what mote does with it |
| --- | --- |
| `system` subtype `init` | Session id, model, tools, `mcp_servers` status. Emits `Session`, `Running`. |
| `stream_event` | `content_block_delta` with `text_delta` becomes `Partial`. The first one carries `ttft_ms`. Others are ignored. |
| `assistant` | Final text and `tool_use` blocks. An `error` field becomes `Error { class }`. |
| `user` | `tool_result` blocks. |
| `control_request` subtype `can_use_tool` | A permission card. The answer goes back as `control_response` with `behavior` allow or deny. |
| `rate_limit_event` | A usage event for the ledger. |
| `result` | `Usage`, `TurnDone`, `structured_output`, `permission_denials`. |

Interrupt. Write `{"type":"control_request","request_id":"mote-int-<uuid>","request":{"subtype":"interrupt"}}` to stdin. Send SIGINT after 3 s if the turn is still active. Send SIGTERM after 10 s. SIGTERM exits 143 and leaves the turn unfinished, so it is the last resort.

Sessions. mote pre-assigns the session UUID per thread. `--fork-session` branches a thread. Session persistence stays on so `--resume` works. Claude Code deletes transcripts after `cleanupPeriodDays` (30 days), so a failed resume starts a fresh session and the thread keeps mote's own event log as history.

Ledger. One row per turn from `result`: lane `think`, model from `init`, session id, input, output and cache tokens summed over `modelUsage`, `total_cost_usd`, `num_turns`, `duration_ms`, and the context pack ids. Each MCP tool call writes its own row from inside the server. Every parsed line passes `mote-redact` before it is stored.

Preflight. Run `claude auth status` before each turn. Accept `loggedIn: true` with `authMethod` `claude.ai` or `oauth_token`. Anything else shows the sign-in card, queues the turn and polls every 30 s.

Tools. `mcp.json` is `{"mcpServers":{"mote":{"command":"<moted path>","args":["mcp","--thread","<id>"]}}}`. The server exposes `memory_search`, `memory_write`, `delegate` and `slack_read` (only when a token exists). On the wire they are `mcp__mote__memory_write` and so on.

## Candidates

| name | repo | language | license | last release | verdict | reason | pin |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Claude Code CLI | github.com/anthropics/claude-code | TypeScript binary | Anthropic commercial terms | 2.1.289 installed | embed as subprocess | Subscription login, stream-json, control protocol, `--resume`. | minimum 2.1.289, checked at startup |
| Claude Agent SDK | github.com/anthropics/claude-agent-sdk-typescript | TypeScript, Python | Anthropic commercial terms | ongoing | skip | No Rust. Bundles its own CLI. Third parties may not offer claude.ai login. The overview tells other languages to run the CLI with `-p`. | none |
| Codex CLI `exec --json` | github.com/openai/codex | Rust | Apache-2.0 | rust-v0.162.0, 2026-10-08 | port the idea, deferred | ChatGPT login, JSONL events, `app-server` JSON-RPC. Overseer parses both already. | rust-v0.158.0 (installed) |
| OpenCode `serve` | github.com/anomalyco/opencode | TypeScript | MIT | v1.18.35, 2026-10-06 | skip | HTTP plus SSE server and `run --format json`. Needs provider credentials. | none |
| rmcp | github.com/modelcontextprotocol/rust-sdk | Rust | Apache-2.0 | 3.5.1, 2026-10-05 | embed | Official SDK. `server`, `macros`, `transport-io` for stdio. | 3.5.1 |
| mcp-sdk | crates.io/crates/mcp-sdk | Rust | Apache-2.0 | 0.0.3, 2025-01-20 | skip | Unofficial and stale. | none |
| github-mcp-server | github.com/github/github-mcp-server | Go | MIT | v2.0.2, 2026-10-08 | sidecar later | Official. Stdio binary. Needs `GITHUB_PERSONAL_ACCESS_TOKEN`, which `gh auth token` can supply. | v2.0.2 |
| `gh` CLI | github.com/cli/cli | Go | MIT | installed | embed through Bash allowlist | Already logged in. `--json` output. No new credential. | installed |
| server-filesystem | github.com/modelcontextprotocol/servers | TypeScript | MIT | 2026.8.31 | skip | Claude Code has Read, Grep and Glob built in. | none |
| korotovsky/slack-mcp-server | github.com/korotovsky/slack-mcp-server | Go | MIT | v1.3.0, 2026-05-14 | skip | Slack read is a watcher concern. One `slack_read` tool in `moted mcp` is enough. | none |
| Slack official MCP | docs.slack.dev/ai/mcp-server | remote only | n/a | n/a | skip | Remote OAuth with admin approval. No stdio. | none |
| Overseer `adapters.rs`, `shim.rs` | ~/projects/overseer | Rust | MIT | a7a5cac0 | port into `mote-harness` | Launch, parse, permission reply, interrupt plan and error classes proven live. | a7a5cac0 |

## What to reuse from Overseer's adapters.rs

- Reuse as-is: `base_env`, `forbidden_env`, `resolve_program`, `version_of`, `classify_error`, every branch of `parse_claude`, the `claude` arm of `permission_reply`, `interrupt_plan`, `check_turn_options` (it refuses `bypassPermissions`), and the stdin message builder with images.
- Change: the `claude` arm of `launch` gains `partial_messages`, `bare` (kept false), `json_schema`, `mcp_config`, `allowed_tools`, plus `--disallowedTools`, `--strict-mcp-config`, `--session-id` and `--fallback-model`. `parse_claude` returns `Partial` for `stream_event` when the launch asked for it. `OVERSEER_` names read `cfg.env_prefix`. `overseer-int-` reads `cfg.request_prefix`.
- Leave in Overseer: the `Norm::Child` run-tree bookkeeping (mote records subagent activity as plain events), codex-app plumbing until DEF-05, and the OpenCode store reader.
- Carry the fixtures: `claude-fixture.js` modes `permission`, `auth`, `ratelimit`, `quota`, `slow`, `limits`, `echo`, `background`, `stop-live`, `unparsed`, and the transcript `claude-2.1.246-auth-expired-live.jsonl`.

## Recommendation

```toml
# crates/mote-harness/Cargo.toml
[dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1.53", features = ["process", "io-util", "sync", "time", "macros"] }
uuid = { version = "1", features = ["v4"] }
base64 = "0.22"
libc = "0.2"
mote-redact = { path = "../mote-redact" }

# moted/Cargo.toml, the MCP server is the `moted mcp` subcommand
[dependencies]
rmcp = { version = "3.5.1", features = ["server", "macros", "transport-io", "schemars"] }
```

Live smoke check, no tools, one short turn:

```bash
claude auth status; echo "exit=$?"
printf '%s\n' '{"type":"user","message":{"role":"user","content":"Reply with one word."}}' \
  | claude -p --output-format stream-json --input-format stream-json --verbose \
      --include-partial-messages --model haiku \
  | head -c 4000
```

## Efficiency targets and what to measure

- Spawn to `system/init`: p50 under 2.5 s without `--bare`. `moted bench think` over 10 cold starts.
- First partial token: `ttft_ms` from the first `stream_event`. p50 under 3 s for a short prompt.
- Child RSS: under 400 MB. `ps -o rss` sampled during a turn.
- Concurrency: at most two think children at once. Others queue.
- Cost: every turn's `total_cost_usd` lands in the ledger and the window sums it per day.
- MCP server: `moted mcp` answers `initialize` under 200 ms so `MCP_TIMEOUT` never trips.
- Exit: the child exits within 2 s after `result` when no background task remains.

## Open decisions with defaults

1. `--bare`. Default: off. Bare mode skips keychain reads and the login is in the keychain. Revisit when Anthropic adds OAuth to bare mode.
2. Permission transport. Default: `--permission-prompt-tool stdio`, as Overseer runs live today. The docs only show an MCP tool name. If a future version refuses `stdio`, add a permission tool to `moted mcp`.
3. Model. Default: the login's default model with `--fallback-model haiku`. A `think.model` setting overrides it.
4. GitHub. Default: `gh` through `Bash(gh *)`. github-mcp-server as a sidecar when typed tools justify a Go binary.
5. Structured output. Default: no `--json-schema` in v0. Whether stream-json carries `structured_output` is unknown. Verify before use.
6. Session id ownership. Default: mote pre-assigns the UUID with `--session-id` and stores it on the thread.
7. Concurrency. Default: two children.
8. Codex. Default: deferred behind DEF-05. Codex 0.158.0 is installed and Overseer parses `exec --json` and `app-server`.
9. Slack read tool. Default: registered only when a token exists.
10. Transcript retention. Default: Claude Code's 30-day cleanup runs. mote's event log is the durable history.

## Acceptance criteria

- **THINK-01** `mote-harness::launch` for `claude` builds Overseer's argv plus `--include-partial-messages`, `--session-id` or `--resume`, `--mcp-config`, `--strict-mcp-config`, `--allowedTools`, `--disallowedTools` and `--fallback-model`, never `--bare`, and forwards no forbidden env key. **Verify:** `cargo test -p mote-harness launch_claude` compares argv and env against a golden list.
- **THINK-02** The preflight runs `claude auth status` and any non-zero exit or an `authMethod` other than `claude.ai` or `oauth_token` yields a sign-in card and a queued turn. **Verify:** shim `auth status` exits 1 in a test, and one live record when the owner's login next expires.
- **THINK-03** Each `stream_event` text delta becomes one `Partial` event in the log within 50 ms, and the final `assistant` text is emitted once. **Verify:** shim `partial` mode emits 20 deltas and the log holds 20 partials in order plus one text.
- **THINK-04** A thread stores its session id before the first line arrives and passes `--resume` on every later turn. **Verify:** shim `echo` mode shows `--session-id` then `--resume` in argv, and a live two-turn thread recalls the first turn.
- **THINK-05** The `result` line writes exactly one ledger row with lane, model, session id, tokens from `modelUsage`, `total_cost_usd`, `num_turns` and `duration_ms`. **Verify:** `SELECT` over the ledger after a shim turn and a live turn.
- **THINK-06** An `authentication_failed` line classifies as `auth`, shows the sign-in card, keeps the turn queued, and resumes once `claude auth status` exits 0. **Verify:** shim `auth` mode plus the golden `Norm` sequence for `claude-2.1.246-auth-expired-live.jsonl`.
- **THINK-07** `moted mcp` serves `memory_search`, `memory_write`, `delegate` and `slack_read`, and each call writes a ledger row and an event. **Verify:** `cargo test -p moted tools::` with an in-process rmcp client, plus a live turn that calls `memory_write`.
- **THINK-08** A `can_use_tool` request becomes a card and the owner's answer returns as `control_response` to the same child. **Verify:** shim `permission` mode for allow, deny and interrupt, ported from Overseer's ac16 test.
- **THINK-09** Interrupt writes the control request, sends SIGINT after 3 s and SIGTERM after 10 s, and the turn ends `interrupted`, not `failed`. **Verify:** shim `slow` mode, ported from Overseer's ac60 test.
- **THINK-10** Every line from the child passes redaction before storage. **Verify:** the shim prints an `sk-` token and the log shows `[redacted]`.
- **THINK-11** Spawn to `init` is under 2.5 s p50 and child RSS under 400 MB. **Verify:** `moted bench think` output in the evidence record.
- **THINK-12** Stdin closes after `result` only when no background task is pending, and the child exits within 2 s. **Verify:** shim `background` and `background-early` modes, ported from Overseer's ac14 tests.

## Sources

- Claude Code CLI reference: https://code.claude.com/docs/en/cli-reference
- Headless mode and bare mode: https://code.claude.com/docs/en/headless
- Authentication and credential order: https://code.claude.com/docs/en/authentication
- Login expired error text: https://code.claude.com/docs/en/errors#login-expired
- Transcript location: https://code.claude.com/docs/en/sessions#where-transcripts-are-stored
- Agent SDK overview: https://code.claude.com/docs/en/agent-sdk/overview
- Message and result shapes: https://code.claude.com/docs/en/agent-sdk/typescript
- Structured outputs: https://code.claude.com/docs/en/agent-sdk/structured-outputs
- Agent SDK monthly credit on subscriptions: https://www.infoworld.com/article/4171274/anthropic-puts-claude-agents-on-a-meter-across-its-subscriptions.html
- Codex non-interactive mode: https://learn.chatgpt.com/docs/non-interactive-mode
- Codex app-server: https://learn.chatgpt.com/docs/app-server
- Codex login: https://learn.chatgpt.com/docs/auth
- Codex release: https://github.com/openai/codex/releases/tag/rust-v0.162.0
- OpenCode server: https://opencode.ai/docs/server
- OpenCode release: https://github.com/anomalyco/opencode/releases/tag/v1.18.35
- github-mcp-server: https://github.com/github/github-mcp-server
- Slack official MCP: https://docs.slack.dev/ai/mcp-server/
- Slack community MCP: https://github.com/korotovsky/slack-mcp-server
- Filesystem MCP server: https://github.com/modelcontextprotocol/servers/tree/main/src/filesystem
- rmcp: https://github.com/modelcontextprotocol/rust-sdk and https://crates.io/crates/rmcp
- mcp-sdk: https://crates.io/crates/mcp-sdk
- Local checks on 2026-10-08: `claude --help`, `claude auth status`, `codex exec --help`, `opencode serve --help`, Overseer `daemon/src/adapters.rs` at `a7a5cac0`.
