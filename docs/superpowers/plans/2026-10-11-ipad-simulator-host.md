# iPad Simulator Host Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver HOST-01 through HOST-05 as a versioned, multi-host, encrypted loopback connection from the iPad Simulator to `moted` while preserving the existing Unix client.

**Architecture:** Extract transport-neutral frames into `mote-protocol`, keep per-host client state in `mote-host-client`, and port the approved Noise gateway into `mote-gateway`. The first carrier binds an encrypted WebSocket to `127.0.0.1`; Bonjour remains an address hint and is not required for the Simulator connection. `moted` remains authoritative for events, surfaces and approvals.

**Tech Stack:** Rust 2021, serde, SQLite, `snow` 0.10, `tokio-tungstenite` 0.30, Noise IK and IKpsk1, existing Mote event log and IPC.

**Spec:** `docs/superpowers/specs/2026-10-11-ipad-host.md`

## Global Constraints

- Preserve `models.local_only = true` and the existing Unix peer-credential boundary.
- Use prologue `mote-gateway-v1`, pairing prefix `MOTE1`, service `_mote._tcp` and protocol major 1.
- Store the host private key with mode 0600 under the existing mode 0700 Mote data directory.
- Keep credentials outside ordinary host-directory JSON.
- Bind the Simulator gateway to `127.0.0.1`; do not expose an unauthenticated LAN listener.
- Hash the exact canonical action intent in memory and persist only its digest. Redact every stored
  event, payload and evidence record through the existing redaction path.
- Raw approval, configuration, file and surface mutation methods remain local-only.
- Run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before every commit.

## Review Focus

- A replayed request after reconnect must return its recorded outcome and never execute twice. Task 4 pins this with `replayed_request_returns_recorded_outcome`.
- A cursor older than retained history must receive an authoritative snapshot, not a partial replay. Task 6 pins this with `expired_cursor_installs_one_host_snapshot`.
- Two hosts with colliding thread, sequence and approval IDs must never share state. Task 2 pins this with `colliding_host_ids_are_isolated`.
- A discovered address claiming a known fingerprint must not replace the endpoint until Noise proves the pinned key. Task 5 pins this with `impersonator_cannot_move_known_host`.
- A protocol or host-key change must become an explicit client state and must not silently fail over. Task 6 pins this with `incompatible_or_changed_host_never_fails_over`.

---

### Task 1: Versioned host protocol

**Files:**
- Create: `crates/mote-protocol/Cargo.toml`
- Create: `crates/mote-protocol/src/lib.rs`
- Create: `crates/mote-protocol/src/version.rs`
- Create: `crates/mote-protocol/src/frame.rs`
- Create: `crates/mote-protocol/src/error.rs`
- Create: `crates/mote-protocol/src/snapshot.rs`
- Create: `crates/mote-protocol/tests/golden.rs`
- Create: `crates/mote-protocol/tests/golden/*.json`
- Modify: `Cargo.toml`
- Modify: `crates/mote-ipc/Cargo.toml`
- Modify: `crates/mote-ipc/src/lib.rs`

**Interfaces:**
- Produces: `PROTOCOL_MAJOR`, `VersionRange`, `HostId`, `RequestId`, `Cursor`, `ClientFrame`, `HostFrame`, `TypedInteraction`, `ApprovalDecision`, `HostSnapshot` and `HostError`.
- Preserves: `mote_ipc::Client`, `Server`, `EventStream`, `Dispatch` and the current newline JSON wire shape.

- [ ] **Step 1: Write failing protocol golden tests**

Add round-trip goldens for hello, call, subscribe, event, committed surface, interaction, approval decision, snapshot, acknowledgement and structured error. Add rejection tests for unsupported major versions, negative cursors, incomplete interaction contracts and malformed approval decisions.

- [ ] **Step 2: Run the protocol tests and confirm the crate is absent**

Run: `cargo test -p mote-protocol`
Expected: FAIL because `mote-protocol` is not a workspace package.

- [ ] **Step 3: Implement the protocol types and validators**

Use serde tagged enums. `HostId` is the lowercase full SHA-256 hex digest of a host public key. `RequestId` is an opaque nonempty string. `Cursor.seq` is nonnegative. `TypedInteraction` includes host, thread, surface sequence, contract, node path, event and arguments.

- [ ] **Step 4: Refactor Unix envelope helpers onto protocol types**

Keep all public Unix client and server behavior source-compatible. Do not move peer UID checks or socket lifecycle into `mote-protocol`.

- [ ] **Step 5: Run focused and compatibility tests**

