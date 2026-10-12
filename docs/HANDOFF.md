# Current development handoff

Updated 2026-10-12 in America/Los_Angeles. This file is a replacement snapshot.

## Contract and scope

Read `AGENTS.md`, `docs/rfcs/v0.md`, the root README Ready for Go board and `docs/superpowers/specs/2026-10-11-ipad-host.md` first. The owner changed the delivery order to internal TestFlight availability before physical iPad testing. IPAD-07 and IPAD-10 are verified. IPAD-11 is open for export compliance and internal availability. IPAD-06 and IPAD-08 still require physical installation and full device journeys. HOST-05 and V0-53 remain open for later owner-present checks. External testing, App Review, public release and cloud relay remain out of scope.

## Verified state

IPAD-01 through IPAD-05, IPAD-07, IPAD-09 and IPAD-10 are checked with evidence in `docs/verification/`. The iPad Simulator passed the gallery, live paired-host chat, typed Matrix interactions, inferred UI and browser approval journeys. The final native Simulator run after the UIKit accessibility bridge and iPad icon passed the gallery and control checks, the paired-host journey, inferred UI and browser approval XCUITests. Evidence is in `docs/verification/IPAD-06.md`. This does not verify physical touch or VoiceOver gestures.

The Apple team is STATION 42 INTERACTIVE L.L.C., ID `FQ6YGD7554`. Existing Overseer Remote and One More Fish app identities were not altered. The owner selected `Mote Agent` after Apple rejected the globally taken name `Mote`. The separate iOS App Store Connect record is https://appstoreconnect.apple.com/apps/6822101041/distribution/info. It has bundle ID `com.beelol.mote`, SKU `mote-ipad-001`, Apple ID `6822101041` and English (U.S.). The native app still displays `Mote`. The TestFlight page showed No Builds before upload. Evidence is in `docs/verification/IPAD-07.md`.

The guarded `ios/Mote/distribute.py` has read-only preflight, signed archive and IPA export, receipt binding, and a separate opt-in upload. The real archive and internal-only IPA for `0.1.0 (1)` were verified locally. Upload using the existing signed-in Xcode account exited 0. App Store Connect showed the upload processing, then Build 1, identifier `e05b53e3-adf2-4c60-baa1-5f36034e8d69`, with status `Missing Compliance`. The script's single-attempt marker and release receipt remain outside Git under `target/distribution/0.1.0-1c`. Do not retry this upload. The verification record is `docs/verification/IPAD-10.md`.

An internal TestFlight group named `Internal Testing` was created with automatic distribution of Xcode builds. The only tester is the owner's App Store Connect account, `bilalitani1@gmail.com`. The group page is https://appstoreconnect.apple.com/teams/c419168b-6bd1-4b72-bbed-78617b507726/apps/6822101041/testflight/groups/7d909949-bf8f-4f94-b475-261afc2e3b2f. It still showed 0 Builds and No Builds Available. There is no external group, and the App Review page has no submission. The public iOS version remains Prepare for Submission. Evidence is in `docs/verification/IPAD-11.md`.

Mote's iOS Rust crate depends on `mote-gateway`, whose Noise transport specifies standard Curve25519, ChaChaPoly and SHA-256 algorithms outside Apple's OS. The Apple compliance form's standard-encryption answer led to a question about distribution in France. Automated safety review refused choosing No because it would make a consequential export-compliance declaration without an established owner decision. The form was canceled without saving. Do not submit or work around that declaration. The owner must determine the France answer and any required export documentation. See Apple's https://developer.apple.com/help/app-store-connect/manage-app-information/overview-of-export-compliance.

The configured Git origin is `https://github.com/beelol/mote.git`. The latest pushed commit before this evidence update is `12ae7ab`. Push each verified commit to `main`. Preserve unrelated `.DS_Store` and `.superpowers/` files. The sixteen Python distribution tests, `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` passed on 2026-10-12.

## Next actions

Commit and push the processing and compliance evidence. Once the owner makes the export-compliance decision, complete the Apple form without changing the encryption answer away from the inspected standard algorithms. Check whether Apple requires documentation. Confirm Build 1 becomes available to the internal group and account holder. Then close IPAD-11 with its full Verify evidence. Do not create an external group, submit to App Review or publish the app. IPAD-06 and IPAD-08 remain open until physical iPad testing. TestFlight distributes a binary, not an Internet route to the local Mac host. A test from another city still needs a separately verified route.
