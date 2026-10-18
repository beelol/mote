# Phase 2 Durable Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `mote-hrns` as Mote's durable personal-task runtime with selectable OpenCode and native adapters, recoverable actions and a common benchmark.

**Architecture:** `mote-hrns` owns task, action and checkpoint state. Harnesses return one structured proposal at a time and cannot execute tools. `moted` supplies the event log, model ledger, approvals, scoped memory and Matrix surfaces.

**Tech Stack:** Rust 2021, rusqlite, serde, serde_json, existing `mote-eventlog`, `mote-harness`, `mote-localmodel`, `mote-context`, `mote-protocol` and Matrix crates.

**Spec:** `docs/superpowers/specs/2026-10-17-phase2-runtime.md`

## Global Constraints

- Keep OpenCode as the default until P2-12 passes and the owner separately approves a change.
- Keep `models.local_only = true` and allow only the three models in `docs/rfcs/v0.md` section 0.
- Send every model call through the existing ledger and every stored event through redaction.
- Parse model verdicts as structured JSON and fail closed.
- Preserve the existing iPad approval boundary and v0 wire compatibility.
- Treat absent Phase 2 configuration as `opencode`, `sqlite` and voice off. Do not change the
  byte-for-byte v0 default configuration block.
- Support one active task and one first-in, first-out queue in this milestone.
- Run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before every commit.
- Record completed criteria in `docs/verification/<ID>.md`, commit directly to `main` and push.

## Review Focus

- A daemon restart between an action commit and its next checkpoint must not repeat the effect. Task 5 pins this.
- A harness switch requested during a model or approval operation must wait for a safe checkpoint. Task 8 pins this.
- Malformed native output and an unknown tool must fail closed without mutating task state. Task 7 pins this.
- Queue races must never produce two active tasks or reorder accepted work. Tasks 2 and 3 pin this.
- Context compaction must preserve pinned instructions and the newest owner turn. Task 6 pins this.

---

### Task 1: Versioned runtime contracts

**Files:**
- Create: `crates/mote-hrns/Cargo.toml`
- Create: `crates/mote-hrns/src/lib.rs`
- Create: `crates/mote-hrns/src/task.rs`
- Create: `crates/mote-hrns/src/action.rs`
- Create: `crates/mote-hrns/src/model.rs`
- Create: `crates/mote-hrns/src/adapter.rs`
- Create: `crates/mote-hrns/tests/contracts.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `TaskId`, `ActionId`, `HarnessKind`, `TaskState`, `ActionState`, `Task`, `Action`, `Checkpoint`, `HarnessRequest`, `ModelStep`, `HarnessError` and `Harness::step(&mut self, HarnessRequest) -> Result<ModelStep, HarnessError>`.
- Produces: `ModelStep::{Message, Tool, Complete, WaitForInput}` with `#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]` payloads.

- [ ] **Step 1:** Write contract tests that round-trip every state and model step, reject unknown fields and prove fake OpenCode and native adapters receive the same request type.
- [ ] **Step 2:** Run `cargo test -p mote-hrns --test contracts` and confirm it fails because the crate and types do not exist.
- [ ] **Step 3:** Implement the types and adapter trait with private nonempty string constructors for IDs and no adapter-specific field in `Task`, `Action` or `Checkpoint`.
- [ ] **Step 4:** Run `cargo test -p mote-hrns --test contracts` and confirm it passes.
- [ ] **Step 5:** Run the workspace test and clippy gates, create `docs/verification/P2-02.md`, commit `feat: define durable harness contracts` and push.

### Task 2: Atomic task store and transition events

