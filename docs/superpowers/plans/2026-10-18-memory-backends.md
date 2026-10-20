# Replaceable Memory Backends Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put corrected SQLite memory and an opt-in loopback TencentDB Agent Memory candidate behind one scoped Rust contract, then compare them with bounded synthetic fixtures.

**Architecture:** `mote-memory` owns backend-neutral types and behavior. SQLite remains embedded and authoritative by default. The Tencent adapter speaks the pinned MemoryCore v3 API over loopback and cannot call a model except through a Mote-owned ledgered proxy.

**Tech Stack:** Rust 2021, rusqlite, sqlite-vec, ureq, url, serde, serde_json, pinned TencentDB Agent Memory MemoryCore revision `e9a82935b52741e0c798c75326e90b7144eb4d5f`.

**Spec:** `docs/superpowers/specs/2026-10-17-memory-backends.md`

## Global Constraints

- Keep SQLite as the production default and event log as the audit authority.
- Require explicit account and realm scope for every operation and returned result.
- Use at most 100 synthetic redacted memories, 30 queries and 100 optional ledgered extraction calls.
- Stop on a non-loopback destination, cross-realm result or deletion leak.
- Pull no model and send no production memory to Tencent.
- Treat a passing Tencent gate as opt-in permission only, not a default switch.
- Run workspace tests and strict clippy before every commit, add MBE evidence and push.

## Review Focus

- Legacy hash vectors queried with Potion embeddings must never produce mixed-model semantic results. Task 2 pins this.
- Realm scope must constrain graph expansion as well as seed retrieval. Task 3 pins this.
- Correction or forget partial failure must remain visible and exclude old text from Mote context. Tasks 4 and 6 pin this.
- Redirects, proxies, wildcard binds and DNS names resolving outside loopback must be refused. Task 6 pins this.
- Candidate extraction must be impossible without a ledgered local-model proxy. Task 7 pins this.

---

### Task 1: Backend-neutral memory contract

