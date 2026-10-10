# Handoff: Matrix structural interaction state after review

Updated 2026-10-10. Replace this file, never append.

Branch: `main`.
Task 4 base: `b07c1ec`.
Task 4 commit: `ac291e4`, `fix(client): reconcile structural interaction state`.
Fix Round 1 base: `ac291e4`.
Fix Round 1 subject: `fix(client): stabilize control widget identities`.
Preserve untracked `.DS_Store` and `.superpowers/`.
Do not push.

The runtime caches edits by surface identity, structural path and control type.
Matrix identity includes its connection epoch.
Other surfaces retain their first committed sequence as a stable instance identity.
Named replacements retain that identity.
Current sequence and structural path remain unchanged in the outbound admission envelope.

Each complete surface draw records its live controls in egui state.
Removed controls lose edited values, text edit state and focus.
Removed Select popups close.
Toggle and Action interactive responses use explicit structural IDs.
The shared plain-control helper supplies those IDs directly to egui interaction.
This fixes focus loss when an earlier chat message disappears.
It does not restore focus after rendering or keep test-only focus state.
Keyboard selection closes Select popups.
No v0 JSON, schema, fixture or host admission contract changed.

Twelve interaction tests cover matching replacements, path and type changes, removal and
reinsertion, host value updates, separate surfaces and reconnect epochs.
They also preserve focus and exact Space events for named and anonymous surviving controls.
Removing a focused surface does not transfer its focus to an equal-label survivor.
Two keyboard tests cover all four interactive fixture types, both Select options,
Tab and Shift-Tab order, and Space and Enter activation.
The parity test compares exact light/dark painted shapes and widget metadata with the prior
egui Checkbox and Button through hover, focus, padded-edge click, Space and Enter.
The daemon test dispatches five real queued keyboard events through host admission.
Field edits and traversal emit no events.
Host updates preserve prior surface events.
Model request, pack and ledger counts stay zero.

Partial evidence: `docs/verification/UI-16.md`, `UI-25.md` and `UI-26.md`.
All three criteria remain open.
Native focus pixels, full-catalog form semantics, keyed selection and broader subscriptions
are not claimed.

Task 1 admission, sequence/path context and reconnect isolation remain intact.
Task 2 decoder and candidate-isolation tests remain intact.
Task 3 literal and forbidden-source evidence remains intact.
The schema, ten consumer fixtures, nine source fixtures and deterministic measurements are unchanged.
No model pull, live model call, remote write or push occurred.

Focused verification passed 83 client tests and the daemon keyboard ownership test.
The full workspace passed 317 tests and all doc tests.
This includes 63 daemon unit tests and two daemon integration tests.
Workspace clippy, the deterministic measurement check and diff checks passed.
Socket-dependent suites ran with local socket access.

Next: independently review Fix Round 1, then continue Task 5 in
`docs/superpowers/plans/2026-10-10-matrix-host-runtime-v0.md`.
Task 5 checks runtime layout invariants.
The report is `.superpowers/sdd/2026-10-10-matrix-host-runtime-v0/task-4-report.md`.

UI-35 native capture remains pending in `docs/verification/UI-35.md`.
Its isolated fixture is `/private/tmp/mote-ui35.XJxtru`.
The Mac was locked during that capture attempt.
Headless evidence does not replace native captures or approve Pi or iPad targets.
The client still persists only its cursor, not chat history.

`docs/rfcs/v0.md` remains authoritative.
Production `models.local_only` stays true.
Every model call uses the ledger and every stored event passes through redaction.
