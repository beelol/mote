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
MOTE1 and Noise with pinned host identity. Its per-host state, encrypted requests, typed outcomes,
redaction, ledger use and local-only model policy are covered by tests. The host plan Tasks 1
through 7 are pushed. They include Bonjour advertisement, address-change identity proof,
durable replay, authoritative snapshot and resync, and an external paired-daemon harness.
HOST-04 stays open for live Simulator discovery and purpose-text verification. HOST-05 stays open
for literal Mac sleep and wake, which is unsafe unattended without a configured wake mechanism.

IPAD-01 is checked with `docs/verification/IPAD-01.md`. The shared Rust client core builds for
both Apple targets. The Xcode 27 native shell launched on an iOS 27 iPad Simulator. XCUITest passed
startup, and the driver independently relaunched the app. The screenshot is
`docs/verification/assets/IPAD-01-simulator-gallery.png`.

UI plan Task 4 delivered eight validated Matrix v1 gallery pages and a checked-in Dashboard
fixture in `8798d92`. The host-side compiler test pins the fixture. The frozen v1 contract has no
text input, so IPAD-02 text entry must use the legacy Field or shell composer.

UI plan Task 5 delivered immediate new-layout hit testing, stable unchanged strokes, 120 ms red
square-pixel border materialization, an 8,192 particle cap and Reduced Motion outline pulse.
Commits `2763573`, `1c7cd86`, `cfe0d8b` and `551e254` are pushed to `origin/main`. Independent
review found and fixed new-slider drag ownership, moved old-bounds drag state and Escape
cancellation including simultaneous release. The final scoped review found no blocking issue.
The driver independently reran `cargo test --workspace`, strict workspace clippy and Apple
Simulator core check with exit 0 before push. IPAD-02 remains open until live Simulator acceptance.

## Current work and next action

`main` and `origin/main` contain UI Task 5 through `551e254`. `docs/HANDOFF.md` is being replaced
and is the only intended tracked work in progress. The UI plan ledger is
`.superpowers/sdd/2026-10-11-ipad-simulator-ui/progress.md`. The host ledger is the corresponding
`2026-10-11-ipad-simulator-host` directory. They are ignored work notes, not deliverables.

Next, commit and push this handoff after workspace tests and strict clippy. Dispatch a fresh UI
implementation subagent for Task 6 of the UI plan from pushed `551e254`. Task 6 must wire the
eight page choices into the native shell and drive every page, scroll, text entry and controls on
two iPad Simulator sizes in both orientations. Capture first, midpoint and final transition
frames, frame timing and Reduced Motion evidence. Keep IPAD-02 open unless its whole Verify clause
is demonstrated. Review the subagent's diff before accepting it. Then proceed to UI Task 7 for
live authenticated localhost pairing, chat, dashboard interaction, reconnect and host isolation.
After that, implement the inferred UI and contained browser plans for IPAD-04 and IPAD-05.

## Limits

Do not claim live gallery controls or sustained 60 fps from core tests alone. The desktop queries
its host identity at window startup. If the host is unavailable, cursor persistence waits for
recovery attachment. Restarted approvals remain visible but cannot re-execute their original file
intent. Interrupted remote request IDs remain indeterminate until reconciled. Never weaken those
fail-closed behaviors silently. Do not read the default Chrome profile or its keychain item.