**Files:**
- Create: `crates/mote-memory/Cargo.toml`
- Create: `crates/mote-memory/src/lib.rs`
- Create: `crates/mote-memory/src/backend.rs`
- Create: `crates/mote-memory/src/types.rs`
- Create: `crates/mote-memory/tests/contract.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `MemoryScope::new(account_id, realm)`, `MemoryId`, `SourceRef`, `MemoryWrite`, `MemoryHit`, `Correction`, `CorrectionResult`, `ForgetResult`, `BackendHealth` and `MemoryError`.
- Produces: `MemoryBackend: Send` with synchronous methods `write`, `retrieve`, `source`,
  `correct`, `forget` and `health` returning structured results.
- Produces: `run_contract_suite(factory: impl Fn() -> Box<dyn MemoryBackend>)` for every adapter.

- [ ] **Step 1:** Write the backend-neutral suite for write, retrieval, source, correction, forget, scope refusal, malformed input, unavailable backend and explicit partial deletion.
- [ ] **Step 2:** Run `cargo test -p mote-memory` and confirm it fails before the crate exists.
- [ ] **Step 3:** Implement validated types, error variants and the object-safe backend trait without SQLite or Tencent fields.
- [ ] **Step 4:** Add an in-memory fake backend only for contract harness testing and run the suite.
- [ ] **Step 5:** Run workspace gates and commit `feat: define scoped memory backend` and push.

### Task 2: Transactional SQLite adapter and embedding migration

**Files:**
- Create: `crates/mote-memory-sqlite/Cargo.toml`
- Create: `crates/mote-memory-sqlite/src/lib.rs`
- Create: `crates/mote-memory-sqlite/src/schema.rs`
- Create: `crates/mote-memory-sqlite/src/embed.rs`
- Create: `crates/mote-memory-sqlite/tests/contract.rs`
- Create: `crates/mote-memory-sqlite/tests/migration.rs`
- Modify: `Cargo.toml`
- Modify: `crates/mote-context/src/mem.rs`

**Interfaces:**
- Produces: `EmbeddingModelId`, `Embedder::model_id(&self) -> &EmbeddingModelId` and `SqliteMemory::open(path, embedder, default_account)`.
- Migrates `mem_node` and `mem_edge` to explicit `account_id`, `realm` and `embedding_model` fields.

- [ ] **Step 1:** Write a failure-injection test that aborts between node, FTS and vector writes and leaves all three absent.
- [ ] **Step 2:** Write migration tests for legacy hash vectors, current Potion vectors, restart and model mismatch refusal.
- [ ] **Step 3:** Run targeted tests and reproduce the existing nontransactional write and unlabelled-vector behavior.
- [ ] **Step 4:** Implement one SQLite transaction for node, FTS and vector writes plus explicit embedding IDs.
- [ ] **Step 5:** Migrate existing hash vectors to `hash-v1` and exclude or reindex them before any `potion-base-8m` semantic query.
- [ ] **Step 6:** Run the common contract and migration suites.
- [ ] **Step 7:** Run workspace gates, add MBE-01 evidence, commit `fix: make memory writes model consistent` and push.

### Task 3: Scoped fused retrieval everywhere

**Files:**
- Create: `crates/mote-memory-sqlite/src/retrieve.rs`
- Create: `crates/mote-memory-sqlite/tests/retrieve.rs`
- Modify: `crates/mote-context/src/lib.rs`
- Modify: `moted/src/mcp.rs`
- Modify: `moted/src/main.rs`

**Interfaces:**
- Consumes: `MemoryScope` and `MemoryBackend::retrieve`.
- Produces stable source IDs in the form `memory:<backend>:<id>:<version>`.
- Removes the separate `search_like` authority from the MCP path.

- [ ] **Step 1:** Add same-name personal, work and changed-account fixtures plus a cross-realm graph edge and a semantic-only query that text matching misses.
- [ ] **Step 2:** Run the tests and confirm current MCP text search returns the wrong or incomplete set.
- [ ] **Step 3:** Implement account and realm predicates in FTS, vector candidate filtering, both graph hops and final row fetch.
- [ ] **Step 4:** Route MCP and context retrieval through the same backend call and stable source IDs.
- [ ] **Step 5:** Test zero unauthorized results after restart, bounded limits, malformed query and unavailable backend.
- [ ] **Step 6:** Run workspace gates, add MBE-02 and MBE-03 evidence, commit `fix: scope fused memory retrieval` and push.

### Task 4: Sources, correction and forget semantics

**Files:**
- Create: `crates/mote-memory-sqlite/src/mutate.rs`
- Create: `crates/mote-memory-sqlite/tests/mutate.rs`
- Modify: `crates/mote-context/src/lib.rs`
- Modify: `moted/src/mcp.rs`
- Modify: `moted/src/pipeline.rs`
- Create: `matrix/examples/memory-controls.matrix`

**Interfaces:**
- Implements `source`, `correct` and `forget` from the common contract.
- Produces Matrix actions `memory.correct` and `memory.forget` bound to scope, source ID and current version.

- [ ] **Step 1:** Write tests proving correction replaces searchable text, forget removes text, FTS and vectors, audit pointers remain, and stale or cross-scope controls are inert.
- [ ] **Step 2:** Add a forced candidate-deletion failure and assert a structured partial result rather than success.
- [ ] **Step 3:** Run targeted tests and confirm the current supersede-only path leaks old index entries or lacks controls.
- [ ] **Step 4:** Implement transactional SQLite mutation and expose source provenance through MCP and Matrix.
- [ ] **Step 5:** Run contract, context and surface tests and confirm old text is absent from search and packs.
- [ ] **Step 6:** Run workspace gates, add MBE-05 evidence, commit `feat: correct and forget memory safely` and push.

### Task 5: Context preservation and SQLite default integration

**Files:**
- Modify: `crates/mote-context/src/lib.rs`
- Create: `crates/mote-context/tests/backend.rs`
- Modify: `moted/src/main.rs`
- Modify: `moted/src/lanes.rs`
- Modify: `moted/src/config.rs`

**Interfaces:**
- `ContextStore` owns `Box<dyn MemoryBackend>` behind its existing daemon mutex instead of owning
  memory tables directly.
- Accepts optional config key `memory.backend` with allowed values `sqlite` and `tencent`; absence
  means SQLite without changing `DEFAULT_CONFIG_TOML`, and configured Tencent never falls back.

- [ ] **Step 1:** Write a long-history test for charter, pinned instructions, newest turn, source coverage, malformed compaction and ledger counts.
- [ ] **Step 2:** Write backend-selection tests proving default SQLite behavior, explicit unavailable Tencent behavior and unchanged event-log authority.
- [ ] **Step 3:** Run tests and confirm current context trimming can discard protected parts and has no backend boundary.
- [ ] **Step 4:** Inject the backend, preserve v0 SQLite as default and use the runtime plan's structured compactor.
- [ ] **Step 5:** Run the common contract through SQLite and all affected v0 context evidence.
- [ ] **Step 6:** Run workspace gates, add MBE-04 and MBE-06 evidence, commit `feat: make memory backend replaceable` and push.

### Task 6: Loopback Tencent v3 adapter

**Files:**
- Create: `crates/mote-memory-tencent/Cargo.toml`
- Create: `crates/mote-memory-tencent/src/lib.rs`
- Create: `crates/mote-memory-tencent/src/client.rs`
- Create: `crates/mote-memory-tencent/src/scope.rs`
- Create: `crates/mote-memory-tencent/src/state.rs`
- Create: `crates/mote-memory-tencent/tests/contract.rs`
- Create: `crates/mote-memory-tencent/tests/http.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `TencentConfig::new(base_url: Url, service_id, team_id, namespace_prefix, state_path,
  timeout) -> Result<TencentConfig, MemoryError>` and `TencentMemory::new(config, client)`.
