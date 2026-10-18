# Mote Phase 2: durable personal agent

Status: owner-activated on 2026-10-17. The root README Ready for Go board is the completion
record. P2-02 through P2-12, P2V-01 through P2V-10 and MBE-01 through MBE-08 are active.
P2-13 remains a separate later decision.

## Outcome

Mote becomes a durable personal agent that can carry ordinary tasks through several steps,
survive interruption and explain what it is doing. Coding is one specialized kind of work. It is
not the shape of the whole agent.

Phase 2 uses option 1. Mote gets a native Rust task engine in a new `mote-hrns` crate. It starts
with one active task and a first-in, first-out queue. OpenCode remains a compatibility harness
while `mote-hrns` replaces its orchestration incrementally. The owner can select the OpenCode or
Mote harness when starting the daemon and while it is running.

Mote remains authoritative for task state, permissions, memory, context, model accounting,
events and Matrix UI. A harness proposes model steps. It does not gain direct authority over
tools or outward effects.

The detailed designs are:

- [durable runtime](../superpowers/specs/2026-10-17-phase2-runtime.md)
- [shared voice foundations](../superpowers/specs/2026-10-17-shared-voice.md)
- [replaceable memory backends](../superpowers/specs/2026-10-17-memory-backends.md)

The owner approved these designs on 2026-10-18. The implementation plans are:

- [durable runtime plan](../superpowers/plans/2026-10-18-phase2-runtime.md)
- [shared voice plan](../superpowers/plans/2026-10-18-shared-voice.md)
- [replaceable memory plan](../superpowers/plans/2026-10-18-memory-backends.md)

Execution starts with the memory contract and runtime contracts. The runtime store, driver,
OpenCode adapter and exactly-once actions follow. SQLite correction and scoped retrieval then
unblock task context. The native adapter and voice foundations can follow independently. Matrix,
Overseer consumption and both comparison reports come after their underlying contracts pass.

## What exists now

The current think lane keeps one `opencode serve` process per Mote thread. It creates OpenCode
sessions for turns and supplies Mote's context. Every OpenCode built-in tool is disabled. The
local model can reach only the tools exposed by `moted mcp`.

OpenCode can technically run non-code agents. It supports custom agents, system prompts,
permissions, subagents, custom tools and local or remote MCP servers. Its built-in product and
tools are centered on codebases. Mote currently uses OpenCode only as a model and tool-calling
loop, so Mote's MCP tools determine whether it can perform a personal task.

This makes OpenCode a useful bridge. It is not the durable personal-agent runtime. It does not
currently give Mote a persisted task queue, Mote-owned checkpoints, safe recovery or a unified
state machine across harnesses.

The existing `mote-harness` crate parses and launches external harnesses. The new `mote-hrns`
crate owns durable task orchestration. The similar names are intentional but the responsibilities
are different. `mote-harness` becomes an adapter dependency and can shrink or disappear after the
native harness reaches parity.

## Product journeys

### General personal task

The owner asks Mote to research a choice, update a plan and prepare a result. Mote creates one
durable task, records the requested outcome and selects only the relevant tools. It can browse,
read and fill within the existing browser boundary. Any submit, purchase, message, download,
OAuth flow or other outward change waits on a Mote approval card. The task can pause, reconnect
from the iPad and continue without repeating a completed action.

### Queue

The owner starts a second task while one is active. Mote stores it as queued. It exposes both
tasks in Matrix. Only the active task may run model steps or tools. The next queued task starts
after the active task completes, fails or is cancelled.

### Change harness

The owner selects `opencode` or `mote` at daemon startup or from a live Mote setting. An idle
change applies immediately. A change during a task is recorded immediately but takes effect only
at the next checkpoint where no tool or approval commit is in flight. The task keeps its Mote
identity, event history, context and action ledger. A missing harness produces a visible failure.
Mote never falls back silently.

### Coding task

Mote identifies a coding task and offers or invokes the Overseer delegate according to the
owner's configured policy. Mote sends a bounded task to `overseerd` over its owner-only socket,
then mirrors progress, permissions and results into the Mote task. Overseer owns coding harnesses,
worktrees, diffs and coding-run state. Mote owns the personal task, approval surface and final
result.

