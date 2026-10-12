# Internal TestFlight packaging

`distribute.py` packages the existing Mote iPad client. It does not create an App Store Connect app record. Apple requires that record to be created in App Store Connect after the display name, bundle ID, SKU and team are confirmed. The script never changes the Simulator bundle ID. Its `preflight` action only reads local tools, identity files and signing certificates. `archive-export` is an explicit signed build. `upload` is a separate explicit action.

## Separate Mote app setup

1. Inspect the selected Apple team's existing app identifiers and App Store Connect apps. Choose a new explicit bundle ID and SKU that belong only to Mote. Keep the existing apps unchanged.
2. Register that explicit App ID in the Apple Developer account. In App Store Connect, create a new iOS app record named Mote using the same bundle ID and the new SKU. Record the generated App Store Connect app ID. Account Holder, Admin or App Manager access is required for record creation.
3. Install or select an Apple Distribution signing identity for that team. Keep the certificate, provisioning material and App Store Connect API private key outside Git. Make a current inventory snapshot and a release manifest with the exact approved identity.
4. Run the read-only `preflight`. After physical iPad verification, run `archive-export` to produce and inspect a signed archive and IPA. Run `upload` separately. Wait for Apple to process the uploaded build.
5. Add that processed build to an internal TestFlight group, then install it through TestFlight on the physical iPad. Recheck pairing, chat, generated UI, browser approval and host isolation. No external group or App Review action is part of this workflow.

Apple's instructions for [registering an App ID](https://developer.apple.com/help/account/identifiers/register-an-app-id/), [creating an app record](https://developer.apple.com/help/app-store-connect/create-an-app-record/add-a-new-app/) and [adding internal testers](https://developer.apple.com/help/app-store-connect/test-a-beta-version/add-internal-testers) are the authority for the account-side steps. The local script automates archive, IPA export and upload. It cannot manufacture the Apple account identity or signing credentials.

The iPad icon is generated from `generate-icon.swift` into `Assets.xcassets/AppIcon.appiconset/AppIcon.png`. The checked-in PNG is opaque and 1024 by 1024 pixels. The archive and IPA checks require the compiled iPad icon. Regenerate it with `swift -module-cache-path /private/tmp/mote-swift-module-cache ios/Mote/generate-icon.swift ios/Mote/Assets.xcassets/AppIcon.appiconset/AppIcon.png` if the source changes.

Keep the manifest and inventory outside Git. The values below are examples, not an assigned Mote identity. The inventory must be a complete, current snapshot of the selected Apple team's apps and builds. A local file alone cannot prove that App Store Connect has not changed since it was checked. Recheck it before archive and upload.

Manifest example before record creation:

```json
{"display_name":"Mote","bundle_id":"com.example.mote","sku":"MOTE-NEW","team_id":"ABCDE12345","version":"0.1.0","build_number":1}
```

Add `"app_store_connect_id":"1234567890"` after the distinct record exists. The numeric ID must match the inventory. The example ID is not a real assigned Mote record.

Inventory example after record creation:

```json
{"checked_at":"2026-10-11","team_id":"ABCDE12345","apps":[{"bundle_id":"com.example.mote","sku":"MOTE-NEW","app_store_connect_id":"1234567890","builds":[]}]}
```

Use a new absolute output path below `target/distribution` for each archive. The script refuses an existing path, symlinks and paths outside that directory. For example:

```sh
python3 -B ios/Mote/distribute.py preflight --manifest /private/tmp/mote-release.json --inventory /private/tmp/mote-apps.json --output /Users/bilal/projects/mote/target/distribution/0.1.0-1
python3 -B ios/Mote/distribute.py archive-export --manifest /private/tmp/mote-release.json --inventory /private/tmp/mote-apps.json --output /Users/bilal/projects/mote/target/distribution/0.1.0-1 --confirm-inventory-current --confirm-physical-device-verified
```

Preflight requires the physical Rust target and a local Apple Distribution identity for the selected team. Archive requires a matching App Store Connect record in the inventory. It uses the Xcode `app-store-connect` export method with `testFlightInternalTestingOnly` enabled. It checks the archive and IPA bundle ID, version, build number, team signature and entitlements. It records a local receipt and does not upload.

Upload requires an App Store Connect API key ID, issuer ID and private key directory outside the repository. It also requires a separate confirmation flag. The private key is never passed on the command line. Use Xcode's `altool`-compatible `AuthKey_<key ID>.p8` file in `API_PRIVATE_KEYS_DIR`.

```sh
MOTE_ASC_API_KEY_ID=... MOTE_ASC_ISSUER_ID=... API_PRIVATE_KEYS_DIR=/private/path/to/keys python3 -B ios/Mote/distribute.py upload --manifest /private/tmp/mote-release.json --inventory /private/tmp/mote-apps.json --output /Users/bilal/projects/mote/target/distribution/0.1.0-1 --confirm-inventory-current --confirm-physical-device-verified --confirm-upload
```

Successful upload is not a TestFlight pass. Check processing in App Store Connect, install the internal build on a physical iPad and repeat the named journeys. Do not enable external testing or App Review.
