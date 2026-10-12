# Current development handoff

Updated 2026-10-12 in America/Los_Angeles. This file is a replacement snapshot.

## Contract and scope

Read `AGENTS.md`, `docs/rfcs/v0.md`, the root README Ready for Go board and `docs/superpowers/specs/2026-10-11-ipad-host.md` first. The owner changed the delivery order to internal TestFlight availability before physical iPad testing. IPAD-07 is verified. IPAD-10 still needs a deliberate upload and Apple processing outcome. IPAD-11 needs a processed build assigned to an eligible internal group. IPAD-06 and IPAD-08 still require physical installation and the full device journeys later. HOST-05 and V0-53 remain open for later owner-present checks. External testing, App Review, public release and cloud relay remain out of scope.

## Verified state

IPAD-01 through IPAD-05, IPAD-07 and IPAD-09 are checked with evidence in `docs/verification/`. The iPad Simulator passed the gallery, live paired-host chat, typed Matrix interactions, inferred UI and browser approval journeys. The final native Simulator run after the UIKit accessibility bridge and iPad icon passed the gallery and control checks, the paired-host journey, inferred UI and browser approval XCUITests. Evidence is in `docs/verification/IPAD-06.md`. This does not verify physical touch or VoiceOver gestures.

The Apple team is STATION 42 INTERACTIVE L.L.C., ID `FQ6YGD7554`. Existing Overseer Remote and One More Fish app identities were not altered. The owner selected `Mote Agent` after Apple rejected the globally taken name `Mote`. The separate iOS App Store Connect record is https://appstoreconnect.apple.com/apps/6822101041/distribution/info. It has bundle ID `com.beelol.mote`, SKU `mote-ipad-001`, Apple ID `6822101041` and English (U.S.). The native app still displays `Mote`. The TestFlight page showed No Builds before upload. Evidence is in `docs/verification/IPAD-07.md`.

The guarded `ios/Mote/distribute.py` has read-only preflight, signed archive and IPA export, receipt binding, and a separate opt-in upload. `altool` failed in Xcode 27, so the script uses Xcode archive upload. The script supports the existing signed-in Xcode account without creating a persistent API key. Xcode's automatic archive signing on this Mac produces development entitlements, then the internal-only App Store Connect export re-signs the IPA with `get-task-allow = false` and `beta-reports-active = true`. The script verifies the archive identity and the exported IPA distribution entitlements separately. Xcode 27's typed `codesign -d --entitlements -` output is parsed fail-closed. The real guarded archive/export into `target/distribution/0.1.0-1c` exited 0 on 2026-10-12 and produced a verified 5.2 MiB IPA for version `0.1.0 (1)`. It did not upload. The receipt is outside Git. Failed local attempts `0.1.0-1` and `0.1.0-1b` were not reused. See `docs/verification/IPAD-10.md`.

The configured Git origin is `https://github.com/beelol/mote.git`. The preceding checkpoint is `242954e`. Push each verified commit to `main`. Preserve unrelated `.DS_Store` and `.superpowers/` files. The signing fix, distribution guide and evidence are ready to commit. The sixteen Python distribution tests, `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` all passed on 2026-10-12.

## Next actions

Commit and push the verified signing fix and evidence. Recheck the Mote Agent TestFlight page for any existing build before upload. Use the guarded `upload` action with `--confirm-inventory-current --confirm-native-ui-verified --confirm-upload --use-xcode-account` and the manifest and inventory under `/private/tmp/mote-preflight-2026-10-12-*.json` against `target/distribution/0.1.0-1c`. The release receipt binds this exact archive and IPA. Only one upload attempt is allowed from this directory. If the command fails or its result is ambiguous, inspect App Store Connect before trying another build. Do not delete the attempt marker to retry blindly.

After Apple processes the build, record its exact state and add it only to an eligible internal TestFlight group. Leave external testing and App Review disabled. Record commands, outputs, environment and date in `docs/verification/IPAD-10.md` and `docs/verification/IPAD-11.md`. Mark these criteria passed only when their full Verify clauses are met. IPAD-06 and IPAD-08 remain open without physical hardware. TestFlight distributes a binary, not an Internet route to the local Mac host. A test from another city still needs a separately verified route.
