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

## Commands and output

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
