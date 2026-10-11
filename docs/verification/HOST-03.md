# HOST-03 verification

Date: 2026-10-11.

Criterion: Owner-present pairing uses the `MOTE1` offer and `mote-gateway-v1` Noise protocol
to mutually authenticate a client and host, encrypt ordered frames, pin both identities and
support device revocation.

Verify: Pass shared Noise vectors, complete one pairing and reject expired or reused offers,
altered identities, replayed counters, tampered frames and a revoked device before any
application action runs.

## Environment

macOS 26.6.2, build 25G83, Darwin 25.6.0, arm64.
Rust 1.89.0 and Cargo 1.89.0.
Rust 2021 workspace.
Base commit: `05c4e6e` on local `main`.
Implementation: the commit introducing this evidence, `feat(gateway): serve paired clients on loopback`.
The gateway binds only `127.0.0.1`.
Tests use ephemeral ports, same-user Unix sockets and scripted local Ollama responses.

## Pairing and authentication

The daemon exposes `gateway.pair`, `gateway.pending`, `gateway.confirm` and `gateway.revoke`
only through local contextual dispatch.
The Unix transport still checks peer UID before it reads a request.
A network pairing consumes the `MOTE1` offer after decrypting and validating message 1.
The session is withheld until the Unix owner confirms the pending device.
The existing pairing tests prove denial, confirmation expiry, offer expiry and single use.
The existing Noise tests pass both the pinned Overseer vector and Mote's `mote-gateway-v1` vector.
They prove ordered encrypted frames, replay and reorder rejection, chunk bounds and poisoning
after identity, direction, framing or authentication failure.

The loopback suite completes encrypted pairing, hello, status, turn and subscription.
The carrier uses a real WebSocket HTTP upgrade and binary frames for every Noise message.
It records zero dispatches before successful authentication.
Plaintext application input is rejected before dispatch.
Expired and reused offers cannot establish a session.
Changed host keys and replayed counters cannot establish a session.
A tampered encrypted turn closes the connection without dispatch.
A revoked device cannot establish a new connection.
Existing sessions also reject its next turn.
The rejection fixture retains a dispatch count of zero throughout these attempts.
Persistent counter and revocation tests reopen the device store before retrying.

## Application boundary

Raw approval, configuration, file, schedule, surface and Matrix mutations are local-only.
The network permits read calls, turn, typed subscribe, typed interaction and typed approval.
Every application frame binds host, device and protocol version through its encrypted envelope.
Typed interactions pass the current committed surface contract to the daemon.
Typed approvals additionally match host, authenticated device capability, thread, approval ID,
exact action digest and expiry before execution.

The canonical intent digest test independently expects SHA-256
`e11c1aa5f1bfb0f02d6fe781cb5366229208a1a971db55f0c0dd6a8666c82e1c`.
It covers exact path, resolved target identity and content.
The database persists that digest with a separate redacted basename and byte-count display.
It does not persist executable file content or full local paths in new approval rows.
The executable intent stays in memory while the host is running.
Legacy structured approval payloads migrate to digest and safe display when watchers open.
Host restart preserves the pending card and its digest.
Approval after restart fails closed because its exact executable intent is unavailable.
The card can still be denied.

The gateway claims each host/device/request ID durably before dispatch.
An identical retry returns its stored terminal outcome.
A changed intent under that ID fails.
Gateway restart preserves completed outcomes.
A claim without a recorded terminal outcome remains inert and is not dispatched again.
An identical retry while the original host run is active receives a typed `pending` state.
It can retry the same ID to retrieve the eventual stored outcome without executing again.
Gateway startup marks all unfinished claims from prior runs as typed `indeterminate` states.
Legacy rows with a NULL outcome and no run owner receive the same fail-closed treatment.
Those IDs never become automatically retryable for execution.
An operator must reconcile the effect before intentionally creating a new action with a new ID.
Old run ownership cannot record a late result over an interrupted claim.
The client has no implicit five-second application deadline.
A transport failure does not establish that the action failed.
Secrets and local path tokens are removed from remote results and events.
Redaction applies to application payloads, not protocol envelope identifiers.
An opaque request ID shaped like a secret survives storage, gateway restart and outbox correlation.
Remote turn input is redacted before reaching the daemon's model path.
The real daemon turn fixture records two approved local model calls and two ledger rows.
It sends no secret to the scripted model, stores one redacted user event and makes no extra
model call when the request is replayed.

## Red and green evidence

