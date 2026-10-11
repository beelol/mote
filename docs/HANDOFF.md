# Handoff: Mote v0 close-out

Updated 2026-10-10 in America/Los_Angeles.
Branch: `main`.
Preserve untracked `.DS_Store` and `.superpowers/`.

The binding contract is `docs/rfcs/v0.md`.
The root README Ready for Go list is the active scope.
All other Matrix work remains in `docs/backlog.md`.

## Completed native client work

Commit `67f019e` records native Matrix and Mote UI evidence.
Commit `ecf4bc0` records the native approval card evidence for V0-52.
Commit `8593b5e` verifies chat states and live Pipeline navigation.
Commit `922bfbe` verifies that Pipeline Config changes the talk model used by the next turn.
Commit `b085b70` implements and verifies the live Pipeline overhead benchmark.

V0-50 now runs the fixed talk benchmark through the production daemon lane.
Benchmark events use the isolated `bench.talk` scope.
The client subscribes to that scope for live Pipeline display.
Benchmark events do not reduce chat state or write the saved main cursor.
The final native pair measured 17.0 ms closed and 19.0 ms open.
The p50 increase was 2.0 ms over 20 samples per condition.
The completed ordinary chat and its 280 stored main events were unchanged.
Independent review approved the fix.
The global ContextStore lock during the batch and the repeated full-ledger scan are deferred minor improvements.

Commit `3de54ef` records the future Vector gallery direction as UI-49.
It specifies separate Layout, Toggle, Slider, Select, List and Form pages.
Changed red structural borders materialize through deterministic square pixels within 120 ms.
The new layout is interactive on the first frame.
Reduced motion swaps immediately with a brief red outline pulse.
This is backlog work and does not change the delivered v0 runtime.

## Verification

`cargo test --workspace` passed 478 tests after V0-50 in an environment that permits local Unix sockets.
`cargo clippy --workspace -- -D warnings` passed.
The restricted command sandbox rejects the socket integration tests with `Operation not permitted`.
That environmental failure was reproduced at the listener bind and the same suite passed with local socket access.

## Remaining v0 work

V0-53 is the only unchecked v0 item.
Install, launchd load, KeepAlive restart, socket recovery, uninstall and unload-failure behavior are already verified in `docs/verification/V0-53.md`.
The literal logout and login survival check remains owner-only because it disrupts the active desktop session.
Do not mark V0-53 complete without observing that exact check.

The configured remote is `https://github.com/beelol/mote.git`.
A push attempt was rejected by the environment safety reviewer because the remote and default branch were not trusted.
Do not bypass that gate.
