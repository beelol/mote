# Current development handoff

Updated 2026-10-11 in America/Los_Angeles. Replace this snapshot rather than appending a log.

## Current work

Branch: `main`.

The owner approved the native iPad host design, including multiple hosts in different locations,
the Mac as the first authoritative host, Bonjour local discovery, a separately encrypted paired
connection, Rust immediate-mode Matrix rendering, host-side inferred UI, host-side browser use and
approval cards answerable from the iPad.

The durable design is `docs/superpowers/specs/2026-10-11-ipad-host.md`.
The active board is root `README.md` under `Ready for Go`.
The iPad feature states its approved Matrix and browser behavior in self-contained HOST and IPAD ACs.
It does not activate additional backlog IDs.
The cloud relay remains deferred, but the protocol must support hosts in different places.

Preserve untracked `.DS_Store` and `.superpowers/`.

## Delivery state

The design spec, README driver goal and HOST-01 through HOST-05 plus IPAD-01 through IPAD-08 are
fully reviewed and verified as one planning batch. No iPad implementation or App Store Connect state
has been created.

The last integrated commits before this design batch are:

- `3de54ef` documents the Vector materialization direction.
- `b085b70` implements and verifies the live pipeline overhead benchmark.
- `e134316` records the v0 close-out state.

The configured remote is `https://github.com/beelol/mote.git` on `main`.

## Evidence and limits

`cargo test --workspace` passed 478 tests on 2026-10-11 with local Unix socket access.
`cargo clippy --workspace -- -D warnings` passed on 2026-10-11.
The first restricted test attempt reproduced the known local-socket failures and was stopped after it
stalled. The complete run with local socket access passed.
Independent acceptance review approved the corrected spec and board after checking scope, links,
unique IDs, the 3,527-character driver goal, parent state and distribution gates.

The current desktop client uses eframe, egui and winit. Winit has an iOS backend in the cached
dependency source. Xcode 27.0 is installed. The Rust iOS targets are not installed. Simulator device
availability has not been verified because the restricted environment could not reach CoreSimulator.
No physical iPad behavior, local-network prompt, signing, bundle ID, App Store Connect record or
TestFlight build has been verified.

V0-53 remains open only for the literal logout and login survival check. Do not mark it complete
without observing that check.

## Blockers and next action

There is no design decision blocker. The written spec now awaits the owner's document review.

After the owner approves the written spec, use the plan-writing workflow to create the implementation
plan. Start implementation only after the written plan is approved and the owner chooses execution in
the current session or a later session. Work HOST-01 before iPad packaging. Do not create the App Store
Connect record until HOST-01 through HOST-05 and IPAD-01 through IPAD-06 pass.
