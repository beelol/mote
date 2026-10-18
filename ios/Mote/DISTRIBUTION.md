# Internal TestFlight packaging

`distribute.py` packages the existing Mote iPad client. It does not create an App Store Connect app record. Apple requires that record to be created in App Store Connect after the display name, bundle ID, SKU and team are confirmed. The script never changes the Simulator bundle ID. Its `preflight` action only reads local tools, identity files and signing certificates. `archive-export` is an explicit signed build. `upload` is a separate explicit action.

## Separate Mote app setup

1. Inspect the selected Apple team's existing app identifiers and App Store Connect apps. Choose a new explicit bundle ID and SKU that belong only to Mote. Keep the existing apps unchanged.
2. Register that explicit App ID in the Apple Developer account. In App Store Connect, create a new iOS app record using the same bundle ID and the new SKU. The App Store Connect name may differ from the native `Mote` display name when the latter is unavailable. Record the generated App Store Connect app ID. Account Holder, Admin or App Manager access is required for record creation.
3. Install or select an Apple Distribution signing identity for that team. Keep the certificate, provisioning material and App Store Connect API private key outside Git. Make a current inventory snapshot and a release manifest with the exact approved identity.
4. Run the read-only `preflight`. After final native Simulator UI verification, run `archive-export` to produce and inspect a signed archive and IPA. Run `upload` separately. Wait for Apple to process the uploaded build.
5. Add that processed build to an internal TestFlight group. Physical iPad installation can follow later. On the device, recheck touch, accessibility, pairing, chat, generated UI, browser approval and host isolation. No external group or App Review action is part of this workflow.

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
python3 -B ios/Mote/distribute.py archive-export --manifest /private/tmp/mote-release.json --inventory /private/tmp/mote-apps.json --output /Users/bilal/projects/mote/target/distribution/0.1.0-1 --confirm-inventory-current --confirm-native-ui-verified
```

Preflight requires the physical Rust target and a local Apple Distribution identity for the selected team. Archive requires a matching App Store Connect record in the inventory. Xcode automatically provisions the archive, which may have development entitlements. The separate `app-store-connect` export re-signs the IPA for distribution with `testFlightInternalTestingOnly` enabled. The verifier checks the archive and IPA bundle ID, version, build number, team signature and entitlements, and requires `get-task-allow = false` in the IPA. It records a local receipt and does not upload.

Upload requires either an App Store Connect API key ID, issuer ID and private key directory outside the repository, or the explicit `--use-xcode-account` option with the selected team already signed in to Xcode. It also requires a separate confirmation flag. The API key contents are never passed on the command line. Put `AuthKey_<key ID>.p8` in `API_PRIVATE_KEYS_DIR` if using that method. The Xcode account method avoids creating a persistent API key. The upload action rechecks the receipt, IPA and a full-content digest of the signed archive app. It then uses Xcode's separate `-exportArchive` upload destination with internal-only options. Xcode may repackage the archive for upload, so the receipt proves the locally exported IPA and archive content, not byte-for-byte equality with Apple's processed build.

Before starting Xcode, the script writes `UploadOptions.plist` and `upload-attempt.json` in the release output directory. It refuses another attempt from that directory. If Xcode reports an error or the result is unclear, inspect App Store Connect before deciding whether to create a new build and output directory. Do not delete the attempt marker just to retry an unknown upload.

When Xcode exits unsuccessfully, the script stores its standard output and error in `upload.log` beside the attempt marker. The file is created with owner-only permissions and stays outside Git. Use it to distinguish a local signing or account failure from an upload that may have reached Apple. The attempt marker still applies even when a diagnostic log exists.

```sh
MOTE_ASC_API_KEY_ID=... MOTE_ASC_ISSUER_ID=... API_PRIVATE_KEYS_DIR=/private/path/to/keys python3 -B ios/Mote/distribute.py upload --manifest /private/tmp/mote-release.json --inventory /private/tmp/mote-apps.json --output /Users/bilal/projects/mote/target/distribution/0.1.0-1 --confirm-inventory-current --confirm-native-ui-verified --confirm-upload
```

For the signed-in Xcode account method, omit the API-key environment variables and add `--use-xcode-account` to the upload command. If the outcome is unclear, inspect App Store Connect before making another attempt. Do not switch authentication methods blindly after an ambiguous result.

Successful upload is not internal TestFlight availability. Check processing in App Store Connect and assign the build only to an internal group. That is an interim outcome, not a physical-device pass. Install the build on a physical iPad later and repeat the named journeys. Do not enable external testing or App Review.

## Manual GitHub Actions release

The `Mote internal TestFlight` workflow in `.github/workflows/mote-testflight.yml` has only `workflow_dispatch`. Select the `main` branch in GitHub Actions and enter a new build number. The `upload` input defaults to false. A build-only run checks the Rust workspace and produces an unsigned iPad build without Apple credentials. That build is not installable through TestFlight. The `native_ui_verified` input is required only for an upload run and must be set only after the final native iPad Simulator UI checks pass for that commit. An upload run enters the protected Mote release environment, archives and exports a signed internal-only IPA, repeats the live build-number check, then invokes the separate upload command. Neither run submits to App Review or creates a tester group.

Create a GitHub environment named `mote-testflight`. Restrict it to `main` and keep its release secrets in that environment, not repository-wide. Add these Mote-prefixed secrets: `MOTE_ASC_API_KEY_ID`, `MOTE_ASC_ISSUER_ID`, `MOTE_ASC_API_KEY_P8`, `MOTE_CERT_P12_BASE64`, `MOTE_CERT_PASSWORD`, `MOTE_PROFILE_BASE64`, and `MOTE_KEYCHAIN_PASSWORD`. The `.p8` secret is the private key text. The certificate and provisioning profile secrets are base64 encodings of their files. Use a new, dedicated Mote App Store distribution provisioning profile for `FQ6YGD7554.com.beelol.mote`. The workflow rejects a profile for another bundle, a development or ad hoc profile, an expired profile, and a duplicate build number. Keep every secret outside Git. A Mote-prefixed secret name does not make an Apple team API key app-scoped. Apple applies team keys across all apps. Do not reuse another app's key or claim that the workflow's identity checks limit what a stolen team key could access.

Apple Distribution certificates and App Store Connect team API keys may grant team-wide access. A Mote-prefixed secret name does not make the underlying Apple credential app-scoped. The workflow limits its own read and upload targets with the exact Mote Apple ID `6822101041`, bundle ID `com.beelol.mote`, SKU `mote-ipad-001`, team `FQ6YGD7554`, the Mote-only profile and signed IPA verification. Do not reuse an Overseer or AquaFriends provisioning profile or API key. Restrict the API key's role and the GitHub environment as far as Apple and GitHub permit. A different Apple team or an app-scoped credential is needed if credential-level isolation is required.

The workflow requires a macOS runner with Xcode 27. Its live inventory reads all apps visible to the API key and all Mote builds. It fails if the Mote record is missing, an identity collides, or the requested version and build number already exist. It signs with the supplied profile and certificate. It does not create App Store Connect apps, identifiers, certificates, profiles, or API keys. First run build-only to check the hosted runner. Configure the Mote release environment and deliberately upload only when ready. The repository is public, so Actions retains only the release receipt and does not publish the signed IPA as an artifact. A successful upload still needs processing and internal-group confirmation in App Store Connect. Do not retry an ambiguous upload with the same build number.
