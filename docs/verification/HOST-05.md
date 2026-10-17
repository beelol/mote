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

## Loopback acceptance followup, 2026-10-11

Base: `3811d8e`. Implementation: `test(gateway): verify paired loopback host`.
The environment and complete command results are in [HOST-01](HOST-01.md#reusable-loopback-acceptance-2026-10-11).

`cargo test -p moted loopback_acceptance -- --nocapture` passed two tests with no failures.
The first drops the event client, reconnects with its credential and resumes after its saved cursor.
The retained events arrive in sequence without earlier events.
The second stops the gateway while a paired subscription is live and observes transport loss.
It adds 5,001 heartbeat events to exceed retention and adds a message in another thread.
It resumes the gateway and reconnects to the same host from the old cursor.
The gateway sends resync followed by an authoritative snapshot.
The snapshot preserves the earlier message, the committed Matrix surface and the bound pending approval.
It includes the other thread's message added during suspension.
The shared client moves from offline through connecting to online without changing host selection.
A later live event is applied once even when the reducer receives it twice.
The pending file remains unwritten.

`cargo test --workspace` passed 580 tests with no failures.
The existing restart, request-fragment, version-mismatch, changed-identity, outbox and chunked-snapshot
tests remain in that run. The two live Bonjour tests remain ignored.
`cargo clippy --workspace -- -D warnings` exited 0.

The scripted suspension stops the gateway only. It does not suspend the Mac or restart the daemon.

## Live sleep preflight, 2026-10-11

At 13:10 America/Los_Angeles, the driver ran these read-only commands on the Mac:

```text
pmset -g sched
pmset -g assertions
pmset -g custom
```

`pmset -g sched` showed only a wake event dated 2026-10-09. It showed no future wake event.
`pmset -g assertions` showed `UserIsActive=1`, an active audio-input sleep-prevention assertion and the ChatGPT app's idle-sleep assertion.
`pmset -g custom` showed AC power with `sleep=0` and Wake on LAN enabled. Wake on LAN is not a verified, controlled wake path for this test.
The preflight did not establish a reliable automatic wake mechanism. No sleep command or power-setting change was issued. The literal live sleep and wake Verify clause remains open.
The literal Mac sleep and wake run was not attempted because no safe wake mechanism was configured.
HOST-05 remains unchecked. This does not prove live iPad connection-state presentation.

## Physical iPad disconnect and reconnect, 2026-10-17

Environment: iPad Pro 11-inch (M4), UDID `00008132-001104661AB9001C`, iPadOS
26.5, Xcode 27.0. The Mac and iPad were on the same Wi-Fi network. The paired host identity was
unchanged. Its saved endpoint had already moved from the direct-link address to
`192.168.50.184:47810` after authenticated Bonjour discovery.

A physical-only UI observer captured the selected host online and saved its exact `Cursor:` label.
The driver stopped only the foreground Mote daemon. The app presented the same host as offline.
The driver restarted the same daemon and the app returned to the exact online label with the same
cursor. No pairing code, credential mutation or host selection was used.

Command:

```text
xcodebuild test-without-building \
  -xctestrun /private/tmp/mote-reconnect-signed/Build/Products/Mote_iphoneos27.0-arm64.xctestrun \
  -destination 'platform=iOS,id=00008132-001104661AB9001C' \
  -resultBundlePath /private/tmp/mote-physical-reconnect1.xcresult \
  -collect-test-diagnostics never \
  -only-testing:MoteUITests/MoteUITests/testPhysicalPairedHostDisconnectsAndReconnectsWithoutRepairing
```

Output: `TEST EXECUTE SUCCEEDED`. The test passed in 56.175 seconds with zero failures. The result
bundle retains `physical-wifi-online-before`, `physical-wifi-disconnected` and
`physical-wifi-reconnected` screenshots.

This verifies the live physical iPad connection-state and recovery journey. HOST-05 remains open
because its Verify clause separately requires a literal Mac sleep and wake run.