- Uses `POST /v3/conversation/add`, `/query`, `/search`, `/delete` for model-free L0 memory and optional `/v3/atomic/query`, `/search`, `/update`, `/delete` for ledgered extraction mode.
- Maps account to `user_id` and realm to `agent_id` under the configured namespace prefix. It
  sends explicit `team_id`, `agent_id` and `user_id` in every request. The pinned L0 response
  schema makes those fields optional, so present scope fields must match and every returned
  server-generated candidate ID and exact content must be bound to the isolated sidecar for the
  requested scope. Unknown IDs, mismatched content and mismatched scope fields are refused.
- Keeps only candidate record, version, source and partial-operation mappings in the adapter's
  isolated sidecar state path. It never opens Mote's production SQLite database.

- [x] **Step 1:** Build a loopback fixture server with success, nonzero envelope, malformed JSON, timeout, redirect, mismatched scope and partial delete responses.
- [ ] **Step 2:** Run adapter tests and confirm the crate is missing.
- [x] **Step 3:** Implement URL validation that accepts only literal loopback or names resolving exclusively to loopback, disables redirects and ignores proxy environment variables.
- [x] **Step 4:** Map write to one-message `conversation/add` in a record-specific session and
  retain the server-generated candidate ID, retrieve to `conversation/search`, source to scoped
  `conversation/query`, correction to checkpoint and add the replacement before deleting the old
  candidate, and forget to scoped delete. Ambiguous writes remain hidden until reconciled.
- [x] **Step 5:** Run the common contract and HTTP refusal tests without starting Tencent.
- [x] **Step 6:** Run workspace gates, commit `feat: add loopback Tencent memory adapter` and push.

### Task 7: Pinned MemoryCore lifecycle and ledgered extraction

**Files:**
- Create: `scripts/tencent-memory.sh`
- Create: `moted/src/memory_proxy.rs`
- Create: `moted/tests/memory_proxy.rs`
- Create: `docs/verification/MBE-07.md`
- Modify: `moted/src/config.rs`

**Interfaces:**
- Script commands: `preflight`, `start`, `status` and `stop`, pinned to revision `e9a82935b52741e0c798c75326e90b7144eb4d5f` and loopback port 8420.
- `LedgeredModelProxy` accepts only loopback MemoryCore requests, the v0 allowlist and explicit Mote scope, then records the normal pack and ledger row.

