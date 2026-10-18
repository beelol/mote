# Shared voice foundations design

Date: 2026-10-17

Status: approved Phase 2 design

## Outcome

Mote extracts the reusable foundations of Overseer's current voice mode into Rust crates that do
not know either product's task model. Mote integrates them behind explicit configuration. A later
Overseer change consumes the same pinned crates without moving Overseer's task routing into Mote.

The audited Overseer source is `beelol/overseer` at revision
`2b1a19a9b6e639628367a22e16155af20df4f86a`. The reusable source is the `voice` crate. The
modules under `daemon/src/voice` remain Overseer-specific orchestration.

## Alternatives considered

Copying the entire `overseer-listener` crate would be fast, but it would preserve Overseer names,
vocabulary and daemon assumptions. A new shared repository would have clean ownership, but it
would add release and dependency work before either product benefits. The chosen approach hosts
product-neutral crates in Mote and lets Overseer consume a pinned Git revision.

## Repository lifecycle

The crates begin under Mote's conventional Rust workspace directory `crates/`. This location does
not make every Mote crate shared. Mote consumes the voice crates by path. Overseer consumes them
through a dependency pinned to a Mote Git revision. When the owner is ready to operate an
independent shared project, the product-neutral crates move into their own repository and both
products pin that repository. The public Rust interfaces and compatibility fixtures are the
migration boundary, so moving repositories does not merge the product runtimes.

## Crate split

```text
crates/mote-voice-protocol/
  commands, events, source labels and JSONL framing

crates/mote-voice-core/
  PCM handling, speech gate, endpointing, word rules and deterministic fixtures

crates/mote-voice-macos/
  macOS capture, call detection, local Whisper recognition and in-memory speech

moted/
  listener supervision, ledger integration, task routing and Matrix events
```

`mote-voice-protocol` and `mote-voice-core` have no product name, daemon state, UI state or
network client. `mote-voice-macos` is an optional target-specific crate. Each product keeps a thin
listener binary and its own supervisor.

## Protocol

The listener receives `speak`, `restore`, `stop`, `hint`, `suppress`, test feed and shutdown
commands. It emits ready, gate, level, partial words, final utterance, dropped input, barge-in,
speech progress, pause, resume, error and end events.

Production events never carry raw audio. Test feed commands may carry bounded PCM only when the
listener starts in an explicit fixture mode. Source labels are `owner` and `agent`, not product
names. Product vocabulary arrives through the hint command.

## Resource and privacy boundary

Voice is off by default. It runs as a separate child process. Recognition is local. Speech uses
the local macOS speech service used by the audited implementation. The process opens no outbound
socket. It holds at most 30 seconds of audio and limits one utterance to 90 seconds. Raw audio is
not persisted or sent to `moted`.

The current implementation reuses Overseer's measured macOS path first. The older Moonshine,
Pocket TTS, Pi and full-duplex research in `docs/reuse/voice.md` remains backlog. It is not mixed
into this extraction.

## Mote integration

`moted` starts the listener only when voice is enabled. It converts a final utterance into one
owner turn for `mote-hrns`. Partial words, level, pause and speech progress become redacted UI
events. STT and TTS operations receive ledger rows with model or engine, audio duration and wall
time.

Barge-in stops or lowers speech according to the protocol and never bypasses task or approval
state. If the listener dies, Mote reports voice unavailable and keeps typed interaction working.
It does not restart in a tight loop.

The Matrix surface shows off, starting, listening, hearing, thinking, speaking, paused and error
states. The iPad receives those states through the existing host connection. iPad microphone
capture is deferred.

## Acceptance criteria

- **P2V-01** `mote-voice-protocol` contains product-neutral commands and events with golden JSONL
  compatibility fixtures derived from Overseer revision `2b1a19a`. **Verify:** round-trip and
  golden tests pass, no production event contains PCM, and a source scan finds no product name.
- **P2V-02** `mote-voice-core` provides PCM, gate, endpoint and word logic without an OS, model,
  daemon or network dependency. **Verify:** deterministic fixtures cover silence, noise, speech,
  endpoint timeout, longest utterance and barge-in word rules.
- **P2V-03** `mote-voice-macos` captures locally, detects another recording app, recognizes with
  the pinned local Whisper path and speaks without a temporary audio file. **Verify:** ported
  fixture tests pass and target-specific compilation succeeds.
- **P2V-04** The listener enforces the 30-second held-audio and 90-second utterance limits and
  opens no outbound socket. **Verify:** bounded feed tests report maximum held audio, and a live
  process network inspection shows no outbound connection.
- **P2V-05** `moted` supervises voice behind an off-by-default setting and exposes visible
  unavailable and failure states without affecting typed interaction. **Verify:** start, stop,
  crash, malformed event and rapid-failure tests pass.
- **P2V-06** Final utterances enter `mote-hrns` once as scoped owner turns while partial words and
  levels never invoke a harness. **Verify:** duplicate, reconnect and partial-event fixtures
  produce one task turn and one ledgered model step.
- **P2V-07** Speaking, barge-in and interruption preserve the played-word boundary in context and
  do not bypass an outstanding approval. **Verify:** scripted speech and interruption fixtures
  assert context, action and approval state.
- **P2V-08** Matrix shows the voice state, partial words, errors and the current speaking line on
  Mac and through the iPad host protocol. **Verify:** deterministic surface snapshots and
  simulator journeys cover every state. Physical iPad microphone capture is not required.
- **P2V-09** Every derived crate records source files, source revision and license notices, and
  redaction finds no secret in carried fixtures. **Verify:** crate READMEs, notices, license
  inventory and fixture scans are recorded in evidence.
- **P2V-10** A clean Overseer worktree can consume the pinned shared crates with its voice and
  daemon tests unchanged at the boundary. **Verify:** record the dependency revision, test output
  and a protocol compatibility comparison. Do not alter the owner's dirty Overseer checkout.

## Deferred work

Physical iPad capture, Pi capture, Moonshine, Pocket TTS, full duplex, cloud speech, automatic
voice enablement and microphone permission journeys are deferred. They require separate criteria.
