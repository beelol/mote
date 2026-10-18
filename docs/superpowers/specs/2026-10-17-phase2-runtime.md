# Phase 2 durable runtime design

Date: 2026-10-17

Status: approved and activated by the owner

## Outcome

Mote gains a durable Rust task runtime named `mote-hrns`. It can run the existing OpenCode
harness or a native Mote harness without changing task identity, approval state, context,
actions, events or checkpoints. OpenCode remains the default until the native harness passes the
same bounded personal-task benchmark and every safety and recovery invariant.

This design activates P2-02 through P2-12. P2-13 remains a later removal decision. It does not
authorize removing OpenCode or changing the production default.

## Ownership boundary

`moted` owns accounts, realms, configuration, local model access, the ledger, the event log and
Matrix delivery. `mote-hrns` owns task and action state, transitions, checkpoints, context packs,
adapter selection and the driver loop. A harness produces structured proposed steps. It never
executes a tool, stores authoritative task state or bypasses Mote approvals.

The first implementation has one active task and a first-in, first-out queue. Every transition
and its redacted event commit in one transaction. Every effect has a stable action ID, request
hash and commit receipt. Recovery may repeat a read but must never repeat a committed outward
effect.

## Crate shape

```text
crates/mote-hrns/
  src/
    lib.rs
    task.rs
    action.rs
    checkpoint.rs
    context.rs
    driver.rs
    events.rs
    model.rs
    store.rs
    adapters/
      mod.rs
      native.rs
      opencode.rs
    delegates/
      mod.rs
      overseer.rs
```

The public contract uses product-neutral serializable values for task state, action state,
checkpoint state, model steps and harness requests. Adapter-specific session identifiers stay in
opaque adapter state. They never become task identity.

## Driver loop

The driver loads one runnable task, builds a bounded context pack, asks the selected harness for
one structured step and validates that step. A tool proposal becomes a Mote action. A safe read
may run immediately. An outward change waits for the existing approval boundary. Tool results,
denials, failures and completions are checkpointed before another model step.

The native adapter calls only the existing approved local model client. Model output is JSON that
matches a versioned schema. Parse failure, an unknown tool, a missing scope or an unavailable
model fails closed. The OpenCode adapter keeps all OpenCode built-in tools disabled and accepts
only Mote-owned MCP tools.

## Harness switching

Startup configuration selects `opencode` or `mote`. A live change is stored immediately but
applies only at a safe checkpoint. A checkpoint is safe when no model response, tool call,
approval commit or event transaction is in flight. An unavailable selected harness produces a
visible failure. There is no implicit fallback.

## Context and memory

The context pack contains the charter, pinned owner instructions, task outcome, current state,
scoped memory with source IDs, committed actions, outstanding approvals and newest owner turns.
Older history is bounded or compacted. Compaction is local, ledgered, structured and fail-closed.
The newest owner turn and pinned instructions are never removed.

Memory access uses the backend-neutral contract in
[`2026-10-17-memory-backends.md`](2026-10-17-memory-backends.md). A harness cannot select a
backend or widen account and realm scope.

## Voice

Final voice utterances enter the same driver as typed owner turns. Partial words and loudness
events update the interface but do not create model steps. Voice capture, recognition and speech
run through the shared foundations in
[`2026-10-17-shared-voice.md`](2026-10-17-shared-voice.md). Voice does not create a separate
agent runtime.

## Personal-task benchmark

Both harnesses run the same deterministic and bounded cases:

1. Research several supplied local documents and return a sourced comparison.
2. Build a plan from owner constraints and revise it after a correction.
3. Recall an account-scoped and realm-scoped fact while refusing a cross-realm decoy.
4. Read and fill a browser fixture without submitting it.
5. Queue one watched trigger exactly once.
6. Propose one outward action that remains inert until approval.
7. Restart the harness and daemon at defined checkpoints without duplicating an effect.

The report records completion, source correctness, approval behavior, recovery, duplicate
effects, latency, model calls, tokens and memory retrieval quality. The native harness can become
the default only after matching all safety and recovery results and meeting the recorded quality
and latency gate.

## Delivery order

1. Add contracts and deterministic fake adapters.
2. Add the atomic durable store and recovery tests.
3. Put OpenCode behind the common adapter while preserving behavior.
4. Add actions, approvals and exactly-once receipts.
5. Add bounded context and backend-neutral memory access.
6. Add the native structured-step adapter.
7. Add live selection and safe checkpoint switching.
8. Add Matrix task controls and voice entry.
9. Run the common benchmark and publish the decision report.
10. Keep OpenCode until P2-13 receives a separate owner decision.

## Safety and compatibility

The v0 schema and SQLite default remain compatible. Every model call uses the ledger. Every
stored event passes redaction. Only the three local models named in `docs/rfcs/v0.md` are allowed.
The iPad approval boundary remains authoritative. Physical iPad verification and external Apple
work remain deferred from this design.
