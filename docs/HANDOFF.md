# Current development handoff

Updated 2026-10-17 in America/Los_Angeles. This file is a replacement snapshot.

## Current goal

Close the remaining Mote iPad and local-host delivery criteria on `main`: V0-53, HOST-05,
IPAD-06, IPAD-08, IPAD-12 and IPAD-13. The separate memory comparison and broader Matrix runtime
backlog are out of scope. Keep Mote signing, provisioning, App Store Connect, TestFlight and CI
resources isolated from every other app.

## Current state

IPAD-14 is complete and pushed at `b729d37`. The physical M4 iPad is paired over Wi-Fi as
`00008132-001104661AB9001C` on iPadOS 26.5. Auto-lock is disabled. The current Mote-only signed
closeout test bundle is `/private/tmp/mote-physical-theme-signed.5gKQa2`.

The host lifecycle command is implemented as `moted host start|status|restart|stop`. It manages
only `com.mote.moted`, preserves configuration and data on stop, and retains
`moted install|uninstall` for persistent login management. Exact command-vector tests pass. A live
restart changed the process ID from 97330 to 71052 while retaining event sequence 9. The change is
committed and pushed on `main` at `cb27915`.

HOST-05 has complete automated reconnect, replay, resync, state and isolation coverage. Its live
physical iPad connection-state journey now passes. The literal Mac sleep and wake run remains open.

The foreground daemon has been replaced by the installed `com.mote.moted` LaunchAgent against the
production Mote data directory. It is running from the repository debug binary. A controlled
`SIGTERM` changed its launchd run count from 1 to 2 and retained event sequence 9. V0-53 still
requires the literal logout and login check before uninstall.

IPAD-06 now has 32 passing bounded physical gallery cases covering every page, every control at
the center and four inset edges, both orientations, native text input, all list actions and all
select options. Exact physical Light and Dark palette checks pass in
`/private/tmp/mote-theme-light-green2.xcresult` and `/private/tmp/mote-theme-dark-green.xcresult`.
System Reduce Motion enabled and disabled journeys pass in
`/private/tmp/mote-reduce-motion-on-green.xcresult` and
`/private/tmp/mote-reduce-motion-off-green3.xcresult`. Split view, paired background recovery, the
local-network privacy prompt and actual VoiceOver navigation remain open.

A reviewed physical-only exhaustive journey covers all eight pages, an independently observed
native control and dropdown inventory, all 20 list actions, every select option, every control at
center and four inset edges, the Preview note field and both orientations. The first
driver-coordinated paired background rerun failed before the host stop marker. Its retained
screenshot shows Mote still full-screen. SpringBoard exposed only system and app-switcher backing
layers, including `AppSwitcherContentView` and Mote's card, with no Home icons. The harness
correctly left the host running and did not claim a background pass. The reviewed correction keeps
the exact observable gate and explicitly activates SpringBoard after the Home action. Rebuild and
rerun the exact stop, offline, restart and recovery sequence. The first rebuilt attempt then
stopped before backgrounding because the fresh install had no selected host. The next attempt
exposed the real iPad local-network privacy sheet. Its retained recording visibly includes Mote's
purpose text and the native Don't Allow and Allow actions. The harness now captures that sheet,
accepts Allow and repairs the fresh-install pairing before entering the unchanged recovery journey.
The repaired setup now selects the non-link-local Wi-Fi advertisement, focuses the native secure
pairing field, enters the fresh code and activates the enabled native Pair button. The host never
reports a pending request, so the iPad has not yet completed the first TCP pairing handshake. The
background stop and recovery phase has therefore not started. Trace port 47810 at the client and
server boundary with a newly generated code before changing pairing behavior.
The first boundary trace used a code generated immediately before the run. The app selected
`192.168.50.184:47810`, entered all 155 code characters and tapped Pair. Repeated socket snapshots
showed no new TCP connection after that tap, while three older authenticated iPad connections
remained established. This localizes the current failure to the client before `Socket::connect`,
most likely the first-run validation path. The in-flight result bundle is
`/private/tmp/mote-background-autolock-off-trace.xcresult`. Inspect its final visible status before
changing product behavior.
The final device screenshot is `/private/tmp/mote-pairing-final-status.png`. It shows the generic
pairing failure after the code field was cleared. Source inspection found the first-run worker
preferred a Keychain credential whenever one survived reinstall, even when the owner supplied a
fresh pairing offer. That path attempted a normal session instead of `/pair`. A regression test
first failed because no precedence rule existed. The implementation now selects the explicit
owner-present offer before any saved credential, and the focused Rust test passes. Rebuild and
sign the physical app and runner, then repeat pairing with a new code.
The rebuilt binary was explicitly installed because `test-without-building` had only activated the
already installed process. The replacement launched online immediately with its preserved valid
credential, proving the rebuilt client and local Wi-Fi connection. The corrected physical
background journey passes in `/private/tmp/mote-background-state-gate-green2.xcresult`. It observed
visible SpringBoard, a hittable Home icon and Mote in the public `RunningBackground` state. It then
stopped only `com.mote.moted`, captured the exact host offline, restarted the same LaunchAgent,
recovered online with cursor 9 and the prior conversation, created a new event at cursor 12 and
proved exactly-once replay after relaunch. Stale post-Home Mote hit-test values are retained only as
diagnostics. A preceding run exposed and corrected a harness-only full-screen OCR comparison that
included the changing iPad status-bar clock.

