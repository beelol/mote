# HOST-01 verification

Date: 2026-10-11.

Criterion: A versioned transport-independent host protocol carries requests, events, committed
Matrix surfaces, typed interactions, approvals, cursors and structured errors while the existing
Unix transport keeps passing its contract.

Verify: Protocol golden tests and the existing Unix reconnect suite pass with no wire-type
dependency on a socket implementation.

## Environment

- Owner's Mac, Darwin 25.6.0, arm64.
- Rust 1.89.0, Cargo 1.89.0, Rust 2021 workspace.
- `MOTE_LIVE` unset. Tests use existing local fixtures and scripted services.
- Unix and workspace tests ran with approved local socket access outside the filesystem sandbox.
- No model calls, model pulls, subscription logins or external service changes were performed.

## Protocol and compatibility coverage

The 12 JSON fixtures cover client and host hello, call, result, subscribe, event, committed surface,
typed interaction, approval decision, snapshot, acknowledgement and structured error.
The tests compare deserialized and serialized frames to literal checked-in JSON.
They reject unsupported major versions, malformed version ranges, negative and noninteger cursors,
invalid host and request IDs, incomplete interaction contracts, malformed approval decisions,
unknown client frame types and fields, and cross-host snapshot or acknowledgement cursors.

`HostId` is a full lowercase SHA-256 digest of the public key.
The host protocol negotiates only implemented major 1.
Matrix interactions and committed surfaces identify contract 1 and their committed sequence.
Approval decisions include host, thread and approval identity with an explicit approve or deny value.

The protocol event DTO has no dependency on the event log or SQLite.
Unix IPC converts between that DTO and its existing public event type at its boundary.
The Unix parser, peer UID checks, socket lifecycle and public transport signatures are preserved.
Added compatibility checks preserve arbitrary request IDs, missing optional local fields, extra
local keys, the legacy event decoder behavior and exact newline event bytes.

## Initial implementation commands and output

The expected initial TDD failure occurred before the crate was created:

```text
$ cargo test -p mote-protocol
error: package ID specification `mote-protocol` did not match any packages
exit status: 101
```

Focused verification after implementation:

```text
$ cargo test -p mote-protocol && cargo test -p mote-ipc
running 9 tests
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
running 12 tests
test tests::local_envelopes_keep_arbitrary_ids_and_the_unversioned_wire_shape ... ok
test tests::local_parser_keeps_missing_fields_extra_keys_and_arbitrary_ids ... ok
test tests::local_event_decoder_keeps_the_legacy_event_dto_behavior ... ok
test tests::subscribe_streams_and_reconnects_without_duplicates ... ok
test tests::fragmented_event_survives_timeouts_at_every_byte_and_requires_newline ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
exit status: 0
```

Workspace verification:

```text
$ cargo test --workspace
test result: ok. 161 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 50.26s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 129 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 16.81s
All 34 unit, integration and documentation test suites passed.
Total: 490 passed; 0 failed.
exit status: 0

$ cargo clippy --workspace -- -D warnings
    Checking mote-protocol v0.0.0 (/Users/bilal/projects/mote/crates/mote-protocol)
    Checking mote-ipc v0.0.0 (/Users/bilal/projects/mote/crates/mote-ipc)
    Checking mote-client v0.0.0 (/Users/bilal/projects/mote/mote-client)
    Checking moted v0.0.0 (/Users/bilal/projects/mote/moted)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 5.15s
exit status: 0

$ cargo fmt -p mote-protocol -- --check
exit status: 0

$ git diff --check
exit status: 0
```

Dependency inspection:

```text
$ cargo tree -p mote-protocol
mote-protocol v0.0.0 (/Users/bilal/projects/mote/crates/mote-protocol)
├── serde v1.0.229
├── serde_json v1.0.151
└── sha2 v0.10.9
```

The direct and transitive tree contains no `mote-ipc`, `mote-eventlog`, SQLite, UIKit or socket
implementation crate. SHA-256 CPU detection has a transitive `libc` dependency.

## Existing warning and sandbox result

The workspace test build emitted the existing `mote-client` test warning that
`ObservationHarness::set_viewport` and `ObservationHarness::legacy` are unused.
Those source lines were not changed. The required clippy command passed with warnings denied.

An initial sandboxed IPC run failed seven Unix bind tests with `Operation not permitted`.
The approved rerun outside the sandbox passed all 12 IPC tests without changing the socket code.
The workspace run also passed with that access.

The complete HOST-01 Verify clause is covered. HOST-01 is checked in the README.

