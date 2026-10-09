# Handoff: pick up mote v0 build

Updated 2026-10-08. Replace this file, never append. Delete it when nothing needs transfer.
Nothing here assumes chat history.

## Where

- Branch: `main`. No remote (owner adds one later).
- Scope: the 55-item v0 list at the bottom of `README.md`. Contract: `docs/rfcs/v0.md`.
- Evidence: `docs/verification/<ID>.md`, one per closed item.

## State of the work

- Done and committed: V0-01 (workspace), V0-02 (mote-redact), V0-03 (mote-paths),
  V0-04..07 (mote-eventlog), V0-08..11 (mote-ipc), V0-12..14 (config + memory gate),
  V0-15..17 (mote-localmodel: Ollama client, memory gate, structured fail-closed).
  mote-testkit has the scripted Ollama server.
- Built with std + threads, no async runtime. Cargo.lock committed. rusqlite bundled.
  HTTP client is ureq (json feature, no tls). Free memory via sysinfo.
- Next crate to grow: mote-context (store, packs, ledger, memory graph) for V0-31+, and the
  talk/router/think lanes (V0-19..30) which the daemon wires together.
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

V0-18: live check that the three approved models pull and `ensure()` succeeds for each
(MOTE_LIVE=1). Needs `qwen3:4b`, `qwen3:0.6b`, `qwen3-coder:30b` pulled (22 GB), and a Modelfile
for `qwen3-coder:30b-64k` (num_ctx 65536). If not pulling now, skip V0-18 and continue with
V0-19 (talk lane) which uses the scripted server.

## Uncommitted work

None after the V0-01 commit.
