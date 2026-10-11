# Current development handoff

Updated 2026-10-11 in America/Los_Angeles. Replace this snapshot rather than appending a log.

## Current work

Branch: `main`.

The active goal is a Rust immediate-mode Mote client running in the iPad Simulator over an
authenticated localhost gateway to the Mac. Active criteria are HOST-01 through HOST-05 and IPAD-01
through IPAD-05 in the root README. V0-53 remains independent and owner-only for its literal logout
and login check.

Physical iPad behavior, Apple signing, App Store Connect and TestFlight are deferred as IPAD-06
through IPAD-08. Do not begin them without new owner authority. The cloud relay also remains
deferred. Preserve untracked `.DS_Store` and `.superpowers/`.

## Written contracts

The approved design is `docs/superpowers/specs/2026-10-11-ipad-host.md`.

Implementation is split into three plans:

- `docs/superpowers/plans/2026-10-11-ipad-simulator-host.md` covers HOST-01 through HOST-05.
- `docs/superpowers/plans/2026-10-11-ipad-simulator-ui.md` covers IPAD-01 through IPAD-03.
- `docs/superpowers/plans/2026-10-11-ipad-agent-browser.md` covers IPAD-04 and IPAD-05.

The plans preserve the desktop Unix client, keep `models.local_only = true`, use only the approved
Ollama models, route model calls through the ledger and redact before storage. Browser work uses a
dedicated Chrome for Testing profile and local fixtures. Outward browser effects require durable,
host-bound approval from the Simulator.

## Delivery state

Commit `60652a3` contains the approved Simulator plans. HOST-01 was delivered in `b60e1ef` and
`7e18050`, adding the protocol crate, Unix boundary conversion, goldens, review fixes and evidence.
The independent task review found two important protocol gaps: unknown nested host event fields were
accepted, and remote approval DTOs lacked exact action and expiry binding. The original implementer
fixed both in `7e18050`. The scoped re-review passed. HOST-01 is checked with evidence. The
driver-side full workspace rerun and strict clippy passed. No Rust Apple
target, Xcode project, signing state, App Store Connect record or TestFlight build has been created.

The configured remote is `https://github.com/beelol/mote.git` on `main`.

## Verified environment and limits

The final HOST-01 run passed 493 workspace tests and strict clippy on 2026-10-11. Local Unix socket tests
required an approved rerun with socket access. Xcode 27.0 and iOS Simulator SDK 27 are installed.
The Rust iOS targets are not installed. `xcrun simctl list devices available` confirms iPad Pro,
iPad Air, iPad mini and base iPad simulators at iOS 26.5 and 27.0. The restricted shell could not
reach CoreSimulator, but the approved local command succeeded.

AccessKit's current iOS adapter cannot provide physical VoiceOver evidence. IPAD-06 remains open for
that and all other physical-device checks. No local-network privacy prompt, signing identity, bundle
ID reservation, App Store record or TestFlight install has been verified.

An independent plan review checked all three plans against the design and README. Its corrections are
incorporated. They include real Simulator Bonjour discovery, Keychain credential storage, both Apple
Rust targets, repeatable XCUITest interaction driving, network-level browser interception, exact
action hashing and a durable inferred-UI activation attestation.

The literal Mac sleep and wake run in HOST-05 is not safe to start unattended without a configured
wake mechanism. Leave that clause and HOST-05 open while completing independent work.

## Exact next action

The owner approved execution of the three plans on 2026-10-11. The active goal is attached to this
chat. Host plan Task 1 is complete and pushed. Its progress ledger is
`.superpowers/sdd/2026-10-11-ipad-simulator-host/progress.md`. Task 2, per-host client directory
and isolation, is committed as `3332860` from base `8b93355`. Eleven focused isolation tests,
five desktop cursor tests, 504 full workspace tests and strict clippy pass. Independent review
approved the task with no blocking findings. The driver independently reran the workspace suite
and strict clippy with exit 0. Commit `3332860` is pushed to `origin/main`. An accidental
workspace-wide formatting pass was reversed on the previously clean unrelated paths.
Current tracked changes are only this handoff.
HOST-02 remains open until Task 4 pairs two fixture hosts. Keep UI work behind the published host interfaces
and inspect every delegated result before integration. Run workspace test and clippy, write the
relevant `docs/verification/<ID>.md`, commit, and push after every completed criterion.

Do not mark a criterion complete from a partial test. If IPAD-04's live 200-prompt gate scores below
190 first-try valid surfaces, leave automatic inference disabled and IPAD-04 open while continuing
independent browser fixture work.
