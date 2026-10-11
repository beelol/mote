# Current development handoff

Updated 2026-10-11 in America/Los_Angeles. This file is a replacement snapshot.

## Scope

The active goal is a Rust immediate-mode Mote client running in the iPad Simulator over an
authenticated localhost gateway to the Mac. The active criteria are HOST-01 through HOST-05 and
IPAD-01 through IPAD-05 in the root README. V0-53 remains independent and owner-only for its
literal logout and login check. IPAD-06 through IPAD-08, physical iPad, Apple signing, App Store
Connect, TestFlight and the cloud relay remain deferred. Do not start them without new owner
authority. Preserve untracked `.DS_Store` and `.superpowers/`.

The approved design is `docs/superpowers/specs/2026-10-11-ipad-host.md`. Its three implementation
plans are `docs/superpowers/plans/2026-10-11-ipad-simulator-host.md`,
`docs/superpowers/plans/2026-10-11-ipad-simulator-ui.md` and
`docs/superpowers/plans/2026-10-11-ipad-agent-browser.md`. The owner approved execution and
requested incremental verified Conventional Commits to `main` and pushes to the configured origin.

## Delivered

HOST-01 was checked and pushed in `b60e1ef` and `7e18050` with protocol goldens and Unix
compatibility evidence. The per-host directory was pushed in `3332860`. HOST-02 stayed open until
two real fixture hosts paired through the gateway.

The Noise pairing core was pushed in `78c2c83` and its security-test fix in `7716594`. The
authenticated daemon loopback gateway was pushed in `ab0e5b4` and its WebSocket and request-state
fix in `fb03d97`. HOST-02 and HOST-03 are checked in the README with
`docs/verification/HOST-02.md` and `docs/verification/HOST-03.md`. Their tests pair two hosts with
colliding IDs, prove state and action isolation, reject revoked devices, and verify encrypted
WebSocket frames, exactly-once outcomes and bound approvals. The final driver-side workspace run
passed 549 tests across 44 suites. `cargo clippy --workspace -- -D warnings` passed. The existing
test-only unused ObservationHarness warning remains unrelated.

The gateway binds `127.0.0.1` and preserves the Unix same-user peer boundary. Pairing uses
`MOTE1`, `mote-gateway-v1` and Noise IKpsk1. Reconnect uses Noise IK. Local-only methods remain
blocked remotely. Model calls use the ledger, remote inputs and stored events are redacted, and
only the approved local Ollama models are permitted.

## Current work

`origin/main` contains Task 5 commits `098aaa2` and `198b3e4`. Bonjour advertises only protocol
major and public fingerprint. Candidate endpoints require pinned-key Noise proof before movement.
The live Mac stack discovered and removed the service. The process-level shutdown test proved
SIGTERM/SIGINT reap the `dns-sd` child and remove registration and Unix listener. The independent
review passed after the fix. The driver reran workspace tests and strict clippy with exit 0.
HOST-04 stays open until the Simulator discovers the service and verifies purpose text in UI plan
Task 7.

