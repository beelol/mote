# Shared Voice Foundations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract Overseer's proven macOS voice foundations into product-neutral Rust crates and integrate them with Mote without sharing either product's agent runtime.

**Architecture:** Protocol and deterministic audio decisions live in portable crates. macOS capture, recognition, call detection and speech live in an optional platform crate. Mote owns listener supervision, model accounting, task routing and Matrix state.

**Tech Stack:** Rust 2021, serde, serde_json, whisper-rs 0.16 with Metal, macOS AudioToolbox and CoreAudio FFI already used by Overseer, existing Mote event log and ledger.

**Spec:** `docs/superpowers/specs/2026-10-17-shared-voice.md`

## Global Constraints

- Derive only from Overseer revision `2b1a19a9b6e639628367a22e16155af20df4f86a`.
- Keep `daemon/src/voice` orchestration in Overseer.
- Emit no raw audio in production events and open no outbound socket.
- Hold at most 30 seconds of audio and cap one utterance at 90 seconds.
- Include the voice surface by default, keep capture stopped until explicit owner action and keep
  typed interaction available after any voice failure.
- Keep iPad microphone capture, Pi audio, Moonshine, Pocket TTS and full duplex deferred.
- Run workspace tests and strict clippy before every commit, add criterion evidence and push.

## Review Focus

- Product names embedded in protocol, vocabulary or word rules must become data, not constants. Tasks 1 and 2 pin this.
- Fixture-only PCM must be impossible in ordinary listener mode. Tasks 1 and 4 pin this.
- A listener crash loop must stop and leave typed interaction healthy. Task 5 pins this.
- Barge-in must preserve the last played words and never approve an action. Task 6 pins this.
- Another recording application must pause both listening and speech without persisting audio. Task 3 pins this.

---

### Task 1: Product-neutral voice protocol

**Files:**
- Create: `crates/mote-voice-protocol/Cargo.toml`
- Create: `crates/mote-voice-protocol/src/lib.rs`
- Create: `crates/mote-voice-protocol/tests/golden.rs`
- Create: `crates/mote-voice-protocol/tests/fixtures/overseer.jsonl`
- Create: `crates/mote-voice-protocol/README.md`
- Create: `crates/mote-voice-protocol/NOTICE`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `Source::{Owner, Agent}`, `Command`, `Event`, `ScriptLine`, `decode_line(&str)` and `encode_line<T: Serialize>(&T)`.
- Produces `decode_command(line: &str, mode: InputMode)` where `InputMode::Production` refuses
  `Command::Feed`, `FeedEnd`, `Simulate` and `Script`.

- [x] **Step 1:** Copy and redact the protocol fixture from the pinned Overseer revision, replacing product source labels with `agent` while preserving all other wire shapes.
- [x] **Step 2:** Write golden tests for every command and event, unknown fields, malformed JSON and production refusal of PCM feed commands.
- [x] **Step 3:** Run `cargo test -p mote-voice-protocol` and confirm it fails before the crate exists.
- [x] **Step 4:** Implement the protocol with strict serde decoding and no product string.
- [x] **Step 5:** Run the targeted tests and a source scan for `Overseer`, `Mote` and raw PCM in production event variants.
- [x] **Step 6:** Run workspace gates, add P2V-01 evidence, commit `feat: define shared voice protocol` and push.

### Task 2: Portable voice core

**Files:**
- Create: `crates/mote-voice-core/Cargo.toml`
- Create: `crates/mote-voice-core/src/lib.rs`
- Create: `crates/mote-voice-core/src/pcm.rs`
- Create: `crates/mote-voice-core/src/gate.rs`
- Create: `crates/mote-voice-core/src/endpoint.rs`
- Create: `crates/mote-voice-core/src/words.rs`
- Create: `crates/mote-voice-core/tests/fixtures.rs`
- Create: `crates/mote-voice-core/README.md`
- Create: `crates/mote-voice-core/NOTICE`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `PcmFrame`, `SpeechGate`, `Endpoint`, `EndpointDecision`, `WordDecision` and `classify_barge(words: &str, vocabulary: &Vocabulary) -> WordDecision`.
- Depends only on the Rust standard library and `mote-voice-protocol`.

