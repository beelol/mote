# Current development handoff

Updated 2026-10-19 in America/Los_Angeles. This file is a replacement snapshot.

## Active goal

Deliver the owner-activated Phase 2 runtime, shared voice foundations and replaceable memory
backends on `main`. OpenCode and SQLite remain the defaults. Local-only model rules remain in
force. Voice controls are present by default, while capture starts only after owner action and
platform permission. Physical iPad voice checks and external Apple work remain deferred.

## Current verified batch

P2-04 is complete. Optional `agent.harness` selection persists across daemon reconstruction,
switches only at a conservative safe checkpoint and never silently falls back. Both OpenCode and
native Mote are available selections; missing configuration still selects OpenCode.

The native Mote harness, Overseer socket delegate and deterministic personal-task benchmark remain
opt-in or partial as recorded in P2-06, P2-11 and P2-12 evidence. Native tool effects are still held
before execution.

P2V-01 and P2V-02 are complete. The product-neutral wire protocol and portable PCM, speech gate,
endpoint and word decisions are strict, bounded and deterministic. P2V-08 is partial: the
immediate-egui circling avatar is present by default in the live iPad shell and offline gallery,
with accessible state labels and reduced motion. The surface commit `d1b2cbe` is on remote `main`.
It reports capture as unavailable honestly until the macOS runtime and supervisor are connected.

The Tencent adapter is ready for its integration commit with loopback-only transport, scoped
sidecar validation, server-generated candidate IDs, conservative ambiguous-write recovery,
corrections and durable forget tombstones. P2V-03 is complete and pushed in `0325980`: the macOS runtime passes focused
tests, strict clippy, explicit Apple target compilation and the full workspace gates. The bounded
listener implementation and focused tests are complete but uncommitted; its live no-outbound-
socket observation and P2V-04 evidence remain. SQLite remains the default.

## Verification state

`cargo test --workspace` and `cargo clippy --workspace -- -D warnings` passed on 2026-10-19 with
explicit live hardware, Simulator, Bonjour and headed-browser tests ignored. The workspace tests
require local loopback permission.

Preserve the owner's untracked `.DS_Store` and `.superpowers/`.

## Next action

Commit and push the Tencent Task 6 adapter without claiming MBE-07. Integrate the bounded listener
as P2V-04 after its live no-outbound-socket observation. Continue with the voice supervisor before
claiming live voice capture or completing P2V-08.
