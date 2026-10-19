# MBE-06: Common memory backend contract

Status: partial. Do not check MBE-06 yet.

Date: 2026-10-18.

Environment: macOS Darwin 25.6.0 arm64, Rust 2021 workspace.

This increment defines the backend-neutral `mote-memory` contract. Every operation receives an
explicit account and realm. The contract covers write, retrieval, source lookup, correction,
forget and health with stable source references, optimistic versions and structured unavailable,
refused, conflict and partial-deletion outcomes. It contains no SQLite or Tencent fields.

The reusable contract suite exercises normal writes and retrieval, account and realm isolation,
source lookup, correction, forget, malformed inputs and explicit unavailable and partial states.
Its in-memory implementation is test-only and does not change Mote's production SQLite default.

The `mote-memory-sqlite` adapter now implements the contract with atomic node, FTS and vector
writes. Stored vectors carry explicit embedding model IDs. Legacy unlabeled vectors are migrated
as `hash-v1` and excluded when the active model differs. Correction and forget use immediate,
scope-bound, version-guarded transactions, including competing-connection tests with exactly one
winner. Forgotten records retain only a scoped tombstone and mutation pointer, not searchable text
or embeddings.

Focused verification:

```text
cargo test -p mote-memory
4 passed; 0 failed

cargo clippy -p mote-memory --all-targets -- -D warnings
passed

cargo test -p mote-memory-sqlite
11 passed; 0 failed

cargo clippy -p mote-memory-sqlite --all-targets -- -D warnings
passed

git diff --check
passed
```

MBE-06 remains open until production selection routes SQLite through this adapter while preserving
the absent-value default and affected v0 memory evidence is rechecked.
