# Handoff: pick up mote v0 build

Updated 2026-10-08. Replace this file, never append. Delete it when nothing needs transfer.
Nothing here assumes chat history.

## Where

- Branch: `main`. No remote (owner adds one later).
- Scope: the 55-item v0 list at the bottom of `README.md`. Contract: `docs/rfcs/v0.md`.
- Evidence: `docs/verification/<ID>.md`, one per closed item.

## State of the work

- V0-01 done and committed: Cargo workspace with binaries `moted`, `mote-client` and nine
  crates under `crates/`. Built with std + threads, no async runtime. Cargo.lock committed.
- All crates are skeletons with a one-line doc comment, to be filled by their items.
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

V0-02: implement `mote-redact` by copying patterns and tests from
`~/projects/overseer/daemon/src/redact.rs`. Verify `cargo test -p mote-redact`.

## Uncommitted work

None after the V0-01 commit.
