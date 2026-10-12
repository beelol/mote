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
