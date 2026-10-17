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

## Exhaustive physical touch journey prepared

Date: 2026-10-17, America/Los_Angeles. Environment: macOS 26.6.2, Xcode 27.1, SDK 27.1. Helper execution used the iPad Pro 11-inch (M5) Simulator on iOS 27.0, device `43F2F6ED-5C86-4121-B03F-40FE38891C4E`. The physical iPad was disconnected. No physical execution or physical pass is claimed in this section.

`testPhysicalGalleryEveryControlAcceptsCenterAndFourEdges` now implements the literal exhaustive control journey. It uses native accessibility hit-target frames for center taps and all four edge midpoints inset by two points. It rejects invalid, clipped or undersized frames. A bounded scroll search brings reachable list controls into view. Vision is used for the existing status-row assertions, not to infer touch geometry.

The shared inventory covers all eight page navigation buttons and 37 page controls. Those controls comprise 24 actions, four toggles, three sliders and six selects. The List page includes every action from Open draft through Open item 20. The select journeys cover all 17 option instances across Priority, model and Mode controls. The same journey runs in portrait and landscape. It requests a one-hour execution allowance for this exhaustive on-device test.

Each tested tap requires a fresh effect. Actions first replace the previous action status with the page selection status. Toggles must change their native On or Off value. Sliders are first set to a different endpoint. Selects are first set to a different option when needed. Every option is chosen at all five positions. Test logs record labels, positions, hit frames and tap points. The physical journey retains a native role tree after each control and a screenshot after each page. All changes are confined to the offline gallery preview.

The global Preview note field is included in the exhaustive physical journey. Each of its five positions starts with a fresh offline launch and no visible keyboard. The native field frame supplies the tap point. Each tap must open the system keyboard, expose the native editing field and accept a distinct marker with the exact expected native value. This repeats in each orientation. These five-position checks are implemented but physically unrun. The separate [IPAD-14](IPAD-14.md) record covers additional native editing behavior. It does not supply the missing five-position physical evidence.

The physical paired-test Simulator skip guards remain. The exhaustive gallery test also skips before launching an app on Simulator. The paired background test is strengthened below. Its former acceptance of a merely background-running app is removed.

Two deterministic helper tests were first observed failing. The inventory stub returned zero controls instead of the five Dashboard controls. The geometry stub returned the center for an edge position. After implementation, the inventory and geometry tests passed. A native helper smoke test then exposed an off-screen lookup error because XCTest was asked for item 20's frame before it existed. The helper now checks existence before reading its frame. The final native smoke test passed through all four control kinds, dropdown options and a scrolled item 20 action.

Simulator command:

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug \
  -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' \
  -derivedDataPath /private/tmp/mote-ipad06-exhaustive-sim \
  -resultBundlePath /private/tmp/mote-ipad06-exhaustive-green3.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testGalleryTapPointsCoverCenterAndFourInsetEdges \
  -only-testing:MoteUITests/MoteUITests/testGalleryControlInventoryIncludesEveryPageAndListAction \
  -only-testing:MoteUITests/MoteUITests/testGalleryNativeHelpersExerciseEachControlKindAndScrolledAction \
  -only-testing:MoteUITests/MoteUITests/testPhysicalGalleryEveryControlAcceptsCenterAndFourEdges \
  -only-testing:MoteUITests/MoteUITests/testPhysicalPairedHostRecoversAfterBackgrounding test
```

Output: exited 0. `xcresulttool` reported three passed tests, zero failed and two skipped. The skips were the physical gallery journey and paired background-recovery journey. These are Simulator helper results only.

Both final test bundles were compiled without device installation or execution:

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug -destination 'generic/platform=iOS' \
  -derivedDataPath /private/tmp/mote-ipad06-exhaustive-device \
  CODE_SIGNING_ALLOWED=NO build-for-testing

xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath /private/tmp/mote-ipad06-exhaustive-sim \
  CODE_SIGNING_ALLOWED=NO build-for-testing
```

Output: both exited 0. The manifests are `Mote_iphoneos27.1-arm64.xctestrun` and `Mote_iphonesimulator27.1-arm64.xctestrun` under their respective `Build/Products` directories. Existing linker warnings report that the deployment target is iOS 16 while the XCTest libraries were built for iOS 17. The final source compilation produced no new source warnings.

Remaining physical action: reconnect and unlock the M4 iPad, sign the current app and test runner with the existing Mote-only development profiles, then run `MoteUITests/MoteUITests/testPhysicalGalleryEveryControlAcceptsCenterAndFourEdges` on device `00008132-001104661AB9001C`. Retain its result bundle, tap logs, observed native inventories, per-control role trees and page screenshots. The unsigned device bundle above cannot supply physical execution evidence. Run the strengthened paired background-recovery test separately with the live host and driver coordination described below. Local-network permission, split view, system appearance, system Reduce Motion and actual VoiceOver evidence remain separate open work. IPAD-06 stays unchecked.

### Review corrections prepared, physical execution pending

Before exercising a page, the physical journey now independently enumerates native actionable elements while scrolling in both directions to stable content boundaries. It collects observed labels and XCTest platform roles without using the declared inventory to locate controls or stop scrolling. Exact comparison rejects missing controls, unexpected controls and role changes. It rejects duplicate labels within a native snapshot. The observed set includes all 20 reachable list actions. Navigation buttons and the global Preview note field are declared separately from page controls. Each select is opened separately to compare its observed native option buttons with its declared options. UIKit exposes buttons, toggles and selects as Button and adjustable sliders as Other. The inactive text-field accessibility mirror is also Other. The inventory comparison uses those actual platform roles, not invented native Toggle or Select types. Each page and select retain observed inventory attachments.

