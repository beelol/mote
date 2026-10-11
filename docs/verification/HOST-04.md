# HOST-04 host-side verification

Date: 2026-10-11.

Status: verified after independent recovery review and final native rerun.
The host-side evidence below and the Simulator followup cover the full README Verify clause.
The physical privacy prompt belongs to IPAD-06.

## Environment

macOS 26.6.2, build 25G83, Darwin 25.6.0, arm64.
Rust 1.89.0 and Cargo 1.89.0.
Rust 2021 workspace.
Base commit: `52e45f3bddd35fff0c6bb8b047cc43c9cf6e2a9e` on local `main`.
Implementation: the commit containing this evidence, `feat(gateway): discover local Mote hosts`.
Tests use temporary data directories and ephemeral loopback ports.
The normal suite uses fake discovery boundaries and real Noise sessions.
The live check uses `/usr/bin/dns-sd` and the Mac mDNSResponder stack.
Local socket and Bonjour checks required approved execution outside the filesystem sandbox.

## Contract and identity proof

The Mac advertiser registers `_mote._tcp` on the local-only interface.
Its proxy hostname resolves to `127.0.0.1`.
The gateway listener remains IPv4 loopback only.
TXT contains exactly `protocol_major` and `fingerprint`.
The fingerprint is the full SHA-256 digest of the public host key.
Native TXT parsing rejects unknown, duplicate, missing or malformed metadata.
The native browser bounds query time, output size and returned service count.
It treats records as address hints.

`EndpointCandidate` first checks the protocol and fingerprint against the pinned credential.
It then connects through the existing authenticated Noise handshake.
Only a successful handshake updates the caller's saved endpoint.
The caller remains responsible for durable address storage and advancing the credential counter before each attempt.
No endpoint persistence occurs during discovery or candidate construction.
The moved-port fixture restarts the same host data directory at a different loopback port.
The successful reconnect preserves its public key and host identity.
The impersonator fixture copies the expected TXT fingerprint but has a different Noise key.
Its handshake fails and the saved endpoint stays unchanged.
An incompatible protocol and a credential from another host also fail without moving the endpoint.

The gateway owns its registration until shutdown.
Every native discovery subprocess is terminated and reaped by its owner.
Discovery-disabled tests skip both advertiser and browser invocation.
The gateway listener remains available when discovery is disabled.
The daemon enables discovery unless `MOTE_DISCOVERY=0`.
Embedded gateway configuration leaves discovery disabled until explicitly enabled.
An unavailable Mac Bonjour stack leaves explicit localhost connection available.
The existing Unix peer UID check remains in force.

## Red and green

Command:

```text
cargo test -p mote-gateway --test bonjour
```

Before implementation:

```text
error[E0432]: unresolved import `mote_gateway::discovery`
could not find `discovery` in `mote_gateway`
exit 101
```

The first post-implementation sandbox run could not bind loopback sockets.
It reported `Operation not permitted (os error 1)` in five fixture tests.
The same suite passed with approved local socket access.
This restriction did not change production behavior.

Command:

```text
cargo test -p mote-gateway --lib native_zone_parser
```

The parser test initially failed because `parse_zone` did not exist.
After implementation:

```text
test discovery::tests::native_zone_parser_rejects_extra_or_malformed_txt ... ok
test result: ok. 1 passed; 0 failed
```

The Bonjour integration suite passes eight tests.
Its live Mac test is ignored in the normal offline run.
The complete gateway suite passes 43 tests with one ignored live test.

## Live Mac stack

Command:

```text
MOTE_LIVE=1 cargo test -p mote-gateway --test bonjour live_mac_stack -- --ignored --nocapture
```

Output from the final live run:

```text
Mac Bonjour discovered 127.0.0.1:63499 with TXT ["protocol_major=1", "fingerprint=4996ccc94ebb637a7bc1a17f00742c8d93dc4549606009e1c1e390ec589e6345"]
Mac Bonjour removed the registration after advertiser shutdown
test live_mac_stack_discovers_and_removes_loopback_registration ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out
exit 0
```