IPAD-13 now has a production numeric `budgets.router` slider binding. The corrected live benchmark
passed activation with 197 of 200 first try, 198 after retry and 3 of 3 sliders first try. Its
activation file is `/private/tmp/mote-ipad13-attestation-20261017.JRF2iJ/surface-activation.json`.
The reviewed harness uses four fresh production hosts so untouched timing evidence is separate from
immediate first-frame action evidence. One live attempt failed before launch because the sandbox
could not reach CoreSimulator. Later attempts exposed a generation-status transition and proved
that the live chat viewport grew as messages arrived, translating every surface stroke. The prime
is restored with an explicit refreshed-surface wait and 500 ms settlement window. The production
chat viewport now reserves its existing 32 percent maximum height and disables vertical auto-shrink
so generated surfaces keep stable absolute geometry. The independent unchanged-stroke and
replacement timing assertions remain strict. The final four-host run passed in 62.43 seconds with
zero failures. Normal transition captured 2,297 particles on the first frame, 2,289 at the midpoint
and zero at 133 ms. Reduced Motion captured zero particles, four outline pulses on the first and
midpoint frames and zero at 133 ms. Both action paths passed immediate slider interaction, integer
persistence, refreshed state, no-repeat and stale-revision rejection. Durable frames and traces are
under `docs/verification/assets/IPAD-13/production-slider-20261017`. IPAD-13 is checked complete.

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

The physical reconnect observer then passed in 56.175 seconds. It captured the same host online,
offline after stopping only the Mote daemon, and online again with the same cursor after restarting
the daemon. The result bundle is `/private/tmp/mote-physical-reconnect1.xcresult`.

The native system appearance bridge is implemented but uncommitted. The system Reduce Motion bridge
is committed at `0a703d2`. The iPad setting was independently queried before both physical motion
runs and is currently off.

Next complete split-view and actual VoiceOver evidence. The local-network privacy prompt is already
retained from the physical first-run journey. Update the evidence before committing. Finish literal Mac sleep and wake,
logout and login, isolated TestFlight credentials, a manual upload and TestFlight device journeys.

## Required delivery loop

Record completed evidence in `docs/verification/<ID>.md`. Do not weaken a Verify clause. Run
`cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before every commit. Make
small conventional commits on `main` and push each verified change. Preserve the unrelated
`.DS_Store` and `.superpowers/` files.
