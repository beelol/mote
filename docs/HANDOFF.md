# Handoff: Mote closeout

Updated 2026-10-10 03:05 PDT. Replace this file, never append. Delete it when nothing needs
transfer. Nothing here assumes chat history.

## Where

- Branch: `main`. `HEAD` and `origin/main` are `7d6b693` before this handoff update.
- Contract: `docs/rfcs/v0.md`. Driver goal: `docs/goal-close-out.md`. Matrix plan:
  `matrix/CLOSEOUT.md`.
- The owner directed this session to drive Mote and delegate Matrix UI work. Do not ask the owner
  questions before 09:00 America/Los_Angeles. Make conservative choices and defer owner decisions.

## Completed and pushed

- Matrix pattern recipes: `f93b87e`.
- Vector design defaults: `bc9cc4b`.
- Frozen Matrix grammar and resolved design defaults: `e7d7895` and `618a0e6`.
- Validated `surface.render` daemon seam and `ui.surface` event: `845eb5e`.
- First Mac-native Matrix runtime with Vector: `ca89996`, `45c7e68` and `d2f4110`.
- Acceptance reconciliation for all open UI criteria: `414c0af`.
- Current closeout state: `57c6880`. Streamed text after a surface remains visible: `5ef7b25`.
- Approval cards and thread-scoped pending actions: `bd6e674`.
- Durable client replay and optimistic turn identity: `214b588`, `91ec919` and `47507d2`.
- Daemon-backed Pipeline data and live main-scope updates: `25526fe`.
- Truthful talk, router and think request timing, ledger metrics and refresh-race fixes: `937b1bd`
  and `7d6b693`.
- The final workspace run passed 128 tests with zero failures. Clippy with warnings denied and
  whitespace checks passed. `HEAD` matches `origin/main` before this documentation update.

The client uses exact `eframe` and `egui` 0.31.0 with `winit` 0.30.13. It renders all eleven
unchanged v0 `Node` variants, supports the fixture preview, follows system appearance and applies
the Vector light and dark themes. The daemon validates requested trees fail-closed before storing
them. It emits one redacted user event before lane work. The client reconnects from the last event
applied by the UI and reconciles optimistic turns by stable local identity. The Matrix grammar
supports only the published v0 lowering subset. Unsupported semantics return
`E_LOWER_UNSUPPORTED`.

The Pipeline screen reads one cached daemon snapshot off the render path. Now merges live
main-scope events without polling. Config exposes all nine section 4 keys. Numbers reports exact
last-hour totals and comparable p50 and p95 latency samples. Requests shows every retained talk,
router and think model call with redacted payload, correlated timing and status. Legacy ledger
rows remain visible but are marked with unknown latency semantics and excluded from incompatible
percentiles.

## Open gates

- The v0 board remains 46 of 55 checked. V0-43 through V0-48, V0-50, V0-52 and V0-53 remain open.
- The Mac was locked during runtime verification. The native process launched and remained alive,
  but that is not visible acceptance. No screenshot criterion is closed.
- The lock was rechecked at 03:03 PDT. A separate isolated daemon completed one live talk and
  router turn with the approved local models. Its Pipeline snapshot showed two correlated request
  and response pairs, nonzero token counts, measured timing and one ledger row per call. V0-49
  contains the excerpt.
- Live daemon-connected cursor replay, message states, `ui.event`, system appearance, Pipeline
  screenshots, a live configuration change and the Pipeline performance comparison remain undone.
- The approval lifecycle is wired and tested, but its required screenshot remains undone.
- V0-53 still requires literal logout and login evidence.
- UI-00 remains an owner decision. It asks the owner to approve the closed catalog, frozen grammar,
  Vector defaults and the split between Matrix as the future source and the unchanged v0 tree as
  the interim consumer contract. No Matrix compiler work is authorized yet.
- `docs/handoff/ui-language/implementation.md` defines the post-approval activation order, owned
  files, fixtures, commands and stop conditions. It does not claim approval.

## Exact next actions

1. On an unlocked Mac, run the fixture preview in light and dark modes and save visible evidence.
2. Connect the client to a live daemon. Verify cursor replay without duplicates, all message states,
   typing, inline surfaces and a button action reaching the daemon as `ui.event`.
3. Exercise all five Pipeline stages and four tabs during a live turn. Change `models.talk` and
   capture ledger rows before and after.
4. Run `moted bench talk` with the Pipeline closed and open to verify the p50 delta is below 5 ms.
5. Capture the approval card, then perform the literal launchd logout and login check for V0-53.
6. After owner approval of UI-00, move only the approved Matrix compiler slice into active scope.

## Rules that still bite

- `models.local_only` stays true. Pull only the three models named in `docs/rfcs/v0.md` section 0.
- Every model call uses the ledger. Redaction runs before storage. Structured verdicts parse
  fail-closed. No API keys.
- Use Conventional Commits with no AI attribution. Run the full workspace tests and clippy before
  every commit. Push accepted work to `main`.
