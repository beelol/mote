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

`mote-memory` defines the scoped backend-neutral contract. `mote-memory-sqlite` now implements it
with transactional node, FTS and vector writes, explicit embedding model IDs, legacy hash-vector
isolation, source attribution, correction and forget. Competing corrections and correction versus
forget have one version-guarded winner. It is not wired into production yet, so the current SQLite
path remains unchanged.

The production daemon now owns harness selection. It preserves the exact v0 config, resolves an
absent value to OpenCode, validates IPC and Matrix changes before persistence, exposes configured,
active and pending state in status and pipeline snapshots, and supplies Matrix bindings for a
selector and live state. Turn handling uses a conservative model-step phase and always returns to
a checkpoint. The native harness is still unavailable, so selecting it fails visibly without
changing configuration.

Required verification passed on 2026-10-18. The focused action, driver, store and approval tests
passed. `cargo test --workspace` passed outside the sandbox because the browser fixture needs a
loopback listener. `cargo clippy --workspace -- -D warnings` and `git diff --check` passed.

## Next action

Route production harness requests through the bounded context builder and durable task driver.
Map OpenCode proposals into durable actions and bind approvals only after exact tool policy
classification. Then wire the new SQLite adapter behind absent-value selection without allowing
the legacy context path to write the migrated schema. Keep OpenCode and SQLite as defaults.