**Files:**
- Create: `crates/mote-hrns/src/store.rs`
- Create: `crates/mote-hrns/tests/store.rs`
- Modify: `crates/mote-hrns/src/lib.rs`
- Modify: `crates/mote-eventlog/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 runtime types.
- Produces: `EventLog::append_with<T, F>(&self, scope: &str, kind: &str, payload: &Value, apply: F) -> rusqlite::Result<(T, Event)>` where `F: FnOnce(&rusqlite::Transaction<'_>) -> rusqlite::Result<T>`.
- Produces: `HrnsStore::open(path: &Path, events: EventLog)`, `enqueue(NewTask)`, `transition(TaskTransition)`, `checkpoint(CheckpointWrite)`, `active()`, `queued()` and `recover()`.

- [ ] **Step 1:** Add event-log tests proving a failed external mutation rolls back the event and broadcasts nothing.
- [ ] **Step 2:** Add store tests for concurrent enqueue, FIFO promotion, a forced transition failure, restart recovery and absence of orphan events.
- [ ] **Step 3:** Run the two targeted test suites and confirm the new APIs are missing.
- [ ] **Step 4:** Implement `append_with` using the existing redaction, projection, retention, commit and post-commit broadcast path.
- [ ] **Step 5:** Implement the `hrns_task`, `hrns_action` and `hrns_checkpoint` tables and state-transition guards through `append_with`.
- [ ] **Step 6:** Run `cargo test -p mote-eventlog` and `cargo test -p mote-hrns --test store` and confirm they pass.
- [ ] **Step 7:** Run the workspace gates, commit `feat: persist durable task transitions` and push.

### Task 3: One-task driver and queue recovery

**Files:**
- Create: `crates/mote-hrns/src/driver.rs`
- Create: `crates/mote-hrns/tests/driver.rs`
- Modify: `crates/mote-hrns/src/lib.rs`

**Interfaces:**
- Consumes: `HrnsStore` and `Harness`.
- Produces: `Driver::new(store, adapters, executor)`, `Driver::enqueue(NewTask)`, `Driver::step() -> Result<DriverOutcome, DriverError>`, `Driver::cancel(TaskId)` and `Driver::recover()`.
- Produces: `ActionExecutor::execute(&self, &Action) -> Result<ActionReceipt, ActionError>`.

- [ ] **Step 1:** Write fake-adapter tests for FIFO work, one active task, cancellation, adapter failure and recovery from every persisted checkpoint.
- [ ] **Step 2:** Run `cargo test -p mote-hrns --test driver` and confirm it fails on missing driver APIs.
- [ ] **Step 3:** Implement one-step driving so each call performs at most one model proposal or one action transition.
- [ ] **Step 4:** Run the targeted test and confirm it passes with no second active task under a concurrent enqueue fixture.
- [ ] **Step 5:** Run the workspace gates, add P2-03 evidence, commit `feat: drive one durable task at a time` and push.

### Task 4: OpenCode compatibility adapter

**Files:**
- Create: `crates/mote-hrns/src/adapters/mod.rs`
- Create: `crates/mote-hrns/src/adapters/opencode.rs`
- Create: `crates/mote-hrns/tests/opencode_adapter.rs`
- Modify: `crates/mote-harness/src/lib.rs`
- Modify: `moted/src/main.rs`

**Interfaces:**
- Consumes: Task 1 `Harness` and the existing `OpenCodeServer`.
- Produces: `OpenCodeAdapter::new(ServerFactory, model: String)` and an opaque `AdapterCheckpoint { session_id, parser_version }` serialized only inside `Checkpoint.adapter`.
- Keeps `think_config` built-in tools disabled and exposes only the Mote MCP server.

- [ ] **Step 1:** Write a scripted OpenCode server fixture that stops after a tool proposal, restarts, then resumes from a Mote checkpoint without owning task state.
- [ ] **Step 2:** Run the adapter test and confirm it fails before the adapter exists.
- [ ] **Step 3:** Move server creation behind a `ServerFactory` in `mote-harness` without changing the existing v0 path.
- [ ] **Step 4:** Implement the adapter and convert `ThinkEvent` values into versioned `ModelStep` values.
- [ ] **Step 5:** Route the existing think lane through the adapter behind an internal compatibility flag while leaving the default output unchanged.
- [ ] **Step 6:** Run the targeted tests plus the existing think-lane and local-only suites.
- [ ] **Step 7:** Run the workspace gates, add P2-05 evidence, commit `feat: run OpenCode behind mote-hrns` and push.

### Task 5: Exactly-once actions and approvals

**Files:**
- Create: `crates/mote-hrns/src/executor.rs`
- Create: `crates/mote-hrns/tests/actions.rs`
- Modify: `moted/src/approvals.rs`
- Modify: `moted/src/watchers.rs`
- Modify: `moted/src/main.rs`

**Interfaces:**
- Consumes: stable `ActionId`, canonical request hash and existing approval records.
- Produces: `ActionReceipt { action_id, request_hash, status, committed_at, result_ref }` and `MoteActionExecutor`.
- Produces: `HrnsStore::claim_action`, `commit_action`, `deny_action` and `fail_action`, each guarded by expected prior state.

- [ ] **Step 1:** Add fault-injection tests at proposed, awaiting approval, ready, running and committed transitions.
- [ ] **Step 2:** Add a test that kills and reopens the store after the effect commits but before the next driver step, then asserts one receipt and one effect.
- [ ] **Step 3:** Run the action test and confirm it fails on missing receipt and claim APIs.
- [ ] **Step 4:** Implement canonical hashing, approval binding, atomic claims and durable receipts by adapting the existing file-write approval boundary.
- [ ] **Step 5:** Run the action, approval and browser-boundary tests and confirm altered, stale, denied and duplicate requests remain inert.
- [ ] **Step 6:** Run the workspace gates, add P2-07 evidence, commit `feat: make task actions exactly once` and push.

### Task 6: Bounded task context

**Files:**
- Create: `crates/mote-hrns/src/context.rs`
- Create: `crates/mote-hrns/tests/context.rs`
- Modify: `crates/mote-context/src/lib.rs`
- Modify: `moted/src/lanes.rs`

**Interfaces:**
- Consumes: `mote_memory::MemoryBackend`, `MemoryScope` and the existing `Pack` ledger format from the memory plan.
- Produces: `TaskContextBuilder::build(ContextRequest) -> Result<ContextPack, ContextError>` and `Compactor::compact(CompactionRequest) -> Result<Compaction, CompactionError>`.
- `ContextPack` orders charter, pinned instructions, task state, scoped memories, committed actions, approvals, newest turns, covered compaction and selected skills.

- [ ] **Step 1:** Write deterministic long-history tests for the newest owner turn, pinned instructions, source coverage, realm isolation, skill visibility and malformed compaction.
- [ ] **Step 2:** Run the context test and confirm it fails before the builder exists.
- [ ] **Step 3:** Implement deterministic budgeting and a structured local, ledgered compaction boundary that retains the last valid pack on failure.
- [ ] **Step 4:** Run the targeted test and affected existing pack tests.
- [ ] **Step 5:** Run the workspace gates, add P2-08 evidence, commit `feat: build bounded task context` and push.

### Task 7: Native structured-step adapter

**Files:**
- Create: `crates/mote-hrns/src/adapters/native.rs`
- Create: `crates/mote-hrns/tests/native_adapter.rs`
- Modify: `moted/src/model_call.rs`
- Modify: `moted/src/lanes.rs`

**Interfaces:**
- Consumes: `mote_localmodel::Ollama`, `ModelRequest`, `ContextPack` and the `ModelStep` JSON schema.
- Produces: `NativeAdapter::new(LocalStepClient, model: String)` and `LocalStepClient::complete(&StepPrompt, &RootSchema) -> Result<RawStep, LocalStepError>`.

- [ ] **Step 1:** Write tests for valid message, tool, wait and complete steps plus malformed JSON, extra fields, unknown tools, timeout and unavailable model.
- [ ] **Step 2:** Run the native adapter test and confirm it fails before the adapter exists.
- [ ] **Step 3:** Implement the fixed prompt, generated schema and fail-closed parser through the existing local model and ledger boundary.
- [ ] **Step 4:** Assert each attempted call creates exactly one pack and ledger row and never names a model outside the v0 allowlist.
- [ ] **Step 5:** Run targeted tests and the existing structured-output and local-only suites.
- [ ] **Step 6:** Run the workspace gates, add P2-06 evidence, commit `feat: add native Mote harness` and push.

### Task 8: Safe harness selection

**Files:**
- Modify: `moted/src/config.rs`
- Create: `moted/src/hrns.rs`
- Create: `moted/tests/hrns_switch.rs`
- Modify: `moted/src/main.rs`
- Modify: `matrix/examples/mote-dashboard.matrix`

**Interfaces:**
- Accepts optional config key `agent.harness` with allowed values `opencode` and `mote`; absence
  means `opencode` without changing `DEFAULT_CONFIG_TOML`.
- Produces: `HarnessSelection { configured, active, pending }` and `HrnsRuntime::request_harness(HarnessKind) -> Result<SwitchStatus, SwitchError>`.

- [ ] **Step 1:** Add config tests that accept the optional key while proving first-start
  `DEFAULT_CONFIG_TOML` remains byte-for-byte unchanged and resolves to OpenCode.
- [ ] **Step 2:** Add switch tests for idle, model step, tool execution and waiting approval, plus unavailable target and revert.
- [ ] **Step 3:** Run the tests and confirm they fail before schema and runtime support exist.
- [ ] **Step 4:** Implement pending selection and apply it only when no model, effect, approval commit or store transaction is in flight.
- [ ] **Step 5:** Expose configured, active and pending values through the existing pipeline snapshot and Matrix binding.
- [ ] **Step 6:** Run targeted and workspace gates, add P2-04 evidence, commit `feat: switch harnesses at checkpoints` and push.

### Task 9: Matrix task surfaces

**Files:**
- Create: `matrix/examples/task-runtime.matrix`
- Modify: `moted/src/pipeline.rs`
- Modify: `moted/src/matrix.rs`
- Modify: `crates/mote-client-core/src/surface_v1.rs`
- Modify: `mote-ios/src/app.rs`
- Create: `moted/tests/hrns_surface.rs`

**Interfaces:**
- Consumes: durable task, queue, step, approval, harness and checkpoint snapshots.
- Produces Matrix actions `task.cancel`, `task.retry` and `task.select_harness` through existing typed action contracts.

- [ ] **Step 1:** Write snapshot and reducer tests for active, queued, waiting approval, failed and complete tasks, including stale controls after reconnect.
- [ ] **Step 2:** Run the targeted tests and confirm missing bindings and actions fail closed.
- [ ] **Step 3:** Add bounded host bindings and compile the task runtime surface without adding a renderer-specific widget.
- [ ] **Step 4:** Route actions through the host identity and current task revision checks.
- [ ] **Step 5:** Run Mac headless snapshots and iPad Simulator journeys for enqueue, reconnect, cancel, retry and completion.
- [ ] **Step 6:** Run the workspace gates, add P2-09 evidence, commit `feat: show durable tasks in Matrix` and push.

### Task 10: Overseer coding delegate

**Files:**
- Create: `crates/mote-hrns/src/delegates/mod.rs`
- Create: `crates/mote-hrns/src/delegates/overseer.rs`
- Create: `crates/mote-hrns/tests/overseer_delegate.rs`
- Modify: `moted/src/hrns.rs`

**Interfaces:**
- Produces: `OverseerDelegate::submit(DelegatedTask) -> Result<DelegationId, DelegateError>` and `poll(&DelegationId, Cursor) -> Result<Vec<DelegatedEvent>, DelegateError>` over the owner-only boundary in `docs/reuse/work.md`.
- The delegate returns normalized progress and permission events. It never exposes worktree creation as a Mote action.

- [ ] **Step 1:** Write a fake Overseer socket fixture for progress, permission, completion and unavailable outcomes.
- [ ] **Step 2:** Run the test and confirm it fails before the delegate exists.
- [ ] **Step 3:** Implement bounded request and event mapping without importing Overseer task state.
- [ ] **Step 4:** Prove non-code tasks never invoke the delegate and unavailable Overseer leaves visible task state.
- [ ] **Step 5:** Run the workspace gates, add P2-11 evidence, commit `feat: delegate coding tasks to Overseer` and push.

### Task 11: Common personal-task benchmark and decision gate

**Files:**
- Create: `crates/mote-hrns/tests/fixtures/personal_tasks.json`
- Create: `moted/src/hrns_bench.rs`
- Create: `moted/tests/hrns_bench.rs`
- Modify: `moted/src/main.rs`
- Create: `docs/verification/P2-10.md`
- Create: `docs/verification/P2-12.md`

**Interfaces:**
- Produces command `moted bench agent --harness <opencode|mote> --fixtures <path> --output <path>`.
- Produces a JSON report with task success, source correctness, approvals, recovery, duplicate effects, p50 and p95 latency, memory results, model calls and tokens.

- [ ] **Step 1:** Check in bounded fixtures for sourced local research, comparison, planning, scoped recall, browser read and fill, one queued trigger, one held outward action and restart recovery.
- [ ] **Step 2:** Write report-schema and scoring tests that fail a harness on any safety, isolation, approval or duplicate-effect violation.
- [ ] **Step 3:** Implement the benchmark runner using fixture servers and fake clocks, with an explicit opt-in for live local models.
- [ ] **Step 4:** Run the deterministic suite for both adapters and record all results without changing the default.
- [ ] **Step 5:** Run the approved bounded live comparison when the named local models are already installed. Record unavailable cases honestly and pull no model.
- [ ] **Step 6:** Run the workspace gates, complete P2-10 and P2-12 evidence only if their full Verify clauses pass, commit `test: compare personal task harnesses` and push.

## Acceptance contract

This technical section preserves the full checks behind the concise root board.

- **P2-02:** Common task, action, checkpoint, model-step and harness contracts contain no
  adapter-specific state. Verify the same state fixtures through fake OpenCode and native
  adapters.
- **P2-03:** The store keeps one active task, a FIFO queue and atomic redacted transition events.
  Verify concurrent enqueue, forced transaction failure and restart without reordering or orphan
  events.
- **P2-04:** Startup and live harness selection switch only at a safe checkpoint. Verify idle,
  model-step, tool-call and approval changes without lost or duplicated work, including revert.
- **P2-05:** OpenCode runs behind `mote-hrns` with built-in tools disabled and Mote-owned
  continuity. Verify OpenCode and daemon restart during a multi-step task without duplicate action.
- **P2-06:** The native adapter uses structured steps through approved local models. Verify
  malformed output, unknown tools, timeout and unavailable models fail closed with one ledger row
  per call and no model outside the allowlist.
- **P2-07:** Stable action IDs, approval state and commit receipts prevent duplicate effects.
  Verify fault injection at every transition produces either no effect or one committed effect.
- **P2-08:** Bounded context preserves the charter, pinned instructions and newest owner turn.
  Verify compaction success and failure, source coverage, realm isolation and tool visibility.
- **P2-09:** Matrix exposes task, queue, step, approval, harness, checkpoint, cancel, retry and
  sourced result state. Verify Mac and iPad journeys through disconnect and reconnect without
  stale controls.
- **P2-10:** Both harnesses complete the same bounded non-code journeys through Mote-owned tools.
  Verify sourced research, comparison, planning, scoped recall, browser read and fill, one queued
  trigger and one held outward action.
- **P2-11:** Coding delegates to Overseer while task authority stays in Mote. Verify progress and
  permissions through a worktree fixture, plus a visible unavailable state.
- **P2-12:** The common benchmark records safety, recovery, quality and latency. Publish
  side-by-side task success, recovery, approval, duplicate-effect, latency, memory and model-call
  results before any default change.
