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

## Remaining verification

Upload and process a new build from current `main`, assign it only to the internal Testing group,
install it, then repeat pairing, live chat, generated Matrix UI, browser approval, reconnect and
host-switch isolation against the installed `com.beelol.mote` TestFlight app. Retain each physical
result bundle and the required screenshots, role trees and observations before checking IPAD-08.