- [ ] **Step 1:** Write dry-run tests for missing Node 22.16, wrong revision, occupied port, non-loopback bind, production data path and absent local model.
- [ ] **Step 2:** Write proxy tests for allowed structured local extraction, forbidden model, malformed response and one ledger row per call.
- [ ] **Step 3:** Implement lifecycle commands without installing globally, pulling a model or starting the full Tencent stack.
- [ ] **Step 4:** Start the pinned standalone MemoryCore only against an isolated temporary directory and verify `/health`, destinations and teardown.
- [ ] **Step 5:** Run synthetic writes with extraction disabled, then optionally enable extraction only through the ledgered proxy within the 100-call ceiling.
- [ ] **Step 6:** Record version, license, setup, network observations, calls, teardown and failures in MBE-07 evidence.
- [ ] **Step 7:** Run workspace gates, commit `feat: supervise local Tencent memory` and push.

### Task 8: Bounded backend comparison and decision report

**Files:**
- Create: `crates/mote-memory/tests/fixtures/comparison.json`
- Create: `moted/src/memory_bench.rs`
- Create: `moted/tests/memory_bench.rs`
- Create: `docs/verification/MBE-08.md`
- Modify: `moted/src/main.rs`

**Interfaces:**
- Produces command `moted bench memory-backends --backend <sqlite|tencent> --fixtures <path> --output <path>`.
- Produces JSON fields `recall_at_5`, `mrr`, `corrections`, `forgets`, `realm_leaks`, `p50_ms`, `p95_ms`, `tokens`, `model_calls`, `wall_ms`, `peak_rss_bytes` and `disk_growth_bytes`.

- [ ] **Step 1:** Check in at most 100 synthetic memories and 30 labelled queries with corrections, forgets and cross-realm decoys.
- [ ] **Step 2:** Add scorer tests for exact metric calculations and immediate stop on isolation, deletion or destination violations.
- [ ] **Step 3:** Implement the runner with identical limits and output budgets for both adapters.
- [ ] **Step 4:** Run corrected SQLite and the pinned Tencent candidate in isolated directories and preserve raw JSON reports.
- [ ] **Step 5:** Apply the ten-point recall gate, nonlower MRR, zero leak and forget requirements, and v0 latency budget without changing the default.
- [ ] **Step 6:** Run workspace gates, complete MBE-08 evidence and the parent only if every MBE criterion passes, commit `test: compare memory backends` and push.

## Acceptance contract

This technical section preserves the full checks behind the concise root board.

- **MBE-01:** Memory writes are atomic and indexed vectors identify their embedding model. Verify
  daemon and MCP paths, injected failure between node, text and vector writes, restart, migration
  and stored model IDs.
- **MBE-02:** Agent search and turn packs use the same scoped fused retrieval with stable source
  IDs. Verify semantic-only matches, budgets, unavailable backend and malformed queries.
- **MBE-03:** Writes, queries, graph expansion, correction and packs enforce account and realm.
  Verify personal and work fixtures, account change, restart and a cross-realm edge leak nothing.
- **MBE-04:** Long-history context preserves the charter, pinned instructions and newest turn.
  Verify bounded or source-covered compaction, malformed compaction, recovery and ledger counts.
- **MBE-05:** Matrix exposes sources, correction and forget with partial failure. Verify old text
  disappears from search and packs, audit pointers remain and candidate deletion failure is shown.
- **MBE-06:** SQLite satisfies the common backend contract and remains the event-backed default.
  Verify write, retrieve, source, correction, forget, scope refusal and unavailable behavior.
- **MBE-07:** Tencent is pinned, opt-in, loopback-only and limited to synthetic redacted data.
  Record revision, license, setup, destinations, calls, teardown and failure behavior. Verify no
  non-loopback call, model pull or production-store access.
- **MBE-08:** Both backends run the same bounded labelled cases. Report recall at five, mean
  reciprocal rank, correction, forget, realm leaks, latency, tokens, calls, wall time, peak memory
  and disk growth. Keep SQLite default unless every opt-in gate passes.
