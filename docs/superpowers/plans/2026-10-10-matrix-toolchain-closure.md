# Matrix Toolchain Acceptance Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close every remaining UI criterion in the Matrix toolchain umbrella with executable evidence and production-safe commitment behavior.

**Architecture:** Preserve the frozen Matrix language and unchanged v0 consumer tree. Add exact acceptance assertions around existing behavior, extend the reproducible measurement utility, enforce expanded-tree limits before prohibited copies, and place a stateful candidate accumulator in front of daemon commitment. Rejections produce a separate diagnostic event that the client shows without replacing the committed surface.

**Tech Stack:** Rust 2021, serde_json, matrix-lang, mote-ui, moted, mote-client, existing IPC and event log, cargo test.

**Spec:** `docs/superpowers/specs/2026-10-10-matrix-toolchain.md`, with exact Verify clauses in `docs/backlog.md` for UI-02, UI-03, UI-04, UI-05, UI-07, UI-09, UI-32 and UI-35.

## Global Constraints

- `docs/rfcs/v0.md` remains authoritative for the existing v0 consumer contract.
- The frozen language is exactly `matrix/grammar.md` and the closed elements are `matrix/catalog.md`.
- Source and resolved JSON each have depth 12, node count 2,000 and size 256 KiB limits.
- Unsupported consumer semantics reject with `E_LOWER_UNSUPPORTED`.
- An incomplete or rejected candidate never replaces or mutates the last committed surface or retained source.
- Diagnostics remain structured and are emitted separately from `ui.surface`.
- `models.local_only` stays true. This plan makes no model calls and pulls no models.
- Every stored event passes through the existing event log redaction path.
- Run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before every commit.
- Each completed criterion gets `docs/verification/<ID>.md` with command, output, environment, date and commit, plus a README evidence link in the same commit.

## Review Focus

- Schema evidence must compare every one of the eleven variant property sets and reject aliases beside otherwise valid properties.
- Measurement output must distinguish in-memory manifest and snapshot encodings from bytes actually sent in request, response and event envelopes.
- Expanded collection limits must accept the exact boundary, reject the first excess and perform the rejection before the next prohibited clone or retained allocation.
- A syntactically complete prefix followed by an invalid suffix must never commit when the candidate has not been finalized.
- Diagnostic events must remain visible through replay and must not reuse `ui.surface` or overwrite the retained valid Matrix source.

---

### Task 1: Static schema, round-trip and case contracts

**Files:**
- Modify: `crates/mote-ui/src/lib.rs`
- Modify: `crates/matrix-lang/tests/parser.rs`
- Modify: `crates/matrix-lang/tests/resolve.rs`
- Create: `docs/verification/UI-02.md`
- Create: `docs/verification/UI-03.md`
- Create: `docs/verification/UI-07.md`
- Create: `docs/verification/UI-09.md`
- Modify: `README.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Consumes: existing `Node`, `allowed_keys`, generated schema, parser, manifest and resolver.
- Produces: exact executable proof for UI-02, UI-03, UI-07 and UI-09 without changing production semantics.

- [ ] **Step 1: Add the acceptance assertions**

Add `generated_schema_properties_match_v0_allowed_keys` covering all eleven `Node` variants and `abbreviated_property_aliases_are_rejected` covering `t`, `c` and `l` both as replacements and extra properties. Extend schema determinism proof to compare two `tree_schema_pretty()` calls. Add `declaration_words_are_case_sensitive` for lowercase `screen` and uppercase `Screen`. Add `published_names_are_case_sensitive` with only zero-argument `CheckIn` published, accepting that spelling and rejecting `checkIn` and `check_in` with `E_RESOLVE_ACTION`.

- [ ] **Step 2: Run the focused tests and classify the result**

Run: `cargo test -p mote-ui`, `cargo test -p matrix-lang --test parser` and `cargo test -p matrix-lang --test resolve`.

Expected: existing behavior may make the new acceptance tests pass immediately. Record that as characterization evidence. Do not alter production code merely to manufacture a red phase. If a real defect fails, retain the failure and make the smallest test-driven production correction.

- [ ] **Step 3: Re-run existing semantic round-trip proof**

Run: `cargo test -p matrix-lang --test format`.

Expected: both pass and preserve parsed declarations and reference spellings after formatting.

- [ ] **Step 4: Record evidence and mark only four criteria verified**

Create the four evidence files. Add UI-02, UI-03, UI-07 and UI-09 to the Matrix toolchain verified list. Keep the umbrella unchecked.

- [ ] **Step 5: Verify and commit**

Run focused package tests, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `git diff --check`.
Commit: `test(matrix): close static toolchain contracts`

### Task 2: Reproducible host and transport measurement

**Files:**
- Modify: `matrix/tools/measure.rs`
- Modify: `matrix/tools/measure.sh`
- Create: `matrix/measurement-output.tsv`
- Modify: `matrix/measurement.md`
- Create: `docs/verification/UI-04.md`
- Modify: `README.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Consumes: the trusted Mote dashboard manifest and snapshot shape in `moted/src/matrix.rs`, the `matrix.render` request, its response, and the committed `ui.surface` event envelope.
- Produces: deterministic byte and `o200k_base` token counts for source, compact consumer JSON, a declared canonical measurement encoding of the in-memory host manifest and binding snapshot, and actual IPC envelopes.

