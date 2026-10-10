# Handoff: approve the Mote UI contract

For a fresh agent. Nothing here assumes chat history. Date: 2026-10-09.

## Purpose

Turn the existing UI research into one small, approvable language contract.

This is a design and reconciliation task. It is not a compiler implementation task. It is not a
renderer implementation task. The output must let the owner close or revise `UI-00` before any
work begins on the deferred UI language compiler.

## Dispatch order

Run the renderer handoff at `docs/handoff/generative-ui-renderer.md` first. That work must produce
`docs/rfcs/renderer.md` and its spike results. This handoff starts after those files exist because
the renderer work decides the display-list boundary, node identity needs, animation hints, and
target constraints that the language must carry.

Do not wait for a production renderer. The renderer RFC and measured spike are sufficient inputs.

## Read in this order

1. `AGENTS.md`.
2. `docs/rfcs/v0.md`, especially sections 0, 1, 12, and 13.
3. The root `README.md`, especially `Ready for Go`.
4. `crates/mote-ui/src/lib.rs` and all ten fixtures under `crates/mote-ui/fixtures/`.
5. `schemas/tree.v0.json`.
6. `docs/rfcs/ui-language.md`.
7. The `UI-00` through `UI-48` block in `docs/backlog.md`.
8. `docs/rfcs/renderer.md` and `docs/research/renderer-research.md`.
9. `docs/rfcs/mote.md`, criteria MOTE-14 through MOTE-16.
10. `docs/research/ui-language-research.md`.

The build contract is `docs/rfcs/v0.md`. Nothing in this task may change or reinterpret v0.

## Current facts

The v0 implementation already has a compact JSON tree in `mote-ui`. It has eleven node types:
`text`, `stack`, `row`, `list`, `table`, `field`, `toggle`, `select`, `action`, `progress`, and
`chat`. It validates unknown types, unknown keys, depth, node count, and byte size. It generates
`schemas/tree.v0.json`. It renders ten deterministic golden PNG fixtures.

The implemented v0 tree does not yet have ids, keys, typed action arguments, typed bindings,
surface metadata, partial-stream markers, themes, anchors, or stable diagnostic codes. Its PNG
renderer is a test renderer made of colored rectangles. Those omissions are not v0 defects unless
section 12 says otherwise.

The current UI language RFC is broader than the owner's later direction. It includes grids,
dialogs, sheets, charts, images, Markdown, style roles, size roles, opt-outs, custom empty and
error states, and many semantic elements. It also names Tauri and Slint as renderer targets. The
renderer handoff exists because those target choices are no longer accepted assumptions.

The owner wants a purposefully restrictive language. Layout is a flexbox subset plus anchors.
There is one inherited theme for a surface. There are no per-node theme or style overrides. The
element catalog is short and closed. Bindings are typed and checked before render. The store and
observer wiring live under the language in the daemon. The model emits meaning, layout, and
bindings. The renderer owns pixels, behavior, accessibility, and animation.

## Recommended architecture

Keep two contracts with one meaning.

1. The canonical Surface Tree is the wire and runtime contract. Models emit its compact JSON in
   the current phase. The daemon validates it. The renderer receives only validated tree commits.
2. Mote UI is a smaller authoring syntax introduced later. It parses, type-checks, and lowers to
   the same canonical tree. The renderer never parses `.mote` and never has language-specific
   behavior.

Do not create a second semantic model for `.mote`. A source program and its lowered JSON must
describe the same surface, bindings, action contracts, identity, and streaming boundaries.

Use whole-surface replacement. The daemon computes the diff. The model never emits patches or
animation code.

Keep state ownership split into three parts.

- The renderer owns transient interaction state such as focus, scrolling, pressed state, and
  in-progress field edits.
- The daemon owns typed data, binding subscriptions, surface history, event validation, and the
  current canonical surface.
- The model owns semantic transitions and emits the next whole surface.

There is no declared store. There are no routes. There are no expressions, scripts, timers,
network requests, or author-defined components.

## Defaults to test and make explicit

These are the starting defaults. Change one only with evidence and call out the change for the
owner.

### Surface and identity

- Keep the v0 node names and meanings compatible. A later version may narrow or compose them but
  must provide an explicit migration from every v0 fixture.
