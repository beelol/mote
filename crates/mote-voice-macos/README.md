# mote-voice-macos

`mote-voice-macos` is the optional, product-neutral macOS I/O layer for the shared voice system.
It provides:

- 16 kHz mono capture and playback through the macOS voice-processing audio unit
- detection of another application using recording and playback at the same time
- local Whisper recognition with caller-supplied vocabulary hints
- macOS system speech rendered into memory without a temporary audio file
- an in-memory phrase queue that can pause, lower, resume or stop speech
- a small coordinator that pauses both capture and speech while another call is active

The real platform implementations are available only on macOS. The traits, deterministic
scripted recognizer, fixture synthesizer and buffered speech output compile on every target. On
other targets, constructing a real microphone, Whisper recognizer or system synthesizer returns
an explicit unavailable error.

This crate opens no network connection, has no product task or daemon dependency and never
persists captured or synthesized audio. A product owns permission prompts, process supervision,
resource limits, task routing, UI state and model-call accounting.

## Provenance

The implementation is derived from the `voice` crate in `beelol/overseer` at revision
`2b1a19a9b6e639628367a22e16155af20df4f86a`, specifically:

- `voice/src/mic.rs`
- `voice/src/calls.rs`
- `voice/src/recognize.rs`
- `voice/src/memspeech.rs`
- `voice/src/speak.rs`
- `voice/src/synth.rs`

Product vocabulary, environment-variable test hooks, listener orchestration and daemon behavior
were removed. See `NOTICE` for license and adaptation details.
