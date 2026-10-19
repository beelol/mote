# Current development handoff

Updated 2026-10-18 in America/Los_Angeles. This file is a replacement snapshot.

## Active goal

Deliver the owner-activated Phase 2 runtime, shared voice foundations and replaceable memory
backends on `main`. OpenCode and SQLite remain the defaults. Local-only model rules remain in
force. Voice controls will be present by default, while capture starts only after owner action and
platform permission. Physical iPad voice checks and external Apple work remain deferred.

## Current verified batch

The native Mote harness is now an opt-in daemon path. Selecting `agent.harness = "mote"` sends an
owner handoff through the durable driver, approved local structured model boundary and shared
database. Every tool proposal is held before execution. Direct and Matrix selection tests pass;
absent configuration still selects OpenCode. P2-06 remains partial until action approval resumes
the driver and complete daemon failure journeys pass.

The Overseer delegate has a concrete owner-only Unix-socket client matching local Overseer commit
`a7a5cac0`. It performs the protocol-v1 hello, bounded submission, strict event replay and
permission replies. P2-11 remains partial until identities and cursors are durable, daemon coding
routing exists and fixture and opt-in live journeys pass.

The personal-task benchmark has a deterministic injected runner, validates the whole ordered suite
before execution and fails immediately on executor or usage-bound failure. P2-12 remains partial
until production OpenCode and Mote executors run the same fixtures and publish a comparison.

P2V-01 is complete. `mote-voice-protocol` is product-neutral, strict and bounded; fixture audio
commands are unavailable in production mode and events contain no audio. The next voice work is
the portable deterministic gate, endpoint and word-decision crate, followed by the optional macOS
runtime and default circling-avatar Matrix surface.

## Verification state

`cargo test --workspace` passed with explicit live hardware, Simulator, Bonjour and headed-browser
tests ignored. `cargo clippy --workspace -- -D warnings`, the focused socket, benchmark, native and
voice suites and `git diff --check` pass. The workspace tests require local loopback permission for
their scripted servers.

Preserve the owner's untracked `.DS_Store` and `.superpowers/`.

## Next action

Commit and push this verified batch in small increments. Then implement `mote-voice-core`, continue
native action reconciliation and durable Overseer delegation, and add deterministic production
benchmark executors. After the portable voice logic is stable, add the macOS listener and expose
voice in the Matrix app with a circling avatar, accessible labels and reduced-motion behavior.