```text
$ cargo test -p mote-gateway --test loopback
error: unresolved imports GatewayClient, GatewayServer
exit status: 101

$ cargo test -p moted remote_approval
error: no method named remote_approvals or remote_decide
exit status: 101

$ cargo test -p mote-gateway --test loopback remote_outcomes
remote_outcomes_and_events_redact_secrets_and_local_paths ... FAILED
assertion failed: !encoded.contains("/Users/owner/private.txt")
exit status: 101

$ cargo test -p moted paired_turn_uses
paired_turn_uses_approved_local_models_ledger_and_redacted_events_once ... FAILED
assertion failed: !call.body.to_string().contains(secret)
exit status: 101

$ cargo test -p mote-gateway --test loopback
test result: ok. 7 passed; 0 failed
exit status: 0

$ cargo test -p moted approval_dispatch_tests
test result: ok. 60 passed; 0 failed
exit status: 0

$ cargo test -p moted watchers::tests
test result: ok. 18 passed; 0 failed
exit status: 0

$ cargo test --workspace
All 44 unit, integration and documentation suites passed.
Total: 544 passed; 0 failed.
exit status: 0

$ cargo clippy --workspace -- -D warnings
Finished dev profile with no warnings.
exit status: 0
```

The sandbox refused the first loopback test run with `Operation not permitted` at bind.
It also refused eleven existing daemon fixture tests at local socket setup.
The same commands passed with approved local socket access.
The sandbox failures are environment failures and are not counted as implementation red evidence.
The workspace test command reports the unchanged unused observation-helper warning.

## Limits

This evidence verifies the encrypted loopback daemon and authenticated client fixtures.
It does not claim the deployed iPad Simulator journey required by IPAD-03.
Bonjour, LAN operation, physical iPad, sleep and wake, TestFlight and cloud work remain deferred.
No model was pulled and no subscription harness was used.

## Review fix evidence

Date: 2026-10-11.
Review base: `ab0e5b4f93c545e1abc515efbb65cc71d5024c62`.
The environment above is unchanged.
The controller selected the approved WebSocket architecture.
The gateway uses `tokio-tungstenite` 0.30 with the existing Noise protocol.
The binary-frame client asserts HTTP status 101, zero dispatches before authentication,
one authenticated turn and rejection of plaintext application bytes on an upgraded connection.
The tamper, revoked-device, vector and application-boundary fixtures remain in the full suite.

```text
$ cargo test -p mote-gateway --test loopback websocket_http_upgrade_and_binary_noise_frames_authenticate_before_dispatch
FAILED: HTTP upgrade returned Connection reset by peer.
exit status: 101

$ cargo test -p mote-gateway --test loopback opaque_request_id_survives_redaction_restart_and_outbox_correlation
FAILED: actual request_id "[redacted]", expected "sk-abcdefghijklmnopqrstuv".
exit status: 101

$ cargo test -p mote-gateway --test loopback concurrent_slow_turn
FAILED: actual terminal invalid_request, expected typed pending state.
exit status: 101

$ cargo test -p mote-gateway --test loopback interrupted_claim
FAILED: actual terminal invalid_request, expected typed indeterminate state.
exit status: 101

$ cargo test -p mote-protocol --test golden request_states
FAILED: unknown variant request_state.
exit status: 101

$ cargo test -p mote-gateway -p mote-protocol
47 passed; 0 failed.
Loopback: 11 passed; 0 failed.
Protocol goldens and strict decoding: 13 passed; 0 failed.
exit status: 0

$ cargo test --workspace
All 44 unit, integration and documentation suites passed.
Total: 549 passed; 0 failed.
exit status: 0

$ cargo clippy --workspace -- -D warnings
Finished dev profile with no warnings.
exit status: 0
```

The slow-turn fixture takes 5.2 seconds and dispatches once across concurrent retries.
A separate mutation restored the old five-second deadline and failed with `deadline has elapsed`.
Restoring the correction made the same test pass.
The interrupted fixture stops the gateway during a running turn, restarts it and retries the same ID.
It receives `indeterminate` before and after the old worker finishes.
The old connection receives no terminal success and the action is not dispatched a second time.
The redaction suite also catches secret-shaped application map keys without changing the envelope ID.
Unknown request states, empty request IDs and extra state fields still fail strict major-1 decoding.

Socket-dependent commands used approved local socket access.
Focused output: `/private/tmp/mote-task4-review.NHXtxx/green-focused-final.log`.
Mutation output: `/private/tmp/mote-task4-review.NHXtxx/red-timeout-mutation.log`.
Workspace output: `/private/tmp/mote-task4-review.NHXtxx/workspace-final.log`.
Clippy output: `/private/tmp/mote-task4-review.NHXtxx/clippy-final.log`.
The workspace test warning still names the unchanged unused observation helpers.
This correction does not add retry scheduling, native iPad recovery or automatic reconciliation.
