# Matrix Host and Runtime v0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to
> implement this plan one task at a time with an independent review after every task.

**Goal:** Close the independently implementable v0 Matrix host and immediate-mode runtime gaps while
leaving native, hardware and versioned full-catalog acceptance visibly open.

**Architecture:** The daemon owns committed surface contracts, bindings, history and event
admission. The client renders only complete validated events and owns transient interaction state
scoped to the live structural identity. A shared headless observation harness exercises the real
event reducer and egui renderer without inventing a second runtime.

**Tech stack:** Rust 2021, serde, serde_json, egui/eframe 0.31, existing event log and IPC crates.

**Inputs:** `docs/backlog.md` UI-13 through UI-31, UI-33 through UI-44 and UI-46 through UI-48;
`matrix/catalog.md`; `matrix/runtime.md`; the audit at
`.superpowers/sdd/2026-10-10-matrix-host-runtime/gap-audit.md`.

## Global constraints

- `docs/rfcs/v0.md` and the unchanged eleven-variant `mote-ui::Node` contract remain authoritative.
- Do not add keys to the v0 consumer schema or approximate unsupported full-catalog semantics.
- UI-18, UI-19, UI-21, UI-23, UI-25 and UI-48 remain gated on a separately versioned consumer
  contract before broader lowering is enabled.
- UI-14 and UI-24 remain gated on their explicit export or experiment approval. Make no model calls.
- Pi and iPad delivery remain outside the approved v0 target. Hardware-only evidence stays open.
- Native screenshots remain honest acceptance evidence. Headless tests must not be described as
  native captures.
- Every rejected event or surface preserves the current committed surface, source and bindings.
- Every persisted event passes through redaction. Source text never enters activation payloads.
- Run `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `git diff --check`
  before every commit. Preserve untracked `.DS_Store` and `.superpowers/` scratch files.

## Review focus

- A forged or stale event must fail before storage, configuration mutation or model forwarding.
- Candidate input must never create a hit target or alter committed layout before final commitment.
- Test instrumentation must observe the real reducer, renderer and outbound queue.
- Runtime state survives only when the same live structural identity remains present.
- Layout measurements distinguish visual rectangles, hit rectangles, clips and scroll extents.
- Host snapshot updates recompile from retained canonical source with no model call.

---

### Task 1: Validate events against the committed v0 surface

**Files:**
- Modify: `moted/src/matrix.rs`
- Modify: `moted/src/main.rs`
- Create: `docs/verification/UI-29-v0.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Produces a host-owned committed Matrix contract for each thread containing the lowered surface,
  exposed field types, exposed zero-argument actions and a monotonically increasing revision.
- Extends the existing `ui.event` request with optional Matrix context in a side envelope. It does
  not add keys to consumer JSON.
- Preserves explicit non-Matrix approval actions under their existing trusted contract.

- [ ] **Step 1: Write failing admission tests**

After committing a Matrix surface, reject an unknown action, a change to a field absent from that
surface, missing or extra arguments, the wrong value type, a stale revision and an unknown node
context. Reject an event derived from a failed replacement candidate. Assert no stored `ui.event`,
configuration mutation, source replacement or model/ledger activity. Keep current Refresh,
toggle/select change and approval tests.

- [ ] **Step 2: Confirm the forged events currently reach storage**

Run the focused daemon tests and record the failing assertions before production edits.

- [ ] **Step 3: Retain and enforce the committed contract**

Derive admission metadata from the validated lowered node and the resolved host field/action
contracts. Replace the metadata only after the matching `ui.surface` append succeeds. Validate
before any side effect. Require Refresh to carry no arguments. Keep approval actions on a separate
explicit route rather than a blanket fallback.

- [ ] **Step 4: Record partial UI-29 evidence**

Document exact current-v0 guarantees and keep the broader full-catalog activation clause open.

- [ ] **Step 5: Verify and commit**

Run focused daemon tests, workspace tests, clippy and diff check.
Commit: `fix(matrix): validate committed surface events`

### Task 2: Prove candidate display and event isolation