Shared crates make the two daemons use compatible protocol, event, harness and security code.
They do not make delegation happen by themselves. The delegation client described in
[`docs/reuse/work.md`](../reuse/work.md) is still required.

## Architecture

```text
iPad or Mac Matrix UI
        |
        v
      moted
        |
        +-- task and action store
        +-- approvals, memory, context, ledger and event log
        +-- Matrix task and progress surfaces
        |
        v
   mote-hrns
        |
        +-- OpenCode adapter during migration
        +-- native Mote Rust loop
        +-- Overseer coding delegate
        |
        v
 Mote-owned tools and guarded effects
```

The proposed crate structure is:

```text
crates/mote-hrns/
  src/
    lib.rs
    task.rs
    driver.rs
    model.rs
    action.rs
    checkpoint.rs
    context.rs
    skills.rs
    events.rs
    adapters/
      mod.rs
      opencode.rs
      native.rs
    delegates/
      mod.rs
      overseer.rs
```

`task.rs` defines durable task and action states. `driver.rs` owns the state transition loop.
`model.rs` accepts structured model steps and rejects malformed output. `action.rs` routes every
tool through Mote's policy and exactly-once boundary. `checkpoint.rs` persists recovery state.
`context.rs` builds bounded context. `skills.rs` progressively loads instructions. `events.rs`
normalizes progress for the event log and Matrix. The adapters do not own task state.
The Overseer delegate is a specialized action target. It is not a third selectable harness.

## State and invariants

Task states are `queued`, `running`, `waiting_for_approval`, `waiting_for_input`, `blocked`,
`complete`, `failed` and `cancelled`.

Action states are `proposed`, `awaiting_approval`, `ready`, `running`, `committed`, `denied`,
`failed` and `cancelled`.

The following rules are fixed:

1. At most one task is active in the first Phase 2 milestone.
2. A transition and its event are stored in one transaction.
3. Every model call goes through the existing ledger.
4. Every event is redacted before storage.
5. Model output is structured JSON and parses fail-closed.
6. Every effect has a stable action ID and request hash.
7. A committed action cannot run twice after retry, reconnect or restart.
8. A harness cannot bypass Mote's tool and approval policy.
9. Account and realm scope apply to task state, context, memory and delegated work.
10. Harness changes occur only at a safe checkpoint.
11. Local-only mode still permits only the models named in the v0 contract.
12. An unavailable harness is visible and never causes an unconfigured fallback.

## Context and skills

Each model step receives a bounded pack in this order:

1. Mote's charter and safety rules.
2. The task outcome, current state and pinned owner instructions.
3. Relevant scoped memory and source evidence.
4. Committed actions and outstanding approvals.
5. The newest conversation turns.
6. Older history or a source-covered compaction.
7. Only the skills and tool descriptions needed for the next step.

Compaction must keep the newest owner turn and pinned instructions. A compaction call is local,
ledgered and fail-closed. A failed compaction leaves the last valid pack available. Skills are
instructions, not capabilities. Loading a skill does not grant a tool or approval.

## OpenCode replacement sequence

1. Put task state, action state, checkpoints and normalized events in `mote-hrns` while OpenCode
   still supplies model steps.
2. Put OpenCode behind the common harness adapter. Stop letting OpenCode session state be the
   authority for Mote task continuity.
3. Add the native Rust model-step loop behind the same adapter contract.
4. Run both harnesses against the same personal-task, recovery and safety cases.
5. Make the native harness the default only after it passes the parity gate.
6. Keep a reversible OpenCode setting through one stable release.
7. Remove OpenCode only after the owner approves removal and rollback evidence exists.

This sequence replaces OpenCode incrementally. It does not require a flag day.

## Security boundary

OpenCode's permission system is not treated as a security sandbox. Mote disables its built-in
tools and exposes only Mote-owned tools. The native loop follows the same rule. Browser and other
outward actions keep the approved iPad approval boundary. Overseer receives only coding tasks and
its events return through a scoped client. No adapter writes Mote state except through the
`mote-hrns` transition API.

