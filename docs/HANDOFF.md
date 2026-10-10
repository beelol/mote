# Handoff: pick up mote v0 build

Updated 2026-10-09. Replace this file, never append. Delete it when nothing needs transfer.
Nothing here assumes chat history.

## Where

- Branch: `main`. No remote. Scope: the 55-item v0 list at the bottom of `README.md`.
- Contract: `docs/rfcs/v0.md`. Evidence: `docs/verification/<ID>.md`, one per item.
- A separate agent is working the UI language (`docs/rfcs/ui-language.md`, `docs/handoff/ui-language/`,
  `matrix/`). That is backlog (UI-NN), not v0. Leave those files to that agent.

## State: 46 of 55 checked (V0-29 now PASS)

`cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings` are green
with no network. The three approved models are pulled (qwen3:0.6b, qwen3:4b, qwen3-coder:30b)
plus the derived qwen3-coder:30b-64k.

Built: workspace + nine crates; event log; IPC; Ollama client + memory gate; talk lane; router;
think-harness stream parser; the persistent `opencode serve` think driver; MCP tool server
(`moted mcp`); context store (packs, ledger); memory graph; charter + watcher; surface tree
validator, generated schema and golden PNG renderer; schedules + approvals; bench; `moted status`;
`moted install`/`uninstall`; `mote-client`. Live talk p50 14 ms, router p50 56 ms, retrieve p50
13 ms at 10k nodes, think first-token p50 0.26 s warm (V0-29).

## V0-29 is closed (this session)

The think lane now runs against one persistent `opencode serve` process
(`mote-harness::OpenCodeServer`) instead of a cold `opencode run` per turn. Warm first-token
**p50 = 0.26 s** over 3 runs, answer verified through MCP `file_read`, gate 3 s -> PASS. The warm
turn that loads the model (~12.5 s) is a one-time server-start cost; in the daemon the server
stays alive across a thread's turns. The earlier "~5 s, unreachable" estimate was wrong: it
charged cold-launch cost to prefill. See `docs/verification/V0-29.md`. The cold `run_opencode`
path remains for one-shot use.

## The 9 remaining items ALL need the owner

Nothing left can be closed by a driver without the owner's desktop or a deploy.

1. **V0-43, 44, 45, 46, 47, 48, 50, 52 (GUI screenshots)** — run `cargo run -p mote-client` with
   `moted` running on the desktop and capture the screenshots each item names (chat + status
   dots + typing, inline surface + a button press as ui.event, system light/dark, the five
   pipeline stages and their tabs, a config.set from the Config tab with ledger rows before/after,
   the talk p50 delta with the pipeline view closed then open, the approval card). The client is
   implemented and builds; see docs/verification/V0-43.md. Drop screenshots beside the evidence
   files and tick the boxes.

2. **V0-53 (launchd)** — a deploy. The owner runs `moted install`, captures `launchctl print
   gui/$(id -u)/com.mote.moted`, logs out and back in to confirm it survives, then `moted
   uninstall`. See docs/verification/V0-53.md. The plist generation is unit tested.

## A follow-on worth noting (not v0)

The think lane's persistent server is proven in `mote-harness` and the live test. Wiring it into
`moted`'s runtime think turn (a server per thread, lifecycle and reuse) is not yet done and is not
a v0 item; raise it when the owner opens the think-lane runtime work.

## Rules that still bite

- models.local_only stays true; never start Claude/Codex. Every Ollama call sets num_ctx. Every
  verdict is JSON parsed fail-closed. Redaction runs before storage. No API keys.
- Conventional Commits, no AI attribution trailers. `cargo test`/`clippy` green before commit.
