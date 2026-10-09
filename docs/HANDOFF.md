# Handoff: pick up mote

Updated 2026-10-08, end of the planning day. Replace this file, never append to it. Delete it
when nothing needs transfer. Nothing here assumes chat history.

## What mote is, in three lines

An open-source, always-on personal agent. Characters run on it. Four parts: mote-cloud (the
Rust engine, all state, on the owner's Mac today and a server later), mote-client (the chat
app, lends the Mac when it runs there), mote-ui (a stateless renderer, alone on a Pi or inside
the client), mote-talk (voice). Twenty product promises are in `docs/concepts.md`.

## Where everything is

| Need | File |
| --- | --- |
| The v0 build list (55 items, the only current scope) and the copyable driver goal | `README.md`, section Ready for Go |
| The v0 build contract: every default decided, one section per piece, linked from each item | `docs/rfcs/v0.md` |
| Every other criterion from planning, 329 with stable IDs, out of scope until moved | `docs/backlog.md` |
| Product decisions, topology, local-only deployment, visibility, criteria MOTE-01 to MOTE-35 | `docs/rfcs/mote.md` |
| The twenty promises and the engineering map to reuse documents | `docs/concepts.md` |
| Crates to extract from Overseer and its migration plan, CRATE-NN | `docs/rfcs/shared-crates.md` |
| The generative UI language research, UI-NN | `docs/rfcs/ui-language.md` |
| One reuse document per engineering piece, each with its own criteria | `docs/reuse/*.md` |
| Raw research notes with sources | `docs/research/*.md` |
| The renderer handoff for the pixel pipeline (a separate agent's job) | `docs/handoff/generative-ui-renderer.md` |
| The two presentation pages | `docs/presentation/key-concepts.html`, `docs/presentation/tech-details.html` |
| Rules for every agent | `AGENTS.md` |

Overseer, the owner's existing Rust daemon that mote reuses, is at `~/projects/overseer`.
Read only. Next Gen Lang, whose principles the UI language follows, is at
`~/projects/nextgenlang/knowledge`. Read only.

## State of the work

- Planning is finished. No code exists. Nothing is committed: the repo at `~/projects/mote` is on `main` with zero commits and every file untracked. The first commit is yours to make once the owner agrees.
- Scope is the 55-item v0 list in the README. `docs/rfcs/v0.md` fixes every default so no item needs an owner decision. The 329 planning criteria moved to `docs/backlog.md` with their IDs; their `-00` owner gates apply only when an item is moved back into scope.
- Local only. `models.local_only` is true. The driver may pull exactly `qwen3:4b`, `qwen3:0.6b` and `qwen3-coder:30b` (22 GB) and must never start Claude Code or Codex. The owner flips the flag in person.
- `docs/rfcs/context-layer.md` is the newest RFC (CTXL-00 to CTXL-56, on the board). Its storage answer: keep SQLite with graph tables and bounded walks as the source of truth; LadybugDB gets a gated spike before any switch. Measured on the owner's Mac: 2-hop at 1M edges in 0.05 ms when both edge columns are indexed. It also found 1.9 GB of Claude Code transcripts on the Mac as a large local source.
- The presentations are published as private artifacts the owner controls. The files in `docs/presentation/` are the source. Republish only if the owner asks.

## Decisions the owner made today, in short

Subscription logins and local models only, never API keys. Four lanes: talk, router, think,
work. A fail-closed JSON verdict for every model decision. Models emit a validated UI tree and
later a small language, never HTML, CSS or JavaScript. The UI language is purposefully small:
flexbox plus anchors, one inherited theme, no overrides. The renderer draws pixels itself, no
webview, and runs alone on a Pi. Memory is a temporal graph; the engine is an open spike.
Everything is auditable: memory browser, pack replay, a charter (`house.md`, `<character>.md`)
with a daemon-enforced header. Full visibility: a pipeline view where any stage can be
inspected and its model or harness swapped hot. The Pi stores nothing. Browser use has a cloud
mode and a local mode. The texting feel from dots (sent, delivered, read, typing) stays.
Characters are options. The product must say what it cannot see or reach.

## Decisions waiting on the owner

1. UI-00: wire format (compact JSON recommended) and state model (no routes, no declared store).
2. CRATE-00: extraction order of the nine crates.
3. MEM-00: the memory model and pinning policy.
4. CTXL-00: the context-layer RFC as a whole, and whether to run the LadybugDB spike now or later.
5. Model pulls: `qwen3:4b`, `qwen3:0.6b`, `clef-flash`, `qwen3-coder:30b` (about 33 GB). Never pull without the owner's yes.
6. A Slack bot token, or the Slack watcher stays disabled.
7. The pack order conflict: memory wants the working set first, context wants volatile parts last for the prompt cache (CTX-00).
8. Whether to make clef-flash the Mac router judge from day one.

## Exact next actions, in order

1. Ask the owner for the first commit and make it with the Conventional Commits rules in `AGENTS.md`. No AI attribution trailers.
2. Run the driver goal from the README on the v0 list, top to bottom, starting at V0-01.
3. In parallel, a separate agent takes `docs/handoff/generative-ui-renderer.md` and produces `docs/rfcs/renderer.md` plus a spike under `spikes/renderer/`. v0 renders with egui so the build never waits on it.
4. When the owner is available: flip `models.local_only`, then move the subscription harness, Overseer work lane, browser and connector items from `docs/backlog.md` into a v1 list.

## Rules that bite

- Never pass `--bare` to `claude -p`. Bare mode skips the keychain login and needs an API key.
- Ollama's default 4k context silently truncates. Set the context explicitly, 32k or more, for any tool-using model.
- Chrome 136 and later ignore the debug port on the default profile. Local browser mode is a dedicated signed-in profile per character. The default profile and its keychain item are never read.
- Every model verdict is JSON parsed fail-closed. No regex over natural language.
- Outward actions (post, send, buy, delete, submit, download, consent, sign-in) wait on the approvals table.
- Plain declarative prose. No em dashes. No emojis. No ticket IDs in code or comments.