The paired background test now requires `RunningBackgroundSuspended` within 60 seconds. RunningBackground is not accepted. After confirmed suspension, the test logs `BACKGROUND COORDINATION` and leaves Mote backgrounded for 30 seconds. The driver must run `moted host stop` for the paired host during that window. On activation, the test requires the exact paired host label to reach offline within 30 seconds and retains a screenshot. Connecting does not satisfy this assertion. Only after offline is observed does it log the second coordination marker. The driver must run `moted host start` for the same host. The test requires that exact host online within 180 seconds, the previous cursor and the previous visible content. It sends one unique user message, requires that native message exactly once and a higher event cursor, then relaunches to require persisted cursor replay and that same message exactly once. New host events may advance the cursor further. The replay cursor must not regress.

This path was later attempted physically and failed at the strict suspension boundary, as recorded below. Run the physical method without `-quiet` and watch the live XCTest output for both coordination markers. If iPadOS does not expose suspension within the strict timeout, this test fails and needs an explicit controlled-interruption alternative. There is no background-running fallback. The driver must retain stop/start command output alongside the result bundle. A run without the required host interruption cannot pass its offline assertion. Simulator skip results are not recovery evidence.

The inventory mismatch and suspension-state helpers were first observed failing in `/private/tmp/mote-ipad06-review-red.xcresult`. The first corrected Simulator run exited 0 with four passed tests, zero failures and two physical skips in `/private/tmp/mote-ipad06-review-green.xcresult`. That run covered the native page inventory, all five fresh text focus/editing positions and both negative helpers. The final combined rerun also includes the opened-option inventory and existing native control smoke journey:

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug \
  -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' \
  -derivedDataPath /private/tmp/mote-ipad06-exhaustive-sim \
  -resultBundlePath /private/tmp/mote-ipad06-review-final.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testGalleryTapPointsCoverCenterAndFourInsetEdges \
  -only-testing:MoteUITests/MoteUITests/testGalleryControlInventoryIncludesEveryPageAndListAction \
  -only-testing:MoteUITests/MoteUITests/testGalleryObservedInventoryRejectsMissingUnexpectedAndWrongRole \
  -only-testing:MoteUITests/MoteUITests/testGalleryRecoveryRejectsMerelyBackgroundRunning \
  -only-testing:MoteUITests/MoteUITests/testGalleryObservedInventoryCoversEveryPageIncludingAllListRows \
  -only-testing:MoteUITests/MoteUITests/testGalleryPreviewNoteFreshFocusAndEditingAtEveryPosition \
  -only-testing:MoteUITests/MoteUITests/testGalleryNativeHelpersExerciseEachControlKindAndScrolledAction \
  -only-testing:MoteUITests/MoteUITests/testPhysicalGalleryEveryControlAcceptsCenterAndFourEdges \
  -only-testing:MoteUITests/MoteUITests/testPhysicalPairedHostRecoversAfterBackgrounding test
```

Final review-correction output: exited 0. `/private/tmp/mote-ipad06-review-final.xcresult` reports seven passed tests, zero failed and two skipped. The native control smoke passed in 192.886 seconds. The independent eight-page and opened-option inventory passed in 119.832 seconds. The remaining passes cover tap geometry, declared inventory, mismatched observations, five-position text focus/editing and strict suspension-state classification. Both physical journeys skipped. These results are Simulator helper evidence, not physical acceptance evidence.

The new physical recovery message entry uses `mote.native-text-input`, matching the current live native text-field bridge. After that physical-only locator correction, the same Simulator command was repeated with only the two physical `-only-testing` selectors and result path `/private/tmp/mote-ipad06-review-final-guards.xcresult`. It exited 0 and reported two skips and zero failures. The physical recovery body is still unrun.

The two unsigned `build-for-testing` commands above were repeated against final source. Both exited 0. Build logs are `/private/tmp/mote-ipad06-review-device-final-build.log` and `/private/tmp/mote-ipad06-review-sim-final-build.log`. The device linker retains the existing iOS 16 versus XCTest iOS 17 warnings. The final Simulator build log is empty. No physical device was installed, activated or tested.

The final offline-boundary correction removes Connecting from the background test's acceptance condition. It waits for the exact paired host's offline native label before emitting the host-start marker. Repeating the same two-selector Simulator guard command with result path `/private/tmp/mote-ipad06-exact-offline-guards.xcresult` exited 0 with two skips and zero failures. Repeating both unsigned bundle build commands exited 0. Logs are `/private/tmp/mote-ipad06-exact-offline-device-build.log` and `/private/tmp/mote-ipad06-exact-offline-sim-build.log`. This correction remains physically unrun.

## System Reduce Motion bridge implementation

Date: 2026-10-17, America/Los_Angeles. Environment: macOS 26.6.2, Xcode 27.1, SDK 27.1. Native automated execution uses the iPad Pro 11-inch (M5) Simulator on iOS 27.0, device `43F2F6ED-5C86-4121-B03F-40FE38891C4E`. No physical system preference toggle or physical Reduce Motion pass is claimed.

The iOS adapter reads `UIAccessibilityIsReduceMotionEnabled()`. Every MoteApp draw applies the logical OR of that current system setting and the existing explicit reduced-motion override before selecting the first-run, live paired or offline gallery branch. The preference is not cached at launch. A native Reduce Motion notification requests an egui repaint. The app owns one observer token. Dropping its bridge clears the repaint context and removes that observer. Native observer installation is absent from non-iOS production builds. Synthetic Rust preference input exists only under `cfg(test)`. No Simulator preference override hook was added.

The focused Rust tests were observed failing before implementation. They rejected an enabled preference being read as false, missing notification repaint and the gallery draw ignoring the system preference. The native observer test also failed before implementation because the observer token was null. After implementation, this focused command passed four tests with zero failures:

```text
cargo test -p mote-ios --lib motion -- --nocapture
```

The tests cover repeated native state reads, conservative handling of unknown nonzero bridge values, explicit-override precedence, on/off changes through all three actual rendering branches and repaint requests only during the bridge lifetime. The native Simulator test compares the getter with UIKit's current preference and posts notifications over three observer lifetimes. It requires exactly one callback per notification and none after teardown. It does not toggle the system preference. A real system-setting toggle on the physical iPad remains required for IPAD-06.

Native command:

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug \
  -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' \
  -derivedDataPath /private/tmp/mote-ipad06-exhaustive-sim \
  -resultBundlePath /private/tmp/mote-motion-native-final.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testNativeMotionPreferenceAndObserverLifetime \
  -only-testing:MoteUITests/MoteUITests/testDeterministicFixtureAndReducedMotionLaunch test
```

