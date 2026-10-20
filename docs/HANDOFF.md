# Current development handoff

Updated 2026-10-20 in America/Los_Angeles. This file is a replacement snapshot.

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
work. P2V-03 is complete and pushed in `0325980`; P2V-04 is pushed in `d7c51e7`. P2V-05 is
complete and pushed in `2d806be`: voice is Off by default, starts only through an owner action,
handles permission and child failures conservatively, and leaves typed turns healthy. This Mac
has no local Whisper model yet, so production Start truthfully reports Unavailable.

The paired iPad voice bridge is implemented as a partial P2V-08 increment. The existing immediate
egui avatar uses the selected host's authenticated Start, Stop and State calls, durable state
events and reconnect refresh. State and pending requests are isolated per host, transport failure
clears optimistic state, and `voice.send` remains local-only. P2V-08 stays open because the avatar
is not yet a Matrix language node and Simulator journeys are not complete.

P2V-06 is implemented in the current batch. Explicit voice starts record durable session IDs,
automatic listener restarts retain them, and complete utterances cross a bounded worker into the
scoped durable Mote HRNS driver. Partial speech never enters the worker. Session and utterance IDs
form deterministic task identities, so duplicate finals do not produce a second model step.

P2V-07 is implemented in the current batch. Completed task replies use the shared in-memory macOS
speech engine and report phrase progress. Lower, restore, immediate stop, phrase stop and spoken
stop words are deterministic. Only fully completed words survive interruption into the next task
context. A pending approval keeps the new voice task queued and no voice control can decide it.
STT and TTS produce redacted tool-ledger rows without creating model-call rows or packs.

## Verification state

`cargo test --workspace` and `cargo clippy --workspace -- -D warnings` passed on 2026-10-20 with
explicit live hardware, Simulator, Bonjour and headed-browser tests ignored. The workspace tests
require local loopback permission.

Preserve the owner's untracked `.DS_Store` and `.superpowers/`.

## Next action

Make the avatar a first-class Matrix surface, finish Simulator journeys and provision the local
Whisper model before claiming live in-app voice or completing P2V-08.