- [ ] **Step 1: Add a failing measurement regression**

Add a checked output fixture containing named rows for `host_manifest`, `binding_snapshot`, `matrix_render_request`, `matrix_render_response` and `ui_surface_event`. Extend `measure.sh` with `--check` so it runs the utility and compares complete TSV output with `matrix/measurement-output.tsv`. The production change that makes this fail is omitting or silently changing a measured envelope.

- [ ] **Step 2: Run the measurement check and confirm the new rows are absent**

Run: `sh matrix/tools/measure.sh --check`.

Expected: failure because the check mode or required host and transport rows do not exist.

- [ ] **Step 3: Extend the utility and report**

Use the same `tiktoken_rs::o200k_base().encode_ordinary` path. Label manifest and snapshot rows as deterministic measurement encodings of in-memory structures, not transmitted bytes. Count actual request, response and event envelopes separately and state which data crosses IPC.

- [ ] **Step 4: Prove reproducibility and record UI-04**

Run `sh matrix/tools/measure.sh --check` twice and compare complete output byte-for-byte. Update `matrix/measurement.md`, create `docs/verification/UI-04.md`, and add UI-04 to the README verified list.

- [ ] **Step 5: Verify and commit**

Run the measurement test twice, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `git diff --check`.
Commit: `docs(matrix): measure host transport costs`

### Task 3: Exact expanded limits and production preservation

**Files:**
- Modify: `crates/matrix-lang/src/resolve.rs`
- Modify: `crates/matrix-lang/tests/resolve.rs`
- Modify: `crates/matrix-lang/tests/lower_v0.rs`
- Modify: `moted/src/main.rs`
- Create: `docs/verification/UI-05.md`
- Modify: `README.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Consumes: parser source bounds, resolver expansion, lowerer compact JSON budget and `Daemon::render_matrix` retained-source semantics.
- Produces: exact accepted-boundary and first-excess behavior for expanded depth, nodes and bytes, with rejection before the next prohibited retained allocation and unchanged production commitment.

- [ ] **Step 1: Write failing expanded-boundary and host-preservation tests**

Add literal boundary pairs for expanded collection depth 12/13, nodes 2,000/2,001 and retained resolved bytes 262,144/262,145. Keep these separate from compact consumer JSON boundary tests. Add daemon cases for source-size, expanded-depth, expanded-node and expanded-byte failures after one valid source commit. Assert no replacement `ui.surface`, the exact structured limit diagnostic, and a later Refresh recompiles the prior retained source.

- [ ] **Step 2: Run the focused tests and verify the expected gaps**

Run the new resolver, lowerer and daemon tests. Confirm at least the aggregate pre-allocation or exact expanded-boundary assertion fails for the missing behavior rather than a fixture mistake.

- [ ] **Step 3: Charge before prohibited copies**

Move aggregate budget checks ahead of cloning bound lists, display strings, values and retained declaration properties where the next copy would cross the limit. Preserve existing byte semantics, diagnostic codes, spans and declaration paths. Do not replace exact accounting with a rough cap.

- [ ] **Step 4: Verify boundaries and record UI-05**

Run all matrix-lang tests and the focused daemon preservation corpus. Create `docs/verification/UI-05.md` and add UI-05 to the README verified list only when every adjacent boundary and production preservation assertion passes.

- [ ] **Step 5: Verify and commit**

Run `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `git diff --check`.
Commit: `fix(matrix): enforce expanded limits before commitment`

### Task 4: Stateful production candidate commitment