Output: exited 0. The result bundle reports two passed tests, zero failed and zero skipped. The native adapter test also rejects null callback installation and safely ignores null observer removal. The second test launches the real app with the preserved explicit override and changes its gallery page. These are automated native bridge and app-launch checks, not physical preference-toggle evidence.

Both unsigned bundle builds exited 0:

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug -destination 'generic/platform=iOS' \
  -derivedDataPath /private/tmp/mote-ipad06-exhaustive-device \
  CODE_SIGNING_ALLOWED=NO build-for-testing

xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath /private/tmp/mote-ipad06-exhaustive-sim \
  CODE_SIGNING_ALLOWED=NO build-for-testing
```

Those initial logs are `/private/tmp/mote-motion-device-bundle.log` and `/private/tmp/mote-motion-sim-bundle.log`. They predate the unconditional-read correction and are superseded by the builds below. Existing XCTest deployment-target linker warnings remain. `cargo clippy -p mote-ios --all-targets -- -D warnings` also exited 0 before that correction.

Review added a read-count assertion for every draw, including when the explicit override is true. It first failed with zero native reads instead of one. The draw now reads the system value into a local before evaluating the OR. `cargo test -p mote-ios --lib motion -- --nocapture` then passed all four focused tests in `/private/tmp/mote-motion-read-count-green.log`.

Both post-correction native builds exited 0:

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug -destination 'generic/platform=iOS' \
  -derivedDataPath /private/tmp/mote-motion-final-device \
  CODE_SIGNING_ALLOWED=NO build-for-testing

xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath /private/tmp/mote-motion-final-sim \
  CODE_SIGNING_ALLOWED=NO build-for-testing
```

Logs: `/private/tmp/mote-motion-unconditional-final-device-build.log` and `/private/tmp/mote-motion-unconditional-final-sim-build.log`. These compile the unconditional system preference read. They do not supply physical preference-toggle evidence.

## Bounded physical gallery cases prepared

Date: 2026-10-17. The previous monolithic physical journey did not pass. Its M4 iPad run on iPadOS 26.5 is retained in `/private/tmp/mote-ipad06-exhaustive-physical2.xcresult`. The result reports zero passed tests and one failed test with `Test crashed with signal kill.` The driver observed the runner terminate after 349 seconds during List-page native inventory. No control assertion failure was reported before that termination. This is incomplete physical evidence, not an application acceptance pass.

The monolithic selector is replaced by 32 independently selectable physical cases. Each orientation has one case for Layout, Toggle, Slider, Select, Form and Fixture. List has five four-row batches. Dashboard has one base-control case and one case per model select. Preview note has a separate case per orientation. Every case requests a 240-second execution allowance. The case sizes and timeout are intended to stay below the previous runner budget. Their physical completion times remain unverified.

Every page case still independently scans the entire native page inventory, compares all labels and platform roles, inventories every opened select and tests all five navigation positions. It then exercises its assigned controls at the center and all four inset edges with the same fresh-effect assertions. It retains inventory attachments, control role trees, tap logs and a case screenshot. The separate Preview note cases retain all five fresh-launch focus/editing checks. Nothing is substituted for the literal control coverage.

`testGalleryPhysicalCasesPartitionEveryControlExactlyOnce` was first observed failing against an empty partition. It now proves every page control is assigned exactly once per orientation, no List batch exceeds four rows, no batch has more than one select, and the runtime XCTest selectors exactly match the expected 32 stable names. The focused Simulator run at `/private/tmp/mote-gallery-partition-final.xcresult` exited 0 with two passed tests, zero failures and all 32 physical cases skipped. The other pass is the native motion observer test. These skips are not physical coverage.

Run the physical suite one selector per invocation. Set `MOTE_GALLERY_XCTESTRUN` to a freshly signed manifest containing the bounded cases. Do not reuse the pre-split manifest. The following command requires `jq`. It preserves a separate result bundle, log and summary for every case, continues after a failed or killed case, and exits nonzero if any invocation fails. A case is reported passed only when its result contains exactly one passed test, no failures and no skips:

```sh
: "${MOTE_GALLERY_XCTESTRUN:?Set the freshly signed bounded-case xctestrun path}"
MOTE_GALLERY_DEVICE=00008132-001104661AB9001C
MOTE_GALLERY_ARTIFACTS=$(mktemp -d /private/tmp/mote-gallery-cases.XXXXXX) || exit 1
gallery_failed=0
for gallery_orientation in Portrait Landscape; do
  for gallery_case in Layout Toggle Slider Select \
    ListItems01Through04 ListItems05Through08 ListItems09Through12 \
    ListItems13Through16 ListItems17Through20 Form DashboardBase \
    DashboardTalkModel DashboardRouterModel DashboardThinkModel Fixture PreviewNote; do
    gallery_selector="testPhysicalGallery${gallery_case}${gallery_orientation}"
    if xcodebuild test-without-building \
      -xctestrun "$MOTE_GALLERY_XCTESTRUN" \
      -destination "platform=iOS,id=$MOTE_GALLERY_DEVICE" \
      -destination-timeout 45 -collect-test-diagnostics never \
      -only-testing:"MoteUITests/MoteUITests/$gallery_selector" \
      -resultBundlePath "$MOTE_GALLERY_ARTIFACTS/$gallery_selector.xcresult" \
      > "$MOTE_GALLERY_ARTIFACTS/$gallery_selector.log" 2>&1 && \
      xcrun xcresulttool get test-results summary \
        --path "$MOTE_GALLERY_ARTIFACTS/$gallery_selector.xcresult" \
        > "$MOTE_GALLERY_ARTIFACTS/$gallery_selector.summary.json" && \
      jq -e '.passedTests == 1 and .failedTests == 0 and .skippedTests == 0' \
        "$MOTE_GALLERY_ARTIFACTS/$gallery_selector.summary.json" > /dev/null; then
      printf '%s passed\n' "$gallery_selector"
    else
      printf '%s failed or interrupted\n' "$gallery_selector"
      gallery_failed=1
    fi
  done
done
printf 'Evidence: %s\n' "$MOTE_GALLERY_ARTIFACTS"
test "$gallery_failed" -eq 0
```

