# HOST-05 partial verification

Status: automated recovery checks pass. The acceptance criterion remains unchecked.

The literal live Mac sleep and wake run has not been performed. An unattended sleep can strand
the driver without a safe wake mechanism. No Mac sleep command was issued.

## Environment

Date: 2026-10-11 UTC.
Machine: Apple Silicon Mac, arm64.
System: macOS 26.6.2, build 25G83.
Rust: rustc 1.89.0 (29483883e 2025-08-04).
Cargo: cargo 1.89.0 (c24e10642 2025-06-23).
Base revision: `98c36b7`.
Implementation revision: the commit containing this evidence, titled
`feat(host): recover paired sessions without duplicates`.

The tests use local SQLite databases and local Unix or encrypted loopback sockets.
Socket tests ran with approved local socket access because the sandbox rejects listener binding.
`MOTE_LIVE` was not enabled. No model was pulled or called outside the scripted test servers.

## Implemented behavior

- Checked replay tracks the highest deleted sequence separately for each scope. Sparse sequences
  from other scopes do not falsely invalidate a cursor. An expired or invalid cursor requests resync.
- Committed events and durable thread projections share one transaction. A projection or retention
  failure rolls back the event and suppresses its broadcast.
- A snapshot reads one SQLite transaction. Its cursor covers its messages, latest committed surfaces,
  pending typed approvals and online status. Approval-device bindings use the actual host identity.
- Retention no longer removes the current chat or committed surfaces from the durable projection.
  Reopening the database restores them. Snapshot output excludes paths, credentials and cookies.
- The gateway emits resync followed by a complete snapshot. It then subscribes after that snapshot's
  cursor. Partial encrypted requests cannot dispatch. Retrying an acknowledged or interrupted
  request uses its original request ID and the daemon's durable outcome.
- The shared client replaces one host's snapshot atomically. It rejects future surfaces, drops
  duplicate events, preserves other hosts and keeps a partially streamed reply together.
- Connecting, online, offline, incompatible and identity-changed states remain explicit per host.
  Resync keeps the last complete visible state until replacement. Host selection never changes
  implicitly.
- The client directory and outbox are saved with a private temporary file, file synchronization,
  atomic rename and directory synchronization. A result alone does not remove a request. A matching
  acknowledgement removes it without advancing past unseen events.

## Red evidence

`cargo test -p mote-gateway --test reconnect && cargo test -p mote-eventlog`
initially exited 101 because `Replay`, `subscribe_checked`, `host_snapshot`, `receive_frame`,
`enqueue_durable`, `receive_frame_durable` and `HostDirectory::load` were absent.
This was an API compilation failure before implementation.

Additional executable red checks established these failures before their fixes:

| Command | Observed failure |
| --- | --- |
| `cargo test -p mote-gateway --test reconnect gateway_ack_and_resync_are_authoritative_after_restart` | `durable outcome must be followed by acknowledgement: Timeout` |
| `cargo test -p mote-gateway --test reconnect snapshot_then_duplicate_and_live_tokens_keep_one_reply` | Message count was 3 instead of 2 |
| `cargo test -p mote-gateway --test reconnect future_surface_snapshot_is_rejected_atomically` | Future surface snapshot was accepted |
| `cargo test -p mote-gateway --test reconnect remote_snapshot_excludes_paths_credentials_and_cookies` | `snapshot leaked raw-cookie` |
| `cargo test -p mote-gateway --test reconnect resync_keeps_visible_state_until_snapshot_and_exposes_connection_states` | Connection remained Offline instead of Online |

The first loopback run without socket permission failed with `Operation not permitted (os error 1)`.
The approved rerun reached the acknowledgement timeout above.

A broader intermediate run found two paired daemon fixtures using an unsafe database file mode.
The fixtures now prepare the private identity and database before opening the shared event and
approval stores. `cargo test -p moted paired_` then reported 2 passed and 0 failed.

## Green evidence

Command:

```text
cargo test -p mote-gateway --test reconnect && cargo test -p mote-eventlog
```

Output:

```text
reconnect: test result: ok. 11 passed; 0 failed; 0 ignored
mote_eventlog: test result: ok. 9 passed; 0 failed; 0 ignored
```

