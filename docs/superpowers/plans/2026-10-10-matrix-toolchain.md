# Matrix Toolchain Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the complete host-side Matrix parser, formatter, resolver and honest v0 lowerer.

**Architecture:** A new dependency-light Rust crate owns source syntax and semantics. It produces a
typed resolved tree and lowers only the documented lossless subset to the existing `mote-ui::Node`.

**Tech Stack:** Rust 2021, serde, serde_json, mote-ui, cargo test.

**Spec:** `docs/superpowers/specs/2026-10-10-matrix-toolchain.md`

## Global Constraints

- `docs/rfcs/v0.md` remains authoritative for the existing v0 consumer contract.
- The frozen language is exactly `matrix/grammar.md` and the closed elements are `matrix/catalog.md`.
- Source and resolved JSON each have depth 12, node count 2,000 and size 256 KiB limits.
- Unsupported consumer semantics reject with `E_LOWER_UNSUPPORTED`.
- Model calls remain local-only and every call goes through the existing ledger. This plan makes no model calls.
- Run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before every commit.

## Review Focus

- A truncated UTF-8 sequence or string must remain incomplete and never become an interactive candidate.
- Unknown words and trailing tokens must reject without parser recovery that invents declarations.
- Numeric JSON edge cases must reject leading zeroes, nonfinite values and invalid range combinations.
- Relative references must never escape their nearest collection row scope.
- Valid full-catalog syntax that v0 cannot represent must parse and resolve, then reject only at lowering.

---

### Task 1: Lexer and structured diagnostics

**Files:**
- Create: `crates/matrix-lang/Cargo.toml`
- Create: `crates/matrix-lang/src/lib.rs`
- Create: `crates/matrix-lang/src/diagnostic.rs`
- Create: `crates/matrix-lang/src/lexer.rs`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `Span`, `Diagnostic`, `DiagnosticCode`, `Token`, `TokenKind`, and `lex(&str)`.

- [ ] **Step 1: Write failing lexer tests**

Cover identifiers, relative paths, arrows, delimiters, JSON strings and numbers with exact spans.
Cover invalid escapes, raw newlines, comments, leading-zero numbers, nonfinite spellings, invalid
punctuation and 256 KiB plus one byte.

- [ ] **Step 2: Run `cargo test -p matrix-lang lexer` and confirm failure**

- [ ] **Step 3: Implement the minimal lossless lexer and diagnostics**

Use byte offsets. Do not classify element names in the lexer. Emit EOF explicitly. Mark incomplete
strings separately from rejected strings without repairing input.

- [ ] **Step 4: Verify and commit**

Run narrow tests, workspace tests, clippy and `git diff --check`.
Commit: `feat(matrix): add source lexer and diagnostics`

### Task 2: Closed AST and recursive-descent parser

**Files:**
- Create: `crates/matrix-lang/src/ast.rs`
- Create: `crates/matrix-lang/src/parser.rs`
- Create: `crates/matrix-lang/tests/parser.rs`
- Modify: `crates/matrix-lang/src/lib.rs`

**Interfaces:**
- Consumes: `lexer::lex` and structured spans.
- Produces: `Surface`, `Declaration`, typed modifiers and actions, `parse_surface`, `parse_fragment`.

- [ ] **Step 1: Write failing parser tests**

Accept examples 1 through 9, every recipe fragment and the nine v0 conformance fragments. Reject
design case 10, invalid nesting, duplicate or out-of-order modifiers, comments, unknown declarations,
screen nesting, missing blocks and trailing tokens. Assert diagnostic codes and spans.

- [ ] **Step 2: Run `cargo test -p matrix-lang --test parser` and confirm failure**

- [ ] **Step 3: Implement the frozen grammar**

Use semantic declaration variants, not a generic property map. Enforce syntax and nesting. Leave
manifest-dependent paths and types to the resolver.

- [ ] **Step 4: Verify and commit**

Run narrow tests, workspace tests, clippy and `git diff --check`.
Commit: `feat(matrix): parse the frozen language`

### Task 3: Canonical formatter and candidate stream

