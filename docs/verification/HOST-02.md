# HOST-02 verification

Date: 2026-10-11.

Criterion: Every host has a stable cryptographic identity, and the client keeps credentials,
cursor, thread, surface and approval state isolated per host.

Verify: Pair two fixture hosts with colliding thread and approval IDs, switch repeatedly and
prove no state or action crosses.

## Environment

macOS 26.6.2, build 25G83, Darwin 25.6.0, arm64.
Rust 1.89.0 and Cargo 1.89.0.
Rust 2021 workspace.
Base commit: `05c4e6e` on local `main`.
Implementation: the commit introducing this evidence, `feat(gateway): serve paired clients on loopback`.
All network endpoints are loopback fixtures.
No model pulls, subscription logins, LAN listeners or external service changes were performed.

## Authenticated two-host journey

`paired_hosts_with_colliding_ids_keep_state_and_actions_isolated` starts two real daemon fixtures.
Each host generates and persists its own 0600 identity key under a 0700 directory.
Each host binds an encrypted gateway to `127.0.0.1:0`.
The reviewed carrier performs a WebSocket HTTP upgrade and carries Noise messages in binary frames.
The same-user Unix owner opens an offer, reads the pending device identity and confirms it.
Each client completes Noise IKpsk1 pairing and pins that host's public key.

The hosts intentionally share thread `main`, approval ID `1`, surface sequence `2`, cursor `3`,
snapshot request ID and subsequent action request IDs.
Their file contents, Matrix surfaces, chat messages, credentials and host identities differ.
Five repeated switches assert the selected host's cursor, selected thread, chat, surface and
pending approval remain intact in the shared host directory.
The eleven existing `mote-host-client` tests also cover foreign event, cursor, snapshot,
interaction and approval rejection, outbox isolation and independent credential lookup.

An office decision carrying home's action digest fails after its host field is changed to office.
A second authenticated home device without the issued approval capability also fails.
Home's typed refresh advances only home's event history.
Home's typed approval writes only home's held file.
Replaying that request returns the recorded outcome without another write or event.
Office's held file remains absent.
Home's credential cannot authenticate against office.
Both hosts keep zero ledger rows during this interaction and approval journey.

## Desktop cursor migration

`desktop_reads_the_daemon_public_key_identity_before_migrating` reads Unix `hello` using an
actual generated host identity and migrates the legacy cursor into that identity's `main` scope.
A durable migration claim records the host before the scoped cursor write.
The scoped write uses a synced temporary file and atomic rename.
The original `client_cursor` remains intact.
A failed write retries only its recorded host.
A second host and a second scope start at zero.
An existing scoped cursor is preserved.

## Commands and output

```text
$ cargo test -p moted paired_hosts_with_colliding_ids
test paired_hosts_with_colliding_ids_keep_state_and_actions_isolated ... ok
test result: ok. 1 passed; 0 failed
exit status: 0

$ cargo test -p mote-client cursor::tests
test result: ok. 8 passed; 0 failed
exit status: 0

$ cargo test --workspace
All 44 unit, integration and documentation suites passed.
Total: 544 passed; 0 failed.
exit status: 0

$ cargo clippy --workspace -- -D warnings
Finished dev profile with no warnings.
exit status: 0
```

Socket-dependent commands used approved local socket access.
The workspace suite includes all eleven `mote-host-client` tests.
The workspace test command reports an existing unused `ObservationHarness::set_viewport`
and `ObservationHarness::legacy` warning.
Their source is unchanged.

## Limits

This is authenticated fixture evidence for HOST-02.
It does not claim an iPad Simulator journey or a native credential-store backend.
The desktop queries the host identity at startup.
If the host is unavailable then, cursor persistence stays disabled for that window.
Recovery integration belongs to the next host task.

## Review fix verification

Date: 2026-10-11.
Review base: `ab0e5b4f93c545e1abc515efbb65cc71d5024c62`.
The environment and fixture scope above are unchanged.
The same two real hosts pair over WebSocket binary frames after the carrier correction.
The colliding IDs, five switches and isolated actions above remain covered.

```text
$ cargo test -p moted paired_hosts_with_colliding_ids
paired_hosts_with_colliding_ids_keep_state_and_actions_isolated ... ok
test result: ok. 1 passed; 0 failed
exit status: 0
```

The command used approved local socket access.
Output: `/private/tmp/mote-task4-review.NHXtxx/green-two-host.log`.
The final `cargo test --workspace` run passed 549 tests across 44 suites with exit status 0.
The final `cargo clippy --workspace -- -D warnings` run completed with exit status 0 and no warnings.
Outputs: `/private/tmp/mote-task4-review.NHXtxx/workspace-final.log` and
`/private/tmp/mote-task4-review.NHXtxx/clippy-final.log`.
The workspace test warning still names the unchanged unused observation helpers.

## Loopback acceptance followup, 2026-10-11

Base: `3811d8e`. Implementation: `test(gateway): verify paired loopback host`.
The environment and complete command results are in [HOST-01](HOST-01.md#reusable-loopback-acceptance-2026-10-11).

`cargo test --workspace` passed 580 tests with no failures.
It reran `paired_hosts_with_colliding_ids_keep_state_and_actions_isolated` successfully.
That existing test retains the literal two-host, colliding-ID and repeated-switch evidence.
The new `loopback_acceptance` tests also preserve the selected cryptographic host across a moved
endpoint and authoritative resync. They reject a wrong-host approval without writing its file.
HOST-02 remains checked. No Simulator host-switch journey is claimed by this harness.
