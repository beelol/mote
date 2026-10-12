# IPAD-06 verification in progress

Date: 2026-10-11, America/Los_Angeles.

Environment: macOS 26.6.2, Xcode 27.0, iPhoneOS SDK 27.0, iOS 27.0 (24A434) Simulator on a temporary iPad Pro 11-inch (M5), Rust 1.89.0. The Simulator identifier was `30C51BEC-10D6-4AF9-8A74-A74CB2003771`.

The native iOS client now mirrors Matrix control records into individual UIKit accessibility elements. Each frame updates their role, label, value, enabled state and clipped frame. Matching ID and role reuse the same UIKit element to preserve focus. Old elements refuse actions. Activation and adjustable requests return to egui through its AccessKit action path. This is implementation evidence, not a physical VoiceOver pass.

The same bridge now records the live host picker, connection and chat text, composer, Send button and browser approval cards. Approval controls retain their online, in-flight and expiry checks when activated through the native bridge. The existing live and browser XCUITests contain native-element assertions, but those longer journeys were not rerun after this extension. They do not count as passing evidence yet.

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