The reconnect suite covers request cuts before any encrypted fragment, after fragment one,
after fragment two, and after completion but before acknowledgement consumption.
The same request ID produces exactly one user turn in every case.

Command:

```text
cargo test --workspace
```

Result: exit 0. The complete output was captured in `/private/tmp/mote-task6-workspace.log`.
Aggregating its 47 `test result: ok` lines gives:

```text
passed=572 failed=0 ignored=2 suite_results=47
```

The daemon suite reported 136 passed. The desktop suite reported 164 passed. The desktop's existing
Unix reconnect, duplicate suppression and frame-split tests remain green.

The two ignored tests require the live Mac Bonjour stack:

- `live_mac_stack_discovers_and_removes_loopback_registration`
- `live_sigterm_reaps_advertiser_and_removes_registration`

Command:

```text
cargo clippy --workspace -- -D warnings
```

Output:

```text
Finished `dev` profile [unoptimized + debuginfo] target(s) in 7.47s
```

Result: exit 0. `git diff --check` also exited 0.

## Remaining limits

The literal Mac sleep and wake clause is open. The README checkbox is unchanged.
This evidence proves the shared client recovery reducer and encrypted gateway behavior.
It does not claim a live iPad connection-state presentation check.
The existing Unix stream already suppresses duplicates and remains unchanged.
The first projection migration can recover only history that still exists in a legacy database.
History deleted before this migration cannot be reconstructed.
Pending file-write approvals remain visible after restart. Their existing exact-intent safety
boundary still refuses execution when the in-memory file intent no longer exists.
Physical iPad, signing, TestFlight and cloud work were not performed.

## Recovery review followup, 2026-10-11

Two review regressions were reproduced on `ea252283ff001de8d692ac121d7831fd36de529a`.

```text
cargo test -p mote-gateway --test reconnect interleaved_threads_reconnect_in_one_host_sequence
exit 101
left: (11, "B")
right: (10, "A")
```

The gateway now subscribes to all host scopes in global sequence order.
Its retention check uses the highest deleted sequence across all scopes.
The private Unix transport still filters by scope.
The regression delivers A10 before B11, reconnects from A10, then delivers B11 and live A12/B13.
An additional test detects a deleted B2 even while A1 remains retained.

```text
cargo test -p mote-gateway --test reconnect snapshot_larger_than_noise_limit_completes_call_and_resync
exit 101
large snapshot call must complete: WebSocket protocol error: Connection reset without closing handshake
```

Snapshots exceeding the Noise message limit now use typed snapshot chunks.
Each chunk carries at most 64 KiB of snapshot data and is reduced until its complete envelope fits.
The sender filters private output before chunking.
Transfer ID, cursor, request ID, index and total byte count are checked during assembly.
The previous client cache remains installed until the complete snapshot validates.
Partial transfer bytes are not persisted and are discarded on the offline transition.
The test covers eight 140 KB messages through both a snapshot call and authoritative resync.
A separate check covers escaped Unicode with a 4 KiB Noise message limit.

The new assembly fault test initially failed with `accepted interleaved`.
The shared assembler now rejects an event or result interleaved with incomplete snapshot chunks.
It also rejects changed host, cursor, transfer ID, total size and index.

Final commands and results:

```text
cargo test -p mote-gateway --test reconnect && cargo test -p mote-eventlog
reconnect: 13 passed; 0 failed
eventlog: 10 passed; 0 failed
cargo test -p mote-gateway --test reconnect snapshot_chunks_fit_small_noise_limits_with_escaped_unicode
1 passed; 0 failed
cargo test -p mote-host-client --test snapshot_chunks
2 passed; 0 failed
cargo test --workspace
exit 0
aggregate: passed=578 failed=0 ignored=2 suite_results=48
cargo clippy --workspace -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] target(s) in 6.69s
exit 0
```

The final workspace run includes all 14 reconnect tests.
Complete outputs are in `/private/tmp/mote-task6-review1-workspace.log` and
`/private/tmp/mote-task6-review1-clippy.log`.
The environment and the two ignored live Bonjour tests are unchanged.
The implementation commit containing this followup is titled
`fix(host): replay all scopes and chunk recovery snapshots`.
HOST-05 remains unchecked because the literal Mac sleep and wake run is still open.
