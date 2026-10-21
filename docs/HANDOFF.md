# Current development handoff

Updated 2026-10-20 in America/Los_Angeles. This file is a replacement snapshot.

## Active goal

Deliver the owner-activated Phase 2 runtime, shared voice foundations and replaceable memory
backends on `main`. OpenCode and SQLite remain the defaults. Local-only model rules remain in
force. Voice controls are present by default, while capture starts only after owner action and
platform permission. Physical iPad voice checks and external Apple work remain deferred.

## Current verified batch

P2V-08 is complete. Voice is a first-class Matrix surface with bounded redacted status, partial
speech and speaking text. The macOS daemon compiles every voice state and handles the Matrix
toggle action. The iPad immediate renderer shows the same Matrix control and state replacement,
with the circling avatar rendered beneath it and reduced-motion behavior preserved.

The dedicated native iOS Simulator journey passed in active and reduced-motion modes and captured
all four expected screenshots. Public Matrix compilation, strict host-protocol decoding, redaction,
bounded status text and client-core gallery behavior are covered by Rust tests.

Earlier Phase 2 harness, native runtime, browser, memory, voice protocol, speech supervisor and
speech reply work remains as recorded in its criterion evidence. OpenCode and SQLite remain the
defaults. The Tencent adapter is optional. This Mac still has no local Whisper model, so a live
production Start reports Unavailable rather than pretending capture is active.

## Verification state

`cargo test --workspace` and `cargo clippy --workspace -- -D warnings` passed on 2026-10-20 after
the P2V-08 implementation. Workspace tests required local loopback permission. The dedicated
native Simulator voice journey passed on iOS 27 with Xcode 27.1.

Preserve the owner's untracked `.DS_Store`, `.superpowers/` and Tencent target directory. Preserve
all unrelated modified files.

## Shared link crates (2026-10-21)

`mote-gateway` now takes base32, local-range rules and the Noise primitives (patterns, chunked
framing, PSK derivation, handshake builder, key helper) from `agent-link-core` in the
agent-engine repository, pinned by Git revision with default features off so Mote keeps its own
rusqlite 0.31 device store. The prologue `mote-gateway-v1`, the `MOTE1` prefix, the wire format,
pairing, device store, session binding and server are unchanged. The Overseer and Mote vector
fixtures are untouched and still pass. `cargo test --workspace` (961 tests) and
`cargo clippy --workspace -- -D warnings` passed. Overseer has not been changed.

## Next action

Complete P2V-09 source, licensing and redaction review for shared voice code, then finish P2V-10
consumer validation from a clean Overseer worktree. Keep physical iPad voice and external Apple
checks deferred until the owner chooses to run them.
