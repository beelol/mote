# Current development handoff

Updated 2026-10-18 in America/Los_Angeles. This file is a replacement snapshot.

## Active goal

Deliver the owner-activated Phase 2 runtime, shared voice foundations and replaceable memory
backends on `main`. Active criteria are P2-02 through P2-12, P2V-01 through P2V-10 and MBE-01
through MBE-08. P2-13 is not authorized. SQLite remains default. OpenCode remains default until
P2-12 proves a change. Physical iPad voice checks and external Apple work are deferred.

## Current checkout

The checkout is `main`. The plan batch follows pushed activation evidence `3c72556`. The
configured remote is `origin`. Existing untracked `.DS_Store` and `.superpowers/` belong to the
owner and must remain untouched.

## Completed and pushed design batch

The root Ready for Go board now activates Phase 2. `docs/phase2/README.md` names the active
criteria and driver goal. These new design files define the stable boundaries:

- `docs/superpowers/specs/2026-10-17-phase2-runtime.md`
- `docs/superpowers/specs/2026-10-17-shared-voice.md`
- `docs/superpowers/specs/2026-10-17-memory-backends.md`

The voice design audits Overseer revision
`2b1a19a9b6e639628367a22e16155af20df4f86a`. The memory design pins TencentDB Agent Memory
revision `e9a82935b52741e0c798c75326e90b7144eb4d5f` for the candidate comparison.

The owner approved the runtime, voice and memory architecture on 2026-10-18. The shared crates
remain in Mote's conventional `crates/` workspace now. Overseer pins the product-neutral subset.
The subset moves into its own repository later when the owner establishes that project.

The owner-facing plan is `docs/phase2/README.md`. The detailed technical plans are:

- `docs/superpowers/plans/2026-10-18-phase2-runtime.md`
- `docs/superpowers/plans/2026-10-18-shared-voice.md`
- `docs/superpowers/plans/2026-10-18-memory-backends.md`

No runtime code or default changed in this batch. Full workspace tests and strict clippy passed
before commit `c8ddbcd` was pushed. P2-01 activation evidence is recorded in
`docs/verification/P2-01.md`. The same required gates passed for that evidence batch.

## Existing delivery work

The separate iPad and TestFlight work remains open. IPAD-08 and IPAD-12 are not closed. HOST-05
and V0-53 still require their literal owner-hardware checks. Do not run the deferred Mac sleep
test during ordinary Phase 2 development.

## Next action

The owner authorized continuing on 2026-10-18. Execute directly without another planning gate.
The first pushed `mote-hrns` slice defines tasks, scoped identities, actions, checkpoints, model
replies and the harness interface.

The current batch adds guarded task and action transitions, atomic event-log mutations and the
first durable task store. Scoped reads, FIFO promotion, revision checks and one-active-task
enforcement survive reopen. Targeted tests pass: 14 event-log tests, 4 contract tests, 5 store
tests and 4 transition tests. Partial evidence is in `docs/verification/P2-02.md` and
`docs/verification/P2-03.md`.

Commit `cd35f39` is pushed to `main` and matches `origin/main`. The complete workspace test suite
and strict clippy passed for it.

Commit `ee2fbf1` adds durable actions and checkpoints to `HrnsStore` and is pushed to `main`.
Focused store tests pass: 7 passed, 0 failed. The fresh complete workspace test, strict workspace
clippy, package formatting and `git diff --check` all passed before the commit.

The current uncommitted batch implements the one-step driver, idempotent executor boundary,
durable model-step checkpoints and restart reconciliation. Targeted results pass: 4 contract
tests, 6 driver tests, 10 store tests and 4 transition tests. Package clippy, formatting and
`git diff --check` pass. The tests cover both harness selections, FIFO work, one active task,
every persisted task state, interrupted final checkpoints, interrupted action checkpoints,
adapter failure, cancellation, stale model responses, durable completion and input text,
redacted step events and one effect across an executor failure and restart.

The complete workspace test and strict workspace clippy passed. P2-02 and P2-03 evidence and
acceptance state are updated. Commit `feat: drive one durable task at a time` and push next.
Then begin Task 4, the OpenCode compatibility adapter behind the common harness boundary. No
daemon behavior or production default has changed yet.
