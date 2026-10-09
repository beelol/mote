# Handoff: pick up mote v0 build

Updated 2026-10-08. Replace this file, never append. Delete it when nothing needs transfer.
Nothing here assumes chat history.

## Where

- Branch: `main`. No remote (owner adds one later).
- Scope: the 55-item v0 list at the bottom of `README.md`. Contract: `docs/rfcs/v0.md`.
- Evidence: `docs/verification/<ID>.md`, one per closed item.

## State of the work

- Done and committed: V0-01..18 (workspace, redact, paths, eventlog, ipc, config+gate,
  localmodel, live model ensure), V0-31/32 (context store packs+ledger),
  V0-37/38/39 (charter). 26 of 55 items checked.
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

In progress: V0-40/41/42 surface tree in mote-ui (validator, generated schema, golden PNG
render). After that: talk lane (V0-19,20,22), router (V0-23,24), think lane (V0-27,28,30),
memory graph retrieval (V0-33,34,35), watchers/approvals (V0-51,52), bench (V0-55).
Live/GUI items needing the owner's desktop: V0-43..50 (client screenshots), V0-53 (launchd),
plus live benches V0-21,25,26,29,36,54.

## Uncommitted work

None after the V0-01 commit.