**Files:**
- Modify: `mote-client/src/state.rs`
- Modify: `mote-client/src/app.rs`
- Modify: `mote-client/src/surface.rs`
- Modify: `mote-client/src/stream.rs`
- Modify: `moted/src/main.rs`
- Modify: `docs/verification/UI-31.md`
- Modify: `docs/verification/UI-33.md`
- Modify: `docs/verification/UI-34.md`
- Modify: `README.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Produces a test-only observation harness around the real event reducer and `surface::draw`.
- Observes displayed structural identities, rectangles and the real outbound event queue.
- Uses the real IPC frame decoder for fragmented JSON lines if byte fragmentation needs a seam.

- [ ] **Step 1: Write failing end-to-end prefix tests**

Cut all ten unchanged JSON fixtures at every byte offset through the real frame/reducer path.
Stream truncated Matrix action and form candidates through the daemon. During every prefix, click
and press Enter on the prior committed action and at candidate-only positions. Record displayed
identity and rectangle sets.

- [ ] **Step 2: Pin the commitment invariants**

Incomplete and malformed input leaves the prior display byte-for-byte and rectangle-for-rectangle
stable. Candidate-only controls emit nothing. Existing controls retain only their existing contract.
A valid final candidate causes exactly one complete switch. No open-node metadata enters v0 JSON.

- [ ] **Step 3: Fix only observed reducer or renderer defects**

Do not create a partial JSON renderer or expose parser candidates to the client.

- [ ] **Step 4: Close UI-31, UI-33 and UI-34 only if every Verify clause is covered**

Write separate evidence files with commands, counts, environment and date. Link completed criteria
from README and leave native-independent unrelated criteria open.

- [ ] **Step 5: Verify and commit**

Run focused client/daemon tests, workspace tests, clippy and diff check.
Commit: `test(client): verify committed candidate isolation`

### Task 3: Pin literal text and forbidden source semantics

**Files:**
- Modify: `mote-client/src/surface.rs`
- Modify: `crates/matrix-lang/tests/parser.rs`
- Modify: `crates/matrix-lang/tests/lower_v0.rs`
- Modify: `matrix/conformance/diagnostics.json`
- Modify: `moted/src/main.rs`
- Create: `docs/verification/UI-13.md`
- Create: `docs/verification/UI-23.md`
- Create: `docs/verification/UI-26.md`
- Modify: `docs/HANDOFF.md`

- [ ] **Step 1: Add exact literal-rendering tests**

Render `<b>hi</b>`, script tags and Markdown punctuation and inspect the real text shapes. Assert no
hidden node, action or markup interpretation. Do not implement Markdown in this task.

- [ ] **Step 2: Add exact forbidden-syntax corpus**

Reject color, size, font, spacing, secondary, animation, `state`, update rules, `route`, tracked
grid, pixel units, absolute positioning and z-index as whole candidates. Assert stable structured
diagnostics and preservation of the committed surface and source.

- [ ] **Step 3: Record only the v0-covered parts of UI-13, UI-23 and UI-26**

Keep Markdown, full theme groups, history ownership and revised-contract clauses open.

- [ ] **Step 4: Verify and commit**

Run Matrix/client/daemon tests, workspace tests, clippy and diff check.
Commit: `test(matrix): pin literal and forbidden semantics`

### Task 4: Reconcile interaction state by live structural identity

**Files:**
- Modify: `mote-client/src/surface.rs`
- Modify: `mote-client/src/state.rs`
- Modify: `mote-client/src/app.rs`
- Create: `docs/verification/UI-25.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Produces typed structural control identities scoped to one committed surface identity.
- Tracks identities observed in a completed frame and discards cached state for removed or
  type-mismatched controls.

- [ ] **Step 1: Write replacement-state regressions**

Edit a field and replace with the same identity. Change its type or path. Remove and reinsert it.
Replace the host value. Render two surfaces with equal labels but separate identities. Assert state
survives only the valid matching case and removed state never resurrects.

- [ ] **Step 2: Extend focus coverage**

Record Tab and Shift-Tab order through every v0 fixture control. Exercise Space and Enter. Assert
keystrokes alone create no IPC or model activity.