- Add one stable `key` concept for reconciliation only when path plus type is insufficient.
- Keep surface id, revision, presenter size, and inline versus full-screen placement outside the
  model-authored node tree unless the renderer RFC proves one belongs in the language.
- Let the daemon select the active inherited theme. Do not let a child node select or override it.
- Add at most one animation intent hint if the renderer RFC proves it is needed. Prefer `key`
  alone and renderer-owned transitions.

### Layout

- Provide vertical and horizontal flex containers.
- Provide wrapping, alignment, distribution, and bounded growth only as named roles.
- Provide anchors for safe areas and fixed edge regions such as top bars and bottom actions.
- Reject pixel sizes, arbitrary positioning, z-index, spacing, colors, fonts, borders, and shadows.
- Do not add grid unless a required fixture cannot be expressed with nested rows and stacks.
- Do not add model-authored conditionals. The model emits the applicable node.
- Allow repetition only through a typed collection binding with a checked item template.

### Elements

- Start from the implemented v0 catalog, not the larger catalog in section 7.4 of the current
  RFC.
- Separate primitive elements from renderer-owned composites. A dialog, card, page, tile, status,
  and empty state should be a role or a renderer behavior unless it requires distinct semantics.
- Keep tables, progress, and chat only if their semantics cannot be represented safely by the
  smaller primitives without losing accessibility or performance.
- Treat adding an element as a versioned language change.
- Do not expose semantic color variants such as `primary` or `danger` by default.
- Do not expose size variants by default.
- Do not expose arbitrary provider widgets or executable embeds. A map or provider view must be a
  daemon-resolved typed resource with renderer-defined behavior if the renderer RFC retains it.

### Typed bindings and actions

- Define `Value<T>` for read-only daemon data.
- Define `Field<T>` for editable values owned by a rendered surface.
- Define `Action<Input, Result>` for events accepted by the daemon.
- Make a toggle require `Field<bool>`.
- Make a select's options and field share the same closed value type.
- Make a collection template type-check against its item type.
- Derive each action input contract from the fields and values it gathers.
- Represent bindings structurally in the canonical tree. Do not rely on unvalidated magic strings
  if a tagged representation can meet the measured token budget.
- Validate an event against its surface revision, node identity, action name, and input schema
  before it reaches the model.
- Expose pending and failure state through the action binding. Do not make the model author those
  states node by node.

### Streaming and failure

- Parse incrementally in the daemon.
- Commit only complete, valid nodes to the renderer.
- Allow text content to extend while it streams if the renderer contract can preserve layout and
  accessibility.
- Keep incomplete interactive nodes inert.
- Fail closed on unknown elements, properties, bindings, actions, and types.
- Preserve the last valid committed surface when the final stream is invalid.
- Keep repair syntactic and bounded. Never infer missing semantic content.
- Produce stable machine-readable diagnostics with a source span and tree path where available.

## Questions this task must settle

1. What is the exact versioned canonical tree shape after v0?
2. Which implemented v0 nodes remain primitives, which become composites, and which are deferred?
3. What is the exact closed property set for each node?
4. What is the exact flexbox and anchor vocabulary?
5. What tagged JSON shape represents `Value<T>`, `Field<T>`, and `Action<Input, Result>`?
6. Who allocates field identity and how long does a field value survive reconciliation?
7. Is `key` enough for animation and reconciliation?
8. Does the host choose the single theme, or may the root surface choose one named theme?
9. How are built-in actions distinguished from model actions without stringly typed dispatch?
10. What is the smallest `.mote` grammar that round-trips without adding semantics?
11. Which incomplete stream prefixes may paint?
12. How do v0 fixtures migrate without changing v0 behavior?

Answer every question. Use `unknown` only when the renderer spike or an unavailable target makes
the answer genuinely unknowable. Give each unknown a safe default and a named experiment.

## Approaches to compare

Compare these three approaches in the RFC. Recommend the first unless measurements reject it.

### Canonical tree first

Freeze the typed canonical tree and its validation rules. Then design `.mote` as lossless syntax
sugar over it. This keeps the daemon and renderer independent of authoring syntax. It also lets
JSON remain the reliable model format until the language proves a validity or token advantage.

### Language first

Design `.mote` as the source of truth and derive the tree from it. This can produce cleaner syntax
but risks hiding runtime requirements and creating compiler-only semantics.

### One format only

