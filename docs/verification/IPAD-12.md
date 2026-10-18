# IPAD-12 verification in progress

Date: 2026-10-12, America/Los_Angeles. Environment: macOS with Xcode 27.0, Python 3, local Mote repository, and a GitHub-hosted Xcode 27 runner. The build-only GitHub Actions path has been dispatched. The signed upload path has not.

Static inspection: `.github/workflows/mote-testflight.yml` contains only `workflow_dispatch`. The build job requires `refs/heads/main` and has no Apple credentials. Upload defaults to false. The separate upload job requires the `mote-testflight` environment and an explicit native UI verification input. The Mote identity is fixed at Apple ID `6822101041`, bundle `com.beelol.mote`, SKU `mote-ipad-001`, team `FQ6YGD7554`. The workflow uses only Mote-prefixed release secrets and a Mote-only provisioning profile. It does not reference another app's bundle, profile, SKU or TestFlight group. The public repository does not retain the signed IPA as an Actions artifact.

Command: `python3 -B -m unittest discover -s ios/Mote -p 'test_*.py'`.

Output: `Ran 20 tests ... OK`. The tests cover inventory identity mismatch, duplicate build refusal, Mote-only profile rejection of another bundle, manual export profile binding and the existing distribution guards. No Apple mutation occurred during these tests.

Command: `cargo test --workspace`. Output: passed with exit status 0 when run with local socket access. The first restricted-sandbox attempt failed only because the existing browser fixture could not bind a localhost socket. The rerun completed all workspace and doc tests. Live tests marked `MOTE_LIVE=1` remained ignored.

Command: `cargo clippy --workspace -- -D warnings`. Output: passed with exit status 0 and no warnings.

Command: `ruby -e 'require "yaml"; p YAML.load_file(".github/workflows/mote-testflight.yml").keys'`. Output: `["name", "on", "permissions", "concurrency", "jobs"]`. This is a syntax check, not a hosted GitHub Actions run.

GitHub UI on 2026-10-12: the `mote-testflight` environment was created for this repository. Its deployment branch policy shows `1 branch and 0 tags allowed`, exact branch `main`. It holds only `MOTE_PROFILE_BASE64`, created from the exact Mote profile, and a generated `MOTE_KEYCHAIN_PASSWORD`. It has no variables. A build-only workflow dispatch for `0.1.0 (2)` from commit `54f797f` completed as [run 38336516684](https://github.com/beelol/mote/actions/runs/38336516684). The build job succeeded in 11 minutes 53 seconds. It ran the workspace checks and unsigned iPad build. The `distribute` job was explicitly shown as skipped. No Apple upload occurred. The runner warned that `actions/checkout@v4` used the deprecated Node.js 20 runtime. The workflow moved checkout to `v5` and receipt upload to `actions/upload-artifact@v6`, which use Node.js 24. A second build-only dispatch from commit `2f861a5` completed as [run 38349927919](https://github.com/beelol/mote/actions/runs/38349927919). GitHub reported success for the workspace verification and unsigned Mote iPad build. It reported `distribute` as skipped. The revised checkout action ran successfully. The receipt upload action was not exercised because the upload job was skipped.

Apple Developer portal on 2026-10-12: created the App Store distribution provisioning profile `Mote iOS App Store Connect`, ID `3TG7MVG3GT`, for the exact App ID `FQ6YGD7554.com.beelol.mote`. It uses the team's existing Apple Distribution certificate and expires 2027-08-16. The AquaFriends and Overseer profiles remained separate. The profile was downloaded outside Git. `security cms -D` could not decode this profile on the local Mac. The command `openssl cms -verify -inform DER -noverify -in <Mote profile> -out <temporary plist>` succeeded with `CMS Verification successful`. The command `python3 -B ios/Mote/ci_profile.py --plist <temporary plist> --output-directory <temporary directory> --github-env <temporary file>` returned `Validated Mote-only App Store provisioning profile`. The decoded app identifier was `FQ6YGD7554.com.beelol.mote`. The workflow now uses the working OpenSSL decoder.

App Store Connect UI refreshed on 2026-10-12: the Mote internal group still showed `0.1.0 (1)` as `Testing`, with one tester and one build. Users and Access showed one Account Holder/Admin user. App Store Connect API showed two active team keys named for AquaFriends and Overseer and zero active individual keys. There was no Mote key to configure without creating new access. Apple states that [team API keys apply across all apps](https://developer.apple.com/help/app-store-connect/get-started/app-store-connect-api/), while an individual key inherits the associated user's app access. This workflow currently requires a team key and issuer ID because Xcode's `-authenticationKeyPath` mode requires `-authenticationKeyIssuerID` per local `xcodebuild -help`. A separate team key would have a Mote name but would not be app-scoped. No key was created or reused during this inspection.

Pending: configure the App Store Connect API key ID, issuer, private key, Apple Distribution certificate and certificate password as Mote release secrets. Run one deliberate upload dispatch using a fresh Mote build number. Confirm processing and the internal group in App Store Connect, and confirm other apps and groups are unchanged. Until then this criterion remains open. The hosted build does not prove the signing and upload path works.

Local release tooling on 2026-10-17 archived and verified Mote `0.1.0 (2)` from current `main`
with the exact Mote identity. Its signed-in Xcode account upload failed locally in
`IDEDistributionUploadAccountStep` because Xcode could not access an App Store Connect account for
team `FQ6YGD7554`. The unified log contains no later transport step. The attempt marker is retained,
so the tool will not retry that release directory blindly. This local failure does not verify the
manual GitHub upload path and does not complete IPAD-12.

The release tool now preserves failed Xcode output in an owner-only `upload.log` outside Git. Unit
coverage verifies the diagnostic file content and mode while the existing attempt guard remains
fail-closed.

The signed-in Xcode upload path now inspects Xcode's Apple Account preference list before writing
either upload file. Tests cover empty and populated account lists and prove an empty list invokes no
Xcode mutation, writes no `UploadOptions.plist` and consumes no `upload-attempt.json`. This prevents
the missing-account failure observed for build 2 from consuming another guarded release directory.
It does not claim that a configured account has current access to team `FQ6YGD7554`; Xcode remains
authoritative for that check during upload.

A read-only GitHub environment inspection on 2026-10-17 again listed only
`MOTE_KEYCHAIN_PASSWORD` and `MOTE_PROFILE_BASE64`. The latest two manual workflow runs remained the
successful build-only runs `38336516684` and `38349927919`; both predate current `main`, and neither
ran the upload job. No credential, secret or workflow dispatch was changed during this inspection.
