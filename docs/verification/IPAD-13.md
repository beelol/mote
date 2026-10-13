# IPAD-13: Agent layout transition

Date: 2026-10-13, America/Los_Angeles.

Environment: Apple Silicon Mac, Rust 1.89.0, egui 0.31.0. The focused check uses the production Matrix renderer with a headless egui context. It is not an iPad Simulator capture.

The live iPad surface used the surface revision in its UI identity. A replacement revision therefore started a new transition cache and could not compare its strokes with the prior revision. The renderer now accepts a stable transition identity while keeping revision-specific control identity. The live view keys that transition by host and thread.

The renderer captures header and footer rules and slider rails as line strokes. A focused regression draws two revisions with different UI identities and one shared transition identity. It changes the header height, which moves the header rule and the slider rail.

```text
cargo test -p mote-client-core live_surface_revision_materializes_rules_with_new_control_identity
test transition::tests::live_surface_revision_materializes_rules_with_new_control_identity ... ok
test result: ok. 1 passed; 0 failed
```

This proves square particles appear on the changed revision, remain at 60 ms and finish at 120 ms in the production renderer. Existing transition tests cover unchanged strokes, first-frame hit targets and reduced-motion behavior. The live paired Simulator capture and first-frame control journey in the Verify clause remain open. Do not mark IPAD-13 complete from this headless evidence.

The full workspace checks passed after this change:

```text
cargo test --workspace
exit 0
cargo clippy --workspace -- -D warnings
exit 0
```

The first sandboxed workspace test run failed because a browser fixture could not bind a localhost listener. The same command passed with local socket access outside the restricted sandbox. This was an environment permission failure, not a test assertion failure.

The native iPad Pro 11-inch (M5), iOS 27.0 Simulator journey rebuilt and installed the current Rust and Xcode client, then paired it with a scripted local host. It passed generated UI success, one diagnostic retry, preservation of the prior surface on invalid output, a visible terminal diagnostic and prose fallback. The test reported 14 model calls, 14 ledger rows, 14 packs and three authenticated typed refusals.

```text
MOTE_LIVE=1 TMPDIR=/private/tmp \
MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E \
MOTE_ARTIFACTS=/private/tmp/mote-ipad13-native-20261013 \
MOTE_SURFACE_ATTESTATION=/private/tmp/mote-surface-attested.8F07uH/surface-activation.json \
cargo test -p mote-ios --test inferred_surface_simulator \
native_inferred_success_retry_preservation_and_prose_fallback -- --ignored --nocapture
test native_inferred_success_retry_preservation_and_prose_fallback ... ok
test result: ok. 1 passed; 0 failed
```

The local build and XCUITest logs and result bundle are in `/private/tmp/mote-ipad13-native-20261013`. This journey verifies native integration after the transition fix but does not capture the 0, 60 and 120 ms transition frames or immediate interaction on the changed layout.