Keep compact JSON permanently and do not build `.mote`. This is the smallest system. Retain it as
the fallback if the language does not improve token cost, first-try validity, or model consistency
enough to justify a compiler.

## Required work

### 1. Reconcile the RFC

Revise `docs/rfcs/ui-language.md` into an approvable contract. Preserve useful research by moving
rejected ideas to clearly marked deferred sections. Remove Tauri, Slint, webview, and widget
toolkit assumptions from normative language. Reference `docs/rfcs/renderer.md` for rendering.

Put a short key-points block first. State the boundary between the canonical tree, `.mote`, the
daemon, and the renderer. State what is v0, what is the next language version, and what is later.

List every place where the prior RFC exceeded the owner's closed-language constraint. Record the
decision for each item. Do not silently delete a capability.

### 2. Freeze examples before grammar

Replace the current examples with a small conformance suite that covers these cases:

- Static text in a vertical layout.
- A row that wraps on a narrow surface.
- A typed field, toggle, and select.
- An action that gathers typed fields.
- A collection bound to `Value<List<T>>`.
- Loading, empty, pending, and failure states supplied by the runtime.
- An inline chat card and the same content on a full-screen surface.
- A stable-key reorder that the renderer can animate.
- An anchored top or bottom region with safe-area behavior.
- A malformed binding that fails before render.

For every case, show canonical compact JSON and `.mote` source. The two forms must lower to the
same typed surface. Include the expected event payload or diagnostic when applicable.

### 3. Specify types and validation

Write the catalog as data tables. For each node and property, give its type, whether it is
required, its allowed parent and children, its default, and its accessibility semantics. Define
binding scope and lifetime. Define action contract derivation. Define reconciliation. Define the
stream commit protocol. Define deterministic canonical serialization.

Do not invent syntax until the type and runtime behavior are clear.

### 4. Measure the language claim

Create a throwaway spike under `spikes/ui-language/`. It may parse only the conformance examples.
Measure compact JSON and `.mote` with the tokenizer already used by mote. Check parse and lower
time. Run the available approved local models against equivalent prompts if the harness already
supports the test without pulling another model.

Do not pull a model. Do not add a production crate. Do not keep the spike as an implicit compiler
implementation.

Report token count, first-try syntax validity, first-try type validity, and round-trip loss. If the
language does not materially beat JSON, recommend keeping JSON as the only model-authored form.

### 5. Rebuild the acceptance gate

Rewrite the `UI-00` through `UI-48` backlog block so it matches the approved architecture and the
renderer RFC. Keep stable ids where the meaning survives. Mark replaced criteria explicitly in
the RFC. Do not move any criterion into the v0 list. Do not check `UI-00`; only the owner can
approve it.

Split later implementation into independently verifiable groups:

- Canonical tree and schema.
- Typed bindings and event contracts.
- Streaming parser and diagnostics.
- Renderer conformance.
- Optional `.mote` parser, formatter, lowerer, and decompiler.
- Model validity and token measurements.

Each criterion must be one sentence plus a complete **Verify** clause.

### 6. Prepare the implementation handoff

Create `docs/handoff/ui-language/implementation.md`. It starts only after the owner approves
`UI-00`. It must name the criteria to move into the active build list, their order, owned files,
test fixtures, commands, and stopping conditions. It must not claim that approval has happened.

## Deliverables

1. A reconciled `docs/rfcs/ui-language.md`.
2. A source-backed update to `docs/research/ui-language-research.md` for new evidence only.
3. A throwaway `spikes/ui-language/` measurement probe and its recorded results.
4. A revised UI criteria block in `docs/backlog.md`.
5. `docs/handoff/ui-language/implementation.md` for work after `UI-00`.
6. A decision summary for the owner with the exact choices needed to approve `UI-00`.

Do not edit `docs/rfcs/v0.md`. Do not edit the v0 list in `README.md`. Do not change production
code. Do not change the v0 fixtures or schema. Do not commit unless the owner asks.

## Completion test

This handoff is complete when the owner can answer `approve`, or name a small set of explicit
changes, without having to resolve an unstated syntax, state, rendering, or validation decision.

Report back with the changed file paths and ten lines covering the canonical tree, catalog,
layout, bindings, state ownership, events, streaming, renderer boundary, measurements, and open
owner decisions.
