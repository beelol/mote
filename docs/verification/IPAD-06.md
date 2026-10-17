# IPAD-06 verification in progress

Date: 2026-10-11, America/Los_Angeles.

Environment: macOS 26.6.2, Xcode 27.0, iPhoneOS SDK 27.0, iOS 27.0 (24A434) Simulator on a temporary iPad Pro 11-inch (M5), Rust 1.89.0. The Simulator identifier was `30C51BEC-10D6-4AF9-8A74-A74CB2003771`.

The native iOS client now mirrors Matrix control records into individual UIKit accessibility elements. Each frame updates their role, label, value, enabled state and clipped frame. Matching ID and role reuse the same UIKit element to preserve focus. Old elements refuse actions. Activation and adjustable requests return to egui through its AccessKit action path. This is implementation evidence, not a physical VoiceOver pass.

The same bridge now records the live host picker, connection and chat text, composer, Send button and browser approval cards. Approval controls retain their online, in-flight and expiry checks when activated through the native bridge. The existing live and browser XCUITests contain native-element assertions. Their later rerun is recorded below.

Command: `cargo check --locked -p mote-ios --target aarch64-apple-ios-sim` on the live and approval extension. Focused host and approval AccessKit tests were also run.

Output: the iOS-target check passed. `native_click_is_consumed_once_by_the_matching_egui_control` and `accesskit_approval_action_targets_only_the_enabled_bound_card` passed with zero failures.

Command: `cargo test --workspace` with local fixture socket access, followed by `cargo clippy --workspace -- -D warnings` after the live accessibility extension.

Output: both exited 0. The workspace test run included the new native action and approval containment tests. The warning-free lint check included `mote-ios` and `mote-client-core`.

Command: `xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'generic/platform=iOS' -derivedDataPath target/ipad-device-accessibility CODE_SIGNING_ALLOWED=NO build` after the live accessibility extension.

Output: exited 0. The final unsigned app was arm64 with bundle ID `local.mote.simulator`, compiled `AppIcon` metadata and `Assets.car`. It was not signed, exported or uploaded.

Command: `cargo check --locked -p mote-ios --target aarch64-apple-ios-sim`.

Output: exited 0 on the updated client.

Command: `xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'generic/platform=iOS' -derivedDataPath target/ipad-device-accessibility CODE_SIGNING_ALLOWED=NO build`.

Output: exited 0. The unsigned `iphoneos` app is an arm64 Mach-O. Its Info.plist includes the compiled iPad icon. The Simulator bundle ID remains `local.mote.simulator`. This is not a signed distribution archive.

Command: `xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=30C51BEC-10D6-4AF9-8A74-A74CB2003771' -derivedDataPath target/ipad-accessibility-derived -resultBundlePath target/ipad-accessibility-fixed.xcresult -collect-test-diagnostics never -only-testing:MoteUITests/MoteUITests/testMatrixControlsAppearAsIndividualNativeAccessibilityElements test`.

Output: exited 0. The result bundle reported one passed test and zero failures. The native hierarchy exposed separate gallery navigation buttons, a toggle with On and Off values, a custom adjustable `Volume` element whose value changed from 4 to 8, and a select whose value changed from Normal to High. Navigation removed the old toggle element. XCUITest classifies the custom adjustable UIAccessibilityElement as `Other`, not as `Slider`; the UIKit element carries the adjustable trait. An initial test that queried `app.sliders` failed for that type mismatch and was replaced by the observed native type. The first failed run's test execution completed, but Xcode hung collecting diagnostics and its process was stopped. A diagnostic rerun confirmed the native tree and the final corrected run passed.

Command: `xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=30C51BEC-10D6-4AF9-8A74-A74CB2003771' -derivedDataPath target/ipad-accessibility-derived -resultBundlePath target/ipad-accessibility-gallery.xcresult -collect-test-diagnostics never -only-testing:MoteUITests/MoteUITests/testGalleryPagesAndControlsInBothOrientations test`.

