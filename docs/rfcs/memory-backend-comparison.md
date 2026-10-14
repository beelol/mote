# RFC: replaceable memory and a bounded Tencent comparison

Status: owner-activated planning on 2026-10-13. The acceptance board is in the root README under Ready for Go, Memory backend comparison. This RFC does not change the v0 defaults in [v0.md](v0.md). SQLite remains the production memory store. TencentDB Agent Memory is a local, opt-in comparison candidate until the decision gate is verified.

## Outcome and authority

Mote owns its event log, source pointers, permissions, realms, working context, task state, ledger, and user controls. A memory backend may index and retrieve eligible redacted memory but cannot grant access, choose a thread's realm, delete an authoritative source, or silently inject context. No raw personal store is copied to a candidate for the comparison. A local Tencent service must not contact a cloud model or fetch a new model. The only permitted models remain those in v0 section 0. Every generative call, including background extraction, must have the same ledger and pack accountability as other Mote calls. Until that can be demonstrated, extraction stays disabled.

The aim is not to implement every proposed memory feature. Fix the current paths, provide one narrow replaceable boundary, and measure whether Tencent improves useful recall enough to justify its runtime and privacy cost. A failed or inconclusive comparison leaves SQLite as default.

## Current state and limits

`crates/mote-context/src/mem.rs` stores local nodes, FTS rows and int8 vectors and fuses text, vector, graph-neighbour and recency signals for turn-time retrieval. `crates/mote-context/src/lib.rs` opens a store with `HashEmbedder`. `moted/src/main.rs` replaces it with `PotionEmbedder` when the model loads, but `moted/src/mcp.rs` opens a separate store for the think tool and calls the text-only `memory_search` path. The exact production embedding mismatch and its migration impact must be reproduced before choosing a fix. A stored vector must never be queried as if it were produced by a different model. `write_node` says it is transactional, but its node, FTS and vector inserts are separate statements. Its failure atomicity must be tested and corrected.

The turn pack contains a working set and up to eight retrieved memories. The working set appends turns without a durable compaction or pinned-instruction policy. `MemHit` contains id, kind and text only. Memory search and write do not take an explicit realm, while writes default to `personal`; retrieval does not filter by realm. Forgetting marks a node superseded but is not a complete erase of copies or derived candidate data. These are gaps, not verified regressions against every earlier AC. Recheck affected v0 evidence after changes.

The broader connector and realm topology in [context-layer.md](context-layer.md) remains backlog. This goal implements only the memory boundary necessary to prevent personal/work mixing. It does not activate all CTXL criteria, Pi work, cloud relay, automatic conversation capture, or a wholesale Tencent proxy.

## Boundary and journeys

Expose one memory interface for write, retrieve, source lookup, correction, and forget. Requests carry an explicit account and realm scope. Results carry stable Mote-owned source references and a backend-neutral score or rank. The daemon checks scope before handing data to a backend and again before placing returned results in a pack. A backend failure returns an explicit unavailable result; it does not silently cross realms or claim an answer from another backend. An opt-in candidate can be disabled without losing Mote's authoritative log.

The owner can ask what Mote knows and why, correct an outdated fact, or forget it. Corrections retain an audit pointer and replace the visible fact. Forget hides it from future packs and both backend indexes, with deletion confirmation or an explicit partial-failure state. The UI presents these controls without requiring the user to know backend names. A work-scoped thread must not retrieve a personal fact even if text, vectors or graph links match. Account changes and a restart retain that boundary.

Context assembly keeps the newest turn and charter intact. A bounded working-set policy preserves pinned instructions and recent turns before older conversational material. Compaction may summarize older history only through an approved local model call with ledger and source coverage, and may not erase raw authoritative events. Malformed summaries leave the prior context intact. A small deterministic test demonstrates that an old long history cannot displace the newest turn.

## Comparison and resource ceiling

Use one checked-in, synthetic, redacted fixture set with labelled expected source IDs. It must include paraphrases, long histories, time changes, corrections, forgets, missing evidence, identical names in separate realms, and one account switch. Run SQLite and Tencent over the same ingest, queries, scope, and output budget. Report recall@5, MRR, correction and forget correctness, cross-realm leak count, p50/p95 write and retrieval latency, tokens, model calls, wall time, peak resident memory, and disk growth. Do not treat speed-only `moted bench memory` results as recall evidence.

Start with at most 100 synthetic memories and 30 labelled queries. Run each backend once with a fixed seed, with one repeated timing run only if results are noisy. Cap generated/extraction calls at 100 total and require explicit opt-in for any live-model run. Use the existing approved local models only. Stop on any outbound network call other than loopback, any cross-realm hit, or any deletion leak. Do not download full benchmark corpora, run unattended extraction over personal history, or keep the Tencent service running after the comparison. Larger benchmarks require a separate justified AC.

The decision report states the exact pinned Tencent revision and license, setup and operational burden, failures, and all measurements. A meaningful win requires at least a 10 percentage point absolute recall@5 gain over corrected SQLite on the same fixture without a lower MRR, zero isolation or forget failures, and no breach of the v0 talk retrieval budget. If no candidate meets that gate, SQLite remains the only enabled production backend. Passing the gate permits a documented opt-in Tencent backend, not an automatic default switch. Default adoption requires a separate owner decision and migration/rollback evidence.

## Evidence and dependencies

Each README criterion has `docs/verification/MBE-<number>.md` with command, output, environment, date, commit and limitations. Before every commit run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`. Tests must exercise refusal and partial-failure paths. Relevant v0 evidence, especially V0-31 and V0-33 through V0-36, must be rechecked where the implementation changes their behavior. Keep the unrelated iPad and TestFlight goal open.

Tencent's own [repository](https://github.com/TencentCloud/TencentDB-Agent-Memory) and [installation guide](https://github.com/TencentCloud/TencentDB-Agent-Memory/blob/feat/server_team/INSTALL.md) are starting references, not proof of local-only behavior or compatibility. The spike verifies the exact version and its network and model-call behavior before connecting it to Mote.
