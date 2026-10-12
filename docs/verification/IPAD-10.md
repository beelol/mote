# IPAD-10 verification in progress

Date: 2026-10-11, America/Los_Angeles.

Environment: macOS 26.6.2, Xcode 27.0 (27A266a), iPhoneOS SDK 27.0, Python 3.14, Rust 1.89.0. No App Store Connect credentials, app record or physical iPad were used for this checkpoint.

The guarded script is `ios/Mote/distribute.py`. `ios/Mote/DISTRIBUTION.md` describes its inputs and explicit actions. The default preflight only reads local tools, a supplied app inventory and signing identities. Archive and export are a separate command. Upload is a further opt-in command. The script does not create an App Store Connect record.

Command: `python3 -B -m unittest ios/Mote/test_distribute.py`.

Output: thirteen tests passed. Missing identity, the Simulator bundle ID, an inventory from a different team, colliding bundle ID or SKU, missing app record, stale inventory, duplicate version and build number, absent selected signing identity and invalid output paths all raised errors without invoking an Apple mutation command. Archive refused missing inventory and physical-device confirmations before preflight or packaging. A simulated successful path asserted the exact signed archive overrides, internal-only export options and local receipt, with no upload command. A changed IPA was refused before upload. A separate simulated upload used only the receipt-matching IPA.

The installed Xcode 27 `codesign -d --entitlements -` prints a human-readable summary. It did not yield a parseable plist. The verifier now requests `:-`, which the installed tool returned as XML plist with a deprecation warning. The signing parser test exercises that XML path. This is a local tooling compatibility check, not evidence of a real distribution signature.

Command: `plutil -lint ios/Mote/Info.plist ios/Mote/Mote.xcodeproj/project.pbxproj`.

Output: both files passed.

Command: `xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' -derivedDataPath target/ipad-derived build`.

Output: exited 0. The rebuilt Simulator app retained bundle ID `local.mote.simulator`, marketing version `0.0.0` and build number `1` after making those values selectable for a signed release.

Command: `xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'generic/platform=iOS' -derivedDataPath target/ipad-device-derived CODE_SIGNING_ALLOWED=NO build`.

Output: exited 0. The unsigned physical-iPad app also retained bundle ID `local.mote.simulator`, marketing version `0.0.0` and build number `1`.

Command: `cargo test --workspace` with local fixture socket access.

Output: exited 0.

Command: `cargo clippy --workspace -- -D warnings`.

Output: exited 0 with no warnings.

This is partial evidence. The criterion remains unchecked. A real signed archive and IPA must still be inspected for the approved team, bundle ID and entitlements. A deliberate upload and App Store Connect processing outcome must be recorded after the distinct app record and physical-device checks exist. A supplied inventory file cannot by itself prove that App Store Connect is complete or unchanged.

## iPad icon packaging prerequisite

Date: 2026-10-11, America/Los_Angeles. Environment: macOS 26.6.2, Xcode 27.0, iPhoneOS SDK 27.0. No Apple account or physical device was used.

Command: `swift -module-cache-path /private/tmp/mote-swift-module-cache ios/Mote/generate-icon.swift ios/Mote/Assets.xcassets/AppIcon.appiconset/AppIcon.png`.

Output: exited 0. `sips` reported a 1024 by 1024 RGB PNG with no alpha channel.

Command: `xcrun actool --compile target/icon-validation --platform iphoneos --minimum-deployment-target 16.0 --target-device ipad --app-icon AppIcon --output-partial-info-plist target/icon-validation-info.plist ios/Mote/Assets.xcassets`.

Output: exited 0 and produced `Assets.car`, the iPad icon raster and a partial Info.plist with `CFBundleIcons~ipad.CFBundlePrimaryIcon.CFBundleIconName = AppIcon`. The local CoreSimulatorService emitted a connection warning but did not prevent asset compilation.

Command: `xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'generic/platform=iOS' -showBuildSettings`. The same check was run for Release.

Output: both configurations reported `ASSETCATALOG_COMPILER_APPICON_NAME = AppIcon` and `TARGETED_DEVICE_FAMILY = 2`. The separate asset compilation does not prove the final signed archive contains the icon.

Command: `python3 -B -m unittest ios/Mote/test_distribute.py`.

Output: fourteen tests passed. The new test confirms that archive verification refuses an app without its compiled iPad icon. A real archive and IPA remain unverified.

Command: `xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'generic/platform=iOS' -derivedDataPath target/ipad-device-accessibility CODE_SIGNING_ALLOWED=NO build`.

Output: exited 0 with the icon and native accessibility bridge together. The resulting arm64 app's Info.plist contains `CFBundleIcons~ipad.CFBundlePrimaryIcon.CFBundleIconName = AppIcon`, and `Assets.car` is present. It remains unsigned with the Simulator identity. This does not satisfy the signed archive or IPA Verify clause.