**Files:**
- Modify: `crates/matrix-lang/src/stream.rs`
- Modify: `crates/matrix-lang/tests/stream.rs`
- Modify: `moted/src/main.rs`
- Modify: `moted/src/matrix.rs`
- Modify: `docs/verification/UI-32.md`
- Modify: `README.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Consumes: `CandidateStream::push`, trusted host bindings and existing atomic Matrix compilation.
- Produces: `CandidateStream::finalize(self) -> Result<Surface, Vec<Diagnostic>>`; `matrix.candidate` RPC with `thread`, `candidate`, byte-array `chunk` and `finish`; response status `incomplete`, `complete`, `committed` or `rejected`; per-candidate state that never commits before finalization.

- [ ] **Step 1: Write failing candidate-lifecycle tests**

Cover every byte prefix of the positive Matrix fixtures through the production dispatcher, including splits inside UTF-8 and escaped strings. Before `finish`, assert no `ui.surface` even when the parser reports a complete prefix. On `finish`, assert one commit only for a complete resolved and lowered candidate. Cover valid-prefix-plus-invalid-tail, unclosed finish, latched rejection, over-limit bytes and two interleaved candidate ids. Assert all failures preserve both the prior surface and retained source.

- [ ] **Step 2: Run focused tests and confirm the RPC is missing**

Run the new daemon candidate tests and the strengthened stream AST-equivalence test.

Expected: failure because `matrix.candidate` and finalization semantics do not exist.

- [ ] **Step 3: Implement the bounded lifecycle**

Keep one `CandidateStream` per `(thread, candidate)` under daemon ownership. Accept chunks as JSON byte arrays so tests and real callers can split UTF-8 safely, rejecting non-integers and values outside 0 through 255 as invalid parameters. Never commit or emit `ui.diagnostic` on a non-final push, including a rejected one. `finalize` consumes the accumulator and converts incomplete input into its parser diagnostics. At `finish`, commit only a finalized complete surface; return structured diagnostics otherwise and remove terminal candidate state. Retain canonical source only after a successful commit. Keep legacy whole-source `matrix.render` behavior compatible.

- [ ] **Step 4: Update evidence and close UI-32**

Strengthen `docs/verification/UI-32.md` with production prefix, finalization and preservation output. Add UI-32 to the README verified list.

- [ ] **Step 5: Verify and commit**

Run matrix-lang stream tests, focused daemon candidate tests, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `git diff --check`.
Commit: `feat(moted): commit finalized Matrix candidates`

### Task 5: Visible Matrix rejection diagnostics

**Files:**
- Modify: `moted/src/main.rs`
- Modify: `mote-client/src/state.rs`
- Modify: `mote-client/src/app.rs`
- Modify: `mote-client/src/main.rs`
- Modify: `docs/verification/UI-35.md`
- Modify: `README.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Consumes: structured Matrix diagnostics returned by whole-source and candidate compilation, event-log replay, and client chat rendering.
- Produces: a redacted `ui.diagnostic` event with Matrix diagnostic fields and candidate identity; a visible client diagnostic message or panel that does not replace `ui.surface`.

- [ ] **Step 1: Write failing exact rejection and visibility tests**

After committing a valid dashboard, submit a truncated string, truncated brace, stray brace and quoted bare modifier through production Matrix paths. For each, assert a structured response, one `ui.diagnostic` event, no replacement `ui.surface`, and Refresh still compiles the valid retained source. Add a valid-prefix-plus-invalid-suffix case proving no skipped line, invented token or prefix commitment. Feed the diagnostic event through `ChatState` and assert a replay-stable visible artifact containing code, span, message and hint when present.

- [ ] **Step 2: Run focused tests and confirm visibility is missing**

Run the new daemon and client state tests.

Expected: daemon has no `ui.diagnostic` event and the client ignores it.

- [ ] **Step 3: Emit and render a separate diagnostic artifact**

Append diagnostics through `EventLog` only after redaction. Do not include full source. Preserve the last surface and retained source. Reduce `ui.diagnostic` into a system diagnostic artifact and render it in the ordinary scrollable chat flow. Replayed events with the same sequence remain deduplicated by the saved cursor path.

- [ ] **Step 4: Capture visible evidence and close UI-35**

Run the real client against an isolated daemon, commit a valid dashboard, submit one malformed candidate, and capture the diagnostic artifact while the prior dashboard remains visible. Update `docs/verification/UI-35.md` with the exact four-case automated output and live artifact evidence. Add UI-35 to the README verified list. If all toolchain criteria are now verified, check the Matrix toolchain umbrella and link every criterion.

- [ ] **Step 5: Verify and commit**

Run focused daemon and client tests, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings` and `git diff --check`.
Commit: `feat(client): show Matrix rejection diagnostics`
