# matrix decisions

This document lists the decisions the owner confirmed to move matrix forward, and it records where
matrix departs from the earlier UI language RFC. The owner approved all four defaults without changes
on 2026-10-10. The gate evidence is `docs/verification/UI-00.md`.

## Decisions already taken in this design

These came from direct owner direction on 2026-10-09 and are treated as settled unless the owner
reverses them.

1. The language is named `matrix`. It is not `mote`, `moted`, `.mote`, or `matter`.
2. matrix is the text syntax. The text is the language. An agent emits matrix source.
3. The compact JSON surface tree is a temporary intermediate and compile target, not the language.
   It may change or be replaced. matrix's design does not depend on its exact shape.
4. The element catalog is complete for common web and app surfaces but closed. A model cannot extend
   it.
5. Heights, spacing, and padding are standard and come from one theme. A model writes no pixel.
6. matrix ships with one default theme. A theme, and only a theme, may override the standard values.
7. The runtime is inferred and animated UI on an immediate mode GUI. The surface is redrawn each
   frame. Animation lives in the runtime, driven by node keys and state transitions, never written
   by the model. See [runtime.md](runtime.md).
8. The runtime is built in Rust, the fastest technology for a per frame layout and parallel draw.
   Recommended stack: Taffy for flexbox layout, egui for the immediate mode shell, wgpu for GPU
   draw, rayon for parallel per node measurement, AccessKit for roles. Each piece measures
   independently, a flexbox pass fits them, and they draw in parallel. See [runtime.md](runtime.md).
9. matrix design lives in its own folder and changes no production code.
   Its interim lowering target is the existing Mote v0 tree.
10. The default theme is Vector: thin retro red, Atari vector line. Dark base, one red accent, thin
    vector strokes instead of fills, sharp corners, no shadows, a mono type scale, tight but
    comfortable spacing, status tones as thin colored strokes, and a materialize line on reveal
    motion with one duration and curve. It is a general base theme that renders every element.