Run: `cargo test -p mote-protocol && cargo test -p mote-ipc`
Expected: PASS, including the unchanged Unix reconnect test.

- [ ] **Step 6: Verify, document and commit HOST-01**

Record `docs/verification/HOST-01.md`, run workspace test and clippy, update the README checkbox, and commit `feat(protocol): add versioned host frames`.

### Task 2: Per-host client directory and isolation

**Files:**
- Create: `crates/mote-host-client/Cargo.toml`
- Create: `crates/mote-host-client/src/lib.rs`
- Create: `crates/mote-host-client/src/directory.rs`
- Create: `crates/mote-host-client/src/state.rs`
- Create: `crates/mote-host-client/src/connection.rs`
- Create: `crates/mote-host-client/src/outbox.rs`
- Create: `crates/mote-host-client/tests/multi_host.rs`
- Modify: `Cargo.toml`
- Modify: `mote-client/Cargo.toml`

**Interfaces:**
- Consumes: protocol types from Task 1.
- Produces: `HostTransport`, `HostEventStream`, `CredentialStore`, `HostDirectory`, `PairedHost`, `HostSessionState`, `ConnectionState` and per-host `Outbox`.

- [ ] **Step 1: Write failing two-host isolation tests**

Add `colliding_host_ids_are_isolated` with two hosts that share thread ID, event sequence, surface sequence and approval ID. Assert independent credentials, cursors, surfaces, approvals, outboxes and selection after repeated switching.

- [ ] **Step 2: Run the focused test and confirm failure**

Run: `cargo test -p mote-host-client --test multi_host`
Expected: FAIL because the crate and types do not exist.

- [ ] **Step 3: Implement directory, state and credential boundaries**

`CredentialStore` exposes load, save and delete by `HostId`. `HostDirectory` serializes descriptors and state but never credentials. Every reducer and outbox method requires an explicit `HostId`.

- [ ] **Step 4: Keep local transport identity cryptographic**

Use full fixture key digests in the directory tests. Do not invent a reserved or sentinel Unix
`HostId`. Delay the desktop cursor migration until the daemon exposes its real host identity in Task
4.

- [ ] **Step 5: Run isolation and cursor tests**

Run: `cargo test -p mote-host-client && cargo test -p mote-client cursor::`
Expected: PASS with no cross-host state.

- [ ] **Step 6: Verify, document and commit HOST-02**

Record `docs/verification/HOST-02.md`, run workspace test and clippy, update the README checkbox, and commit `feat(client): isolate state by host`.

### Task 3: Noise pairing core