Output: exited 0. The result bundle reported one passed test and zero failures. The existing native gallery regression visited its pages and controls in both orientations on the accessibility-enabled build.

Command: `xcrun devicectl list devices`.

Output: no physical iPad was connected. The only physical device listed was an unavailable iPhone Air. All listed iPads were simulated. Physical touch, keyboard, rotation, split view, background recovery, local-network permission, light and dark appearance, reduced motion and actual VoiceOver gestures remain unverified. IPAD-06 stays unchecked.

## Final native Simulator regression after the accessibility extension

Date: 2026-10-12, America/Los_Angeles. Environment: macOS 26.6.2, Xcode 27.0, iOS 27.0 (24A434) on a temporary iPad Pro 11-inch (M5) Simulator. The temporary device ID was `79374702-1858-4AB2-BD79-C7C06DADC0AB`. The current native app was rebuilt from `main` at `0131b9f`. These are Simulator checks, not physical iPad evidence.

Command: `xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=79374702-1858-4AB2-BD79-C7C06DADC0AB' -derivedDataPath /private/tmp/mote-native-regression-20261012-derived build`, followed by `xcrun simctl install` on that temporary device.

Output: both exited 0. The freshly built native app installed on the isolated Simulator.

Command: `xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=79374702-1858-4AB2-BD79-C7C06DADC0AB' -derivedDataPath /private/tmp/mote-native-regression-20261012-derived -resultBundlePath /private/tmp/mote-native-regression-20261012-gallery.xcresult -collect-test-diagnostics never -only-testing:MoteUITests/MoteUITests/testMatrixControlsAppearAsIndividualNativeAccessibilityElements -only-testing:MoteUITests/MoteUITests/testGalleryPagesAndControlsInBothOrientations test`.

Output: exited 0. `xcresulttool` reported two passed tests, zero failed and zero skipped. The tests checked separate native toggle, adjustable slider and select elements, gallery page changes, controls and both orientations.

Command: `MOTE_LIVE=1 MOTE_SIMULATOR_ID=79374702-1858-4AB2-BD79-C7C06DADC0AB MOTE_ARTIFACTS=/private/tmp/mote-native-regression-20261012-live cargo test -p mote-ios --test simulator_live -- --ignored --nocapture`.

Output: exited 0. The native XCUITest result bundle reported one pass and zero failures. The fixture paired two hosts, used the native host picker and chat composer, changed the talk-model select, resumed cursor without duplicate chat, moved one authenticated host endpoint and rejected an impostor. It restored and deleted the temporary native Keychain credentials.

Command: `MOTE_LIVE=1 TMPDIR=/private/tmp MOTE_SIMULATOR_ID=79374702-1858-4AB2-BD79-C7C06DADC0AB MOTE_ARTIFACTS=/private/tmp/mote-native-regression-20261012-inferred MOTE_SURFACE_ATTESTATION=/private/tmp/mote-surface-attested.8F07uH/surface-activation.json cargo test -p mote-ios --test inferred_surface_simulator native_inferred -- --ignored --nocapture`.

Output: exited 0. The native XCUITest result bundle reported one pass and zero failures. Success, one retry, prior-surface preservation, visible terminal diagnostic and prose fallback passed. The fixture reported 14 model calls, 14 ledger rows, 14 packs and three authenticated typed refusals. The attestation and scripted model fixture were isolated under `/private/tmp`.

Command: `MOTE_LIVE=1 MOTE_CHROME='/private/tmp/mote-browser-runtime-155/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing' MOTE_SIMULATOR_ID=79374702-1858-4AB2-BD79-C7C06DADC0AB MOTE_ARTIFACTS=/private/tmp/mote-native-regression-20261012-browser cargo test -p mote-ios --test browser_approval native_paired -- --ignored --nocapture`.

