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

P2V-01 is complete. The next ready voice batch is implemented but not yet committed: portable
PCM, gate, endpoint and word decisions plus the default immediate-egui circling avatar in the live
iPad shell and offline gallery. The UI reports capture as unavailable honestly until the macOS
runtime and supervisor are connected.

The Tencent adapter is implemented in an uncommitted crate with loopback-only transport, scoped
sidecar validation, corrections and forget tombstones. It still needs root workspace integration,
then live pinned MemoryCore and comparative benchmark work. SQLite remains the default.

## Verification state

`cargo test --workspace` and `cargo clippy --workspace -- -D warnings` passed on 2026-10-19 with
explicit live hardware, Simulator, Bonjour and headed-browser tests ignored. The workspace tests
require local loopback permission.

Preserve the owner's untracked `.DS_Store` and `.superpowers/`.

## Next action

Commit and push P2-04, then verify and commit P2V-02 and the voice surface as separate increments.
Integrate the Tencent crate without changing the default backend. Continue with the macOS voice
runtime, bounded listener and supervisor before claiming live voice capture.