- [x] **Step 1:** Port deterministic silence, noise, speechlike, endpoint, timeout and stop-word fixtures from Overseer.
- [x] **Step 2:** Add a dependency-boundary test that rejects OS, model, daemon and network crates.
- [x] **Step 3:** Run the core tests and confirm they fail before the modules exist.
- [x] **Step 4:** Port PCM, gate, endpoint and word algorithms with product vocabulary supplied through `Vocabulary`.
- [x] **Step 5:** Run the fixture and dependency tests and compare decisions against the pinned Overseer outputs.
- [x] **Step 6:** Run workspace gates, add P2V-02 evidence, commit `feat: extract portable voice decisions` and push.

### Task 3: Optional macOS voice runtime

**Files:**
- Create: `crates/mote-voice-macos/Cargo.toml`
- Create: `crates/mote-voice-macos/src/lib.rs`
- Create: `crates/mote-voice-macos/src/mic.rs`
- Create: `crates/mote-voice-macos/src/calls.rs`
- Create: `crates/mote-voice-macos/src/recognize.rs`
- Create: `crates/mote-voice-macos/src/memspeech.rs`
- Create: `crates/mote-voice-macos/src/speak.rs`
- Create: `crates/mote-voice-macos/src/synth.rs`
- Create: `crates/mote-voice-macos/tests/runtime.rs`
- Create: `crates/mote-voice-macos/README.md`
- Create: `crates/mote-voice-macos/NOTICE`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `Microphone`, `CallMonitor`, `Recognizer`, `SpeechOutput` and `MacVoiceRuntime` traits or structs with explicit `start`, `stop` and callback lifetimes.
- Consumes: portable `PcmFrame`, protocol events and a caller-supplied vocabulary hint.

- [x] **Step 1:** Add target-gated tests for scripted recognition, in-memory speech buffers and pause/resume when the fake call monitor changes state.
- [ ] **Step 2:** Run `cargo test -p mote-voice-macos` and confirm it fails before the crate exists.
- [x] **Step 3:** Port the pinned capture, call, recognition and speech modules while removing Overseer process and task dependencies.
- [x] **Step 4:** Keep `whisper-rs = 0.16` Metal support target-specific and make non-macOS workspace checks compile a documented unavailable stub.
- [x] **Step 5:** Run the targeted tests and `cargo check -p mote-voice-macos --target aarch64-apple-darwin`.
- [x] **Step 6:** Run workspace gates, add P2V-03 evidence, commit `feat: extract macOS voice runtime` and push.

### Task 4: Bounded listener binary

**Files:**
- Create: `moted/src/bin/mote-voice-listener.rs`
- Create: `moted/tests/voice_listener.rs`
- Modify: `moted/Cargo.toml`

**Interfaces:**
- Produces binary `mote-voice-listener --input <mic|fixture> --model <path>` with JSONL stdout and stdin.
- Produces: `ListenerConfig { input, model_path, fixture_mode, held_audio_ms: 30_000, utterance_ms: 90_000 }`.

- [x] **Step 1:** Add fixture-mode tests for 30-second held-audio eviction, 90-second utterance termination, malformed commands and absence of PCM in emitted events.
- [ ] **Step 2:** Run the listener test and confirm it fails before the binary exists.
- [x] **Step 3:** Implement the listener loop from shared crates with explicit fixture-mode capability.
- [x] **Step 4:** Run the bounded tests and inspect a live listener process for outbound sockets using the command recorded in P2V-04 evidence.
- [x] **Step 5:** Run workspace gates, add P2V-04 evidence, commit `feat: add bounded voice listener` and push.

### Task 5: Mote supervision and configuration

**Files:**
- Create: `moted/src/voice.rs`
- Create: `moted/tests/voice_supervisor.rs`
- Modify: `moted/src/config.rs`
- Modify: `moted/src/main.rs`
- Modify: `moted/src/pipeline.rs`

