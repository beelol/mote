# Goal: close mote out to something that works

Self-goal for the daemon/close-out driver, set by the owner on 2026-10-10. This is an operational
goal, not a spec. The build contract stays `docs/rfcs/v0.md`; the board stays the README v0 list.

## Definition of "works"

mote is "closed out" when, on the owner's Mac:

1. `moted` runs under launchd and stays up (V0-20/V0-53 path).
2. A person types in a GUI window and gets a streamed talk answer, with the status dots and typing
   indicator.
3. A think-routed turn runs end to end: router hands off, the persistent OpenCode server answers
   through the `moted mcp` tools, events and a ledger row land. (Daemon side done; see V0-29.)
4. Memory persists across turns and the pipeline view shows the stages.
5. The GUI is the matrix runtime rendering the surface tree, not the dead eframe-0.28 client.

Item 5 is the real remaining blocker to a working product: `mote-client` (eframe 0.28 -> winit
0.29 -> icrate 0.0.4) aborts on macOS 26 before drawing (NSScreen ABI, see
`docs/verification/V0-43.md`). The owner's direction is to adopt the matrix agent's runtime as the
GUI rather than build a throwaway eframe upgrade.

## Budget

At most about **$10 of agent spend** on this goal. Pace with the matrix agent instead of polling
tightly; do not spin on wakeups. Stop at a bounded milestone, when blocked on the matrix runtime,
or when the only work left is owner-only (logout/login, model pulls, approvals). Models stay local
(Ollama) or subscription per AGENTS.md; no API keys, so "spend" means this agent's own compute.

## Standing mandate: adopt the matrix GUI as it lands

The matrix language/runtime is built by the matrix agent in `matrix/` (session `mote-91`). As that
runtime becomes able to render a surface, swap it in as mote's client. Keep the daemon's side of
the seam ready: the surface-tree validator and types in `crates/mote-ui`, the IPC event stream,
and `ui.event` handling. Re-read `matrix/` on each resume; where `matrix/` and the old
`docs/rfcs/ui-language.md` disagree, matrix wins.

Owner direction already relayed to the matrix agent (2026-10-10): the first/base theme is a thin
retro-red Atari vector-line identity that materializes elements in as strokes; layout stays strict
(col/row + header/footer + `fill` + align roles + the closed catalog), no new primitives.

## Confirmed architecture (matrix agent, 2026-10-10)

- The runtime is a **separate consumer process**, not daemon code. `moted` stays headless under
  launchd, owning data, subscriptions, validation and surface history; it streams validated
  surface-tree commits plus binding values over the existing IPC event stream and takes `ui.event`
  back. The runtime holds only interaction state and redraws the newest committed surface. The new
  runtime **is** the replacement client.
- One Rust immediate-mode runtime (Taffy, egui, wgpu, rayon, AccessKit) covers Mac, Pi and the
  newly added **iPad** target. The JSON tree stays the consumer contract, so a second native
  consumer (e.g. SwiftUI on iOS) stays possible without changing the language.
- Default theme is named **Vector** (thin retro-red Atari vector-line; the direction the owner
  locked). Layout model locked: col/row, header/footer, fill, align, auto-scroll; no
  grid/absolute/z-index/pixels.
- **Gate:** the matrix agent is the design session and will NOT build the runtime binary until the
  owner closes **UI-00**. So the working-GUI path is: owner closes UI-00 -> matrix builds the
  first-milestone runtime -> it pings this driver -> swap it in as the client. Until then there is
  no runtime to adopt.
- **Owner stance (2026-10-10):** let matrix keep iterating; the owner is in no rush on UI-00 and
  will close it on their own timeline (or when the matrix agent is done). Do NOT nag the owner
  about UI-00. UI is the last thing, picked up when matrix lands. Work everything else first.

## Seam readiness (daemon side, checked 2026-10-10)

- `moted` handles `ui.event` (logs it) and `crates/mote-ui::validate()` is fail-closed (typed
  `Node` or a `Diagnostic`). `mote-client` already consumes `ui.surface` -> `Node` and returns
  `ui.event`; the matrix runtime replaces that client against the same contract.
- Gap: **no lane emits `ui.surface` yet** — talk/think emit text events only, so a GUI shows chat
  text, not generative surfaces. Wiring a lane to emit a validated `ui.surface` is the small
  daemon-side piece to show generative UI once the runtime can draw one. Do this when it adds value
  and stays in budget; it is not required for a chat-only working window.

## Prioritized close-out backlog

Everything except UI comes first. UI is last, picked up only when the matrix runtime lands.

1. **Think-lane runtime correctness (daemon, doable now).** The persistent server is wired
   (`ThinkServers`); settle session reuse vs fresh-per-turn and the `moted mcp` `file_read` root so
   a real think turn reads the owner's intended files, not the server scratch dir. Record it.
2. **End-to-end soundness (daemon, doable now).** A turn's message and answer must persist so the
   next turn has continuity: confirm the working set and memory are updated per turn, and that
   talk -> router -> think -> memory holds together across turns. Fix gaps, add tests.
3. **Integration seam prep (doable now).** Keep the surface tree the matrix runtime will consume
   exactly what `crates/mote-ui` validates and what the client subscribes to, so the swap is
   mechanical. (Do not emit `ui.surface` from a lane until the runtime can draw one; agreed with
   the matrix agent.)
4. **Owner-only, leave open with clear asks:** V0-53 logout/login survival; any model pulls.
5. **UI, last.** When the matrix runtime renders a surface and pings, swap it in as the client and
   wire a lane to emit a validated `ui.surface`; that closes the bulk of V0-43..48, 50, 52.
6. Keep `docs/HANDOFF.md` current each resume: what changed, what the matrix agent shipped, exact
   next action, blockers.

## Resume procedure

Read `AGENTS.md`, `docs/rfcs/v0.md`, this file, `docs/HANDOFF.md`, and `matrix/README.md` +
`matrix/decisions.md`. Check `matrix/` git log and the matrix agent (`mote-91`) for new runtime
output. Pick the smallest unblocked item above, do it, verify (`cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`), commit small, update the handoff. Stop on
the budget, an owner stop, or when only matrix-blocked or owner-only work remains.
