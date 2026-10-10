# Inferred Matrix surfaces implementation plan

## Goal

Let a successful normal Mote conversation optionally add one validated Matrix surface after its
prose answer. Keep prose unchanged, local-only model policy intact, every model call redacted and
ledgered, and invalid UI fail closed to the prior display.

Production activation is gated by UI-24. The live `qwen3:4b` first-attempt validity rate must reach
95 percent on the fixed 200-prompt corpus. Record a lower result and leave automatic generation
disabled.

## Task 1: Add a strict, redacted and ledgered surface call

**Files:**
- Create: `moted/src/model_call.rs`
- Create: `moted/src/inferred_ui.rs`
- Modify: `moted/src/lanes.rs`
- Modify: `moted/src/main.rs`
- Modify: `crates/mote-localmodel/src/lib.rs`

**Interfaces:**
- Add a structured-reply helper that returns both the parsed typed value and raw response content.
- Keep existing structured-call behavior by delegating through the new helper.
- Define a strict tagged `SurfaceDecision` with only `none` and `matrix { source }` variants.
- Define a `generate_surface` boundary using `qwen3:4b`, 32,768 context, thinking disabled,
  temperature zero and Ollama `format`.

- [ ] **Step 1: Pin the strict envelope**

Reject missing fields, unknown fields, wrong types, prose and trailing text. Bound Matrix source at
1 through 262,144 bytes. The JSON schema constrains the envelope only. Matrix parsing, resolution,
lowering and commitment remain authoritative.

- [ ] **Step 2: Redact before the model boundary**

Redact the request, prose, previous canonical source and diagnostics before pack construction and
before the HTTP request. Strengthen talk, router and think boundary tests to inspect outbound bodies,
not only stored events.

- [ ] **Step 3: Account for every invocation**

Move shared request timing and ledger helpers into `model_call.rs`. Every surface invocation,
including failure, writes exactly one five-part pack, one `surface` ledger row, one redacted
`model.request` event and one `model.response` event.

Commit: `fix(models): redact and account for surface calls`

## Task 2: Share candidate finalization and bound retries

**Files:**
- Modify: `moted/src/main.rs`
- Modify: `moted/src/matrix.rs`
- Modify: `moted/src/inferred_ui.rs`

**Interfaces:**
- Factor one candidate finalizer used by both `matrix.candidate` and inferred generation.
- Return either a committed response or structured diagnostics.
- Add a retry budget keyed by thread and committed surface sequence.

- [ ] **Step 1: Use the real candidate stream**

Push generated source through `CandidateStream` and its normal finalization. Do not add a parallel
parse or compile path.

- [ ] **Step 2: Add one bounded retry**

Allow one retry per conversation turn and at most three retries per rolling minute for the current
thread and committed-surface sequence. The retry input contains redacted diagnostics and the prior
canonical source. A malformed envelope, transport failure or `none` does not retry.

- [ ] **Step 3: Preserve prose and prior UI**

Keep the first rejection quiet while retrying. A second rejection emits one redacted
`ui.diagnostic`, preserves the prior surface, source and controls, and leaves delivered prose
visible. Each attempt owns one pack and ledger row.

Commit: `feat(matrix): retry rejected inferred surfaces`

## Task 3: Integrate one surface after successful prose

**Files:**
- Modify: `moted/src/main.rs`
- Modify: `moted/src/inferred_ui.rs`
- Modify: `mote-client/src/state.rs` only if real event ordering exposes a reducer defect

**Interfaces:**
- Add an internal `infer_surface_after_answer` operation keyed by thread and `turn.user` sequence.
- Use candidate id `surface-{turn_seq}`.

- [ ] **Step 1: Preserve turn behavior and event order**

Call the surface lane only after a completed talk or think answer. Do not call it after talk
unavailability, failed think or an unfinished handoff. Preserve answer events before the optional
surface call and emit at most one committed `ui.surface`.

- [ ] **Step 2: Release shared locks before Matrix compilation**

Do not hold the context-store lock across candidate finalization because Matrix compilation reads
the pipeline snapshot. Keep existing answer and ledger identifiers unchanged.

- [ ] **Step 3: Prove ordinary and UI turns**

Test talk and think answers with a committed dashboard, `none` with unchanged chat, two invalid
attempts with prose plus prior surface, exact event order and exact ledger counts. Prove the
supported host subset can express configuration toggles, selects, table or list data, progress and
refresh. Unsupported actions, arguments, keyed collections, selection, pending and failure must
reject before commitment.

Keep the production call site disabled until Task 4 passes.

Commit: `test(matrix): verify inferred surface turn flow`

## Task 4: Measure validity and gate production activation

**Files:**
- Modify: `moted/src/bench.rs`
- Modify: `crates/mote-testkit/src/lib.rs` or add a deterministic corpus fixture
- Modify: `moted/src/main.rs` only after the gate passes
- Create or modify: `docs/verification/UI-24.md`
- Create or modify: `docs/verification/UI-36.md`
- Create or modify: `docs/verification/UI-37.md`
- Create or modify: `docs/verification/UI-38.md`
- Replace: `docs/HANDOFF.md`
- Modify: `README.md` only for fully verified criteria

- [ ] **Step 1: Add `moted bench surface`**

Run a fixed 200-prompt corpus through the real surface call ledger. Record strict envelope, Matrix
parse, host resolution, v0 lowering, first-attempt and after-one-retry validity. Do not pull a model.
Use only the already approved `qwen3:4b` under `MOTE_LIVE=1`.

- [ ] **Step 2: Enforce the activation gate**

Enable the production call site only when first-attempt validity reaches at least 95 percent. A
lower result is valid evidence but leaves automatic inference disabled. After-retry validity alone
does not pass UI-24.

- [ ] **Step 3: Record honest criterion boundaries**

UI-36 may close after the bounded retry tests cover retry input, rate exhaustion, ledger rows,
display preservation and prose fallback. UI-37 may close only for the local Ollama v0 path and must
state that the envelope, not Matrix grammar, is constrained by `format`. Keep UI-38 open unless the
owner accepts the two-call prose-then-JSON-envelope composition as its final framing.

Commit after the live gate passes: `feat(moted): infer Matrix surfaces from conversations`

## Verification for every task

Run focused tests, then:

```text
cargo test --workspace
cargo clippy --workspace -- -D warnings
git diff --check
```

Keep `models.local_only = true`. Pull no model. Do not add hosted model paths, arbitrary bindings,
new consumer fields, selection, overlays, keyed collections or typed action arguments.
