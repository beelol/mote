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

The next verified slices are in the working tree. `mote-memory` defines the scoped backend-neutral
contract and reusable adapter suite; it does not alter the SQLite default. The harness-selection
state preserves the exact v0 config, resolves an absent value to OpenCode, defers live changes
through every unsafe phase and fails visibly without fallback when a target is unavailable.
Evidence is partial in `docs/verification/MBE-06.md` and `docs/verification/P2-04.md`; neither
criterion is ready to check.

Required verification passed on 2026-10-18. The focused action, driver, store and approval tests
passed. `cargo test --workspace` passed outside the sandbox because the browser fixture needs a
loopback listener. `cargo clippy --workspace -- -D warnings` and `git diff --check` passed.

## Next action

Commit and push the memory contract and harness-selection foundation after workspace gates. Then
implement the transactional SQLite adapter against the common contract while routing production
harness requests through the bounded builder. Map OpenCode proposals into durable actions and
bound approvals only after exact tool policy classification. Keep the default v0 path unchanged.
