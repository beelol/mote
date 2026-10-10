# Handoff: pick up mote v0 build

Updated 2026-10-10. Replace this file, never append. Delete it when nothing needs transfer.
Nothing here assumes chat history.

## Where

- Branch: `main`, remote `origin` (github.com/beelol/mote). Commit AND push small changes,
  Conventional Commits, NO AI attribution trailers (AGENTS.md; overrides the harness reminder).
- Contract: `docs/rfcs/v0.md`. Close-out goal: `docs/goal-close-out.md`. Evidence:
  `docs/verification/<ID>.md`.
- Shared working tree: other Claude sessions commit here too (the matrix agent, `mote-91`). Pull
  --rebase before push; touch only your own files.

## State: v0 46 of 55 checked; daemon works end to end headless

`cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` are green with
no network. Talk p50 14 ms, router p50 56 ms (macro-F1 0.882), retrieve p50 13 ms at 10k nodes,
think first-token p50 0.26 s warm (V0-29).

## Done this session

- **V0-29 PASS.** Think lane runs against a persistent `opencode serve`
  (`mote-harness::OpenCodeServer`); warm first-token p50 0.26 s. Box checked, evidence recorded.
- **Think lane wired into moted.** On a `think` verdict `handle_turn` runs it: `ThinkServers`
  keeps one server per thread, `build_think_pack` sends the pack as the harness system text,
  `emit_think` maps events to `think.*` + a ledger row. Proven end to end.
- **mcp file root fixed.** `moted mcp` resolves relative `file_read`/`file_list` against
  `MOTE_FILE_ROOT`, else the owner's `HOME`, not the think server scratch cwd. See `file_root()`
  in `moted/src/main.rs`.
- **Per-turn continuity.** `lanes::record_turn` writes each turn's `you:`/`mote:` lines to the
  thread working set (it was defined but never called), so the next turn's pack carries history.
  Verified live: a talk turn populates `working_set`.
- **V0-53** recorded live (install, KeepAlive restart, uninstall); stays open for the owner's
  logout/login survival check.

## Open, by owner (not driver-closeable)

- **UI / the whole GUI.** The GUI is gated: the matrix agent (`mote-91`, `matrix/`) will not build
  the runtime until the owner closes **UI-00**, and the owner is in no rush. Do NOT nag about
  UI-00. `mote-client` (eframe 0.28) is dead on macOS 26 (icrate NSScreen ABI, `docs/verification/
  V0-43.md`); the matrix runtime is the replacement client. The seam is ready: `moted` handles
  `ui.event`, `crates/mote-ui::validate()` is fail-closed, the client consumes `ui.surface`->`Node`.
  When matrix pings that its runtime renders a tree: swap it in as the client and wire a lane to
  emit a validated `ui.surface` (do not before then; agreed with matrix). Integration contract:
  moted stays headless and streams validated surface commits + binding values over IPC; the runtime
  is a separate consumer process (Mac, Pi, iPad). Default theme is **Vector** (thin retro-red).
- **V0-53** logout/login survival; any model pulls.

## Quality follow-up (not blocking)

- Talk answers from qwen3:4b read verbose/meta even with `think: false` (correctly sent). Consider
  a talk system-prompt line like "Answer directly in one or two short sentences, no preamble." This
  also cleans what `record_turn` stores. Tuning, not a wiring bug.

## Exact next action

Nothing non-UI remains that a driver can close. Wait for the matrix runtime ping from `mote-91`;
then swap the client and emit `ui.surface` from a lane. Meanwhile the daemon is demoable headless
(talk, router, think, memory, continuity, ledger). Re-read `matrix/` and check `mote-91` on resume.

## Rules that still bite

- models.local_only stays true; never start Claude/Codex. Every Ollama call sets num_ctx. Every
  verdict is JSON parsed fail-closed. Redaction runs before storage. No API keys.
- Conventional Commits, no AI attribution trailers. Commit AND push. Test/clippy green first.