The browser resolved the service through the Mac stack and returned its exact endpoint and TXT.
After dropping the advertiser, a fresh browser operation did not return the registration.
The check was repeated after native parser changes and passed again.
The initial sandbox-only `dns-sd` probe returned `DNSServiceCreateConnection returned -65563`.
The approved live run succeeded.
All three exploratory fixture subprocesses were identified and terminated.
A final `pgrep -fl '^/usr/bin/dns-sd -lo'` returned no remaining task processes.

## Workspace regression

Commands:

```text
cargo test -p mote-gateway
cargo test --workspace
cargo clippy --workspace -- -D warnings
git diff --check
```

All commands exited zero.
The workspace test run reported no failures.
The Unix IPC suite passed 12 tests, including peer credentials and reconnect without duplicates.
The daemon suite passed 136 tests.
The gateway suite passed 43 tests with the live Bonjour check explicitly ignored in the normal run.
Clippy finished the dev profile with no warnings.
The workspace test build reported an existing `dead_code` warning for `ObservationHarness::set_viewport` and `legacy` in `mote-client/src/observation.rs`.
The offline second-instance process fixture explicitly disables Bonjour.

## Remaining evidence

No Simulator discovery journey ran in the original host-side phase.
No physical iPad, signing, App Store Connect, TestFlight or relay work ran.
At the end of the original host-side phase, HOST-04 remained open for Simulator evidence.

## Daemon signal shutdown

Review found that default SIGTERM could bypass the registration owner's destructor.
A real daemon process test reproduced an exited daemon with a live advertiser child and a retained registration.
The daemon now installs SIGTERM and SIGINT handlers before it starts resources.
The signal handler only sets an atomic shutdown flag.
The main loop returns normally and drops the gateway before the Unix listener.
The advertiser destructor terminates and reaps `dns-sd`.
The shutdown guard restores the previous handlers after resources are dropped.

Command:

```text
cargo test -p moted --test shutdown sigterm_and_sigint -- --nocapture
```

Red output:

```text
signal 15 bypassed graceful cleanup: signal: 15 (SIGTERM)
test sigterm_and_sigint_release_the_unix_listener ... FAILED
exit 101
```

Green output:

```text
signal 15: daemon exited normally and released its Unix listener
signal 2: daemon exited normally and released its Unix listener
test sigterm_and_sigint_release_the_unix_listener ... ok
test result: ok. 1 passed; 0 failed
exit 0
```

Command:

```text
MOTE_LIVE=1 cargo test -p moted --test shutdown live_sigterm -- --ignored --nocapture
```

Red output:

```text
Before SIGTERM: daemon 20468 owns live advertiser 21073
After SIGTERM: status=signal: 15 (SIGTERM), child_exists=true, registration_exists=true
SIGTERM orphaned advertiser 21073
test live_sigterm_reaps_advertiser_and_removes_registration ... FAILED
exit 101
```

Green command:

```text
MOTE_LIVE=1 cargo test -p moted --test shutdown -- --include-ignored --nocapture
```

Green output:

```text
Before SIGTERM: daemon 14520 owns live advertiser 15035
After SIGTERM: status=exit status: 0, child_exists=false, registration_exists=false
test live_sigterm_reaps_advertiser_and_removes_registration ... ok
signal 15: daemon exited normally and released its Unix listener
signal 2: daemon exited normally and released its Unix listener
test sigterm_and_sigint_release_the_unix_listener ... ok
test result: ok. 2 passed; 0 failed
exit 0
```

These checks use the same Mac environment described above.
The live test discovers the registration before signaling the daemon.
It checks that the advertiser PID is gone and a fresh browser no longer finds the host afterward.
The offline signal test keeps discovery disabled.
The fixtures serialize their fixed gateway listener within the test harness.

## Loopback endpoint acceptance followup, 2026-10-11

