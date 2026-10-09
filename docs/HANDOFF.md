# Handoff: pick up mote v0 build

Updated 2026-10-08. Replace this file, never append. Delete it when nothing needs transfer.
Nothing here assumes chat history.

## Where

- Branch: `main`. No remote (owner adds one later).
- Scope: the 55-item v0 list at the bottom of `README.md`. Contract: `docs/rfcs/v0.md`.
- Evidence: `docs/verification/<ID>.md`, one per closed item.

## State of the work (updated 2026-10-09)

- 42 of 55 checked. Remaining: V0-25 (open: router macro-F1 0.333 < 0.85, recorded, 0.6b too
  weak), V0-29 (live think via OpenCode), V0-42 (surface render, in progress), V0-43..50 and
  V0-52's card screenshot and V0-53 (need the owner's desktop/GUI or a deploy action), V0-53
  install is a deploy (owner-gated).
- Memory graph done: sqlite-vec + fused retrieval (V0-33/34/35/36); production embedder is
  model2vec potion-base-8M, tests use a local HashEmbedder.

## Older notes

- Checked: V0-01..24 (all except 21,25,26 which are live), V0-27,28,30 (think harness parser,
  MCP tool server, local-only guard), V0-31,32 (context store), V0-37,38,39 (charter),
  V0-40,41 (surface tree validator + schema). 33 of 55 items checked.
- Daemon (moted) wires: event log, config+memory gate, charter watcher, context store, talk lane,
  router, a `turn` IPC method, and `moted mcp` (stdio MCP tool server). Startup refuses a
  subscription think harness while local_only is true.
- The three approved models are pulled: qwen3:0.6b, qwen3:4b, qwen3-coder:30b, plus the derived
  qwen3-coder:30b-64k (num_ctx 65536). Ensure() live-verified.
- Built with std + threads, no async runtime. Cargo.lock committed. rusqlite bundled (FTS5).
  HTTP client is ureq (json feature, no tls). Free memory via sysinfo. Tokens via tiktoken-rs
  o200k_base. Charter watched via notify.
- mote-context has the memory graph TABLES but not retrieval/embeddings yet (V0-33..36 pending:
  needs model2vec-rs potion-base-8M embeddings + vector search; the riskiest remaining piece).
- Approved models NOT yet pulled. Ollama currently has qwen2.5 variants only. Live items
  (V0-18, 21, 25, 26, 29, 36, 43-54) need `qwen3:4b`, `qwen3:0.6b`, `qwen3-coder:30b` pulled
  (22 GB). Pull only those three, only when reaching a live item.

## Rules that bite

- models.local_only stays true. Never start Claude Code, Codex or any subscription harness.
- Every Ollama call sets num_ctx explicitly (min 32768 for tool users).
- Every model verdict is JSON parsed fail-closed. No regex over natural language.
- Redaction runs before storage. No secret enters a model call. No API keys.
- `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` green before commit.
- Conventional Commits, no AI attribution trailers.

## Next action

Remaining items grouped by risk:
- Bench + live latency (models ready): V0-55 (bench writes lines), then live V0-21 (talk p50),
  V0-26 (router p50), V0-25 (router macro-F1 on 60 labelled turns).
- Memory graph (riskiest; needs an embedder + vector search): V0-33,34,35 and live V0-36.
  Decision pending: model2vec-rs potion-base-8M (download) + sqlite-vec vs a local embedder.
- Watchers/approvals: V0-51 (cron tick, controlled clock), V0-52 (approval lifecycle; the
  screenshot part needs the GUI).
- Surface tree render: V0-42 (golden PNG; egui headless rendering risk).
- Think lane live: V0-29 (needs OpenCode installed + Ollama provider).
- Client/pipeline GUI (owner desktop for screenshots): V0-43..50, service V0-53/54.

## Uncommitted work

None after the V0-01 commit.
