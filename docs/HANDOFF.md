# Current development handoff

Updated 2026-10-11 in America/Los_Angeles. This file is a replacement snapshot.

## Contract and scope

Read `AGENTS.md`, `docs/rfcs/v0.md`, the root README Ready for Go board and `docs/superpowers/specs/2026-10-11-ipad-host.md` first. The active goal is a distinct physical-iPad and internal TestFlight build of the existing Rust immediate-mode Matrix client. IPAD-06 through IPAD-10 are active. HOST-05's literal Mac sleep and wake test and V0-53's logout and login test remain open for later. Cloud relay, external testing, App Review and public release remain out of scope.

## Verified state

HOST-01 through HOST-04 and IPAD-01 through IPAD-05 are checked with evidence in `docs/verification/`. The iPad Simulator has passed the gallery, live two-host pairing, chat, typed Matrix interactions, inferred UI and browser approval journeys. The Mac host uses local Ollama only. Browser approval remains host-bound and inert until a valid exact decision.

IPAD-09 is checked and pushed as `03434e4`. The Rust physical-iPad target and unsigned Xcode `iphoneos` arm64 build passed. The native Simulator gallery and live paired-host regressions passed. Evidence is in `docs/verification/IPAD-09.md`.

The IPAD-10 packaging tool began in `340db58`. `ios/Mote/distribute.py` has a read-only preflight, explicit signed archive and IPA export, and a separate opt-in upload. It requires a current supplied app inventory for the selected team, a matching distinct App Store Connect record before archive, local distribution signing, a new output path and physical-device confirmation. It checks bundle identity, version, build, signing and entitlements. `ios/Mote/DISTRIBUTION.md` describes the inputs. Thirteen Python tests pass, including cross-team rejection and mocked archive/export/upload paths. The installed Xcode's entitlement output exposed a parser mismatch, and the verifier now requests parseable XML. Partial evidence is in `docs/verification/IPAD-10.md`. IPAD-10 remains unchecked because no real signed archive, IPA or upload has been verified.

After that commit, a fresh temporary iPad Pro 11-inch (M5) Simulator ran the native inferred-UI and browser-approval journeys against the current app. Both Rust harnesses passed and both XCUITest result bundles reported one pass and zero failures. The inferred run checked success, retry, preservation and prose fallback. The browser run checked denial, exactly one approved effect despite repeated taps and natural expiry without another effect. The temporary Simulator was shut down and deleted. The evidence is appended to `docs/verification/IPAD-09.md` and needs a documentation-only commit and push after the required source gates.

The repository's configured `origin` is `https://github.com/beelol/mote.git`. Push each verified commit to `main`. Preserve unrelated `.DS_Store` and `.superpowers/` files.

## Remaining work

No physical iPad was connected when `xcrun devicectl list devices` was checked on 2026-10-11. IPAD-06 requires physical touch, keyboard, rotation, split view, background recovery, local-network permission, light and dark appearance, reduced motion and VoiceOver evidence. Do not count the Simulator as a physical device.

IPAD-07 needs an approved Mote display name, unique bundle ID, SKU, Apple team, signing configuration and a separate App Store Connect app record. Do not invent these identifiers or reuse another app. Archive only after physical UI verification and record creation. IPAD-08 needs a processed internal TestFlight build installed on the physical iPad with pairing, live chat, generated Matrix UI, browser approval, reconnect and host-switch isolation. IPAD-10 still needs a real signed archive and IPA inspection, a deliberate upload and processing evidence. Do not mark any of these criteria complete without their full Verify clauses.

TestFlight distributes the client binary. It does not create an Internet route to the local Mac host. A test from another city needs a separately verified route. Preserve pairing offers, credentials, client state, browser captures and host databases outside Git.
