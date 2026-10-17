# Current development handoff

Updated 2026-10-16 in America/Los_Angeles. This file is a replacement snapshot.

## Current goal

IPAD-14 adds one reusable UIKit text-input bridge beneath Matrix and egui fields. Egui remains the
visual renderer. The bridge covers generated fields, first-run pairing, offline preview and live
chat. It exposes a 48-point native editing surface with the system keyboard, cursor and selection
state, marked text, dictation, hardware-keyboard events and native copy, cut and paste menus.

Automated physical-device verification now passes for edge focus, typing, cursor movement and the
native Copy, Cut and Paste flows. A launch-only UIKit sequence now verifies that marked text survives
multiple egui frames before commit and that the documented `UITextField` dictation fallback reaches
the field. The same sequence passes in the iPad Simulator. Its physical-iPad run is prepared and is
waiting for the locked device to become available.

## Current implementation state

The Mac daemon listens on local IPv4 interfaces and Bonjour advertises it on the LAN. Incoming
gateway peers are restricted to loopback, private and link-local ranges. The iPad accepts only local
discovery endpoints. First-run setup discovers hosts, validates the selected fingerprint against the
pairing offer and waits for authenticated completion. Owner pairing commands are available through
the local daemon socket. `MOTE_GATEWAY_BIND` exists so isolated integration tests can use an
ephemeral local port while the real daemon owns port 47810.

The physical device is iPad Pro 11-inch (M4), UDID `00008132-001104661AB9001C`, iPadOS 26.5. The
app bundle is `com.beelol.mote`. The Mote-only test plug-in bundle is
`com.beelol.mote.xctrunner` with profile `Mote UI Tests Development`. The runner host bundle is
`com.beelol.mote.xctrunner.xctrunner` with profile `Mote UI Test Runner Host Development`. The app
and runner are installed on the iPad. Do not reuse another app's provisioning resources.

The physical test runner cannot set the device pasteboard directly. The app accepts the test-only
launch argument `--mote-test-pasteboard <text>` and seeds the system pasteboard before Rust starts.
This makes native paste verification deterministic without changing the normal application flow.
Commit `ca5d947` contains that change.

The App Store Connect record remains Mote Agent, Apple ID `6822101041`. Build `0.1.0 (1)` is in the
internal Testing group. The manual-only `.github/workflows/mote-testflight.yml` workflow remains
guarded and separate from other apps. Its Mote-specific App Store profile exists, but the App Store
Connect API key and Apple Distribution certificate secrets are still absent.

## Verification and next actions

`cargo test --workspace` and `cargo clippy --workspace -- -D warnings` passed after the deterministic
paste test change. The physical UI test passed in 24.743 seconds with result bundle
`/private/tmp/mote-native-text-green-signed2.xcresult`. It covered right-edge focus, the system
keyboard, typing, hardware-keyboard cursor movement and native Copy, Cut and Paste.

An earlier Simulator run passed in 29.122 seconds with result bundle
`/private/tmp/mote-native-text-sim15.xcresult`. The new composition and dictation-fallback test passed
in 8.099 seconds with result bundle `/private/tmp/mote-native-text-green-sequence2.xcresult`. A
combined rerun confirmed the new test again, but the older edit-menu test stopped because the
Simulator software keyboard is hidden. Toggling Device Hub's software keyboard setting requires
owner confirmation. This does not invalidate the previous passing Simulator result.

The updated physical test build is signed with the existing Mote-only profiles at
`/private/tmp/mote-native-text-physical-marked1`. Its active `xcodebuild test-without-building` run
is waiting at device preflight because the iPad locked itself. Unlock the iPad before that process
times out. After it passes, record the result in `docs/verification/IPAD-14.md`, check the README
criterion, rerun the required workspace checks, commit and push.

The requested Mac host lifecycle command is a bounded follow-up. The proposed interface is
`mote host start|status|restart|stop`. It should manage only Mote's launchd service, enable LAN
advertising, show the reachable address and shut down cleanly. Implementation must wait for explicit
approval of that short design under the repository workflow.

IPAD-06, IPAD-08, IPAD-12 and IPAD-13 remain separate open criteria. The memory backend comparison
remains a separate goal with SQLite as the production default. External testing, App Review, public
release and cloud relay stay out of scope.

Preserve unrelated `.DS_Store` and `.superpowers/` files.