Host plan Task 6 was pushed in `ea25228` and review fix `54c2870`. It adds durable authoritative
snapshots, checked replay, explicit connection states and a per-host acknowledged outbox. The review
found and fixed interleaved-thread loss under a shared cursor and oversized snapshot transport
failure. Host events now replay in global order. Large snapshots use bounded chunks installed
atomically after identity and cursor checks. Scoped re-review found no remaining blocking issue.
The driver independently reran workspace tests and strict clippy with exit 0. HOST-05 remains open
for literal Mac sleep and wake. Host plan Task 7 was pushed in `4d62156` and review fix `15a36b4`.
It provides a reusable real-daemon paired loopback harness, with encrypted chat, Matrix, typed
interaction, host-bound approval, endpoint rotation, 5,001-event retention and resync tests. The
fix exposes an opt-in external test fixture without changing the production CLI or authentication.
The independent review and scoped re-review passed. The driver reran workspace tests, the external
fixture test, default strict workspace clippy and feature-enabled strict workspace clippy with
exit 0. HOST-04 and HOST-05 remain open for Simulator discovery and literal Mac sleep/wake.
UI plan Task 1 was pushed in `5c6312f`. Both Apple Rust targets are installed. The prechange
Simulator check reproduced glutin's missing backend error. WGPU now passes
`cargo check -p mote-client --target aarch64-apple-ios-sim`. The Mac client suite passed all 164
tests. The independent review found no blocking issue. The driver reran workspace tests, strict
clippy and Simulator target check with exit 0. IPAD-01 remains open until the native shell builds.
UI plan Task 2, the behavior-neutral shared Rust client core, is assigned to `ipad_core_impl` from
pushed base `57e812b`. Its brief is
`.superpowers/sdd/2026-10-11-ipad-simulator-ui/task-2-brief.md`. Review its diff and report
before pushing.
The core extraction keeps Mac-specific ObservationHarness, navigation and worker integration in
the desktop shell. A narrow ruling permits `serde_json` as a pure-data codec if existing typed
Matrix reducers need it. The core must still exclude eframe, mote-ipc, filesystem, sockets,
SQLite and UIKit.
The implementer is moving neutral renderer, state, gallery and theme modules and their tests.
The event-log Event type will re-export the identical protocol DTO to keep Mac call sites source
compatible. Focused parity checks and the Apple target check remain in progress.
The core and observation-enabled core both compile for `aarch64-apple-ios-sim`. Core tests passed
58 cases. Task 2 and its observation fix were pushed as `6f85b44` and `c515750`. The extraction
keeps exact Matrix IDs, roles, shapes, hit regions and typed events, with an event DTO shared through
the protocol. The review and scoped re-review found no blocking issue. The driver independently
reran workspace tests, strict clippy and Simulator core check with exit 0. The 32-frame observation
regression is fixed and its test passes. Mac-specific observation integration remains in the
desktop shell. IPAD-01 and IPAD-02 remain open. Next is UI Task 3, the minimal native iPad shell.
UI Task 3 was pushed in `33e8fd9`. Both Rust Apple targets, Xcode 27 build and the iOS 27 iPad
Simulator launch passed. The iOS 27 scene adapter attaches the existing winit window without a
second UIKit loop. The offline gallery screenshot confirms the safe-area heading. XCUITest passed
one startup test, the process survived 52 seconds, and the driver independently relaunched it.
Workspace tests and strict clippy passed. IPAD-01 is checked with evidence in
`docs/verification/IPAD-01.md`. No physical device, signing or TestFlight action has started.
Next is UI Task 4, page-based gallery data. IPAD-02 remains open until the full gallery and
transition are driven live on two Simulator sizes and orientations.
The host plan ledger is
`.superpowers/sdd/2026-10-11-ipad-simulator-host/progress.md`. The UI plan ledger is
`.superpowers/sdd/2026-10-11-ipad-simulator-ui/progress.md`. The UI subagent has not yet been
dispatched because the reviewed host foundation interfaces are still being completed.
The literal Mac sleep and wake run in HOST-05 is unsafe unattended without a configured wake
mechanism. Leave that clause open while completing independent work. After stable host interfaces,
dispatch the UI plan to a UI subagent, then implement the inferred UI and contained browser plans.

## Known limits

The desktop queries its real host identity at window startup. If the host is unavailable then,
cursor persistence remains disabled for that window until the recovery task attaches it. Pending
approval cards survive host restart, but exact executable file intent is not stored. A restarted
approval cannot execute and can still be denied. Interrupted remote request IDs remain
indeterminate and fail closed until an operator reconciles the effect and issues a new ID. These
are recorded in Task 4's report and must not be silently relaxed.

Xcode 27.0 and iOS Simulator SDK 27 are installed. iPad Pro, Air, mini and base iPad simulators are
available at iOS 26.5 and 27.0. Rust Apple targets are not installed yet. No Xcode project,
signing state, App Store Connect record or TestFlight build has been created. The restricted shell
cannot reach CoreSimulator, but an approved local command confirmed the devices.
