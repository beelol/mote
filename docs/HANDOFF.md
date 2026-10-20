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

The loopback-only Tencent adapter is pushed in `8394c17` with scoped sidecar validation,
server-generated candidate IDs, conservative ambiguous-write recovery, corrections and durable
forget tombstones. SQLite remains the default and the pinned MemoryCore lifecycle remains later
work. P2V-03 is complete and pushed in `0325980`. P2V-04 is complete: the bounded listener passes
its deterministic tests and a live process inspection found no network descriptors.

## Verification state

`cargo test --workspace` and `cargo clippy --workspace -- -D warnings` passed on 2026-10-19 with
explicit live hardware, Simulator, Bonjour and headed-browser tests ignored. The workspace tests
require local loopback permission.

Preserve the owner's untracked `.DS_Store` and `.superpowers/`.

## Next action

Commit and push the bounded listener as P2V-04, then implement the owner-activated voice supervisor
and configuration for P2V-05. Do not claim live in-app voice or complete P2V-08 until daemon state
and final utterances are actually connected to the app journey.