Output: exited 0. The native XCUITest result bundle reported one pass and zero failures. Deny caused zero effects. Repeated Approve taps caused exactly one effect. Natural expiry caused no additional effect. The same validated Matrix result remained visible. The browser used the pinned Chrome for Testing and a local fixture, not the user's regular profile or a remote site.

Each result bundle summary was read with `xcrun xcresulttool get test-results summary --path <bundle> --format json`. The temporary Simulator was shut down and deleted after the tests. No physical iPad was connected or tested. IPAD-06 remains unchecked.

Command: `cargo test --workspace` with local fixture socket access and `cargo clippy --workspace -- -D warnings` after recording this evidence.

Output: both exited 0. The live native tests above were run separately because the default workspace suite correctly leaves them ignored without their explicit fixture environment.

## Physical gallery and Wi-Fi endpoint verification

Date: 2026-10-17, America/Los_Angeles.

Environment: iPad Pro 11-inch (M4), UDID `00008132-001104661AB9001C`, iPadOS
26.5, Xcode 27.0. The app, UI-test plug-in and XCTest runner used only the existing
Mote bundle identities and Mote development profiles on team `FQ6YGD7554`.

The paired app initially retained the direct-link endpoint `169.254.9.119:47810`.
The Mac daemon advertised the matching host identity `040bf4649b7e678c06ea460e` through
Bonjour on Wi-Fi. After a normal app launch, the saved endpoint became
`192.168.50.184:47810`. The full host identity and cursor sequence 9 remained unchanged.
This verifies authenticated endpoint migration to Wi-Fi without repairing. It does not replace
the remaining interruption and recovery journey.

The first physical gallery run exposed a test-harness defect. Vision recognized the visible
`Open item 20` label as adjacent fragments. The assertion saw the joined text, but the tap locator
required one observation to contain the entire label. A focused regression now requires adjacent
fragments on the same visual row to produce the smallest combined target box. The red build failed
because `textTargetBox` did not exist. The focused Simulator test passed after the implementation.

Commands:

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug -sdk iphonesimulator \
  -derivedDataPath /private/tmp/mote-locator-green \
  CODE_SIGNING_ALLOWED=NO build-for-testing

xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug \
  -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' \
  -derivedDataPath /private/tmp/mote-locator-green \
  -resultBundlePath /private/tmp/mote-locator-green.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testTextTargetCombinesAdjacentFragmentsInOneRow test
```

Output: both exited 0. The focused result bundle reported one passed test and zero failures.

The updated test binary was placed into a copy of the previously verified Mote-only signed test
bundle. The test plug-in and runner host were re-signed with the Apple Development identity for
team `FQ6YGD7554`. Their preserved bundle identities were `com.beelol.mote.xctrunner` and
`com.beelol.mote.xctrunner.xctrunner`. Deep signature verification passed.

Command:

```text
xcodebuild test-without-building \
  -xctestrun /private/tmp/mote-ipad06-fixed-signed/Build/Products/Mote_iphoneos27.0-arm64.xctestrun \
  -destination 'platform=iOS,id=00008132-001104661AB9001C' \
  -resultBundlePath /private/tmp/mote-ipad06-gallery-fixed2.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testTextTargetCombinesAdjacentFragmentsInOneRow \
  -only-testing:MoteUITests/MoteUITests/testGalleryPagesAndControlsInBothOrientations
```

Output: `TEST EXECUTE SUCCEEDED`. The physical gallery test passed in 267.002 seconds. The
locator regression passed in 0.039 seconds. The run visited all eight gallery pages, exercised the
existing Layout, Toggle, Slider, Select, List, Form, Dashboard and Fixture journeys, scrolled to
and activated item 20, entered text through the system keyboard, repeated the journey in portrait
and landscape, and retained screenshots in the result bundle.

IPAD-06 remains open. This run does not yet satisfy the literal requirement for center and edge
taps on every control. Split view, paired background recovery, the physical local-network privacy
prompt, system light and dark appearance, the system Reduce Motion setting and actual VoiceOver
navigation also remain to be recorded.
