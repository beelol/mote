# HOST-02 partial verification

Date: 2026-10-11.

Status: Partial. HOST-02 remains open in the README.

Criterion: Every host has a stable cryptographic identity, and the client keeps credentials,
cursor, thread, surface and approval state isolated per host.

Verify: Pair two fixture hosts with colliding thread and approval IDs, switch repeatedly and
prove no state or action crosses.

This evidence covers the client directory and state boundary from host plan Task 2.
It does not cover authenticated pairing or dispatch to two fixture gateways.
Task 4 must provide that pairing evidence before the criterion can be checked.

## Environment

- Owner's Mac, macOS 26.6.2, build 25G83, arm64.
- Rust 1.89.0 and Cargo 1.89.0. Rust 2021 workspace.
- Base commit: `8b9335573c19cccdc45e575abcddde13eac95c74`.
- Implementation commit: the commit introducing this evidence, `feat(client): isolate state by host`.
- `MOTE_LIVE` unset. Fixtures use two public keys, `[1; 32]` and `[2; 32]`.
- Host IDs use the full SHA-256 digest of each fixture public key.
- No reserved local identity or desktop cursor migration is introduced.
- No model calls, model pulls, subscription logins or external service changes were performed.

## Covered behavior

`mote-host-client` defines a serializable host directory and immutable session views.
Each descriptor pins the public key and its derived host ID.
Each session owns its cursor, selected thread, chat snapshot, surface history, pending approvals,
connection state and outbox.
Every reducer and outbox operation receives an explicit host ID.
Host selection is display state and never chooses the destination of an action implicitly.

The credential store interface loads, saves and deletes opaque credentials by host ID.
The directory cannot hold a credential store or a credential value.
Tests use a separate in-memory credential fixture.
This verifies the separation of interfaces and does not verify a native keychain backend.

The isolation fixture uses identical thread, message, event sequence, surface sequence,
approval and outgoing request IDs for two hosts.
Five repeated switches retain the appropriate host's content and selection.
Event progress and acknowledgements affect one host only.
Foreign snapshots, events, interactions, approvals and subscription cursors are rejected.
Directory restoration rejects mismatched pinned keys, cursor ownership and nested state ownership.
The public descriptor, session and outbox deserializers enforce the same invariants independently.
The outbox preserves enqueue order and rejects changed content under an existing request ID.

## TDD evidence

The initial test was written before the package existed:

```text
$ cargo test -p mote-host-client --test multi_host
error: package ID specification `mote-host-client` did not match any packages
exit status: 101
```

After the first implementation, all seven initial isolation tests passed.
Self-review added four failing tests before strengthening the implementation:

```text
$ cargo test -p mote-host-client --test multi_host
test standalone_host_deserialization_cannot_bypass_identity_checks ... FAILED
test standalone_outbox_deserialization_cannot_bypass_identity_checks ... FAILED
test standalone_session_deserialization_cannot_bypass_identity_checks ... FAILED
test outbox_preserves_action_order_when_request_ids_sort_differently ... FAILED
assertion left: ["status", "turn"]
assertion right: ["turn", "status"]
test result: FAILED. 7 passed; 4 failed
exit status: 101
```

Checked standalone deserializers and an ordered outbox resolved those failures:

```text
$ cargo test -p mote-host-client
test result: ok. 11 passed; 0 failed; 0 ignored
exit status: 0

$ cargo test -p mote-client cursor::
test result: ok. 5 passed; 0 failed; 0 ignored; 156 filtered out
exit status: 0
```

## Workspace verification

The first workspace test run used the filesystem sandbox.
Four existing desktop tests failed because Unix socket binding returned
`PermissionDenied: Operation not permitted`:

- `app::tests::close_and_reopen_subscribes_from_durable_cursor_and_ignores_stale_replay`.
- `app::tests::reconnect_removes_expired_matrix_controls_and_preserves_chat_and_cursor`.
- `app::tests::restart_between_status_and_subscription_rechecks_the_watermark`.
- `tests::rejected_matrix_submission_still_opens_chat_for_logged_diagnostic`.

That run ended with 157 desktop tests passed and four failed, exit status 101.
HOST-01 records the same local socket restriction.
The required workspace command was rerun with approved local socket access.

```text
$ cargo test --workspace
test result: ok. 161 passed; 0 failed; finished in 47.84s
test result: ok. 11 passed; 0 failed; finished in 0.00s
test result: ok. 12 passed; 0 failed; finished in 0.13s
test result: ok. 129 passed; 0 failed; finished in 13.82s
All 37 unit, integration and documentation suites passed.
Total: 504 passed; 0 failed.
exit status: 0

$ cargo clippy --workspace -- -D warnings
    Checking mote-host-client v0.0.0
    Checking mote-client v0.0.0
    Checking moted v0.0.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 9.76s
exit status: 0

$ cargo fmt -p mote-host-client -- --check
exit status: 0

$ git diff --check
exit status: 0
```

The existing workspace test warning reports unused `ObservationHarness::set_viewport` and
`ObservationHarness::legacy` methods.
Their source is unchanged.

## Remaining verification

The directory receives descriptors from a completed pairing boundary.
This task does not implement that boundary.
The gateway task must pair two authenticated fixture hosts and prove routed actions remain isolated.
The desktop cursor stays unchanged until the daemon exposes its real public-key-derived identity.
Recovery dispatch, native credential storage and atomic on-disk outbox writes are later integration work.
HOST-02 remains open.
