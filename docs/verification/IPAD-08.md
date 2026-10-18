# IPAD-08 partial verification

Status: the processed internal TestFlight build is installed on the physical iPad. The criterion
remains unchecked until the named pairing, live chat, generated Matrix UI, browser approval,
reconnect and host-switch isolation journeys pass against the TestFlight app.

## Environment

Date: 2026-10-17, America/Los_Angeles.

- Device: iPad Pro 11-inch M4, `iPad16,4`.
- Device UDID: `00008132-001104661AB9001C`.
- Device OS: iPadOS 26.5, build `23F77`.
- TestFlight app: `com.apple.TestFlight`, version 4.4.0.
- Mote App Store Connect app: `6822101041`.
- Mote display name: Mote Agent.
- Mote bundle ID: `com.beelol.mote`.
- Mote version: `0.1.0`.
- Mote build: `1`.
- Developer team: `FQ6YGD7554`.
- Internal group: Testing.

## Internal TestFlight installation

Command:

```text
xcodebuild test-without-building \
  -xctestrun /private/tmp/mote-testflight-install-device/Build/Products/Mote_iphoneos27.1-arm64.xctestrun \
  -destination 'platform=iOS,id=00008132-001104661AB9001C' \
  -only-testing:MoteUITests/MoteUITests/testPhysicalInstallMoteFromInternalTestFlight \
  -resultBundlePath /private/tmp/mote-ipad08-testflight-install12.xcresult
```

Output:

```text
Test Case '-[MoteUITests testPhysicalInstallMoteFromInternalTestFlight]' passed (27.141 seconds).
Executed 1 test, with 0 failures (0 unexpected).
** TEST EXECUTE SUCCEEDED **
```

The journey launched the signed-in TestFlight app, opened its sidebar, selected Mote Agent, closed
the visible sidebar control and selected the exact TestFlight install action. TestFlight reported
that a version was already installed and asked whether to replace it with the test version. The
journey confirmed the replacement. TestFlight then exposed the exact
`TestFlight.offerButton.open` control.

The retained [screenshot](assets/IPAD-08/testflight-install-20261017/installed.png),
[role tree](assets/IPAD-08/testflight-install-20261017/role-tree.txt) and
[observations](assets/IPAD-08/testflight-install-20261017/observations.txt) show Mote Agent,
version `0.1.0`, build `1`, developer STATION 42 and the terminal Open state.

The TestFlight record remains assigned only to the internal Testing group. No external testing,
App Review or public release action was enabled during this run.

## Build 1 LAN refusal

The first shipped-app pairing attempt targeted the installed `com.beelol.mote` app explicitly.
The reusable UI-test harness defaults to the scheme app and accepts a nonempty
`MOTE_TARGET_BUNDLE_IDENTIFIER` only when a distribution journey opts in. The focused selector
test and generic iOS test build pass. The physical command was:

```text
xcodebuild test-without-building \
  -xctestrun /private/tmp/mote-testflight-install-device/Build/Products/Mote_iphoneos27.1-arm64.xctestrun \
  -destination 'platform=iOS,id=00008132-001104661AB9001C' \
  -only-testing:MoteUITests/MoteUITests/testPhysicalTestFlightPairsAndShowsLiveHost \
  -resultBundlePath /private/tmp/mote-ipad08-testflight-pair1.xcresult
```

The test failed before pairing. The visible shipped app reported:

```text
Offline gallery
Host setup failed: invalid Simulator host
```

The archived build 1 executable contains the literal `invalid Simulator host`. That build was
uploaded on 2026-10-12. LAN pairing landed later in commit `35d90d0` on 2026-10-14 and replaced the
loopback-only validation with local-range validation and Bonjour first-run pairing. Build 1
therefore cannot satisfy IPAD-08. A new internal TestFlight build from current `main` is required.
This is retained negative evidence. It does not weaken or complete the criterion.

## Build 2 archive and pre-transport upload failure

Current `main` was archived as Mote `0.1.0 (2)` at
`target/distribution/0.1.0-2a`. The guarded release tool verified bundle
`com.beelol.mote`, team `FQ6YGD7554`, App Store Connect app `6822101041`, the signed
archive and the internal-only IPA. The IPA SHA-256 was
`380297e88fae4a5df3d3bf6668a22fece5f3c64a7ba05051b2fb69b0aa19a602`.

The explicit signed-in Xcode account upload attempt exited with status 70. A narrow unified-log
inspection for the exact attempt window reported:

```text
Failed to find an account with App Store Connect access for team
<IDEProvisioningBasicTeam ... teamID='FQ6YGD7554' ...>
Step failed ... "Failed to Use Accounts" ...
App Store Connect access for “FQ6YGD7554” is required.
```

Xcode failed in `IDEDistributionUploadAccountStep` before an Apple transport step began. This is
evidence that the local signed-in account was unavailable during the attempt, not evidence that
build 2 reached TestFlight. The release directory retains `upload-attempt.json`, and the guard
forbids a blind retry. App Store Connect must be inspected before choosing the next build number.

App Store Connect was inspected after the failed attempt on 2026-10-17. The Mote Agent iOS Builds
table showed only version `0.1.0`, build `1`, with status `Testing`, group `Internal Testing`, one
invite and one install. Build 2 was absent. This confirms the pre-transport diagnosis and makes
build 3 the next reserved number. No group, tester or app record was changed during this check.

Xcode 27.1 Apple Accounts settings showed `Sign in to your Apple Account` and no configured account.
The login keychain also exposed zero valid code-signing identities while the Mac session was locked.
A build 3 manifest and current inventory snapshot were prepared outside Git, but its preflight
correctly stopped before archive with `No local Apple Distribution signing identity for the selected
team`. No upload was attempted.

## On-device additional host pairing

Commit `652655e` exposes a touch-sized `Pair another host` action in the live host header. It reuses
the Bonjour discovery and native pairing-code field, retains the current live session while the
pairing view is open and exposes `Cancel` instead of leaving for the offline gallery. A new host is
not written to `live-hosts.json` until its own authenticated connection reaches `Online`. An already
online saved host cannot make the new pairing appear complete. Duplicate identities and a ninth host
are rejected.

Command:

```text
cargo test -p mote-ios
```

Output:

```text
46 library tests passed.
31 integration tests passed.
5 native Simulator tests were ignored because MOTE_LIVE was not set.
0 tests failed.
```

The localhost-dependent recovery and loopback tests ran with local socket access. The unsigned
physical target was then rebuilt with:

```text
xcodebuild -quiet -project ios/Mote/Mote.xcodeproj -scheme Mote \
  -configuration Debug -destination 'generic/platform=iOS' \
  -derivedDataPath /private/tmp/mote-add-host-device-closeout \
  CODE_SIGNING_ALLOWED=NO build-for-testing
```

The build exited 0. Xcode emitted the existing iOS 16 versus XCTest iOS 17 linker warnings. This
proves the production path and physical target compile. It does not replace the required two-host
journey against the installed TestFlight build.

## Remaining verification

Sign in to Xcode and unlock the login keychain, then archive and upload build 3 from current `main`.
Assign the processed build only to the
internal Testing group and install it. Pair the first Mac, use `Pair another host` to pair a second
fixture host without developer-only fixture injection, then repeat live chat, generated Matrix UI,
browser approval, reconnect and host-switch isolation against the installed `com.beelol.mote`
TestFlight app. Retain each physical result bundle and the required screenshots, role trees and
observations before checking IPAD-08.