- [ ] **Step 3: Implement minimal reconciliation**

Use the existing immediate-mode state facilities. Do not simulate keyed full-catalog selection or
add state to consumer JSON.

- [ ] **Step 4: Record partial UI-16/UI-25/UI-26 evidence**

Keep native focus and revised keyed-selection clauses open.

- [ ] **Step 5: Verify and commit**

Run client tests, workspace tests, clippy and diff check.
Commit: `fix(client): reconcile structural interaction state`

### Task 5: Enforce current runtime layout invariants

**Files:**
- Modify: `mote-client/src/surface.rs`
- Modify: `mote-client/src/app.rs`
- Modify: `mote-client/src/theme.rs`
- Optionally create: `mote-client/src/layout_tests.rs`
- Create: `docs/verification/UI-15.md`
- Create: `docs/verification/UI-17.md`
- Create: `docs/verification/UI-43.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Adds test-only observations for visual rectangles, hit rectangles, clip rectangles, scroll
  viewport/content extents and reading order.

- [ ] **Step 1: Measure all fixtures at both viewports**

Exercise every v0 fixture at 800 by 480 and 1280 by 800. Stress a 200-item list and 500-row table.
Prove the last row is reachable, content remains clipped by the intended scroll area, hit regions do
not overlap unintentionally and reading order is stable.

- [ ] **Step 2: Measure every interactive control**

Field, Toggle, Select trigger/options and Action require at least a 48 by 48 logical point hit area.
Drive center and padded-edge input. Preserve the existing long-label behavior.

- [ ] **Step 3: Fix only measured defects**

Add collection scrolling or padding only where the real egui observations fail. Do not replace the
runtime or introduce Taffy solely for tests.

- [ ] **Step 4: Record headless evidence honestly**

UI-15 native screenshots and later touch targets remain open. Close UI-43 only if its entire current
approved Verify clause is covered without requiring native pixels.

- [ ] **Step 5: Verify and commit**

Run client stress tests, workspace tests, clippy and diff check.
Commit: `fix(client): enforce runtime layout invariants`

### Task 6: Refresh surfaces from validated host snapshots

**Files:**
- Modify: `moted/src/matrix.rs`
- Modify: `moted/src/main.rs`
- Modify: `mote-client/src/state.rs`
- Modify: `mote-client/src/surface.rs`
- Modify: `docs/verification/UI-30.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Factors resolve, supported lowering, validation and commit around an internal `HostBindings`
  snapshot input.
- Adds fixed host-owned progress and populated-list snapshot update seams for production behavior and
  tests. No client-side subscription or arbitrary manifest injection is exposed.

- [ ] **Step 1: Write failing snapshot-update tests**

Change progress between exactly representable values and change populated list contents and row
count. Assert newly resolved/lowered JSON, one consumer redraw, no source path in displayed text and
unchanged model request and ledger counts.

- [ ] **Step 2: Cover rejection and preservation**

Reject unsupported pending/failure/key/selection semantics before commitment. Preserve the prior
surface, source and accepted host snapshot.

- [ ] **Step 3: Implement the internal refresh seam**

Reuse retained canonical source and the existing atomic commit path. Keep snapshot ownership in the
daemon.

- [ ] **Step 4: Update UI-30 evidence without overstating full-catalog bindings**

Keep pending/failure and revised consumer clauses open.

- [ ] **Step 5: Verify and commit**

Run focused daemon/client tests, workspace tests, clippy and diff check.
Commit: `feat(matrix): refresh validated host snapshots`

## Deferred follow-ups

- Mac compiler p95 and arrival-to-display instrumentation for UI-40 and UI-46.
- AccessKit role baselines for UI-22.
- Surface-generation retry and scoped constraint controller for UI-36 through UI-38.
- A separately versioned full-catalog consumer contract and consistency gate for UI-18, UI-19,
  UI-21, UI-23, UI-25 and UI-48.
- One unlocked native capture session for UI-15, UI-16, UI-20, UI-22, UI-35, UI-39, UI-40 and UI-42.
- Pi/iPad work only after target and hardware approval. UI-44 remains conditional and requires no
  implementation now.
