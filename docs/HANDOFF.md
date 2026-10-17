# Current development handoff

Updated 2026-10-17 in America/Los_Angeles. This file is a replacement snapshot.

## Current goal

Close the remaining Mote iPad and local-host delivery criteria on `main`: V0-53, HOST-05,
IPAD-06, IPAD-08, IPAD-12 and IPAD-13. The separate memory comparison and broader Matrix runtime
backlog are out of scope. Keep Mote signing, provisioning, App Store Connect, TestFlight and CI
resources isolated from every other app.

## Current state

IPAD-14 is complete and pushed at `b729d37`. The physical M4 iPad is connected as
`00008132-001104661AB9001C` on iPadOS 26.5. The prior Mote-only signed test bundle is at
`/private/tmp/mote-native-text-physical-marked2`. A temporary copy at
`/private/tmp/mote-ipad06-fixed-signed` contains the corrected gallery locator test binary.

The host lifecycle command is a bounded proposed change. The presented interface is
`moted host start|status|restart|stop`. It manages only `com.mote.moted`, preserves configuration
and data on stop, and retains `moted install|uninstall` for persistent login management. Product
code must wait for explicit owner approval of that short design.

HOST-05 has complete automated reconnect, replay, resync, state and isolation coverage. The literal
Mac sleep and wake run and live iPad connection-state evidence remain open. A physical-iPad Wi-Fi
interruption and recovery may provide the live disconnect evidence, but it does not replace the
criterion's explicit Mac sleep and wake clause.

IPAD-06 now has a passing physical gallery run in portrait and landscape. Physical native
accessibility elements, reduced-motion launch, foreground survival, native text input and a
landscape Dashboard select also pass. Literal center and edge taps for every control, split view,
paired background recovery, the local-network privacy prompt, system light and dark appearance,
the system Reduce Motion setting and actual VoiceOver navigation remain open.

IPAD-13 has headless transition timing and paired Simulator integration evidence. Its live paired
replacement captures and immediate changed-control action remain open. A UI audit found a real
Reduce Motion gap. `MoteApp::draw` applies the transition setting only on the offline gallery path.
First-run and live paired rendering return before it is applied. The iOS system Reduce Motion
preference is also not bridged into Rust. Fix this before claiming physical Reduce Motion or paired
IPAD-13 evidence.

Mote Agent is App Store Connect app `6822101041`, bundle ID `com.beelol.mote`, SKU
`mote-ipad-001`, team `FQ6YGD7554`. Build `0.1.0 (1)` is processed and assigned only to the internal
Testing group. IPAD-08 still requires installing through TestFlight and repeating the named
journeys. TestFlight is not installed on the connected iPad.

IPAD-12 still requires isolated Mote CI credentials and one manual upload run from `main`. GitHub
environment `mote-testflight` is restricted to exact branch `main`. It contains only
`MOTE_PROFILE_BASE64` and `MOTE_KEYCHAIN_PASSWORD`. Build-only runs `38336516684` and `38349927919`
passed and skipped distribution. The missing upload secrets are `MOTE_ASC_API_KEY_ID`,
`MOTE_ASC_ISSUER_ID`, `MOTE_ASC_API_KEY_P8`, `MOTE_CERT_P12_BASE64` and `MOTE_CERT_PASSWORD`.
The Mac has a team Apple Distribution identity and files named `AuthKey_L783ML8SXN.p8` and
`dist.p12`, but they are not proven Mote-owned. Do not reuse or upload them without verifying their
scope and provenance.

## Latest verified work

The full physical gallery and locator regression passed with zero failures in
`/private/tmp/mote-ipad06-gallery-fixed2.xcresult`. The gallery run took 267.002 seconds. It visited
all eight pages, exercised the existing control journeys, scrolled to and activated item 20,
entered text with the system keyboard and repeated in portrait and landscape. The focused locator
test took 0.039 seconds. `docs/verification/IPAD-06.md` records the commands and results.

The live daemon advertises the same paired host identity through Bonjour. A normal physical app
launch migrated its saved endpoint from `169.254.9.119:47810` to the Mac Wi-Fi endpoint
`192.168.50.184:47810` without repairing and preserved cursor sequence 9.

The physical timing probe is invalid because `--mote-timing-probe` is intentionally Simulator-only.
Do not include it in physical selections. The separate physical reduced-motion launch test already
passed, but it is not proof of the iPad system preference.

Next add literal center and edge coverage, physical appearance, split-view, background,
local-network permission and VoiceOver evidence. Fix Reduce Motion propagation. Complete the live
paired transition capture. Then finish TestFlight and the hosted upload.

## Required delivery loop

Record completed evidence in `docs/verification/<ID>.md`. Do not weaken a Verify clause. Run
`cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before every commit. Make
small conventional commits on `main` and push each verified change. Preserve the unrelated
`.DS_Store` and `.superpowers/` files.
