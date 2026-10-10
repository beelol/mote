# matrix decisions

This document lists the decisions an owner confirms to move matrix forward, and it records where
matrix departs from the earlier UI language RFC. Nothing here is approved. The formal gate is `UI-00`
in `docs/backlog.md`, which only the owner closes.

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

Confirm or name explicit changes to these four defaults.
Nothing here closes `UI-00` or moves work into v0.

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
The next milestone draws validated v0 trees without changing that schema.
A future consumer contract revision requires explicit owner approval.
This boundary does not defer a grammar decision and does not approve UI-00.
