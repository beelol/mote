# Internal TestFlight packaging

`distribute.py` packages the existing Mote iPad client. It does not create an App Store Connect app record. Apple requires that record to be created in App Store Connect after the display name, bundle ID, SKU and team are confirmed. The script never changes the Simulator bundle ID. Its `preflight` action only reads local tools, identity files and signing certificates. `archive-export` is an explicit signed build. `upload` is a separate explicit action.

Keep the manifest and inventory outside Git. The values below are examples, not an assigned Mote identity. The inventory must be a complete, current snapshot of the selected Apple team's apps and builds. A local file alone cannot prove that App Store Connect has not changed since it was checked. Recheck it before archive and upload.

Manifest example before record creation:

```json
{"display_name":"Mote","bundle_id":"com.example.mote","sku":"MOTE-NEW","team_id":"ABCDE12345","version":"0.1.0","build_number":1}
```

Add `"app_store_connect_id":"1234567890"` after the distinct record exists. The numeric ID must match the inventory. The example ID is not a real assigned Mote record.

Inventory example after record creation:

```json
{"checked_at":"2026-10-11","apps":[{"bundle_id":"com.example.mote","sku":"MOTE-NEW","app_store_connect_id":"1234567890","builds":[]}]}
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