## Review fix verification

Date: 2026-10-11, completed at 08:30 UTC in the same environment.

Host event deserialization now rejects unknown fields inside the event envelope.
The legacy Event DTO remains permissive for Unix IPC.
Approval decisions and pending approval snapshots now require `action_hash` as an `ActionDigest`.
The digest is a full lowercase SHA-256 hex string.
Both envelopes also require `expires_at` as nonnegative integer Unix time in milliseconds.
The host will compare these fields to the held intent and expiry before execution in the gateway task.
No existing remote deployment needs compatibility with the earlier unbound approval fixture.
The protocol remains major 1.

The new regression test failed before the strict host deserializer was implemented:

```text
$ cargo test -p mote-protocol --test golden host_events_reject_unknown_nested_fields_without_changing_legacy_events
test host_events_reject_unknown_nested_fields_without_changing_legacy_events ... FAILED
assertion failed: serde_json::from_value::<HostFrame>(frame).is_err()
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.00s
exit status: 101
```

The same command passed after implementation:

```text
$ cargo test -p mote-protocol --test golden host_events_reject_unknown_nested_fields_without_changing_legacy_events
test host_events_reject_unknown_nested_fields_without_changing_legacy_events ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.00s
exit status: 0
```

The updated approval fixtures and new binding tests failed before the binding fields were added:

```text
$ cargo test -p mote-protocol --test golden
test approval_decisions_require_a_complete_action_hash_and_expiry_binding ... FAILED
test pending_approvals_require_a_complete_action_hash_and_expiry_binding ... FAILED
test client_frames_preserve_the_golden_wire_contract ... FAILED
test host_frames_preserve_the_golden_wire_contract ... FAILED
test approvals_reject_untyped_incomplete_and_extra_decisions ... FAILED
unknown field `action_hash`, expected one of `host_id`, `thread`, `approval_id`, `decision`
unknown field `action_hash`, expected one of `approval_id`, `thread`, `kind`, `payload`, `expires_at`
test result: FAILED. 7 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit status: 101
```

After implementation, the binding tests reject missing fields, empty and truncated digests,
uppercase and nonhex digests, nonstring digests, and negative, fractional, overflowing,
string or null expiry values in both decisions and pending approvals.

```text
$ cargo test -p mote-protocol --test golden
test host_events_reject_unknown_nested_fields_without_changing_legacy_events ... ok
test approval_decisions_require_a_complete_action_hash_and_expiry_binding ... ok
test pending_approvals_require_a_complete_action_hash_and_expiry_binding ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
exit status: 0

$ cargo test -p mote-protocol && cargo test -p mote-ipc
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test tests::local_event_decoder_keeps_the_legacy_event_dto_behavior ... ok
test tests::subscribe_streams_and_reconnects_without_duplicates ... ok
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
exit status: 0

$ cargo test --workspace
test result: ok. 161 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 63.25s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 129 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 17.47s
exit status: 0

$ awk '/test result: ok\./ {passed += $4; failed += $6; suites += 1} END {print "passed=" passed, "failed=" failed, "suites=" suites}' /private/tmp/mote-host-01-fix-workspace-test.log
passed=493 failed=0 suites=34

$ cargo clippy --workspace -- -D warnings
    Checking mote-protocol v0.0.0 (/Users/bilal/projects/mote/crates/mote-protocol)
    Checking mote-ipc v0.0.0 (/Users/bilal/projects/mote/crates/mote-ipc)
    Checking mote-client v0.0.0 (/Users/bilal/projects/mote/mote-client)
    Checking moted v0.0.0 (/Users/bilal/projects/mote/moted)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.39s
exit status: 0
```

The existing mote-client test warning remains unchanged.
All 34 suites passed, including documentation tests.
The full HOST-01 Verify clause remains covered after both fixes.

## Reusable loopback acceptance, 2026-10-11

Base: `3811d8e` on `main`.
Implementation: the commit containing this section, `test(gateway): verify paired loopback host`.
Environment: Apple Silicon Mac, macOS 26.6.2 build 25G83, Rust 1.89.0, Cargo 1.89.0, Rust 2021.
Socket tests used approved local socket access. `MOTE_LIVE` was unset.

`mote_testkit::gateway::LoopbackHost` owns temporary storage and the real encrypted gateway.
Its factory supplies the real daemon dispatcher and the daemon's event log.
It exposes the MOTE1 offer, owner confirmation, paired client, endpoint rotation, gateway suspension,
reconnection, retention fill and retained event inspection.
The daemon fixture uses scripted Ollama responses, a real persisted event log and approval store.
Its context store is in memory. Its model calls still pass through the real lane and ledger.
No production daemon behavior was changed.