Command: `security find-identity -v -p codesigning`.

Output: `0 valid identities found`. No Apple Distribution certificate was available on this Mac for a signed archive. `MOTE_ASC_API_KEY_ID`, `MOTE_ASC_ISSUER_ID` and `API_PRIVATE_KEYS_DIR` were also absent from the process environment. No credentials or identity values were printed or added to Git. The selected Apple team, distinct app record and upload remain unverified.

## Signing availability recheck

Date: 2026-10-11, America/Los_Angeles. Environment: the same local Mac. This is a later read-only check and supersedes the earlier certificate inventory above.

Command: `security find-identity -v -p codesigning`.

Output: three valid identities were present, including one Apple Distribution identity. Its team has not been approved for Mote and must not be assumed from the certificate. No signing or archive command was run.

Command: check whether `MOTE_ASC_API_KEY_ID`, `MOTE_ASC_ISSUER_ID` and `API_PRIVATE_KEYS_DIR` are set, without printing values.

Output: all three were unset. The Mac was locked for native app access. A read-only browser navigation to App Store Connect reached its Apple Account sign-in page with `authResult=FAILED`, so the app inventory could not be inspected. No login, Mote app record or unique identity was confirmed. IPAD-10 remains unchecked.

## Upload tool compatibility

Date: 2026-10-11, America/Los_Angeles. Environment: Xcode 27.0 (27A266a) on the local Mac. No App Store Connect credentials or remote upload were used.

Command: `xcrun altool --help` and `xcrun altool --version`.

Output: both exited 1 before showing help or a version because this Xcode installation could not open `Defaults.properties`. The earlier upload command would not run here. `xcodebuild -help` listed `-exportArchive`, the `destination` options `export` and `upload`, App Store Connect authentication key flags, and `testFlightInternalTestingOnly`.

The upload action now rechecks the local IPA hash and the signed archive identity, then prepares a distinct Xcode upload export using the approved team, bundle ID and internal-only option. It requires the exact API key file outside the repository. An attempt marker prevents silent retries if Apple's processing outcome is unclear. No upload has been attempted.

Command: `python3 -B -m unittest ios/Mote/test_distribute.py`.

Output: fifteen tests passed. A mocked upload checked the Xcode command, credential path, exact internal-only options, archive recheck and one-attempt marker. Missing archive, missing key and changed IPA cases stopped before an Apple mutation command. A real signed archive and actual upload remain unverified.

## Archive binding after local export

Date: 2026-10-11 to 2026-10-12, America/Los_Angeles. Environment: the local Python test fixture. No Apple signing or account access was used.

The receipt now records a deterministic digest of the entire signed archive app. Upload compares this digest before Xcode receives the archive. The digest rejects external symlinks and includes file paths and contents. This prevents a different validly signed app with the same identity and build number from replacing the archive after local IPA verification.

Command: `python3 -B -m unittest ios/Mote/test_distribute.py`.

Output: sixteen tests passed. Changing the archived icon after writing the receipt stopped upload before the mocked Apple mutation. An external archive symlink was refused. The real archive and remote upload remain unverified.

Command: `cargo test --workspace`.

Output: exited 0 on 2026-10-12. Live tests requiring a physical device or explicit fixture environment remained ignored. This does not replace IPAD-06 or a signed distribution check.

Command: `cargo clippy --workspace -- -D warnings`.

Output: exited 0 with no warnings on 2026-10-12.

## Account-backed preflight

Date: 2026-10-12, America/Los_Angeles. Environment: the signed-in STATION 42 INTERACTIVE L.L.C. Apple account, Xcode 27.0 and the local Mac. The account inventory and proposed Mote identity are recorded in `docs/verification/IPAD-07.md`. No Apple mutation was made.

Command: `python3 -B ios/Mote/distribute.py preflight --manifest /private/tmp/mote-preflight-2026-10-12-manifest.json --inventory /private/tmp/mote-preflight-2026-10-12-inventory.json --output /Users/bilal/projects/mote/target/distribution/0.1.0-1`.

Output: exited 0 outside the restricted shell sandbox. It confirmed Xcode 27.0, iPhoneOS SDK 27.0, the physical Rust target and a local Apple Distribution identity for the observed team. The input is a manually supplied snapshot, not a live API query. The bundle ID and SKU remain provisional.

Command: the same `archive-export` invocation with `--confirm-inventory-current` but without a Mote App Store Connect record or physical-device confirmation.

Output: exited 1 with `A distinct App Store Connect record is required`. No archive output was created. This is a real refusal check, not a signed archive or upload pass.
