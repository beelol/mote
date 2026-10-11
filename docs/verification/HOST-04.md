# HOST-04 host-side verification

Date: 2026-10-11.

Status: partial.
The README checkbox remains open.
UI plan Task 7 must still drive discovery from the iPad Simulator and verify the checked-in local-network purpose text.
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

No Simulator discovery journey ran in this task.
No physical iPad, signing, App Store Connect, TestFlight or relay work ran.
HOST-04 remains open until the Simulator verifies service browsing, identity-proved reconnect and the declared local-network purpose text.
