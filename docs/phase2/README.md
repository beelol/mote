# Mote Phase 2

Status: active. The root README Ready for Go board is authoritative.

## What ships

- `mote-hrns`, a durable Rust task runtime with one active task and a FIFO queue.
- Restart-safe tasks, checkpoints, approvals and exactly-once outward actions.
- OpenCode and native Mote harnesses behind one interface. OpenCode stays default until the
  comparison passes and the owner changes it.
- Bounded context with pinned instructions, recent turns, sourced memory and relevant skills.
- A Matrix task surface for progress, queue, approvals, retry, cancel and results.
- Coding delegation to Overseer without giving it authority over the personal task.
- Shared voice crates used by Mote and, from a pinned revision, Overseer.
- A local Mac voice path that is off by default and preserves typed interaction.
- Replaceable memory backends with corrected SQLite as default and Tencent as an opt-in local
  comparison adapter.

## Shape

```text
iPad or Mac Matrix UI
          |
        moted
          |
    +-----+----------------+----------------+
    |                      |                |
 mote-hrns              memory           voice
    |                 SQLite | Tencent      |
    +-- OpenCode                              +-- shared crates
    +-- native Mote
    +-- Overseer delegate for coding only
          |
   Mote-owned tools and approval boundary
```

Mote owns task state, permissions, actions, context, memory, model accounting and UI. Harnesses
propose steps. They cannot execute tools or bypass approval policy.

## Delivery order

1. Define the runtime and memory contracts.
2. Add the durable store, queue, recovery and OpenCode adapter.
3. Fix SQLite isolation, retrieval, correction and forgetting. Connect it to bounded context.
4. Add exactly-once actions, harness switching and the Matrix task surface.
5. Add the native harness, voice crates and Overseer coding delegate.
6. Compare both harnesses and both memory backends. Keep the existing defaults unless their gates
   pass and the owner approves a change.

## Fixed rules

- Preserve v0 behavior and wire compatibility.
- Keep `models.local_only = true` and use only the three approved Ollama models.
- Ledger every model call. Redact every stored event. Parse model decisions fail-closed.
- Keep the existing iPad approval boundary for outward effects.
- Apply harness changes only at safe checkpoints. Never fall back silently.
- Scope every task and memory operation by account and realm.
- Keep SQLite and OpenCode as defaults during this goal.
- Do not remove OpenCode. P2-13 requires a later owner decision.

## Done means

The root README has verified evidence for P2-02 through P2-12, P2V-01 through P2V-10 and MBE-01
through MBE-08. P2-13 is not active. Physical iPad voice checks, external Apple work, parallel
tasks, cloud execution and a memory default change are deferred.

## Technical references

Designs:
[runtime](../superpowers/specs/2026-10-17-phase2-runtime.md),
[voice](../superpowers/specs/2026-10-17-shared-voice.md),
[memory](../superpowers/specs/2026-10-17-memory-backends.md).

Technical execution plans:
[runtime](../superpowers/plans/2026-10-18-phase2-runtime.md),
[voice](../superpowers/plans/2026-10-18-shared-voice.md),
[memory](../superpowers/plans/2026-10-18-memory-backends.md).

## Driver goal

```text
Deliver Mote Phase 2 from the repository state, not chat history.

Read AGENTS.md, root README Ready for Go, this page, docs/HANDOFF.md and the technical plan for the selected criterion. Active work is P2-02 through P2-12, P2V-01 through P2V-10 and MBE-01 through MBE-08. P2-13 is not active.

Choose the smallest dependency-ready criterion. Preserve v0, local-only models, the model ledger, event redaction, fail-closed structured output, SQLite and OpenCode defaults and the iPad approval boundary. Mote owns task state and effects. Harnesses only propose steps. Delegate coding to Overseer through its scoped client.

Implement test-first. Verify the full clause in its linked design or RFC. Record commands, output, environment, date and commit in docs/verification/<ID>.md. Run cargo test --workspace and cargo clippy --workspace -- -D warnings before each small Conventional Commit. Commit to main and push. Unpushed work is incomplete.

Update the root checkbox only after its evidence passes. Recheck affected criteria after integration. Replace docs/HANDOFF.md after each work batch with current state and the exact next action. Continue until the active criteria are verified and pushed, the owner stops work or all remaining work needs unavailable authority.
```
