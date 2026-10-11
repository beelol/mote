# Current development handoff

Updated 2026-10-11 in America/Los_Angeles. This file is a replacement snapshot.

## Scope and authority

The active goal is a Rust immediate-mode Matrix client running in the iPad Simulator over an
authenticated localhost Mote gateway. The root README Ready for Go section is the active board.
HOST-01 through HOST-05 and IPAD-01 through IPAD-05 are in scope. V0-53 is independent and needs
the owner's literal logout and login. IPAD-06 through IPAD-08, physical iPad, signing, App Store
Connect, TestFlight and a cloud relay are deferred. Do not start owner-only work without authority.
Do not prompt the owner before 9am America/Los_Angeles. Preserve untracked `.DS_Store` and
`.superpowers/`.

The approved design is `docs/superpowers/specs/2026-10-11-ipad-host.md`. The implementation plans
are `docs/superpowers/plans/2026-10-11-ipad-simulator-host.md`,
`docs/superpowers/plans/2026-10-11-ipad-simulator-ui.md` and
`docs/superpowers/plans/2026-10-11-ipad-agent-browser.md`. The owner requested small verified
Conventional Commits on `main` and pushes to configured `origin`.

## Delivered and verified

HOST-01, HOST-02 and HOST-03 are checked with evidence in `docs/verification/`. The gateway uses
MOTE1 and Noise with pinned host identity. Per-host state, encrypted requests, typed outcomes,
redaction, ledger use and local-only model policy are covered by tests. The host plan Tasks 1
through 7 are pushed. They include Bonjour advertisement, address-change identity proof,
durable replay, authoritative snapshot and resync, and an external paired-daemon harness.
HOST-04 stays open for live Simulator discovery and purpose-text verification. HOST-05 stays open
for literal Mac sleep and wake, which is unsafe unattended without a configured wake mechanism.

IPAD-01 is checked with `docs/verification/IPAD-01.md`. The shared Rust client core builds for
both Apple targets. The Xcode 27 native shell launched on an iOS 27 iPad Simulator. XCUITest passed
startup, and the driver independently relaunched the app.

UI plan Task 4 delivered eight validated Matrix v1 gallery pages and a checked-in Dashboard
fixture in `8798d92`. The host-side compiler test pins the fixture. The frozen v1 contract has no
text input, so the shell Preview note composer supplies the IPAD-02 text-entry route.

UI plan Task 5 delivered immediate new-layout hit testing, stable unchanged strokes, 120 ms red
square-pixel border materialization, an 8,192 particle cap and Reduced Motion outline pulse.
Commits `2763573`, `1c7cd86`, `cfe0d8b` and `551e254` are pushed. Independent review found and
fixed new-slider drag ownership, moved old-bounds state and Escape cancellation including
simultaneous release. Driver workspace tests, strict clippy and Apple Simulator core check passed.

UI plan Task 6 has a reviewed interactive slice in `42d5044` and test hardening in `8a22702`,
both pushed. The iPad shell selects all eight pages immediately, exposes local typed interactions,
has a scrollable 20-row List and a keyboard-safe Preview note composer. Both iOS 27 iPad sizes
passed final seven-test native suites in portrait and landscape with zero failures. Workspace
tests passed 615 cases with zero failures and two ignored; strict clippy and Apple Simulator target
check passed. The driver independently reran the Rust checks. Evidence and five retained
screenshots are in `docs/verification/IPAD-02.md`. IPAD-02 remains open. Simulator screen recording
is variable-rate and cannot prove first, midpoint and final transition frames at 60 fps, sustained
60 fps or temporal Reduced Motion pulse.

## Current work and next action

`main` and `origin/main` contain the UI work through `dda4478`. `docs/HANDOFF.md` is the only
controller-owned tracked work in progress. The UI plan ledger is
`.superpowers/sdd/2026-10-11-ipad-simulator-ui/progress.md`. The UI implementer
`ipad_sim_gallery_impl` owns Task 6 follow-up code and must not edit this handoff or push.

Independent re-review approved `8a22702` with no Critical or Important issue. It found one Minor
assertion gap: substring matching for `Changed volume: 0.0` could accept `0.01`. The original
implementer fixed it in `dda4478`, with a red negative case and three green focused native tests.
Scoped re-review found no remaining issue. The driver independently reran workspace tests, strict
clippy and Apple Simulator target check with exit 0, then pushed the commit.

The implementer is now building an opt-in Simulator-only GPU screenshot and repaint-cadence probe. The
probe must retain raw frame sequence, timestamps and images, not override egui input time, and
distinguish app update, renderer paint callback, GPU copy and unknown present/scanout timing.
Report rendered-frame cadence without claiming display scanout timestamps. Check IPAD-02 only if
the full live temporal Verify clause is demonstrated. Review every new commit, independently run
workspace tests and strict clippy, then push verified commits. Next UI Task 7 connects the shell
to the authenticated localhost gateway for live chat, Matrix and cursor recovery. Then follow the
inferred UI and contained browser plan for IPAD-04 and IPAD-05.

## Limits

The pinned iOS AccessKit backend exposes one window TextView to XCUITest. Native journeys use
Vision-recognized text, exact state assertions, measured hit regions and screenshot evidence.
Do not claim individual accessibility nodes from that workaround. Xcode debugger diagnostics in
successful native logs are documented non-failing tooling noise.

The desktop queries host identity at startup. If the host is unavailable, cursor persistence waits
for recovery attachment. Restarted approvals remain visible but cannot re-execute their original
file intent. Interrupted remote request IDs remain indeterminate until reconciled. Never weaken
those fail-closed behaviors silently. Do not read the default Chrome profile or keychain item.
