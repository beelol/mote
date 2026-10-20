# P2V-08 paired-device voice bridge increment

Status: partial. P2V-08 remains open.

Date: 2026-10-20

Environment: Apple Silicon macOS, Rust 1.89 workspace on `main`. Verification used deterministic
in-process clients and the encrypted loopback gateway. It did not start microphone capture or use
a physical iPad.

## Verified behavior

The existing immediate-egui circling avatar in the live iPad shell now reads voice state from the
selected paired host instead of an app-local placeholder. Start and Stop issue authenticated calls
to that host. Voice state is kept separately for every host, restored from durable `voice.state`
events, and queried after reconnect.

The paired-device allowlist exposes only `voice.start`, `voice.stop` and `voice.state`. It continues
to refuse `voice.send` and configuration methods. Result correlation is host-bound, so a frame from
another host cannot consume a pending request. A command transport failure clears pending state and
reports Unavailable while typed chat remains present.

## Commands and results

```text
cargo test -p mote-ios --lib voice_
PASS: 5 tests

cargo test -p mote-gateway paired_voice_allowlist_exposes_controls_but_not_listener_commands
PASS: 1 test

cargo test -p moted approval_dispatch_tests::loopback_acceptance::paired_voice_controls_route_but_listener_commands_remain_local -- --exact
PASS: 1 encrypted loopback acceptance test
```

## Remaining P2V-08 work

The circling avatar is an immediate Rust UI component, but it is not yet represented as a
first-class Matrix language node or driven by partial words and the current spoken line. Complete
Matrix snapshots, iPad Simulator journeys and a provisioned local Whisper model remain required
before P2V-08 can be checked.