At this implementation checkpoint the 32 cases were physically unrun. Their later complete physical run is recorded below. The older monolithic commands above are historical and are not the current exhaustive-suite command. No README acceptance checkbox is changed.

The Simulator partition/guard command was:

```sh
gallery_selectors=()
for gallery_orientation in Portrait Landscape; do
  for gallery_case in Layout Toggle Slider Select \
    ListItems01Through04 ListItems05Through08 ListItems09Through12 \
    ListItems13Through16 ListItems17Through20 Form DashboardBase \
    DashboardTalkModel DashboardRouterModel DashboardThinkModel Fixture PreviewNote; do
    gallery_selectors+=("-only-testing:MoteUITests/MoteUITests/testPhysicalGallery${gallery_case}${gallery_orientation}")
  done
done
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug \
  -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' \
  -derivedDataPath /private/tmp/mote-ipad06-exhaustive-sim \
  -resultBundlePath /private/tmp/mote-gallery-partition-final.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testGalleryPhysicalCasesPartitionEveryControlExactlyOnce \
  -only-testing:MoteUITests/MoteUITests/testNativeMotionPreferenceAndObserverLifetime \
  "${gallery_selectors[@]}" test
```

Output: exited 0 with two passed tests, zero failures and 32 physical skips. Both unsigned native builds were repeated after the final bounded-case source change using the post-correction build commands above. Both exited 0. Logs are `/private/tmp/mote-gallery-partition-device-build.log` and `/private/tmp/mote-gallery-partition-sim-build.log`. The device manifest is `/private/tmp/mote-motion-final-device/Build/Products/Mote_iphoneos27.1-arm64.xctestrun`. The Simulator manifest is `/private/tmp/mote-motion-final-sim/Build/Products/Mote_iphonesimulator27.1-arm64.xctestrun`.

## Final automated source verification

Date: 2026-10-17. The fresh post-correction workspace run exited 0. Its 65 suite summaries total 748 passed tests, zero failed and 21 ignored. Ignored live and physical journeys do not supply acceptance evidence.

```text
cargo test --workspace -- --test-threads=4
cargo clippy --workspace -- -D warnings
cargo test -p mote-ios --lib motion -- --nocapture
cargo clippy -p mote-ios --all-targets -- -D warnings
cargo build --locked -p mote-ios --target aarch64-apple-ios --target aarch64-apple-ios-sim
cargo clippy -p mote-ios --target aarch64-apple-ios --target aarch64-apple-ios-sim -- -D warnings
```

All commands exited 0. The focused motion command passed four tests after the test-only read-count adapter's needless-return lint was fixed. The corresponding logs are `/private/tmp/mote-motion-final-workspace-tests.log`, `/private/tmp/mote-motion-workspace-clippy.log`, `/private/tmp/mote-motion-final-focused-tests.log`, `/private/tmp/mote-motion-final-host-clippy.log`, `/private/tmp/mote-motion-final-apple-build.log` and `/private/tmp/mote-motion-final-apple-clippy.log`. The documented shell blocks passed `bash -n` without execution. `git diff --check` passed. No physical system preference toggle or bounded gallery case pass is claimed.

## Physical system-setting selectors prepared

Date: 2026-10-17, America/Los_Angeles. Build environment: macOS 26.6.2, Xcode 27.1 and SDK 27.1. Focused helper execution uses the iPad Pro 11-inch (M5) Simulator on iOS 27.0, device `43F2F6ED-5C86-4121-B03F-40FE38891C4E`. Light was attempted physically and failed in the pre-launch harness check described below. The corrected four system-setting journeys have no physical pass yet. No setting is changed by XCTest.

`testPhysicalSystemAppearanceLight` and `testPhysicalSystemAppearanceDark` require the driver to select and independently query the corresponding physical system appearance first. The public `devicectl device info appearance` result is the authoritative system precondition. XCTest's appearance getter is retained only as explicitly non-authoritative diagnostic metadata. The app is launched with only `--mote-offline`. The test selects Layout and verifies its full native actionable inventory. It retains the checked window screenshot, native role tree and observed settings before pixel assertions, including on an assertion failure. Selector-specific exact paper and ink checks remain fail-closed for the wrong theme.

The physical RED result `/private/tmp/mote-system-appearance-light.xcresult` reports zero passed, one failed and zero skipped on iPad Pro 11-inch (M4), iPadOS 26.5. The failure at the former line 1115 was `XCUIDevice.appearance` equal to 0 rather than expected 1, before app launch. The independent `/private/tmp/mote-ipad-appearance-before.json` reports `info.outcome=success`, `result.userInterfaceStyle=light` and `result.reduceMotion.enabled=false`. This establishes an unreliable XCTest getter gate, not a product theme failure. The correction removes both getter equality gates, retains the real launch and palette assertions, and adds symmetric Light/Dark helper fixtures rejecting the opposite paper and ink. Physical rerun is still required.