The new journey pairs on `127.0.0.1`, exchanges hello and subscribes to `main`.
It sends a turn and receives ordered chat events and a validated committed Matrix envelope.
It sends a typed Refresh interaction, reconnects from the cursor and receives only later events.
It executes a host-bound approval once and retrieves its recorded outcome after endpoint rotation.

```text
cargo test -p moted loopback_acceptance -- --nocapture
test result: ok. 2 passed; 0 failed; 0 ignored
cargo test --workspace
exit 0
aggregate: passed=580 failed=0 ignored=2 suites=48
cargo clippy --workspace -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.02s
exit 0
```

The full run includes protocol goldens and the existing Unix reconnect suite.
The two ignored tests require the live Mac Bonjour stack.
The existing test-only warning names unused `set_viewport` and `legacy` observation helpers.
Strict workspace clippy reports no warnings.
Logs: `/private/tmp/mote-task7-focused.log`, `/private/tmp/mote-task7-workspace.log` and
`/private/tmp/mote-task7-clippy.log`.
After tightening the Matrix retention assertion, the focused suite and full workspace suite passed
again with the same counts. Final workspace log: `/private/tmp/mote-task7-final-workspace.log`.

The test-first run exited 101 because `mote_testkit::gateway` did not yet exist.
The first executable run inside the sandbox could not bind local sockets.
An approved run then exposed an incorrect test expectation: wrong-host frames are rejected by the
client and close its Noise session. The test now checks that refusal and reconnects before approval.
These were test setup corrections. They did not require changing production authentication.

HOST-01 remains checked. This harness alone does not close any IPAD criterion.

## External fixture review followup, 2026-10-11

Review found that the generic harness required a daemon factory that existed only in a private
binary test module. An external integration test reproduced this with E0433 for the unresolved
`moted` crate when running `cargo test -p moted --test live_loopback --no-run`.

The daemon package now exposes an opt-in test-support library through feature `test-fixture`.
It compiles the same dispatcher source as a private module and exports `moted::test_fixture::start`.
The production binary entry point is unchanged. The library is empty without that feature.
The constructor shares the persisted event, approval, context and ledger database, and uses the
production model gate against the supplied local scripted server.
Both original acceptance tests now use this shared constructor.

An external host-side integration test can declare this dependency:

```toml
[dev-dependencies]
moted = { path = "../moted", features = ["test-fixture"] }
mote-testkit = { path = "../crates/mote-testkit" }
```

Keep the scripted server alive while using the returned host:

```rust
let models = mote_testkit::ScriptedServer::start()?;
let host = moted::test_fixture::start(&models)?;
let offer = host.open_pairing()?;
let paired = host.pair(&offer, "Simulator fixture")?;
```

`moted/tests/live_loopback.rs` is compiled as an external crate and imports only this public API.
It passes actual Noise pairing, chat with a ledger reference, committed Matrix snapshot, rejection
of remote configuration mutation, and reconnect with exactly-once replay after endpoint movement.

```text
cargo test -p moted --features test-fixture --test live_loopback -- --nocapture
1 passed; 0 failed
cargo test -p moted loopback_acceptance -- --nocapture
2 passed; 0 failed
cargo clippy --workspace -- -D warnings
exit 0
cargo clippy --workspace --features moted/test-fixture -- -D warnings
exit 0
```

The environment is unchanged from the loopback acceptance section.
No test command, bypass or fixture mode was added to the production daemon command line.
The test library does not run the daemon's process startup, scheduler loop or charter watcher.
The Simulator-specific clauses and literal sleep/wake clause remain open.

Both final workspace configurations passed before the review-fix commit:

```text
cargo test --workspace
passed=580 failed=0 ignored=2 suites=48
exit 0
cargo test --workspace --features moted/test-fixture
passed=581 failed=0 ignored=2 suites=49
exit 0
```

The two ignored live Bonjour checks and existing test-only observation warning are unchanged.
Logs: `/private/tmp/mote-task7-review1-workspace.log`,
`/private/tmp/mote-task7-review1-feature-workspace.log`,
`/private/tmp/mote-task7-review1-clippy.log` and
`/private/tmp/mote-task7-review1-feature-clippy.log`.
The review-fix commit is titled `fix(testkit): expose real daemon loopback fixture`.