**Files:**
- Create: `crates/matrix-lang/src/format.rs`
- Create: `crates/matrix-lang/src/stream.rs`
- Create: `crates/matrix-lang/tests/format.rs`
- Create: `crates/matrix-lang/tests/stream.rs`
- Modify: `crates/matrix-lang/src/lib.rs`

**Interfaces:**
- Produces: `format_surface`, `format_fragment`, `CandidateStream`, `CandidateStatus`.

- [ ] **Step 1: Write failing round-trip and prefix tests**

Assert parse-format-parse equality and formatter idempotence for every positive fixture and 1,000
deterministically generated bounded ASTs. Cut UTF-8 fixtures at every byte offset and assert only a
complete valid candidate becomes committable.

- [ ] **Step 2: Run `cargo test -p matrix-lang --test format --test stream` and confirm failure**

- [ ] **Step 3: Implement canonical output and fail-closed streaming**

Use two-space indentation and JSON string escaping. Buffer at most 256 KiB. Never expose a partial
AST as interactive.

- [ ] **Step 4: Verify and commit**

Run narrow tests, workspace tests, clippy and `git diff --check`.
Commit: `feat(matrix): format and stream candidates`

### Task 4: Host manifest resolution and type checking

**Files:**
- Create: `crates/matrix-lang/src/types.rs`
- Create: `crates/matrix-lang/src/manifest.rs`
- Create: `crates/matrix-lang/src/resolve.rs`
- Create: `crates/matrix-lang/tests/resolve.rs`
- Modify: `crates/matrix-lang/src/lib.rs`

**Interfaces:**
- Produces: `Type`, `Manifest`, `BindingSnapshot`, `ResolvedSurface`, and `resolve`.

- [ ] **Step 1: Write failing resolver tests**

Cover control types, displays, row scopes, nested collections, field ownership, selection, keys,
options, built-ins and manifest actions. Reject unknown paths, wrong types, duplicate fields,
selected root collisions, action shadowing and bad arguments.

- [ ] **Step 2: Run `cargo test -p matrix-lang --test resolve` and confirm failure**

- [ ] **Step 3: Implement typed resolution without source inference**

The manifest defines every record, enum, value, field and action. Validate snapshots before
declarations. Resolved references contain values, not source path strings.

- [ ] **Step 4: Verify and commit**

Run narrow tests, workspace tests, clippy and `git diff --check`.
Commit: `feat(matrix): resolve host bindings and actions`

### Task 5: Exact v0 lowering and conformance

**Files:**
- Create: `crates/matrix-lang/src/lower_v0.rs`
- Create: `crates/matrix-lang/tests/lower_v0.rs`
- Create: `matrix/conformance/v0/*.json`
- Create: `docs/verification/UI-01.md`
- Create: `docs/verification/UI-06.md`
- Create: `docs/verification/UI-08.md`
- Create: `docs/verification/UI-10.md`
- Create: `docs/verification/UI-11.md`
- Create: `docs/verification/UI-12.md`
- Create: `docs/verification/UI-32.md`
- Create: `docs/verification/UI-35.md`
- Create: `docs/verification/UI-45.md`
- Modify: `crates/matrix-lang/src/lib.rs`
- Modify: `README.md`

**Interfaces:**
- Produces: `lower_v0(&ResolvedSurface) -> Result<mote_ui::Node, Vec<Diagnostic>>`.

- [ ] **Step 1: Write failing lowering snapshots**

Compare the nine fragments against canonical JSON for text, col, row, list, table, field, toggle,
select and button. Assert each unsupported valid element returns `E_LOWER_UNSUPPORTED` with path and
span.

- [ ] **Step 2: Run `cargo test -p matrix-lang --test lower_v0` and confirm failure**

- [ ] **Step 3: Implement only lossless mappings**

Map `col` to `Stack` and `button` to `Action`. Expand collection snapshots through their template.
Never add v0 keys or approximate headers, footers, fill, sliders, selection or full-catalog behavior.

- [ ] **Step 4: Record evidence and close only fully verified criteria**

Record exact commands, output, environment, date and commit. Leave hardware and host integration
criteria open.

- [ ] **Step 5: Verify and commit**

Run `cargo test -p matrix-lang`, workspace tests, clippy, `sh matrix/tools/measure.sh` and
`git diff --check`.
Commit: `feat(matrix): lower the v0 consumer subset`
