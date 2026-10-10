# Handoff: Bounded inferred Matrix retry

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `266f4d3`.
Requested subject: `feat(matrix): retry rejected inferred surfaces`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

Task 2 of the inferred Matrix surfaces plan is implemented. It remains disconnected from ordinary
talk and think turns. No production turn calls the inferred surface controller.

The public `matrix.candidate` path and inferred generation now use one candidate finalizer. The
inferred path pushes the complete generated source into a real `CandidateStream` and finalizes it
through the shared parser, host resolver, v0 lowerer and atomic commit path. It has no parallel
parser or compiler.

The first rejected source stays quiet while the controller retries once. The retry receives
redacted structured diagnostics and the redacted last committed canonical source. A valid retry
commits one `ui.surface`. A second rejected source emits one redacted `ui.diagnostic` and preserves
the prior source, surface and control contract. Prose is not changed by this controller.

The retry budget is keyed by thread and committed surface sequence. It allows at most one retry in
one controller invocation and three retries in a rolling minute. Attempts expire at the minute
boundary. Threads are independent. A newly committed surface sequence receives a fresh budget.

`none`, malformed envelopes and transport failures stay candidate-neutral and do not retry. Each
real model attempt owns one five-part pack, one `surface` ledger row, one `model.request` event and
one `model.response` event. Tests plant a malformed source literal and a secret. Neither reaches
the retry request or stored events.

Focused shared-finalizer tests pass 7 public candidate cases. Focused inferred tests pass 13 cases
after the rolling-window test. The full workspace passes 357 tests. Workspace clippy, deterministic
Matrix measurement check and `git diff --check` pass.

UI-36 has partial evidence in `docs/verification/UI-36.md`. It remains open because Task 3 must
connect the controller after completed prose and verify ordinary-turn rendering and event order.
Task 4 still controls production activation through the fixed 200-prompt validity gate.

The exact next action is Task 3 from
`docs/superpowers/plans/2026-10-10-inferred-matrix-surfaces.md` after review accepts this commit.
No remote write or push occurred.
