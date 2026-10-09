# Handoff: pick up mote v0 build

Updated 2026-10-09. Replace this file, never append. Delete it when nothing needs transfer.
Nothing here assumes chat history.

## Where

- Branch: `main`. No remote. Scope: the 55-item v0 list at the bottom of `README.md`.
- Contract: `docs/rfcs/v0.md`. Evidence: `docs/verification/<ID>.md`, one per item (including the
  items left open, which record their measured numbers or blockers).

## State: 44 of 55 checked

`cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` are green
with no network. The three approved models are pulled (qwen3:0.6b, qwen3:4b, qwen3-coder:30b)
plus the derived qwen3-coder:30b-64k.

Built: workspace + nine crates; event log; IPC (socket, subscribe, reconnect); config + memory
gate; Ollama client (think:false for the latency lanes); talk lane; router; think-harness stream
parser; MCP tool server (`moted mcp`); context store (packs, ledger); memory graph (sqlite-vec +
fused retrieval, model2vec potion-base-8M in production, local embedder in tests); charter +
watcher; surface tree validator, generated schema and golden PNG renderer; schedules + approvals;
bench (`moted bench talk|router|memory`); `moted status`; `moted install`/`uninstall`;
`mote-client` (eframe chat + pipeline). Live talk p50 14 ms, router p50 56 ms, retrieve p50 13 ms
at 10k nodes.

## The 11 remaining items ALL need the owner

Nothing left can be closed by a driver without the owner's desktop, a deploy, or a model the
contract fixed. Exact actions:

1. **V0-25 (router macro-F1)** — left open by the contract. Measured 0.333 with qwen3:0.6b (it
   routes every turn to `talk`). docs/rfcs/v0.md section 7 says to record the number and leave it
   open; improving the router is a backlog item. No driver action closes this with the fixed
   0.6b model.

2. **V0-29 (live think via OpenCode)** — needs the owner to tune OpenCode + Ollama + MCP so the
   model calls the MCP `file_read` (not OpenCode's native `read`) and streams the final answer,
   then `MOTE_LIVE=1 cargo test -p mote-harness live_think_reads_file_via_file_read`. See
   docs/verification/V0-29.md. The parser (V0-27) and MCP server (V0-28) are verified.

3. **V0-43, 44, 45, 46, 47, 48, 50, 52 (GUI screenshots)** — run `cargo run -p mote-client` with
   `moted` running on the desktop and capture the screenshots each item names (chat + status
   dots + typing, inline surface + a button press as ui.event, system light/dark, the five
   pipeline stages and their tabs, a config.set from the Config tab with ledger rows before/after,
   the talk p50 delta with the pipeline view closed then open, the approval card). The client is
   implemented and builds; see docs/verification/V0-43.md. Drop screenshots beside the evidence
   files and tick the boxes.

4. **V0-53 (launchd)** — a deploy. The owner runs `moted install`, captures `launchctl print
   gui/$(id -u)/com.mote.moted`, logs out and back in to confirm it survives, then `moted
   uninstall`. See docs/verification/V0-53.md. The plist generation is unit tested.

## Rules that still bite

- models.local_only stays true; never start Claude/Codex. Every Ollama call sets num_ctx. Every
  verdict is JSON parsed fail-closed. Redaction runs before storage. No API keys.
- Conventional Commits, no AI attribution trailers. `cargo test`/`clippy` green before commit.