## Acceptance criteria

- [ ] **[P2 durable personal agent](#p2-durable-personal-agent)**
  - [x] **P2-00** The owner chose a native Rust engine, one active task plus a queue, selectable
    OpenCode and Mote harnesses, incremental OpenCode replacement and the crate name `mote-hrns`.
    This records the 2026-10-17 planning decision. It does not activate implementation.
  - [x] **P2-01** The owner explicitly activates Phase 2 implementation. **Verify:** the root
    README moves this board into Ready for Go and names the active child criteria.
    [Evidence](../verification/P2-01.md).
  - [ ] **P2-02** `mote-hrns` defines the common task, action, checkpoint, model-step and harness
    contracts with no adapter-specific state in those types. **Verify:** contract tests run the
    same state transition fixtures through fake OpenCode and native adapters.
  - [ ] **P2-03** The durable store enforces one active task and a first-in, first-out queue, and
    stores every state transition with its redacted event atomically. **Verify:** concurrent
    enqueue tests, forced transaction failures and daemon restarts show one active task, stable
    order and no orphan event.
  - [ ] **P2-04** Startup configuration selects `opencode` or `mote`, and a live setting changes
    harness only at a safe checkpoint. **Verify:** switch while idle, during a model step, during a
    tool call and while waiting for approval. No step or effect is lost or duplicated. Revert
    restores the prior harness.
  - [ ] **P2-05** The OpenCode adapter runs under `mote-hrns` while every built-in tool remains
    disabled and Mote owns task continuity. **Verify:** restart OpenCode and `moted` during a
    multi-step fixture and resume from the Mote checkpoint with no duplicate action.
  - [ ] **P2-06** The native adapter performs structured model, tool-result and completion steps
    directly through the approved local model client. **Verify:** malformed output, unknown tools,
    timeouts and unavailable models fail closed. Every call has one ledger row and uses no model
    outside the v0 allowlist.
  - [ ] **P2-07** Actions use stable IDs, approval state and commit receipts across retry,
    disconnect and restart. **Verify:** fault injection at every action transition proves that an
    outward effect is either absent or committed exactly once.
  - [ ] **P2-08** Context construction preserves the charter, pinned instructions and newest owner
    turn while bounding older history and progressively loading skills. **Verify:** deterministic
    long-history tests cover compaction success, malformed compaction, source coverage, realm
    isolation and tool visibility.
  - [ ] **P2-09** Matrix shows the active task, queued tasks, current step, approvals, harness,
    checkpoint, cancel, retry and final sourced result. **Verify:** Mac and iPad journeys cover
    enqueue, pause, disconnect, reconnect, cancel, approval and completion without stale controls.
  - [ ] **P2-10** Both harnesses can complete the same non-code personal-task journeys using only
    Mote-owned tools and policies. **Verify:** bounded fixtures cover multi-step web research with
    sources, comparison and planning, scoped memory recall, a generated Matrix result, browser
    reading and filling without submission, one scheduled or watched trigger that enters the
    queue exactly once, and one outward action that remains inert until its approval is accepted.
  - [ ] **P2-11** Mote delegates coding tasks to Overseer through the scoped client in
    `docs/reuse/work.md`, while non-code task state remains in Mote. **Verify:** a worktree coding
    fixture mirrors progress and permissions without Mote creating a worktree or running a coding
    harness. An unavailable Overseer leaves a visible queued or blocked task.
  - [ ] **P2-12** OpenCode and native harnesses run the same bounded personal-task suite. The native
    harness becomes default only if it matches every safety and recovery invariant and meets the
    recorded quality and latency gate. **Verify:** publish side-by-side task success, recovery,
    approval, duplicate-effect, latency, memory and model-call results.
  - [ ] **P2-13** OpenCode removal is separately approved after one stable native-default release,
    with rollback and data migration evidence. **Verify:** the owner decision, compatibility
    window and rollback drill are recorded before deleting the dependency or adapter.

Shared voice uses P2V-01 through P2V-10 in
[`2026-10-17-shared-voice.md`](../superpowers/specs/2026-10-17-shared-voice.md). Replaceable
memory uses MBE-01 through MBE-08 in
[`memory-backend-comparison.md`](../rfcs/memory-backend-comparison.md). Their checkboxes live in
the root README Ready for Go board.

### P2 durable personal agent

The parent completes only when P2-01 through P2-13 are fully verified. P2-00 records the approved
design. It is not implementation evidence.

## Deferred beyond the first milestone

Phase 2 initially has one active task. Parallel task execution and Mote subagents are deferred.
Recurring schedules may enqueue tasks only after recovery and exactly-once behavior is verified.
Cloud execution, a cloud relay, paid models, subscription harnesses inside Mote, a Tencent default
memory backend and automatic public actions remain outside this plan.

## Active driver goal

```text
Implement the activated Mote Phase 2 durable personal agent from repository state. Do not rely on chat history.

Read AGENTS.md, docs/rfcs/v0.md, root README.md Ready for Go, docs/phase2/README.md, the three linked Phase 2 designs, docs/rfcs/memory-backend-comparison.md, docs/reuse/work.md, docs/HANDOFF.md if present, and the affected code and tests. P2-02 through P2-12, P2V-01 through P2V-10 and MBE-01 through MBE-08 are authorized. P2-13 is not authorized. Preserve every v0 invariant. Keep models.local_only true. Use only the three approved Ollama models. Ledger every model call. Redact every event before storage. Parse structured model output fail-closed.

Build the Rust crate as crates/mote-hrns. Mote owns durable task state, actions, checkpoints, context, approvals and events. Harness adapters never own that state. Support one active task and a FIFO queue first. Keep OpenCode as a compatibility adapter while the native Rust loop is built behind the same contract. Allow startup and live harness selection, but apply a running-task change only at a safe checkpoint. Never silently fall back. Delegate coding to Overseer through its owner-only client boundary. Do not make Mote run coding harnesses or create worktrees. Extract the reusable Overseer voice foundations at the pinned source revision into the three product-neutral crates in the voice design, then integrate them with moted and Matrix behind an off-by-default setting. Add the memory contract, corrected SQLite adapter and opt-in loopback Tencent adapter. Keep SQLite default.

Choose the smallest actionable unchecked P2 criterion or coherent dependency set. Reproduce defects before fixing them. If the owner authorizes delegation, give a bounded task to a capable subagent with AC IDs, owned files, constraints and evidence requirements. Inspect every result. Keep unrelated work intact.

Verify each full clause, including failure, restart, retry, cancellation, realm isolation and duplicate-effect cases. Record command, output, environment, date and commit in docs/verification/<ID>.md. Run cargo test --workspace and cargo clippy --workspace -- -D warnings before every commit. Inspect the diff. Commit small Conventional Commits on main and push to the confirmed origin. An unpushed change is not completed mainline work.

Check a parent only when every child is verified. Do not make the native harness default before P2-12. Do not remove OpenCode before P2-13. After each integration reread the P2 board, reconcile the next action and recheck affected completed criteria. Add a testable P2 criterion for any discovered in-scope gap. Never weaken a Verify clause.

Replace docs/HANDOFF.md with the current ACs, checkout, delegated work, uncommitted changes, results, blockers and exact next action. Checkpoint before delegation or long operations and after each edit batch, test run, commit, push or blocker. Delete it only when nothing needs transfer. Continue until the activated P2 criteria are verified and pushed, the owner stops the work, or every remaining item needs unavailable input or authority. State exact blockers honestly.
```

## Research references

- [OpenCode agents](https://docs.opencode.ai/docs/agents/)
- [OpenCode tools and permissions](https://docs.opencode.ai/docs/tools/)
- [OpenCode MCP servers](https://docs.opencode.ai/docs/mcp-servers/)
- [OpenCode server API](https://docs.opencode.ai/docs/server/)
- [Mote work lane through Overseer](../reuse/work.md)
- [Shared crates RFC](../rfcs/shared-crates.md)