Light requires the renderer's paper RGB `(244, 241, 234)` and ink RGB `(26, 26, 26)`. Dark requires paper RGB `(11, 11, 12)` and ink RGB `(232, 227, 218)`. Native hit-target geometry locates the Layout navigation button and Preview action. Each control has four interior background patches. At least 95 percent of each patch must match the expected paper within four RGB levels. At least 0.1 percent of each inset control interior must match the expected ink within eight RGB levels. The screenshot is converted to sRGB. Only each small crop is decoded. Invalid, empty or clipped regions fail closed. Transparent pixels are never counted as palette evidence. The log retains sampled pixel rectangles and match fractions. These are visible theme assertions, not an average-brightness guess or an appearance override.

`testPhysicalSystemReduceMotionEnabled` and `testPhysicalSystemReduceMotionDisabled` require the corresponding real `mote_ios_reduce_motion_enabled()` result and UIKit preference before launching Mote. They read the preference again while the app is in use and at the end. They launch with only `--mote-offline`, with no `--mote-reduced-motion`. They verify Layout inventory, replace it with Toggle, verify the replacement inventory and require a fresh native toggle-value change. Layout, replacement and post-interaction screenshots, role trees and preference observations are retained. Each physical setting selector requests a 180-second execution allowance. Simulator skips occur before constructing or launching Mote.

The native preference getter runs inside the XCTest runner process. Public XCTest does not expose Mote's internal transition setting. Per-app Accessibility overrides could make the app's preference differ from the runner's preference. The driver must leave Mote and the test runner on default per-app settings. The observation attachment explicitly marks the target-process motion mode as not exposed. Passing these selectors establishes the real runner preference, real no-override app launch, completed layout replacement and continued interaction. It does not by itself prove the physical renderer used the expected animation mode.

Animation-video observation remains required. Public XCTest tap and screenshot synchronization can outlast the 120 ms transition. The retained settled images are not first-frame or midpoint animation evidence. Record the physical replacement in each system preference mode. Confirm the enabled mode has no spatial particle travel and the disabled mode retains normal materialization. Retain the settings evidence, recording and observed result before treating physical Reduce Motion as verified. No production test flag, product preference override or renderer shortcut was added.

### Driver commands

Use a freshly signed device manifest containing the four new selectors. Set `MOTE_SYSTEM_XCTESTRUN` to that manifest. This helper requires `jq`, retains one result bundle and log per selector, and rejects skipped or empty runs:

```sh
: "${MOTE_SYSTEM_XCTESTRUN:?Set the freshly signed system-setting xctestrun path}"
MOTE_SYSTEM_DEVICE=00008132-001104661AB9001C
MOTE_SYSTEM_ARTIFACTS=$(mktemp -d /private/tmp/mote-system-settings.XXXXXX) || exit 1
mote_system_case() {
  mote_system_selector=$1
  xcodebuild test-without-building \
    -xctestrun "$MOTE_SYSTEM_XCTESTRUN" \
    -destination "platform=iOS,id=$MOTE_SYSTEM_DEVICE" \
    -destination-timeout 45 -collect-test-diagnostics never \
    -only-testing:"MoteUITests/MoteUITests/$mote_system_selector" \
    -resultBundlePath "$MOTE_SYSTEM_ARTIFACTS/$mote_system_selector.xcresult" \
    > "$MOTE_SYSTEM_ARTIFACTS/$mote_system_selector.log" 2>&1 &&
  xcrun xcresulttool get test-results summary \
    --path "$MOTE_SYSTEM_ARTIFACTS/$mote_system_selector.xcresult" \
    > "$MOTE_SYSTEM_ARTIFACTS/$mote_system_selector.summary.json" &&
  jq -e '.passedTests == 1 and .failedTests == 0 and .skippedTests == 0' \
    "$MOTE_SYSTEM_ARTIFACTS/$mote_system_selector.summary.json"
}
```

Disable Automatic in Settings so the appearance cannot change during a selector. The following public device commands set and independently query the system mode. Retain both JSON files. The query and its exact value are required before the corresponding test, regardless of the informational XCTest getter:

```sh
xcrun devicectl device settings appearance --device "$MOTE_SYSTEM_DEVICE" --mode light \
  --json-output "$MOTE_SYSTEM_ARTIFACTS/light-set.json" &&
xcrun devicectl device info appearance --device "$MOTE_SYSTEM_DEVICE" \
  --json-output "$MOTE_SYSTEM_ARTIFACTS/light-before.json" &&
jq -e '.info.outcome == "success" and .result.userInterfaceStyle == "light"' \
  "$MOTE_SYSTEM_ARTIFACTS/light-before.json" &&
mote_system_case testPhysicalSystemAppearanceLight
```

Select and query Dark, then run its exact-palette selector:

```sh
xcrun devicectl device settings appearance --device "$MOTE_SYSTEM_DEVICE" --mode dark \
  --json-output "$MOTE_SYSTEM_ARTIFACTS/dark-set.json" &&
xcrun devicectl device info appearance --device "$MOTE_SYSTEM_DEVICE" \
  --json-output "$MOTE_SYSTEM_ARTIFACTS/dark-before.json" &&
jq -e '.info.outcome == "success" and .result.userInterfaceStyle == "dark"' \
  "$MOTE_SYSTEM_ARTIFACTS/dark-before.json" &&
mote_system_case testPhysicalSystemAppearanceDark
```

Leave Mote and its runner without per-app Accessibility overrides. In Accessibility, Motion, enable Reduce Motion. Retain the settings screenshot and record the app replacement journey, then run:

```sh
mote_system_case testPhysicalSystemReduceMotionEnabled
```

Disable the same system preference. Retain its settings screenshot and record the replacement journey, then run:

```sh
mote_system_case testPhysicalSystemReduceMotionDisabled
```

Retain the device, iPadOS version, authoritative appearance query JSON, result bundles, observations, role trees, checked screenshots and both motion recordings. A successful command is evidence for its implemented assertions only. The corrected commands have not been run on the physical iPad. IPAD-06 remains open.

### Focused automated verification

The deterministic pixel test was first observed failing against a sampler that returned zero. The implemented sampler then passed exact colors, mixed-region fractions, tolerance boundaries, wrong-theme rejection, transparency rejection and invalid-region checks. One Objective-C XCTest macro compile error in the new physical toggle assertion was corrected before the final run. It did not involve product code.

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug \
  -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' \
  -derivedDataPath /private/tmp/mote-ipad06-exhaustive-sim \
  -resultBundlePath /private/tmp/mote-system-settings-final.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testSystemThemePixelSamplerRejectsWrongColorsAndInvalidRegions \
  -only-testing:MoteUITests/MoteUITests/testPhysicalSystemAppearanceLight \
  -only-testing:MoteUITests/MoteUITests/testPhysicalSystemAppearanceDark \
  -only-testing:MoteUITests/MoteUITests/testPhysicalSystemReduceMotionEnabled \
  -only-testing:MoteUITests/MoteUITests/testPhysicalSystemReduceMotionDisabled test
```

Output: exited 0. The final result reports one passed helper test, zero failures and four physical skips. The four physical selectors skipped before Mote launch. The result bundle does not supply physical system-setting evidence.

Both final unsigned bundles compiled:

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug -destination 'generic/platform=iOS' \
  -derivedDataPath /private/tmp/mote-system-settings-device \
  CODE_SIGNING_ALLOWED=NO build-for-testing

xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath /private/tmp/mote-system-settings-sim \
  CODE_SIGNING_ALLOWED=NO build-for-testing
```

Output: both exited 0. Logs are `/private/tmp/mote-system-settings-final-device-build.log` and `/private/tmp/mote-system-settings-final-sim-build.log`. Existing XCTest deployment-target linker warnings remain. The new device manifest is `/private/tmp/mote-system-settings-device/Build/Products/Mote_iphoneos27.1-arm64.xctestrun`. The new Simulator manifest is `/private/tmp/mote-system-settings-sim/Build/Products/Mote_iphonesimulator27.1-arm64.xctestrun`. They are unsigned build artifacts, not physical execution evidence.

The seven documented shell blocks passed `bash -n` without execution. `git diff --check` passed. A workspace verification attempt was stopped with exit 143 at the driver's request because another agent was actively editing daemon files. That interrupted attempt is not cited as verification. Final workspace tests and clippy await the coordinated stable-source check.

## Bounded physical gallery result

Date: 2026-10-17. Environment: iPad Pro 11-inch (M4), UDID `00008132-001104661AB9001C`, iPadOS 26.5 build 23F77. The corrected independently selected cases total 32 passed, zero failed and zero skipped. Layout portrait is `/private/tmp/mote-layout-portrait-bounded2.xcresult`. The other 31 result bundles, logs and summary JSON files are under `/private/tmp/mote-gallery-physical.kIkGji`. Every summary reports exactly one passed, zero failed and zero skipped. This covers the declared exhaustive center-and-four-inset-edge gallery journey in both orientations, including all list batches, Dashboard selects and separate Preview note cases. It does not establish system appearance, VoiceOver, system Reduce Motion, split view or background recovery.

The preceding `/private/tmp/mote-layout-portrait-bounded.xcresult` failed during installation because the preserve-only resign omitted `application-identifier`. It is packaging-only incomplete evidence and supplies no control result. The corrected bundle `/private/tmp/mote-motion-signed.LDFheG` was signed with explicit entitlements extracted from Mote's own embedded provisioning profile into `/private/tmp/mote-base-profile-entitlements.plist`. The driver verified `application-identifier=FQ6YGD7554.com.beelol.mote`, team `FQ6YGD7554`, `get-task-allow=true` and the Mote team keychain groups before the successful run.

The evidence was checked with:

```sh
xcrun xcresulttool get test-results summary \
  --path /private/tmp/mote-layout-portrait-bounded2.xcresult
node -e 'const fs=require("fs"); const p="/private/tmp/mote-gallery-physical.kIkGji"; const files=fs.readdirSync(p).filter(f=>f.endsWith(".summary.json")); const invalid=files.filter(f=>{const j=JSON.parse(fs.readFileSync(p+"/"+f)); return j.passedTests!==1||j.failedTests!==0||j.skippedTests!==0}); console.log(JSON.stringify({summaries:files.length,invalid})); if(files.length!==31||invalid.length) process.exit(1)'
```

Output: first summary `passedTests=1`, `failedTests=0`, `skippedTests=0`; aggregate `{"summaries":31,"invalid":[]}`. IPAD-06 remains open.

## Physical background suspension RED and diagnostic correction

Date: 2026-10-17. The same M4 iPad on iPadOS 26.5 ran `testPhysicalPairedHostRecoversAfterBackgrounding` in `/private/tmp/mote-background-physical-current.xcresult`. The summary reports zero passed, one failed and zero skipped. After Home, `waitForState:RunningBackgroundSuspended timeout:60` failed at the former line 1217. The driver did not stop the host because the required suspension coordination marker was not reached. This is no recovery pass and supplies no stop/start evidence.

The original result does not identify which alternate application state persisted. The diagnostic correction polls the same public `app.state`, logs monotonic elapsed time, raw state and exact state name, and retains `physical-background-suspension-state-samples` before asserting the wait result. The timeout remains 60 seconds. Only `RunningBackgroundSuspended` satisfies the predicate. It still rechecks exact suspension before and after the driver's 30-second stop window. There is no acceptance of merely background-running or not-running. Deterministic helper assertions distinguish all five public states and an unrecognized value. A physical rerun is required to determine whether this iPad reports persistent background-running or another state. No product background mode or recovery shortcut was added.

