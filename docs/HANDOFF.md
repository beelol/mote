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

The current action increment adds canonical request hashes, durable commit receipts,
revision-guarded transitions, no-redispatch recovery, HRNS-bound approvals and daemon reconciliation
of approved, denied and failed effects. Reconciliation preserves the action's account and realm.
Evidence is in `docs/verification/P2-07.md`. P2-07 remains open until harness tool proposals create
these approvals and run through the production executor end to end.

A bounded-context increment preserves charter, pins and the newest owner turn; rejects cross-scope
memory; loads visible selected skills; and requires ledgered, source-covered structured
compaction. It also produces exact hashed `mote-context` ledger parts. Evidence is in
`docs/verification/P2-08.md`. P2-08 remains open pending backend-neutral memory retrieval and
production daemon integration.

Required verification passed on 2026-10-18. The focused action, driver, store and approval tests
passed. `cargo test --workspace` passed outside the sandbox because the browser fixture needs a
loopback listener. `cargo clippy --workspace -- -D warnings` and `git diff --check` passed.

## Next action

Route production harness requests through the bounded builder and map OpenCode tool proposals into
durable actions and bound approvals behind the existing compatibility flag. Keep the default v0
path unchanged. Add backend-neutral memory retrieval before checking P2-08.
