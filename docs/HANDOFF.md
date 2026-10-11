# Current development handoff

Updated 2026-10-11 in America/Los_Angeles. This file is a replacement snapshot.

## Contract and scope

Read `AGENTS.md`, `docs/rfcs/v0.md` and the root README Ready for Go board first.
The active goal is the Rust immediate-mode Matrix client in the iPad Simulator over an authenticated localhost Mac host.
It includes the component gallery, local-model inferred UI and contained browser actions with iPad approval cards.
The approved design is `docs/superpowers/specs/2026-10-11-ipad-host.md`.
The implementation plans are under `docs/superpowers/plans/2026-10-11-ipad-*.md`.
Physical iPad, signing, App Store Connect, TestFlight and cloud relay work are deferred.
V0-53 requires the owner's literal logout and login and is independent.

## Verified state

HOST-01 through HOST-04 and IPAD-01 through IPAD-05 are checked in the README with evidence in `docs/verification/`.
The iPad gallery, red pixel materialization, paired live chat, typed Matrix interactions, two-host isolation and inferred UI are verified in the Simulator.
The approved local `qwen3:4b` model passed the fixed 200-prompt gate with 192 first-try valid decisions.
No subscription harness was enabled and no model outside `docs/rfcs/v0.md` section 0 was pulled.

The browser runtime uses pinned Chrome for Testing 155.0.8059.39 with a temporary per-character Mote profile and loopback CDP.
It blocks protected requests before dispatch, plus WebSocket and WebRTC side channels.
The daemon exposes named browser tools and retains one exact request for a durable host-bound approval.
The paired gateway grants decision authority only after complete card delivery.
Approval rechecks origin, stable reference, page state, request fingerprint and expiry.
Attempt accounting is durable before release.
Independent review cleared the browser coordinator and iPad approval client.
The browser and UI implementations are locally committed as `3019700` and `b91ebae`.

A real iPad Pro 11-inch (M5) Simulator run passed denial, relaunch with pending approval, two rapid approval taps with exactly one POST, a validated Matrix HTTP 200 result, and natural 127-second expiry with no extra effect.
XCUITest passed one native test with zero failures.
The Rust evidence test passed one live test in 185.34 seconds.
The test re-resolved an Xcode-changed app data container, restored prior Documents files, and confirmed removal of its temporary Keychain credential.
The native harness and recovery regressions are locally committed as `cd2ff0f` after 709 workspace tests, strict workspace clippy, and the Apple Simulator target check passed.
`docs/verification/IPAD-05.md` links the selected redacted captures, headed tests, native run and final source gates.

## Remaining work

HOST-05 remains open.
Its scripted reconnect, retention resync, version mismatch and identity-change cases exist, but its Verify clause also requires one literal Mac sleep and wake run.
Do not sleep the owner's Mac unattended without a reliable wake mechanism.
The 2026-10-11 read-only power preflight found no future wake event and showed active use and audio input. The exact observations are in `docs/verification/HOST-05.md`.
Do not check HOST-05 from scripted tests alone.
IPAD-06 through IPAD-08 remain deferred until the owner can provide a physical iPad and distribution authority.

The local `main` contains the browser coordinator, iPad approval client and native harness commits.
`origin/main` ends at `33e52a6`.
Pushes to `https://github.com/beelol/mote.git` were rejected by safety review pending explicit owner confirmation of that destination.
Do not route around the rejection.
Continue verified local commits and leave pushes pending until the destination is approved.

Preserve unrelated `.DS_Store` and ignored `.superpowers/` work.
Do not check in pairing offers, private credentials, persisted client state, raw filled-page snapshots, host databases or Xcode derived data.

## Limits

The pinned iOS AccessKit backend exposes one window TextView to XCUITest.
Native journeys use screenshot text recognition, exact state assertions and measured touch targets.
Do not claim individual accessibility nodes from that workaround.
Simulator GPU captures and renderer encoding cadence are not display scanout measurements.
The desktop queries host identity at startup.
If the host is unavailable, cursor persistence waits for recovery attachment.
Interrupted remote request IDs remain indeterminate until reconciled.
Never read the default Chrome profile or keychain item.
