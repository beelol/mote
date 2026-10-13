# Current development handoff

Updated 2026-10-12 in America/Los_Angeles. This file is a replacement snapshot.

## Contract and scope

Read `AGENTS.md`, `docs/rfcs/v0.md`, the root README Ready for Go board and `docs/superpowers/specs/2026-10-11-ipad-host.md`. The owner moved internal TestFlight ahead of physical iPad checks and added a manual GitHub Actions release path. IPAD-11 is verified. IPAD-12 is open until a protected Mote release environment and credentials are configured and its hosted build and upload runs succeed. IPAD-06 and IPAD-08 still require physical iPad installation and journeys. HOST-05 and V0-53 remain for owner-present checks. External testing, App Review, public release and cloud relay remain out of scope.

## Verified state

IPAD-01 through IPAD-05, IPAD-07, IPAD-09, IPAD-10 and IPAD-11 are checked with evidence in `docs/verification/`. The iPad Simulator passed the gallery, paired-host chat, typed Matrix interactions, inferred UI and browser approval journeys. The final native Simulator run passed the gallery and control checks, paired-host journey, inferred UI and browser approval XCUITests. `docs/verification/IPAD-06.md` records those results. Simulator evidence does not verify physical touch or VoiceOver gestures.

The distinct App Store Connect app is Mote Agent, Apple ID `6822101041`, bundle `com.beelol.mote`, SKU `mote-ipad-001`, team `FQ6YGD7554`. Its native display name is Mote. Build `0.1.0 (1)`, ID `e05b53e3-adf2-4c60-baa1-5f36034e8d69`, was uploaded from the locally verified signed archive. It is listed as `Testing` in the `Internal Testing` group with one account-holder tester invited. There is no external tester group or App Review submission. The public version remains `Prepare for Submission`. The group is https://appstoreconnect.apple.com/teams/c419168b-6bd1-4b72-bbed-78617b507726/apps/6822101041/testflight/groups/7d909949-bf8f-4f94-b475-261afc2e3b2f/builds. The upload attempt marker remains under `target/distribution/0.1.0-1c`. Do not retry that upload.

The app's Noise transport uses standard Curve25519, ChaChaPoly and SHA-256 outside Apple's OS. The owner said they added France. The export-compliance form was not observed being submitted in the agent session, but the build subsequently reached `Ready to Test` and internal `Testing`. Do not claim that a public App Store territory setting was changed. See `docs/verification/IPAD-11.md`.

The manual-only `.github/workflows/mote-testflight.yml` workflow is implemented. Its build-only job requires `main`, checks Rust and builds an unsigned iPad client without Apple credentials. Its optional upload job requires explicit UI verification, a Mote-only provisioning profile, live App Store Connect inventory, a fresh build number and signed IPA verification. It uses the existing guarded `ios/Mote/distribute.py` and new `ci_inventory.py` and `ci_profile.py`. It has not run on GitHub. The environment `mote-testflight` and its dedicated Mote release secrets still need configuration before an upload run. Apple team API keys and distribution certificates may be team-wide even when stored under Mote-only secret names. Do not reuse Overseer or AquaFriends profiles or keys. See `ios/Mote/DISTRIBUTION.md` and `docs/verification/IPAD-12.md`.

The configured origin is `https://github.com/beelol/mote.git`. Push each verified commit to `main`. Preserve unrelated `.DS_Store` and `.superpowers/` files. Run the Python release tests, `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before committing.

## Next actions

Complete the protected Mote GitHub environment and release credentials, then run one build-only dispatch from `main` and one deliberate upload dispatch with a fresh Mote build number. Confirm only the Mote app and internal group changed. Do not enable automatic triggers. Physical iPad IPAD-06 and IPAD-08 remain open until hardware is available. TestFlight distribution does not make the local Mac reachable from another city; that route requires separate verification.
