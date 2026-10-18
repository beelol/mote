# Replaceable memory backends design

Date: 2026-10-17

Status: approved and already activated under MBE-01 through MBE-08

## Outcome

Mote owns one narrow memory contract. The corrected SQLite implementation remains the default.
TencentDB Agent Memory is an optional local adapter evaluated against the same synthetic cases.
The adapter cannot receive production memory, widen scope or make an unledgered model call.

This design refines the existing contract in `docs/rfcs/memory-backend-comparison.md`. The MBE
criteria remain authoritative and are not renumbered.

## Alternatives considered

Replacing SQLite wholesale would make candidate behavior authoritative before isolation,
correction and deletion are proven. Embedding Tencent's TypeScript implementation inside the
Rust daemon would add a second runtime and blur process ownership. The chosen approach keeps a
Rust contract and talks to a pinned loopback-only Tencent MemoryCore process through its v3 API.

## Crate shape

```text
crates/mote-memory/
  backend contract, scope, records, sources, corrections and forget results

crates/mote-memory-sqlite/
  transactional nodes, text index, vectors, graph edges and migrations

crates/mote-memory-tencent/
  bounded loopback HTTP adapter for the pinned v3 MemoryCore API
```

`mote-context` builds context packs from `mote-memory` results. It does not know the backend.
`moted` owns configuration, lifecycle, the local model ledger and events. The Tencent adapter does
not start a service, read Mote's production database or call a model directly.

## Contract

Every operation requires an explicit account and realm scope. The contract supports write,
retrieve, source lookup, correction, forget and health. Results carry stable Mote source IDs and
backend provenance. Unavailable, partial and refused outcomes are structured.

Correction writes a new authoritative value, invalidates the old search value and preserves an
audit pointer. Forget removes the searchable value and embeddings while retaining only the
minimum tombstone needed to prove the operation. A partial candidate deletion is visible and
never reported as success.

## SQLite adapter

The SQLite adapter commits the node, text index, vector, embedding model identifier and relevant
edges in one transaction. Query vectors use the same embedding model as indexed vectors. Existing
hash vectors receive an explicit migration or remain excluded from incompatible semantic search.

The agent tool and context pack use the same scoped fused retrieval. Plain text matching is not a
separate authority.

## Tencent adapter

The candidate revision is
`TencentCloud/TencentDB-Agent-Memory@e9a82935b52741e0c798c75326e90b7144eb4d5f` on its
`feat/server_team` branch. The candidate is MIT licensed at that revision. Only the standalone
MemoryCore service is considered.

The adapter uses v3 identifiers for `team_id`, `agent_id` and `user_id`. Mote maps its account and
realm into every request and verifies returned scope. The configured base URL must resolve to
loopback. Redirects, proxies and non-loopback destinations are refused.

Remote embeddings are disabled. Extraction is disabled unless MemoryCore is configured to use a
Mote-owned loopback model proxy that writes the normal ledger and enforces the v0 model allowlist.
The comparison can run BM25-only without an extraction call.

## Evaluation

Both adapters receive at most 100 synthetic redacted memories and 30 labelled queries. Optional
local extraction is capped at 100 ledgered calls. The run stops on a non-loopback destination,
cross-realm result or deletion leak.

The report records recall at five, mean reciprocal rank, correction and forget behavior, realm
leaks, p50 and p95 latency, model calls, tokens, wall time, peak memory and disk growth. Tencent
must improve recall at five by at least ten absolute percentage points without lowering mean
reciprocal rank or breaking any safety and latency gate. Passing permits opt-in use. It does not
change the default.

## Failure behavior

An unavailable candidate produces a structured unavailable result. It never falls through to a
different backend silently. A backend change is configuration for a new operation. It does not
silently migrate or duplicate existing personal data. Migration, default adoption and rollback
need a later owner decision.