### Post-correction focused verification

The final owned-source Simulator execution used:

```text
xcodebuild -quiet test-without-building \
  -xctestrun /private/tmp/mote-ipad06-exhaustive-sim/Build/Products/Mote_iphonesimulator27.1-arm64.xctestrun \
  -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' \
  -resultBundlePath /private/tmp/mote-system-settings-diagnostic-final.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testSystemThemePixelSamplerRejectsWrongColorsAndInvalidRegions \
  -only-testing:MoteUITests/MoteUITests/testGalleryRecoveryRejectsMerelyBackgroundRunning \
  -only-testing:MoteUITests/MoteUITests/testPhysicalSystemAppearanceLight \
  -only-testing:MoteUITests/MoteUITests/testPhysicalSystemAppearanceDark \
  -only-testing:MoteUITests/MoteUITests/testPhysicalSystemReduceMotionEnabled \
  -only-testing:MoteUITests/MoteUITests/testPhysicalSystemReduceMotionDisabled \
  -only-testing:MoteUITests/MoteUITests/testPhysicalPairedHostRecoversAfterBackgrounding
```

Output: exited 0, two passed helpers, zero failed and five physical skips. Log: `/private/tmp/mote-system-settings-diagnostic-final-tests.log`. The compiled XCTest binary timestamp was later than the final source edit. The pixel helper covers exact Light/Dark colors and symmetric opposite-theme rejection. The state helper rejects background-running and tests exact diagnostic state names. None of the five physical selectors launched Mote on Simulator.

The final device build repeated the unsigned `generic/platform=iOS` build-for-testing command above. It exited 0, with log `/private/tmp/mote-system-settings-diagnostic-final-device-build.log` and manifest `/private/tmp/mote-system-settings-device/Build/Products/Mote_iphoneos27.1-arm64.xctestrun`. A concurrent initial Simulator build exited 65 because another agent's in-progress `mote-ios/src/probe.rs` called color component methods on `ColorMode`. That unrelated compile failure is retained in `/private/tmp/mote-system-settings-appearance-corrected-sim-build.log`. It is not an XCTest assertion failure. The owner corrected that file separately.

Repeating the unsigned `generic/platform=iOS Simulator` build-for-testing command after that correction exited 0. Log: `/private/tmp/mote-system-settings-diagnostic-final-sim-build.log`. Manifest: `/private/tmp/mote-system-settings-sim/Build/Products/Mote_iphonesimulator27.1-arm64.xctestrun`. Existing XCTest deployment-target linker warnings and three Rust probe dead-code warnings are recorded in that build log. Eight documented shell blocks passed `bash -n`. `git diff --check` passed. No final workspace test or clippy result is claimed while coordinated daemon edits remain active. The corrected appearance selectors and background diagnostic still require physical reruns. No checkbox is changed.

## Physical Light product RED

Date: 2026-10-17. The corrected Light selector ran on the M4 iPad with the authoritative device appearance still Light. Result: `/private/tmp/mote-system-appearance-light2.xcresult`. Mote launched and its full Layout native inventory passed. The selector then failed the unchanged exact Light palette assertion at the former line 1175. The log reports `SYSTEM PALETTE Layout background=0.000000 pixels={{28, 192}, {8, 8}}`.

The retained `system-appearance-light` screenshot is visibly dark. This is a real iOS system-theme propagation defect, not a sampler-coordinate failure. The exported PNG is 1668 by 2420 pixels. The retained role tree reports an 834 by 1210 point window and Layout frame `{{8, 90}, {96, 48}}`. The first four-point patch begins six points inside that frame at `(14, 96)`. Its exact two-times pixel mapping is the reported `(28, 192)` with size eight by eight. Independent sRGB reads at pixels `(28,192)`, `(32,196)`, `(40,200)`, `(100,200)` and `(1000,1000)` all returned opaque `(11,11,13)`. These are dark pixels, not the required Light paper `(244,241,234)`.

The failure attachments were exported with:

```text
xcrun xcresulttool export attachments \
  --path /private/tmp/mote-system-appearance-light2.xcresult \
  --output-path /private/tmp/mote-appearance-light2-export
sips -g pixelWidth -g pixelHeight -g orientation \
  /private/tmp/mote-appearance-light2-export/EEE0BA09-11E5-4AC1-A57D-AD5BAA78B847.png
```

The checked screenshot is `/private/tmp/mote-appearance-light2-export/EEE0BA09-11E5-4AC1-A57D-AD5BAA78B847.png`. Its native role tree is `/private/tmp/mote-appearance-light2-export/D792CB44-C221-410B-A24A-B1888217D903.txt`. No sampler, coordinate, palette, tolerance or product code was changed in response to this RED. No coordinate regression is asserted because the observed mapping is correct. A product fix and a new physical Light run are required. IPAD-06 remains open.

## Physical background state API RED

Date: 2026-10-17. `/private/tmp/mote-background-state-diagnostic.xcresult` reports zero passed, one failed and zero skipped on the M4 iPad, iPadOS 26.5. All 60 timestamped samples after Home report raw state 4, `foreground`, from elapsed 1.042 through 60.004 seconds. The independent device screenshot `/private/tmp/mote-after-home-diagnostic.png` visibly shows SpringBoard's Home Screen, including application icons and the dock. Home succeeded. This physical XCTest state getter is not reliable evidence of foreground or suspension on this device. The driver did not reach the host-stop coordination window. No recovery pass is claimed.