**Interfaces:**
- Voice controls are present in normal builds without a developer configuration key. Capture is
  stopped until an owner start action and platform permission. Optional `voice.input` and
  `voice.model_path` overrides remain local-only and leave `DEFAULT_CONFIG_TOML` unchanged.
- Produces: `VoiceSupervisor::start`, `stop`, `send`, `state` and `VoiceState::{Off, Starting, Listening, Hearing, Thinking, Speaking, Paused, Unavailable, Error}`.

- [x] **Step 1:** Add supervisor tests for default availability with no capture, permission denial,
  start, clean stop, malformed event, child crash and rapid-failure backoff.
- [ ] **Step 2:** Run the tests and confirm missing config keys and supervisor APIs fail.
- [x] **Step 3:** Implement child supervision with bounded restart attempts and redacted state events.
- [x] **Step 4:** Prove typed turns continue when voice is unavailable or the child exits.
- [x] **Step 5:** Run workspace gates, add P2V-05 evidence, commit `feat: supervise optional voice mode` and push.

### Task 6: Task routing, speech and barge-in

**Files:**
- Modify: `moted/src/voice.rs`
- Modify: `moted/src/hrns.rs`
- Create: `moted/tests/voice_task.rs`
- Modify: `crates/mote-hrns/src/context.rs`

**Interfaces:**
- Final `Event::Utterance` calls `HrnsRuntime::enqueue_owner_turn(scope, text, VoiceSource)` once.
- Produces: `SpokenLine { line_id, task_id, text, played_through_word }` and records STT and TTS rows through existing ledger structures.

- [ ] **Step 1:** Write tests that duplicate final utterances, interleave partial words and reconnect the listener while asserting one task turn and no partial-triggered model call.
- [ ] **Step 2:** Write scripted speech tests for lower, restore, stop word and phrase stop while an approval remains pending.
- [ ] **Step 3:** Run the tests and confirm the routing and played-word APIs are missing.
- [ ] **Step 4:** Implement utterance deduplication, ledger entries, speech commands and context trimming at the played-word boundary.
- [ ] **Step 5:** Run the targeted tests plus task action and approval tests.
- [ ] **Step 6:** Run workspace gates, add P2V-06 and P2V-07 evidence, commit `feat: route voice through durable tasks` and push.

### Task 7: Matrix voice surface and shared-consumer proof

**Files:**
- Create: `matrix/examples/voice-status.matrix`
- Modify: `moted/src/pipeline.rs`
- Modify: `moted/src/matrix.rs`
- Modify: `crates/mote-client-core/src/gallery.rs`
- Create: `moted/tests/voice_surface.rs`
- Create: `docs/verification/P2V-08.md`
- Create: `docs/verification/P2V-09.md`
- Create: `docs/verification/P2V-10.md`

**Interfaces:**
- Consumes: `VoiceState`, partial words, current spoken line and redacted error.
- Produces: a default circling-avatar Matrix surface with bounded bindings, an accessible state
  label, reduced-motion behavior and no audio bytes or model path.

- [ ] **Step 1:** Add deterministic surface snapshots for every voice state, active and reduced
  avatar motion and transitions that replace partial words without stale controls.
- [ ] **Step 2:** Implement the Matrix surface and run Mac headless and iPad Simulator host-protocol journeys.
- [ ] **Step 3:** Record source files, pinned revision, license inventory, notices and redaction scans for all shared crates.
- [ ] **Step 4:** Create a clean Overseer worktree, replace only its listener foundation with pinned Mote Git dependencies and run its voice and daemon boundary tests.
- [ ] **Step 5:** Confirm the owner's dirty Overseer checkout is unchanged and record the clean worktree revision and comparison output.
- [ ] **Step 6:** Run both repositories' required gates, complete P2V-08 through P2V-10 evidence, commit `feat: expose shared voice mode` in Mote and the corresponding conventional commit in the clean Overseer worktree, then push each approved repository change.
