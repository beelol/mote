# Mote gateway core

This crate provides owner-confirmed pairing and authenticated encrypted host frames.
It has no daemon dispatcher, socket listener, discovery service or model integration.

## Provenance

The owner approved reuse from Overseer revision
`1488b4be0ce0f67311f21c2e5ba7565f4268187d`.
The source boundary is `daemon/src/gateway/base32.rs`, `noise.rs`, `pairing.rs`,
`devices.rs`, `classes.rs` and `net.rs`, plus `protocol/vectors/noise.json`.
Base32 and local network helpers retain the source algorithms.
Noise framing retains the source chunk format.
Pairing retains IKpsk1, the 16-byte secret, the 120-second window and the five-attempt limit.
Device storage retains the atomic counter update and revocation rule.
Authorization retains denial of unknown and local-owner methods.
The crate rewrites the source daemon coupling as a standalone Mote API.
It never links or starts Overseer.

The preserved Overseer fixture is byte-identical to the pinned source.
The Mote fixture uses `mote-gateway-v1` and full host and client identity fields.
It is regenerated with `cargo run --quiet -p mote-gateway --example vectors`.
Fixture private keys are deterministic test material.
They are never runtime credentials.

## Security boundary

`GatewayConfig::new` selects prologue `mote-gateway-v1`, prefix `MOTE1`,
protocol major 1 and bind address `127.0.0.1:47810`.
The prologue, prefix, data directory, bind address, limits and timeouts are explicit fields.
The protocol major must be supported by `mote-protocol`.

`HostIdentity::load_or_create` creates a 0700 data directory and a 0600 host key.
It publishes a synced complete key through an atomic hard link without replacing an existing key.
It refuses symlinks, wrong ownership, unsafe modes and malformed key lengths.
The full SHA-256 digest of the public key is the stable `HostId`.

`PairingOffer::open` must be called by the local owner surface.
The offer is memory-only and closes at expiry, after five failed attempts or after one valid use.
The code carries only the public host key, pairing secret, port and discovery addresses.
`PendingHandshake::pairing` authenticates message 1 before consuming the offer.
`PendingHandshake::confirm` requires the owner's affirmative decision before storing the client
and returning message 2 or a session.
The pending confirmation expires independently after 60 seconds.

`ClientCredential` does not implement serialization or secret-bearing debug output.
Its storage accessor is exclusively for the client's secure local credential store.
`restore` pins the same host public key after credential reload.
The client must persist and advance its counter before each session attempt.
The server atomically accepts only a counter above the last accepted value.
The first pairing counter is included in that rule.

`DeviceStore` persists public client identities, names, counters and revocation in SQLite.
Every row is isolated by the full host ID.
No private key or pairing secret enters the database or protocol frames.
A revoked identity cannot reconnect or re-register through another offer.
The listener must check revocation for active sessions when it implements immediate disconnection.

Both handshake payloads bind protocol, host identity and client identity.
`NoiseSession` permits application frames only after the completed authenticated handshake.
An encrypted envelope binds every frame to the same three fields.
Frames use ordered Noise nonces and bounded encrypted chunks.
Tampering, replay, reordering, wrong role, invalid identity, malformed JSON and size breaches
permanently close the session reader and writer.
The caller closes the carrier on any error.

## Integration limits

The daemon must enforce `handshake_timeout` while receiving transport bytes.
It must expose pairing start and confirmation only to the local owner.
It must perform event redaction before storage and filter outbound state before serialization.
Encryption does not grant permission to export secrets, browser cookies or local paths.
The `classes` allowlist is a narrow starting point for the future dispatcher.
The dispatcher must explicitly classify any additional method before exposing it remotely.
Bonjour, WebSocket framing, keychain integration and physical-device verification belong to later tasks.
