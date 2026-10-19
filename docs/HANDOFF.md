# Current development handoff

Updated 2026-10-18 in America/Los_Angeles. This file is a replacement snapshot.

## Active goal

Deliver the owner-activated Phase 2 runtime, shared voice foundations and replaceable memory
backends on `main`. OpenCode and SQLite remain the defaults. Local-only model rules remain in
force. Physical iPad voice checks and external Apple work are deferred until a complete build is
ready.

## Completed foundations

The durable driver has common contracts, one active FIFO task, restart recovery, OpenCode and
native adapters, guarded action receipts, bounded context and restart-safe owner input. Owner
input resumes exactly once, preserves the adapter checkpoint and is redacted before its atomic
event and checkpoint write. P2-03 is complete. P2-05, P2-07, P2-08 and P2-09 remain open for their
production integration and full Verify clauses.

The daemon contains a production-shaped native step client. It fixes the approved model and
context, validates the strict schema, applies the configured gate, redacts before every boundary
and records one atomic context-pack and ledger pair for every attempt. It is intentionally not
activated yet, so OpenCode remains the default and P2-06 remains partial.

The common benchmark now has strict bounded fixtures and report scoring for both harnesses. It
covers all eight required personal-task journeys and fails on wrong citations, cross-realm memory,
approval errors, duplicate effects, recovery failures and usage bounds. It does not yet run the
production harnesses, so P2-10 and P2-12 remain open.

The Overseer delegate contract bounds submissions, progress, permissions and completion behind a
transport interface and gives Mote no worktree authority. The local Overseer source was inspected
at `a7a5cac0`. The real socket client, durable cursor mirroring and live worktree fixture remain,
so P2-11 is partial.

`mote-memory` defines the backend-neutral scoped contract and `mote-memory-sqlite` implements it.
SQLite remains the production default. Tencent remains an opt-in comparison candidate and is not
wired into production.

## Verification state

Focused tests for context atomicity, owner-input resume, native ledgering, benchmark scoring and
Overseer delegation pass. Focused all-target clippy and `git diff --check` pass. Run the full
workspace test and clippy gates immediately before every commit. The workspace tests need local
loopback permission for their scripted servers.

Preserve the owner's untracked `.DS_Store` and `.superpowers/`.

## Next action

Commit and push the four verified increments separately. Then wire the native step client through
the daemon's durable driver, add a real Overseer socket transport with atomic cursor mirroring and
build the benchmark runner against the production OpenCode and Mote harness paths. Keep OpenCode
and SQLite as defaults until the full comparison passes and the owner separately approves a
change.