Base: `3811d8e`. Implementation: `test(gateway): verify paired loopback host`.
The environment and complete command results are in [HOST-01](HOST-01.md#reusable-loopback-acceptance-2026-10-11).

`cargo test -p moted loopback_acceptance -- --nocapture` passed both journeys.
The first binds a new loopback port while the old listener still holds its port.
It confirms the endpoint changed and reconnects with the existing pinned Noise credential.
The host identity stays the same and an acknowledged approval returns its recorded outcome.
`cargo test --workspace` also reran the discovery and impersonator rejection tests successfully.
Its result was 580 passed, no failures and two ignored live Bonjour checks.
`cargo clippy --workspace -- -D warnings` exited 0.

Discovery is disabled in this reusable acceptance harness.
This run adds authenticated endpoint movement evidence, not Simulator network-stack evidence.
At the end of the loopback-only followup, HOST-04 still required Simulator evidence.
No physical-device privacy prompt or IPAD criterion is closed here.

## Native Simulator followup, 2026-10-11

The environment, commands, exact outputs and inspected screenshots are in [IPAD-03](IPAD-03.md).
The successful native run uses Xcode 27.0 and an iPad Pro 11-inch (M5) on iOS 27.0.
The app browses through native DNSServiceBrowse, DNSServiceResolve and DNSServiceGetAddrInfo.
It does not run the Mac dns-sd browser subprocess inside the app.
The bounded browse runs off the render thread on the Simulator local-only interface.
Both genuine services and the impersonator resolve to IPv4 loopback.
Each TXT record contains exactly `protocol_major=1` and the public-key fingerprint.
The [native network report](artifacts/ipad-live-2026-10-11/live-network.json) records these resolved endpoints and both TXT fields.

Home's identity is `ac92420fa323d2ea83ac86d1750a4237aa34f36a38c27809c99d998a61721a65`.
Its initial endpoint is `127.0.0.1:61551`.
The harness releases the old advertisement, moves the real gateway to `127.0.0.1:61630` and advertises the new port.
The native browser rediscovers the new port.
The pinned Noise handshake succeeds before the app updates its durable endpoint.
The report records accepted identity proof with prior `127.0.0.1:61551` and current `127.0.0.1:61630`.
The native app relaunches and reconnects using the same Keychain identity at saved cursor 12.
The screenshot and durable state retain the same Home chat and committed dashboard.

The impersonator is at `127.0.0.1:61553`.
It advertises Home's genuine fingerprint but has its own host key.
Its handshake is rejected before any application action.
Both rejection records preserve the prior genuine endpoint.
An Office record is also refused as a candidate for Home.
The command connection observes the authenticated endpoint move through the shared host endpoint.
The real-daemon regression test verifies that a command after rediscovery reaches the moved genuine host.

The checked-in Info.plist declares `NSBonjourServices` as `_mote._tcp`.
Its `NSLocalNetworkUsageDescription` is:

```text
Mote discovers and connects to your paired Mote hosts on the local network.
```

This verifies the purpose text and native Simulator network stack.
It does not claim a physical-device privacy prompt.
That prompt remains IPAD-06 evidence.
Simulator-only local ad-hoc entitlement stamping supports actual Keychain access without a team, certificate or provisioning profile.
Physical and distribution signing settings are unchanged.
Workspace tests and strict clippy pass as recorded in IPAD-03.

The Mac native-stack check was also repeated after the Simulator integration.
Command: `MOTE_LIVE=1 cargo test -p mote-gateway --test bonjour live_mac_stack -- --ignored --nocapture`.
Output:

```text
Mac Bonjour discovered 127.0.0.1:64040 with TXT ["protocol_major=1", "fingerprint=fd0452dbfbdba96c89eef621dfa126fb29b186d6b6030399702c89e7c80aecc8"]
Mac Bonjour removed the registration after advertiser shutdown
test live_mac_stack_discovers_and_removes_loopback_registration ... ok
test result: ok. 1 passed; 0 failed; 0 ignored
exit 0
```

## Recovery review follow-up

Date: 2026-10-11, 09:55 to 09:56 PDT.
The reviewed source was rebuilt and rerun on the same native iPad Simulator environment.
Command: `MOTE_LIVE=1 MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E MOTE_ARTIFACTS=/private/tmp/mote-task7-live-attempt9 cargo test -p mote-ios --test simulator_live -- --ignored --nocapture`.
Output: 1 passed, 0 failed, 69.80 seconds, exit 0, native `TEST SUCCEEDED`.
Home moved from `127.0.0.1:57459` to `127.0.0.1:58022` only after pinned Noise proof.
The forged-fingerprint service at `127.0.0.1:57461` was rejected without replacing the endpoint.
The app resumed Home at cursor 12 without duplicate chat while Office remained isolated at cursor 1.
Actual Simulator Keychain restoration and deletion passed again.
The same-address restart regression now invalidates retained command sessions before a new action.
The saved-host unavailable-at-launch regression now recovers without restarting the worker or replaying actions.
The Mac shell socket-replacement regression rejects the new identity before an action can execute.
Its real stream retains arrival timestamp and replay watermark while reducing through an ephemeral host-bound directory.
Fresh full workspace tests and strict clippy both exited 0.
Exact commands, environment, outputs and seven screenshots are recorded in [IPAD-03](IPAD-03.md#recovery-review-verification).
Public proof logs are in [review artifacts](artifacts/ipad-live-review-2026-10-11/).
The independent recovery review approved all three findings without new breakage.

## Final reviewed-source discovery check

Date: 2026-10-11, 10:22 to 10:23 PDT.
Source HEAD: `6131d0c`, with Task 7 recovery tip `8003027`.
Environment: the same macOS 26.6.2, Xcode 27.0 and iPad Pro 11-inch iOS 27.0 Simulator listed in IPAD-03.
Command: `MOTE_LIVE=1 MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E MOTE_ARTIFACTS=/private/tmp/mote-task7-live-attempt10 cargo test -p mote-ios --test simulator_live -- --ignored --nocapture`.
Output: 1 passed, 0 failed, 80.71 seconds, exit 0, native `TEST SUCCEEDED`.
The native browser resolves only loopback endpoints and the two required TXT fields.
Home is `952ccc20a0c663433b2fec355fe7e2bd357c646650f24e0ed05cdb719e9b19ea`.
It moves from `127.0.0.1:62723` to `127.0.0.1:62792` only after pinned Noise proof.
The forged-fingerprint service at `127.0.0.1:62725` is rejected without changing Home's endpoint.
The [final network report](artifacts/ipad-live-final-2026-10-11/live-network.json) records discovery, identity proof and Keychain restoration.
The app relaunches with Home cursor 12 and switches to isolated Office cursor 1.
The purpose text remains exactly the checked-in text above.

The Mac native stack is also repeated on final source:

```text
MOTE_LIVE=1 cargo test -p mote-gateway --test bonjour live_mac_stack -- --ignored --nocapture
Mac Bonjour discovered 127.0.0.1:62823 with TXT ["protocol_major=1", "fingerprint=94a24f6fb0797aee033f3bc15524f50ac92120ea37c01de5f491eb87e0dce6b8"]
Mac Bonjour removed the registration after advertiser shutdown
test result: ok. 1 passed; 0 failed; 0 ignored; finished in 3.53s
exit 0
```

The full dated native output and seven inspected screenshots are in [IPAD-03](IPAD-03.md#final-reviewed-source-verification).
The exact Mac log is in [final artifacts](artifacts/ipad-live-final-2026-10-11/mote-task7-bonjour-final.log).
The README checkbox is closed by the evidence commit.
No physical-device privacy prompt, signing or sleep and wake claim is made.
The final pre-commit workspace17 gate exits 0 with 667 passed, 7 ignored and 60 suites.
Strict clippy20 exits 0 in 0.25 seconds.
Both exact logs are included in the final artifact directory.
