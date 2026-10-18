# Current development handoff

Updated 2026-10-17 in America/Los_Angeles. This file is a replacement snapshot.

## Goal

Close V0-53, HOST-05, IPAD-06, IPAD-08, IPAD-12 and IPAD-13 on `main`. Keep every
signing, App Store Connect, TestFlight and CI resource isolated to Mote. The memory
comparison and broader Matrix runtime backlog remain out of scope.

## Completed and pushed

- `cb27915` adds `moted host start|status|restart|stop` and verifies lifecycle isolation.
- `771bf48` prepares the exhaustive physical iPad UI journeys.
- `0a703d2` honors system Reduce Motion.
- `5dbf6e9` completes IPAD-13, adds system appearance handling, fixes fresh pairing
  precedence and verifies physical background stop, recovery and exactly-once replay.

The physical iPad is `00008132-001104661AB9001C`, an iPad Pro 11-inch M4 running
iPadOS 26.5. It is paired over Wi-Fi, connected and unlocked. Auto-lock is disabled.
The installed development app connects to `com.mote.moted` at `192.168.50.184:47810`.

## Completed physical IPAD-06

All required physical journeys now pass. The two-app test opens Safari, selects the
native `Left and Right` layout action and proves Mote narrows from 834 to 412 points
while Safari occupies the other non-overlapping 412-point half before exercising
every gallery page and control. The passing bundle is
`/private/tmp/mote-ipad06-split-green6.xcresult`.

The VoiceOver test handles the first-run tutorial and notification sheet, normalizes
Mote to a full window, verifies unique native names and reading order on all eight
pages, changes the toggle by double tap, adjusts the slider by swipe up and records
the navigation swipe sequence. The final source-matched passing bundle is
`/private/tmp/mote-ipad06-voiceover-green12.xcresult`. A later experimental attempt
to prove every swipe by activation failed honestly and was not adopted. VoiceOver
was restored to off and the disabled state was retained. `docs/verification/IPAD-06.md`
contains the split and VoiceOver commands, outputs, device details, role trees and
screenshots. The README IPAD-06 checkbox is complete. The final elevated workspace
test and clippy gates passed before the pending commit.

## Remaining closeout

HOST-05 still needs the literal Mac sleep and wake run. The owner deferred it to an
isolated test session. Do not run Mac sleep as part of ordinary development. V0-53
still needs literal logout and login before uninstall. IPAD-08 has installed Mote Agent
through internal TestFlight and still needs the named device journeys against that app.
IPAD-12 needs Mote-only upload credentials and one manual workflow upload from `main`.

The first direct pairing attempt against installed TestFlight build 1 failed with the visible
message `Host setup failed: invalid Simulator host`. The archived build 1 executable contains that
literal. It was uploaded before LAN pairing commit `35d90d0`. Build 1 cannot connect to the Mac over
Wi-Fi. Upload and install a new internal build from current `main` before rerunning IPAD-08.

Mote Agent is App Store Connect app `6822101041`, bundle ID `com.beelol.mote`, SKU
`mote-ipad-001`, team `FQ6YGD7554`. Build `0.1.0 (1)` is processed and assigned
only to the internal Testing group. Mote Agent `0.1.0 (1)` is installed through TestFlight
on the physical iPad. The passing installation bundle is
`/private/tmp/mote-ipad08-testflight-install12.xcresult`.

GitHub environment `mote-testflight` is restricted to exact branch `main`. It has
only `MOTE_PROFILE_BASE64` and `MOTE_KEYCHAIN_PASSWORD`. The missing isolated
upload secrets are `MOTE_ASC_API_KEY_ID`, `MOTE_ASC_ISSUER_ID`,
`MOTE_ASC_API_KEY_P8`, `MOTE_CERT_P12_BASE64` and `MOTE_CERT_PASSWORD`. Do not
reuse unproven credentials from other apps.

## Delivery loop

Record evidence in `docs/verification/<ID>.md`. Do not weaken Verify clauses. Run
`cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before every
commit. Make small conventional commits directly on `main` and push each one.
Preserve unrelated `.DS_Store` and `.superpowers/`.
