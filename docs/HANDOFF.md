# Current development handoff

Updated 2026-10-11 in America/Los_Angeles. This file is a replacement snapshot.

## Contract and scope

Read `AGENTS.md`, `docs/rfcs/v0.md` and the root README Ready for Go board first.
The previous goal delivered the Rust immediate-mode Matrix client in the iPad Simulator over an authenticated localhost Mac host.
The active goal is a distinct physical-iPad and internal TestFlight build of that same client, including guarded, repeatable archive and export tooling.
The approved design is `docs/superpowers/specs/2026-10-11-ipad-host.md`.
The implementation plans are under `docs/superpowers/plans/2026-10-11-ipad-*.md`.
The owner activated IPAD-06 through IPAD-10 on 2026-10-11. Physical iPad, signing and internal TestFlight work are now in scope.
HOST-05's literal sleep and wake test and V0-53's logout and login test are deferred without being marked passed.
Cloud relay, external testing, App Review and public release remain out of scope.

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

The new distribution planning edit passed `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` on 2026-10-11. Commit and push only the three scoped documentation files next.
IPAD-09 is the first independent implementation task: add the physical Rust target and Xcode device build while keeping the Simulator target unchanged.
IPAD-10 adds a guarded no-mutation preflight and explicit archive/export/upload script. Its signed export and upload evidence depends on confirmed Apple identity and IPAD-07.
IPAD-06 requires a physical iPad. IPAD-07 requires a unique bundle ID, Apple team, SKU, signing and a separate App Store Connect record. IPAD-08 requires a processed internal build installed and exercised on the physical iPad.
Do not invent these Apple identifiers or assume the installed app works from another city. TestFlight distribution does not add an Internet route to the local Mac gateway.
HOST-05 remains open for a literal Mac sleep and wake run. The read-only power preflight found no future wake event. Its exact observations are in `docs/verification/HOST-05.md`.

The local `main` contains the browser coordinator, iPad approval client and native harness commits.
On 2026-10-11 the owner confirmed the repository's configured `origin`, `https://github.com/beelol/mote.git`, as the push destination and requested pushes whenever work is ready.
The prior 26 pending commits through `301736a` were pushed. Local `main` and `origin/main` matched at `5521a1b` before the new distribution planning edit.
Push later verified commits to this configured origin when ready.

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
