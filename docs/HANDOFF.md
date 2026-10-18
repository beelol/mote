# Current development handoff

Updated 2026-10-17 in America/Los_Angeles. This file is a replacement snapshot.

## Active goal

Deliver the owner-activated Phase 2 runtime, shared voice foundations and replaceable memory
backends on `main`. Active criteria are P2-02 through P2-12, P2V-01 through P2V-10 and MBE-01
through MBE-08. P2-13 is not authorized. SQLite remains default. OpenCode remains default until
P2-12 proves a change. Physical iPad voice checks and external Apple work are deferred.

## Current checkout

The checkout is `main` at `bd74a43` before the pending design commit. The configured remote is
`origin`. Existing untracked `.DS_Store` and `.superpowers/` belong to the owner and must remain
untouched.

## Pending design batch

The root Ready for Go board now activates Phase 2. `docs/phase2/README.md` names the active
criteria and driver goal. These new design files define the stable boundaries:

- `docs/superpowers/specs/2026-10-17-phase2-runtime.md`
- `docs/superpowers/specs/2026-10-17-shared-voice.md`
- `docs/superpowers/specs/2026-10-17-memory-backends.md`

The voice design audits Overseer revision
`2b1a19a9b6e639628367a22e16155af20df4f86a`. The memory design pins TencentDB Agent Memory
revision `e9a82935b52741e0c798c75326e90b7144eb4d5f` for the candidate comparison.

No runtime code or default changed in this batch. Run `cargo test --workspace` and
`cargo clippy --workspace -- -D warnings`, inspect the diff, then commit and push the design. The
owner reviews these specifications before implementation plans or product code.

## Existing delivery work

The separate iPad and TestFlight work remains open. IPAD-08 and IPAD-12 are not closed. HOST-05
and V0-53 still require their literal owner-hardware checks. Do not run the deferred Mac sleep
test during ordinary Phase 2 development.

## Next action

Verify, commit and push the Phase 2 design batch. Present the specifications for owner review.
After approval, write the implementation plans. Then execute the smallest dependency-first
increments with evidence files, workspace tests, strict clippy, small conventional commits and a
push after every verified increment.