11. The layout model is locked and strict: two containers `col` and `row`, two anchors `header` and
    `footer`, the `fill` role (the owner's word is "stretch"), the align roles, and automatic
    scrolling. No grid tracks, no absolute, no z-index, no pixels. Anchors plus `fill` plus the
    closed catalog are the whole connection model. Any new layout primitive goes to the owner first.
12. The runtime is a consumer process, not daemon code. The daemon stays headless and streams the
    validated tree over IPC; the runtime renders and sends events back, and replaces the dead eframe
    client. One Rust runtime targets Mac, Pi, and iPad through wgpu. The tree is the consumer
    contract, so a second native immediate mode consumer such as SwiftUI is allowed on iOS only if
    the Rust path is impractical, with no change to the language or the tree.

## Owner confirmation list for UI-00

The owner confirmed these four defaults without changes on 2026-10-10.

1. Keep the closed [catalog](catalog.md), its coverage recipes and the explicit deferrals below.
2. Confirm [grammar.md](grammar.md), host owned types and actions, scoped optional selection,
   keys without animation hints and the fixed semantic icon names.
3. Keep Vector, one theme per surface, six status meanings, no per element style, strict layout,
   three bindings and whole surface replacement.
4. Confirm Matrix as the future source format and the unchanged v0 tree as the interim consumer
   contract. Accept explicit rejection of unsupported lowering and approve a separate contract
   revision before the full Matrix catalog can render.

## Departures from the earlier UI language RFC

The earlier RFC is `docs/rfcs/ui-language.md`. matrix keeps its research and most of its constraints
and departs in these places. None is a silent deletion. Each is a recorded decision.

| Earlier RFC | matrix | Reason |
|---|---|---|
| Model emits compact JSON for v0; the `.mote` language comes later once validity is proven. | matrix source is the language and the intended model output; the JSON tree is a temporary intermediate. | Owner direction on 2026-10-09. The language is the text syntax. |
| Name is `Mote UI`, file ending `.mote`. | Name is `matrix`. No `.mote`. | The old name collides with the `mote` product and the `moted` daemon. |
| Catalog words in section 7.4 include `page`, `screen`, `card`, `grid`, `dialog`, `sheet`, `chart`, `series`, and more. | Catalog is reorganized and completed in [catalog.md](catalog.md). `chart` and `series` are deferred. `grid` is a collection, not a layout primitive. | Favor a catalog organized by the slice each element needs, and defer charting until a real surface needs it. |
| Semantic color roles `primary`, `danger`, `secondary` and size roles `large`, `small` are modifiers. | Color is never written. `primary`, `confirm`, and `danger` survive only as intent on buttons and actions. Size is never written. | The owner constraint is no style options beyond the theme. |
| Renderer targets named Tauri 2 and Slint, with a license open decision. | Rust runtime with the recommended stack in [runtime.md](runtime.md). | Owner direction on 2026-10-09 chooses Rust and an immediate mode consumer. Crate choices remain recommendations. |

## Deferred elements and why

These were considered and left out of the closed catalog for now. Each has a reason and a trigger for
reconsidering.

- `chart` and `series`. Charting is a large sub language of its own. Defer until a generated surface
  needs a chart that a `table` or a `stat` cannot carry.
- A free `grid` with custom track sizes. Rejected. The wrapping `grid` collection covers tiles.
  Any request to reconsider tracked layout goes to the owner before a catalog change.
- A rich text editor. Rejected. `area` and `compose` cover text entry. A full editor is an
  application, not a generated surface.
- A web view or embedded script. Rejected permanently. This reopens the validation hole matrix
  exists to close.
- `toast` and transient notifications. Deferred. These are a host concern that may not belong in the
  surface language at all.
- A time grid calendar, where an event's top is its start and its height is its duration. Rejected.
  Proportional placement needs pixels, which matrix forbids. The agenda list in `recipes/calendar.md`
  is the accepted representation. Reconsider only if a required surface cannot be read as an agenda.

## Resolved defaults from the grammar iteration

Date 2026-10-10.
These recommendations resolve the five prior open questions.
They remain subject to the owner confirmation list.

1. Exact grammar is frozen in [grammar.md](grammar.md).
   Braces group children and named Action arguments.
   Direct paths replace helper calls and parentheses.
   No comments or expressions are accepted.
   [measurement.md](measurement.md) records bytes and tokens without claiming parser validity.
2. The host owns row records and publishes the versioned binding manifest.
   Source only references those types.
   The PullRequest record examples document that host contract.
3. Stable collection keys alone drive reorder identity and motion.
   Duplicate keys reject the update.
   Keys are local to one collection.
   Cross collection moves use leave and enter motion.
   No animation intent hint is added.
4. Header and footer remain the only anchors.
   Primary actions use a footer or a normal button.
   No floating action affordance is added.
5. Icon names are a separate fixed semantic set listed in the grammar.
   Themes draw those names and cannot extend them.
   Unknown names fail closed.

The iteration makes no catalog addition.
Literal display text and typed alt text are reconciled with existing examples.
Only timestamp accepts relative formatting.
Calendar weeks and kanban columns use the existing grid collection rather than repeated col syntax.
Kanban moves use an explicit target field and Action.
Drag to move is deferred until a declared drop target contract exists.
The secondary presentation modifier is removed because per element style is forbidden.
Search and row opening use existing buttons rather than undeclared event clauses.
Selection is local to the collection's parent and cannot leak out of a repeated template.
The host checks derived argument types against known Action contracts.
An unknown Action never creates a new untyped event contract.

## Runtime follow-up boundary

The v0 tree cannot carry the full catalog, keys, selection or typed Action argument metadata.
The exact available lowering subset is in [runtime.md](runtime.md).
The first Mac runtime milestone draws all eleven unchanged v0 Node variants.
It runs as a separate eframe and egui 0.31.0 consumer on winit 0.30.13 without icrate.
Vector, fixture preview and automated interaction tests are present.
Visible native screenshots and the live daemon-connected v0 checks remain open because the Mac is locked.
The evidence and its limits are in `docs/verification/V0-43.md`.
This milestone does not implement a Matrix parser or keyed reconciliation.
A future consumer contract revision requires explicit owner approval.
This boundary does not defer a grammar decision and does not approve UI-00.

## Acceptance reconciliation

Date 2026-10-10.
The acceptance block in `docs/backlog.md` now follows the Matrix grammar and runtime boundary.
Every criterion remains unchecked.
The criteria specify future verification rather than compiler implementation claims.
The owner gate still controls parser, formatter and compiler work through DEF-04.
The broader catalog requires a separately approved consumer contract revision.
Mac delivery comes first.
Pi and iPad evidence follows explicit target approval.
Recommended Taffy, wgpu, rayon and AccessKit integration is not claimed as delivered.

These IDs preserve their prior purpose with updated scope and observable verification:
UI-00, UI-02, UI-03, UI-04, UI-05, UI-08, UI-10, UI-11, UI-13, UI-14, UI-15, UI-16,
UI-17, UI-18, UI-19, UI-20, UI-22, UI-24, UI-25, UI-26, UI-27, UI-28, UI-29, UI-30,
UI-33, UI-34, UI-36, UI-37, UI-38, UI-42, UI-43, UI-45 and UI-46.
The preserved requirements use current host contracts rather than earlier implicit type inference.
Limits use the current depth, node and byte bounds rather than the earlier row and string limits.
Selection and keyed state checks wait for the contract that can represent them.
Grammar export and validity experiments do not authorize a backend or model-policy change.
Diagnostics remain a future structured interface beyond the existing v0 strings.

The following IDs replace an incompatible requirement with its nearest Matrix requirement.

| ID | Earlier requirement | Reconciled requirement and reason |
|---|---|---|
| UI-01 | Compact nested JSON is the only wire form. | Matrix source is the language and the host lowers only its supported subset to the unchanged v0 JSON consumer contract. Source and transport are separate layers. |
| UI-06 | Every earlier RFC example parses and lowers to a matching tree. | The ten Matrix design cases exercise the frozen grammar, with exact v0 comparisons limited to the documented subset. Most current catalog semantics cannot lower to v0. |
| UI-07 | A decompiler reconstructs the original source after lowering. | Parsed source and binding references round-trip through authorized formatting. Resolved v0 snapshots discard bindings and cannot reconstruct the full source semantics. A decompiler needs a separate lossless contract. |
| UI-09 | Names are case invariant. | Names and declaration words are case sensitive, as frozen in the grammar. |
| UI-12 | Source declarations derive new Action contracts. | Derived argument types are checked against known versioned host Action contracts. Unknown actions cannot create untyped events. |
| UI-21 | Responsive layout includes tracked grids and table-to-card collapse. | Strict row and col layout, screen anchors, fill and automatic scroll are verified. Grid is a collection. Undeclared layout transformations are not promised. |
| UI-23 | Style words are the only styling. | One theme owns presentation and motion. Intent words remain only where the grammar allows them. Per-element style and animation declarations are rejected. |
| UI-31 | Partial JSON renders progressively through open-node metadata. | The current v0 consumer displays complete validated trees. The contract has no open-node keys or partial-tree envelope. |
| UI-32 | Every newline commits a source node. | Token and brace completion governs Matrix streaming. Newlines have no declaration-ending meaning. Incomplete source cannot replace the committed surface. |
| UI-35 | Bounded syntax repair accepts a prefix. | Invalid Matrix source is diagnosed and rejected as a whole. Repair, skipped lines and guessed tokens would contradict the frozen fail-closed grammar. |
| UI-39 | Tauri webview rendering with a CSP. | The first Mac consumer draws all eleven v0 variants in Rust with the delivered eframe and egui dependency path. Native strings do not create a model-reachable web execution surface. |
| UI-40 | Tauri Channel partial-tree delivery within 50 ms. | A separate Rust process uses the existing IPC stream and ui.event calls, with the Mac arrival-to-display budget and live saved-cursor evidence still required. moted stays headless. |
| UI-41 | Slint on Pi DRM is the primary language target. | Later approved Rust runtime delivery covers Pi and iPad after Mac. Actual hardware evidence must establish presenter and input support. Stack recommendations alone prove neither. |
| UI-44 | egui on Pi DRM is a fallback renderer. | An iOS alternative is conditional on the Rust path being impractical and must preserve the approved consumer contract. No competing Pi renderer is silently selected. |
| UI-47 | Slint per-surface compilation meets 50 ms or is replaced. | Rust runtime preparation has a 50 ms Pi budget without per-surface toolkit compilation, plus the existing 30 fps software-path scrolling floor. Measurements wait for Pi delivery. |
| UI-48 | Catalog changes update a tree and golden for every element. | Broader lowering requires explicit consumer contract approval and conformance coverage together. Unsupported semantics retain rejection tests instead of invented v0 equivalents. |

The four-item owner confirmation list above is unchanged by this reconciliation.
No backlog criterion has moved into v0.
