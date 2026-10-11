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

Branch and `origin/main` are at `fb03d97` before this handoff checkpoint. HOST-04 and HOST-05 are
open. Host plan Task 5 is next. It adds Bonjour address hints but does not trust addresses as
identity. The host plan ledger is
`.superpowers/sdd/2026-10-11-ipad-simulator-host/progress.md`. The UI plan ledger is
`.superpowers/sdd/2026-10-11-ipad-simulator-ui/progress.md`. The UI subagent has not yet been
dispatched because the reviewed host foundation interfaces are still being completed.

The next action is to dispatch host plan Task 5 from the current head, review its diff, run the
workspace suite and strict clippy, and push. Keep HOST-04 unchecked until the Simulator discovers
the service in UI plan Task 7. Task 6 then adds authoritative resync and visible connection states.
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