The owner approved using observable background evidence instead of that unreliable getter. This does not claim OS suspension. IPAD-06 requires recovery after backgrounding. The controlled host interruption remains the required recovery boundary. The corrected journey must observe SpringBoard with a hittable Home icon, no hittable Mote application surface and no hittable Mote control before the stop marker and again before activation. `app.state` remains diagnostic only. SpringBoard and Mote role trees, raw state observations and a full-device screenshot must be retained. The exact paired-host offline assertion must still pass before the start marker, followed by exact-host online recovery, prior cursor/content, one new cursor-backed event and exactly-once replay after relaunch. Neither a reported background state alone nor a Home press alone supplies sufficient observable evidence.

### Observable background correction

`testPhysicalPairedHostRecoversAfterBackgrounding` keeps its physical-only pre-launch Simulator skip. It first requires Mote's surface hittable while foregrounded. After Home it polls for at most 60 seconds. Success requires SpringBoard's native window, at least one hittable native Home icon, a non-hittable Mote application surface and no hittable Mote native descendant. The raw states of both applications are retained but do not decide success. A full-device screenshot and both native role trees are attached before asserting success, including on timeout. The first coordination marker is now `BACKGROUND COORDINATION: Home observed and Mote hidden`. The driver stops the same paired host within the following 30 seconds and leaves it stopped until the offline capture. The exact observable Home/Mote boundary is checked and attached again before activation. The offline, start marker, online, cursor, new-event and exactly-once replay assertions are unchanged.

This supersedes the earlier strict-suspension implementation and its helper selector. It does not infer suspension from Home or from the unreliable state API. There is no product background mode, app-state override or alternate host connection added. The real host interruption remains necessary because the test cannot progress past its exact offline assertion if the host never stops.

The new deterministic `testGalleryBackgroundEvidenceRequiresHomeAndHiddenMote` checks all 16 combinations of the four observable conditions. Only native Home visibility with a hittable Home icon and both Mote visibility signals false is accepted. Background and foreground state names are still tested as diagnostics. The RED run `/private/tmp/mote-background-observable-red.xcresult` exited 65 with one failed test because an intentionally rejecting helper returned false for otherwise sufficient observable evidence. That stub was then replaced with the four-condition conjunction. No physical pass is claimed for this correction.

The focused command for the corrected source is:

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug \
  -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' \
  -derivedDataPath /private/tmp/mote-ipad06-exhaustive-sim \
  -resultBundlePath /private/tmp/mote-background-observable-final.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testGalleryBackgroundEvidenceRequiresHomeAndHiddenMote \
  -only-testing:MoteUITests/MoteUITests/testPhysicalPairedHostRecoversAfterBackgrounding test
```

The final command exited 0 with one passed helper, zero failures and one physical pre-launch skip. Log: `/private/tmp/mote-background-observable-final-tests.log`. Both unsigned `generic/platform=iOS` and `generic/platform=iOS Simulator` build-for-testing commands above were repeated against the final source and exited 0. Logs: `/private/tmp/mote-background-observable-final-device-build.log` and `/private/tmp/mote-background-observable-final-sim-build.log`. Manifests remain `/private/tmp/mote-system-settings-device/Build/Products/Mote_iphoneos27.1-arm64.xctestrun` and `/private/tmp/mote-system-settings-sim/Build/Products/Mote_iphonesimulator27.1-arm64.xctestrun`. The earlier GREEN attempt and both initial builds failed to compile because the harness requested a screenshot from `XCUIDevice`. This was corrected to the public `XCUIScreen.mainScreen.screenshot` before these final passes. Existing XCTest deployment-target linker warnings remain. No physical observable-background or host-recovery pass is claimed.

## Physical background recovery pass

Date: 2026-10-17. Environment: iPad Pro 11-inch (M4), UDID
`00008132-001104661AB9001C`, iPadOS 26.5 build 23F77, and the installed
`com.mote.moted` LaunchAgent on the paired Mac Wi-Fi host. Result bundle:
`/private/tmp/mote-background-state-gate-green2.xcresult`.

This run supersedes the observable-background claims above. Physical evidence showed that iPadOS
26.5 retained stale Mote hit-test values after SpringBoard became visible. The public Mote process
state was `RunningBackground`, SpringBoard was foreground, its window was visible and a native Home
icon was hittable. The fail-closed gate now requires the visible SpringBoard window, the hittable
Home icon and either `RunningBackground` or `RunningBackgroundSuspended`. It rejects unknown,
not-running and foreground states. Stale Mote hit-test values remain in the retained diagnostics but
do not decide the result.

The driver stopped only `com.mote.moted` after the first coordination marker. Mote then displayed
the exact paired host as offline. The driver restarted the same LaunchAgent after the second marker.
Mote returned online with `Cursor: 9` and the pre-existing `User: whats good?` row. The journey sent
a fresh UUID-backed message, observed the cursor advance to 12, terminated and relaunched Mote, and
found exactly one copy of the new message with the advanced cursor. The durable content check uses
native `User:` rows instead of full-screen OCR because the preceding run correctly exposed the
unrelated system clock changing from 3:43 PM to 3:44 PM.

Command:

```text
xcodebuild test-without-building \
  -xctestrun /private/tmp/mote-background-state-gate-device/Build/Products/Mote_iphoneos27.1-arm64.xctestrun \
  -destination 'id=00008132-001104661AB9001C' \
  -resultBundlePath /private/tmp/mote-background-state-gate-green2.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testGalleryBackgroundEvidenceRequiresHomeAndHiddenMote \
  -only-testing:MoteUITests/MoteUITests/testPhysicalPairedHostRecoversAfterBackgrounding
```

Output: two tests passed, zero failed and zero skipped in 75.205 seconds. The preceding
`/private/tmp/mote-background-state-gate-green.xcresult` is retained as a harness RED. It reached
successful offline and online recovery but failed only because full-screen OCR included the changing
status-bar clock. Background recovery is complete. Split view and actual VoiceOver remain open, so
IPAD-06 remains unchecked.
