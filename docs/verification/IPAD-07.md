# IPAD-07 verification in progress

Date: 2026-10-12, America/Los_Angeles.

Environment: App Store Connect in the signed-in Chrome session for STATION 42 INTERACTIVE L.L.C. This was a read-only inventory. No physical iPad was connected and IPAD-06 remains open.

Manual steps: open App Store Connect Apps, inspect the App Information page for each listed app, then open and cancel the New App form without submitting it. Open the linked Apple Developer Certificates, Identifiers & Profiles account for the same team and inspect All Identifiers.

Observed result: the account listed Overseer Remote and One More Fish. Overseer Remote uses bundle ID `com.beelol.overseer.phone`, SKU `overseer-phone` and Apple ID `6816774779`. One More Fish uses bundle ID `io.station42.aquafriends`, SKU `aquafriends` and Apple ID `6802165664`. The New App form showed no selectable unused bundle ID. It directed the user to register a new bundle ID in Certificates, Identifiers & Profiles. No Mote app record was present in this app list.

The Developer account showed team ID `FQ6YGD7554` and only the App IDs `com.beelol.overseer.phone` and `io.station42.aquafriends`. This cross-check covers the displayed App ID inventory, but does not approve a new identity or prove that a proposed SKU is globally available.

Command: `python3 -B ios/Mote/distribute.py preflight --manifest /private/tmp/mote-preflight-2026-10-12-manifest.json --inventory /private/tmp/mote-preflight-2026-10-12-inventory.json --output /Users/bilal/projects/mote/target/distribution/0.1.0-1`.

Output: exited 0 outside the restricted shell sandbox. The sandboxed run could not see the Apple Distribution identity. The read-only preflight saw Xcode 27.0, iPhoneOS SDK 27.0, the physical Rust target and an Apple Distribution identity for team `FQ6YGD7554`. It accepted a provisional `Mote`, `com.beelol.mote`, `mote-ipad-001`, version `0.1.0` build `1` manifest against the observed two-app inventory. The manifest and inventory are local temporary files. The proposed bundle ID and SKU are not registered or owner-approved for release. Preflight did not create a record or sign or upload anything.

No new identifier or app record was created. The criterion remains unchecked. The physical UI verification in IPAD-06 precedes the new Mote identity, signed archive and record under the current acceptance criterion.

Command: `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`.

Output: both exited 0 on 2026-10-12. The workspace test needed local fixture socket access outside the restricted shell sandbox. These checks do not replace the physical-device or Apple distribution verification.
