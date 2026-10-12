# IPAD-09 verification

Date: 2026-10-11, America/Los_Angeles.

Environment: macOS 26.6.2, Xcode 27.0 (27A266a), iPhoneOS SDK 27.0, iOS Simulator 27.0 (24A434), Rust 1.89.0. The Simulator device was an iPad Pro 11-inch (M5), arm64. The live test used a temporary iPad Pro 11-inch (M5) Simulator with the same runtime. Its device was deleted after the test. The existing Simulator and its app data were not changed by the live test.

## Physical iPad build

Command: `cargo check -p mote-ios --target aarch64-apple-ios`.

Output: exited 0.

Command: `xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'generic/platform=iOS' -derivedDataPath target/ipad-device-derived -showBuildSettings`.

Output: `ARCHS = arm64`, `PLATFORM_NAME = iphoneos`, `SUPPORTED_PLATFORMS = iphoneos iphonesimulator`, `CODE_SIGNING_ALLOWED = NO`, and `SDKROOT` was iPhoneOS27.0.sdk.

Command: `xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'generic/platform=iOS' -derivedDataPath target/ipad-device-derived CODE_SIGNING_ALLOWED=NO build`.

Output: `** BUILD SUCCEEDED **`. The resulting `Mote.app/Mote` was a Mach-O arm64 executable. Its bundle ID remained `local.mote.simulator`. The build was intentionally unsigned. It was not installed on a physical device or uploaded.

## Simulator regression

Command: `xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' -derivedDataPath target/ipad-derived -resultBundlePath target/ipad-09-gallery.xcresult -only-testing:MoteUITests/MoteUITests/testGalleryPagesAndControlsInBothOrientations test`.

Output: exited 0. The result bundle summary reported `result: Passed`, one passed test, zero failed tests. The native test visits Layout, Toggle, Slider, Select, List, Form, Dashboard and Fixture in portrait and landscape. It exercises controls and scrolling.

Command: `MOTE_LIVE=1 MOTE_SIMULATOR_ID=4CE8E2BC-E689-437C-ABF8-8FE17EEFA73E MOTE_ARTIFACTS=/private/tmp/mote-ipad-09-live cargo test -p mote-ios --test simulator_live -- --ignored --nocapture`.

Output: one passed test, zero failed tests. It paired two hosts, completed native chat and a typed dashboard edit, resumed a cursor without duplicates, changed a host endpoint, rejected an impostor, and restored and deleted native Keychain credentials. The temporary Simulator was shut down and deleted after the run. The test output reported the Home cursor advancing from 1 to 12.

The Simulator app kept the existing `local.mote.simulator` identity. Physical-device touch and permission checks, signed archive, App Store Connect record and TestFlight remain separate criteria.

## Repository gates

Command: `cargo test --workspace` with local fixture socket access.

Output: exited 0. The first sandboxed attempt was denied a local browser fixture socket with `Operation not permitted`. The rerun with local socket access passed.

Command: `cargo clippy --workspace -- -D warnings`.

Output: exited 0 with no warnings.

## Final native Simulator journey recheck

Date: 2026-10-11, America/Los_Angeles. Environment: macOS 26.6.2, iPad Pro 11-inch (M5) Simulator, iOS 27.0 (24A434), arm64. The app included the later packaging-time version settings from `340db58`. A new temporary Simulator was used and deleted after the checks. No physical iPad was involved.

Command: `MOTE_LIVE=1 TMPDIR=/private/tmp MOTE_SIMULATOR_ID=54E1E19F-42B1-4E01-8AF5-2B8B2DB5463D MOTE_ARTIFACTS=/private/tmp/mote-ipad-final-inferred-20261011 MOTE_SURFACE_ATTESTATION=/private/tmp/mote-surface-attested.8F07uH/surface-activation.json cargo test -p mote-ios --test inferred_surface_simulator native_inferred -- --ignored --nocapture`.

Output: one passed Rust test. The native XCUITest result bundle reported one passed test and zero failures. It verified successful inferred UI, one retry, prior-surface preservation, a terminal diagnostic and prose fallback. The harness reported 14 model calls, 14 ledger rows, 14 packs and three authenticated typed refusals.

Command: `MOTE_LIVE=1 MOTE_CHROME='/private/tmp/mote-browser-runtime-155/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing' MOTE_SIMULATOR_ID=54E1E19F-42B1-4E01-8AF5-2B8B2DB5463D MOTE_ARTIFACTS=/private/tmp/mote-ipad-final-browser-20261011 cargo test -p mote-ios --test browser_approval native_paired -- --ignored --nocapture`.

Output: one passed Rust test. The native XCUITest result bundle reported one passed test and zero failures. Denial released zero effects, repeated approval taps released exactly one effect, natural 120-second expiry released no additional effect, and the validated Matrix result matched.

These rechecks strengthen Simulator evidence only. They do not substitute for installation or interaction on a physical iPad, or for the final signed TestFlight build.
