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
Conventional Commits on `main` and pushes to configured `origin`. A current safety review rejected
new pushes to `https://github.com/beelol/mote.git` pending explicit destination approval from the
owner. Do not route around that rejection. Continue local verified commits and await approval.

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

IPAD-02 is checked with `docs/verification/IPAD-02.md`. The offline iPad gallery has eight
selectable Matrix v1 pages, a scrollable List and a keyboard-safe Preview note composer. The
frozen v1 contract was not expanded for text entry. Both iOS 27 iPad sizes passed final native
interaction suites in portrait and landscape. Normal motion uses immediate new hit targets and
120 ms red square-pixel border materialization. Reduced Motion swaps immediately with a fading
outline pulse. The opt-in Simulator probe captured distinct GPU images from each live transition
at 0, about 66.7 and about 133.3 ms and measured at least 60 renderer paint encodings per second
over ten seconds in eight device, orientation and motion combinations. Controlled core tests
prove exact 120 ms completion. The first available live 60 Hz frame after that boundary is about
133.3 ms. The acceptance decision combines these complementary facts and explicitly does not
claim display scanout timestamps. Independent review recalculated the raw reports and inspected
the images. The driver independently reran workspace tests, strict clippy and Apple target check
with exit 0. The gallery, test hardening and timing-probe commits through `b17b9f9` are pushed.

## Current work and next action

`main` and `origin/main` contain IPAD-02 acceptance commit `33e52a6`. The UI
plan ledger is `.superpowers/sdd/2026-10-11-ipad-simulator-ui/progress.md`.

Task 7 is assigned to fresh UI subagent `ipad_live_host_impl` from pushed `33e52a6`.
Its report target is `.superpowers/sdd/2026-10-11-ipad-simulator-ui/task-7-report.md`.
The controller owns this handoff and the implementer must not push. Task 7 must connect the Simulator to
the authenticated localhost gateway, pair with host identity proof, persist credentials in the
Simulator Keychain, discover Bonjour, stream live chat and Matrix events, return typed controls,
resume from cursor after relaunch and isolate two hosts with colliding IDs. It can close HOST-04
and IPAD-03 only with their full live Verify clauses and evidence. Review every subagent result,
independently verify, commit and push. After Task 7, follow the inferred UI and contained browser
plan for IPAD-04 and IPAD-05.
The Task 7 agent created its own active goal. It found that existing `HostTransport` has no
response channel and is adding a neutral command/update worker and `HostDirectory` reducer before
the iPad network adapter, Keychain and native DNS-SD browser. Initial tests target real daemon
loopback and Mac Unix reducer parity. No live pairing or checked Task 7 AC is claimed yet.

An independent agent owns Task 1 of the inferred-UI and browser plan. It is adding the dormant
production inference call site and fail-closed activation attestation in `moted` and
`mote-testkit`. It does not own iPad transport files, `Cargo.lock`, this handoff or pushes.
IPAD-04 remains open until the live model gate and Simulator states pass.
The Task 1 foundation and production rejection tests were committed locally as `70e213a` and
`60a154f` after workspace tests and clippy passed. Independent review found the blocking test
gap fixed. A first isolated live 200-prompt run of approved `qwen3:4b`
scored 20 first-try valid decisions out of 200, with 200 ledger rows and 200 packs. The model
selected no surface for all requested surfaces. No activation record was written.

## Limits

The pinned iOS AccessKit backend exposes one window TextView to XCUITest. Native journeys use
Vision-recognized text, exact state assertions, measured hit regions and screenshot evidence.
Do not claim individual accessibility nodes from that workaround. Simulator GPU capture and
renderer encoding cadence are not display scanout measurements. Xcode debugger diagnostics in
successful native logs are documented non-failing tooling noise.

The desktop queries host identity at startup. If the host is unavailable, cursor persistence waits
for recovery attachment. Restarted approvals remain visible but cannot re-execute their original
file intent. Interrupted remote request IDs remain indeterminate until reconciled. Never weaken
those fail-closed behaviors silently. Do not read the default Chrome profile or keychain item.