**Files:**
- Create: `crates/mote-gateway/Cargo.toml`
- Create: `crates/mote-gateway/README.md`
- Create: `crates/mote-gateway/NOTICE`
- Create: `crates/mote-gateway/src/lib.rs`
- Create: `crates/mote-gateway/src/base32.rs`
- Create: `crates/mote-gateway/src/noise.rs`
- Create: `crates/mote-gateway/src/pairing.rs`
- Create: `crates/mote-gateway/src/devices.rs`
- Create: `crates/mote-gateway/src/classes.rs`
- Create: `crates/mote-gateway/src/net.rs`
- Create: `crates/mote-gateway/tests/noise.rs`
- Create: `crates/mote-gateway/tests/pairing.rs`
- Create: `crates/mote-gateway/tests/security.rs`
- Create: `crates/mote-gateway/tests/vectors/overseer-noise.json`
- Create: `crates/mote-gateway/tests/vectors/mote-noise.json`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`

**Interfaces:**
- Consumes: `mote-protocol` frames.
- Produces: `GatewayConfig`, `PairingOffer`, `HostIdentity`, `ClientCredential`, `DeviceStore`, `NoiseSession` and encrypted frame reader/writer.

- [ ] **Step 1: Write failing vector and pairing tests**

Test the preserved Overseer vector and a Mote vector using `mote-gateway-v1`. Test prefix `MOTE1`, single use, expiry, wrong static keys, altered identity, tampering, replayed counters and revocation.

- [ ] **Step 2: Run the focused tests and confirm failure**

Run: `cargo test -p mote-gateway`
Expected: FAIL because the crate does not exist.

- [ ] **Step 3: Port the approved source boundary**

Port only the gateway files from Overseer commit `1488b4be`. Record the source revision and derivation in README and NOTICE. Parameterize prologue, prefix, protocol version, identity directory, bind address and timeouts. Do not start or link the Overseer daemon.

- [ ] **Step 4: Implement key storage and host identity**

Create the host key atomically at mode 0600. Derive `HostId` from the public key. Store device credentials, counters and revocation in a `DeviceStore` implementation without exposing private keys through protocol frames.

- [ ] **Step 5: Run Noise and security tests**

Run: `cargo test -p mote-gateway`
Expected: PASS for both vectors and every fail-closed case.

- [ ] **Step 6: Commit the pairing core**

Run workspace test and clippy, then commit `feat(gateway): add Mote Noise pairing`.

### Task 4: Authenticated loopback daemon gateway

**Files:**
- Create: `crates/mote-gateway/src/server.rs`
- Create: `crates/mote-gateway/src/client.rs`
- Create: `crates/mote-gateway/tests/loopback.rs`
- Modify: `crates/mote-ipc/src/lib.rs`
- Modify: `moted/Cargo.toml`
- Create: `moted/src/gateway.rs`
- Modify: `moted/src/main.rs`
- Modify: `moted/src/watchers.rs`
- Modify: `mote-client/src/cursor.rs`

**Interfaces:**
- Consumes: Noise session from Task 3 and daemon `Dispatch` from `mote-ipc`.
- Produces: `GatewayServer::bind_loopback`, `GatewayClient::connect`, `Actor::{Local, PairedDevice}`, `Dispatch::call_with_context` and durable `remote_requests` outcomes.

- [ ] **Step 1: Write failing authenticated loopback tests**

Add an encrypted `127.0.0.1:0` journey for pair, hello, status, subscribe, turn, typed interaction and typed approval. Add `replayed_request_returns_recorded_outcome` and assert no dispatch before authentication.

- [ ] **Step 2: Run the loopback tests and confirm failure**

Run: `cargo test -p mote-gateway --test loopback`
Expected: FAIL because server and client are absent.

- [ ] **Step 3: Add contextual daemon dispatch and allowlist**

Give `Dispatch` a source-compatible default `call_with_context`. Unix uses `Actor::Local`. Paired devices may call hello, ping, status, subscribe, turn, typed interaction, snapshot and typed approval only. Reject raw approval, configuration, file and surface mutation methods.

- [ ] **Step 4: Add durable devices and request outcomes**

Create or migrate `devices` and `remote_requests` in `mote.db`. Claim each `(device_id, request_id)` before dispatch and return the stored terminal outcome on retry.

- [ ] **Step 5: Bind approval decisions to host and action**

Canonicalize and hash the exact intent in memory, then discard it and persist only the digest plus a
separately redacted display payload. A remote decision must match authenticated device, host, thread,
approval ID, action hash and expiry before execution.

- [ ] **Step 6: Migrate the desktop cursor under the real host identity**

After the daemon exposes the public-key-derived `HostId`, migrate the existing single
`client_cursor` once into that host and `main` scope. Leave the original file intact until the
per-host write succeeds atomically.

- [ ] **Step 7: Run daemon and gateway tests**

Run: `cargo test -p mote-gateway --test loopback && cargo test -p moted approval_dispatch_tests`
Expected: PASS with no duplicate side effect.

- [ ] **Step 8: Verify, document and commit HOST-03**

Record `docs/verification/HOST-03.md`, run workspace test and clippy, update the README checkbox, and commit `feat(gateway): serve paired clients on loopback`.

### Task 5: Bonjour address hints

**Files:**
- Create: `crates/mote-gateway/src/discovery.rs`
- Create: `crates/mote-gateway/tests/bonjour.rs`
- Modify: `moted/src/gateway.rs`

**Interfaces:**
- Produces: `Advertiser`, `DiscoveryBrowser`, `DiscoveryRecord { endpoint, protocol_major, fingerprint }` and `EndpointCandidate`.
- Constraint: discovery updates an endpoint only after Noise proves the pinned host key.

- [ ] **Step 1: Write failing discovery tests**

Add exact TXT tests containing only protocol major and fingerprint. Add browser discovery, moved-port,
advertiser shutdown, discovery-disabled and `impersonator_cannot_move_known_host` tests.

- [ ] **Step 2: Run the tests and confirm failure**

Run: `cargo test -p mote-gateway --test bonjour`
Expected: FAIL because discovery is absent.

- [ ] **Step 3: Implement the advertiser boundary**

Keep process invocation behind `Advertiser` and `DiscoveryBrowser`; construct `dns-sd` arguments
without a shell and always terminate the child. Tests use fake implementations. The Simulator can
still use explicit localhost when discovery is unavailable.

- [ ] **Step 4: Verify identity before endpoint movement**

Treat Bonjour as an address hint. Do not persist the new endpoint until the gateway handshake proves the known public key.

- [ ] **Step 5: Record host-side evidence and commit the discovery implementation**

Run gateway tests, workspace test and clippy. Record the fake-browser and live Mac-stack results in
`docs/verification/HOST-04.md`, but leave HOST-04 open until the UI plan drives discovery from the
Simulator. Commit `feat(gateway): discover local Mote hosts`.

### Task 6: Resync, snapshot and connection states

**Files:**
- Modify: `crates/mote-eventlog/src/lib.rs`
- Create: `moted/src/host_snapshot.rs`
- Modify: `moted/src/main.rs`
- Modify: `crates/mote-host-client/src/connection.rs`
- Modify: `crates/mote-host-client/src/outbox.rs`
- Create: `crates/mote-gateway/tests/reconnect.rs`
- Modify: `mote-client/src/stream.rs`

**Interfaces:**
- Produces: `EventLog::oldest_retained`, checked replay, `HostSnapshotBuilder`, durable
  `ThreadSnapshot` projections, explicit `ConnectionState`, atomic per-host snapshot installation and
  acknowledged outbox removal.

- [ ] **Step 1: Write failing recovery tests**

Add frame-cut reconnect cases, pending approval survival, daemon restart,
`expired_cursor_replaces_all_visible_state_for_one_host`, and
`incompatible_or_changed_host_never_fails_over`.

- [ ] **Step 2: Run focused tests and confirm failure**

Run: `cargo test -p mote-gateway --test reconnect && cargo test -p mote-eventlog`
Expected: FAIL because checked replay and snapshots are absent.

- [ ] **Step 3: Add checked replay and authoritative snapshots**

Return `ResyncRequired` when the requested cursor predates retained history. Maintain a durable
client-visible projection transactionally with committed events. Build a snapshot through one fixed
cursor containing thread identities and messages, latest committed surfaces, pending typed approvals
and current host status. Exclude local paths, credentials, cookies and unredacted payloads.

- [ ] **Step 4: Implement explicit client recovery**

Expose connecting, online, offline, incompatible and identity-changed states. Normal reconnect ignores `seq <= cursor`. Resync replaces only the selected host cache. Never select another host automatically.

- [ ] **Step 5: Implement the per-host acknowledged outbox**

Persist unacknowledged requests by `HostId` and `RequestId`. Remove only after `Ack`; reuse the same request ID after transport loss so the daemon can return its stored outcome.

- [ ] **Step 6: Verify, document and commit HOST-05**

Run host, gateway, event-log and workspace tests plus clippy; record `docs/verification/HOST-05.md`; update the README checkbox; commit `feat(host): recover paired sessions without duplicates`.

### Task 7: Loopback host acceptance

**Files:**
- Create: `crates/mote-testkit/src/gateway.rs`
- Modify: `crates/mote-testkit/src/lib.rs`
- Modify: `docs/verification/HOST-01.md`
- Modify: `docs/verification/HOST-02.md`
- Modify: `docs/verification/HOST-03.md`
- Modify: `docs/verification/HOST-04.md`
- Modify: `docs/verification/HOST-05.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Consumes: completed HOST-01 through HOST-05 interfaces.
- Produces: one reusable paired loopback test harness for the iPad Simulator plan.

- [ ] **Step 1: Build the paired loopback harness**

Expose a temporary daemon, `MOTE1` offer, paired client, endpoint rotation, forced disconnect, retention fill and event inspection without bypassing Noise.

- [ ] **Step 2: Run the full loopback journey**

Pair at `127.0.0.1`, subscribe to `main`, send a turn, receive chat and a committed Matrix surface, send a typed interaction, reconnect from the host cursor and answer a host-bound approval.

Run the scripted suspend and reconnect journey autonomously. Leave the literal Mac sleep and wake
clause open unless a safe preconfigured wake mechanism is available or the owner is present. Do not
substitute daemon restart evidence for that clause.

- [ ] **Step 3: Run final host verification**

Run: `cargo test --workspace`
Expected: PASS with all host and existing Unix tests.

Run: `cargo clippy --workspace -- -D warnings`
Expected: PASS with no warnings.

- [ ] **Step 4: Check parent consistency and commit**

Add the loopback results to the five HOST evidence files. Check only criteria whose full Verify clause
has passed. HOST-04 remains open until Simulator discovery evidence exists. HOST-05 remains open if
the literal sleep/wake run is deferred. Do not check any IPAD criterion from this harness alone.
Update the handoff and commit `test(gateway): verify paired loopback host`.
