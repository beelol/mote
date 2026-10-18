# Current development handoff

Updated 2026-10-18 in America/Los_Angeles. This file is a replacement snapshot.

## Active goal

Deliver the owner-activated Phase 2 runtime, shared voice foundations and replaceable memory
backends on `main`. OpenCode and SQLite remain the defaults. Local-only model rules remain in
force. Physical iPad voice checks and external Apple work are deferred until a complete build is
ready.

## Repository state

The checkout is `main` and pushes to `origin`. Preserve the owner's untracked `.DS_Store` and
`.superpowers/`. Commits `37b00ab` and `765e0f4` added the guarded OpenCode adapter and routed
completed think turns through it only when `MOTE_HRNS_OPENCODE_COMPAT=1`. P2-05 remains open
because durable tool continuation is not wired through the daemon.

The current verified increment adds canonical action request hashes, durable commit receipts,
revision-guarded claim, commit, deny and fail operations, no-redispatch recovery after an
interrupted claim, HRNS-bound browser approval records and idempotent HRNS-bound file-write
approval creation. Evidence is in `docs/verification/P2-07.md`. P2-07 remains open until the daemon
uses this path end to end.

Required verification passed on 2026-10-18. The focused action, driver, store and approval tests
passed. `cargo test --workspace` passed outside the sandbox because the browser fixture needs a
loopback listener. `cargo clippy --workspace -- -D warnings` and `git diff --check` passed.

## Next action

Commit and push the guarded receipt increment. Then wire the durable driver and bound approval
reconciliation into the daemon behind the existing compatibility flag so a held OpenCode tool
survives restart without duplicate dispatch. Keep the default v0 path unchanged. Once P2-07 and
P2-05 close together, continue with Task 6, bounded task context.
