# Current development handoff

Updated 2026-10-11 in America/Los_Angeles. This file is a replacement snapshot.

## Contract and scope

Read `AGENTS.md`, `docs/rfcs/v0.md`, the root README Ready for Go board and `docs/superpowers/specs/2026-10-11-ipad-host.md` first. The active goal is a distinct physical-iPad and internal TestFlight build of the existing Rust immediate-mode Matrix client. IPAD-06 through IPAD-10 are active. HOST-05's literal Mac sleep and wake test and V0-53's logout and login test remain deferred and open. Cloud relay, external testing, App Review and public release remain out of scope.

## Verified state

HOST-01 through HOST-04 and IPAD-01 through IPAD-05 are checked with evidence in `docs/verification/`. The iPad Simulator has passed the gallery, live two-host pairing, chat, typed Matrix interactions, inferred UI and browser approval journeys. The Mac host uses local Ollama only. The browser uses a pinned Chrome for Testing profile separate from the owner's default profile. Approval remains host-bound and inert until a valid exact decision.

IPAD-09 is checked and pushed as `03434e4`. `cargo check -p mote-ios --target aarch64-apple-ios` and an unsigned Xcode `iphoneos` arm64 build passed. The native Simulator gallery test and the live paired-host regression passed after the Xcode change. The temporary Simulator used for the live test was deleted. The existing Simulator was left intact. Evidence is in `docs/verification/IPAD-09.md`. The physical build is unsigned and retains `local.mote.simulator`. It is not a TestFlight build.

The prior planning commit `6ead290` and the IPAD-09 commit were pushed to `origin/main` at `https://github.com/beelol/mote.git`. The latest full workspace test and strict clippy gates passed before `03434e4`.

## Work in progress

The IPAD-10 packaging checkpoint is uncommitted. `ios/Mote/distribute.py` now has read-only preflight, explicit signed archive and IPA export, and separate opt-in upload commands. It uses a supplied current app inventory and requires a matching distinct App Store Connect record before archive. It checks local signing, duplicate builds, output paths, archive and IPA identity, signatures and entitlements. `ios/Mote/DISTRIBUTION.md` explains the inputs. `ios/Mote/test_distribute.py` has eight passing fail-closed tests. Xcode and plist syntax passed. A rebuilt Simulator app kept bundle ID `local.mote.simulator`, version `0.0.0` and build `1`. Partial evidence is in `docs/verification/IPAD-10.md`. The criterion is not checked because no signed archive, export or upload exists yet.

The script review, eight Python tests, full workspace test, strict clippy, plist lint, and unsigned physical-device rebuild passed. Commit only scoped files, then push `main`. Do not stage unrelated `.DS_Store` or `.superpowers/`.

## External dependencies

IPAD-06 requires a physical iPad and its real local-network privacy prompt, touch and accessibility checks. IPAD-07 requires the confirmed display name, unique bundle ID, SKU, team, signing and a separate App Store Connect record. IPAD-08 requires a processed internal build installed and exercised on the physical iPad. Do not invent Apple identity values or reuse another app. Do not run a signed archive or upload before the physical-device checks and App Store Connect record are confirmed.

TestFlight distributes the client binary. It does not provide an Internet route to the local Mac host. A test from another city needs a separately verified route. The existing localhost gateway and Bonjour discovery do not provide that route.

Preserve user data and do not check in pairing offers, credentials, persisted client state, raw browser captures, host databases or Xcode derived data. All model calls must use the ledger and all stored events must pass redaction.
