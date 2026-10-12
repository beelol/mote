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

## Delivery order and App ID registration

Date: 2026-10-12, America/Los_Angeles. Environment: the signed-in Apple Developer account for STATION 42 INTERACTIVE L.L.C., team `FQ6YGD7554`, in Chrome. The owner's later instruction moved internal TestFlight availability before physical iPad installation. The README and iPad host spec now retain the physical Verify clauses under IPAD-06 and IPAD-08 while allowing IPAD-07, IPAD-10 and IPAD-11 first. The earlier physical prerequisite and provisional-identity statements above are historical observations, not the current order.

Manual action: open Certificates, Identifiers & Profiles, choose a new App ID of type App, enter description `Mote` and explicit bundle ID `com.beelol.mote`, leave optional capabilities off, review the confirmation page and register it.

Observed result: the All Identifiers page listed `Mote` with `com.beelol.mote` alongside the unchanged `aquafriends` and `Overseer` entries. The separate App Store Connect record is still pending. Its New App form lists `Mote - com.beelol.mote` as the only available bundle ID. The form offers only Full Access for user access. Action-time confirmation was requested before creating the record. No archive or upload has occurred.

The owner allowed the form's Full Access setting. This was not evidence that any other App Store Connect users exist; the earlier assumption about existing users was corrected. The form then rejected the proposed name `Mote` with `The app name you entered is already being used.` No app record was created. The bundle ID remains registered. A new user-facing App Store Connect name requires the owner's choice before retrying.
