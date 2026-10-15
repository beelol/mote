# Current development handoff

Updated 2026-10-14 in America/Los_Angeles. This file is a replacement snapshot.

## Current goal

IPAD-14 adds one reusable UIKit text-input bridge beneath Matrix and egui fields. Egui remains the
visual renderer. The bridge now covers generated fields, first-run pairing, offline preview and live
chat. It exposes a 48-point native editing surface with the system keyboard, cursor and selection
state, marked text, dictation, hardware-keyboard events and native copy, cut and paste menus.

The iPad Simulator UI test passes. The full Rust workspace suite and clippy pass. A signed Debug app
is installed on the physical iPad. A Mote-only UI test runner App ID and development profile were
created for physical automation. The physical UI test is waiting because the iPad is locked. After
unlock, rerun or resume the selected UI test, then observe marked-text composition and dictation on
the device. Keep IPAD-14 unchecked until those physical observations are recorded in
`docs/verification/IPAD-14.md`.

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

The App Store Connect record remains Mote Agent, Apple ID `6822101041`. Build `0.1.0 (1)` is in the
internal Testing group. The manual-only `.github/workflows/mote-testflight.yml` workflow remains
guarded and separate from other apps. Its Mote-specific App Store profile exists, but the App Store
Connect API key and Apple Distribution certificate secrets are still absent.

## Verification and next actions

`cargo test --workspace` and `cargo clippy --workspace -- -D warnings` passed after the LAN pairing,
first-run setup and native text bridge changes. The selected Simulator test passed in 26.264 seconds
with result bundle `/private/tmp/mote-native-text-test14.xcresult`. It covered edge focus, the system
keyboard, a hardware cursor edit and native Copy, Cut and Paste.

Finish the physical IPAD-14 run after the owner unlocks the iPad. Then update the evidence, check the
criterion, run the required workspace checks again, commit and push. IPAD-06, IPAD-08, IPAD-12 and
IPAD-13 remain separate open criteria. The memory backend comparison remains a separate goal with
SQLite as the production default. External testing, App Review, public release and cloud relay stay
out of scope.

Preserve unrelated `.DS_Store` and `.superpowers/` files.
