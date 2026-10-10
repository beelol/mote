# Handoff: Inferred surface turn flow

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `77dc172`.
Requested subject: `test(matrix): verify inferred surface turn flow`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

Task 3 of the inferred Matrix surfaces plan is implemented behind an inactive production boundary.
Production `handle_turn` does not call the surface lane. A production-gate test confirms an ordinary
turn makes only the talk and router calls and creates no surface ledger row or `ui.surface` event.

The internal completed-answer operation is keyed by thread and `turn.user` sequence. It derives the
candidate id `surface-{turn_seq}` and returns an explicit ineligible, already attempted, none,
committed or rejected outcome. Completed talk and think answers enter the operation after their done
event. Talk
unavailability, think failure and unfinished handoff do not enter it.

The daemon retains the latest attempted inferred turn sequence per thread. Duplicate and older
completion calls return already attempted before a model call. They cannot commit a second surface
for the same turn.

Talk and think integration tests preserve the answer event order and existing ledger identifiers.
The talk flow records one talk row, one router row and one surface row. The think flow records one
think row and one surface row around the internal operation. A scripted outbound surface request
contains no planted secret.

The supported generated dashboard uses progress, toggle, select, list data and argument-free
refresh. Unknown actions, action arguments, keyed lists, selection, pending snapshots and failed
snapshots reject before commitment. Two invalid attempts keep the prose, prior canonical source and
prior committed surface. One diagnostic uses the exact turn-derived candidate id.

The real surface generator releases the context-store lock before Matrix finalization reads the
pipeline snapshot. The deadlock regression completes the model call and finalization within two
seconds.

Focused inferred turn and Matrix integration tests pass 50 cases. The full workspace passes 368
tests. Workspace clippy and `git diff --check` pass. Matrix measurements were not affected because
this task changes turn composition and tests, not parser, resolver, lowering, consumer or transport
measurements.

UI-36 and UI-38 have updated partial evidence. Both remain open. Task 4 must run the fixed live
200-prompt corpus, enforce the 95 percent first-attempt validity gate and retain the disabled
production boundary if the gate does not pass.

The exact next action is Task 4 from
`docs/superpowers/plans/2026-10-10-inferred-matrix-surfaces.md` after review accepts this commit.
No remote write or push occurred.
