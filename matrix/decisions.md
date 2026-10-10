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
9. matrix lives in its own isolated folder and does not depend on the rest of mote.

## Decisions the owner must confirm for UI-00

Each has a recommended answer. Confirm or change each one.

1. Catalog coverage. Is the catalog in [catalog.md](catalog.md) the right closed set? Recommended:
   yes, with the explicit deferrals below. Changing it means adding or removing a word now, before a
   parser exists.
2. One theme per surface, no per element style. Recommended: keep. This is the owner constraint from
   2026-10-08.
3. Three bindings only: `Value`, `Field`, `Action`. Recommended: keep.
4. Whole surface replacement, host computes the diff, model never patches. Recommended: keep.
5. Selection exposes a typed `selected` value. Recommended: keep.
6. Status tones are a closed set of six meanings, colored by the theme. Recommended: keep.
7. matrix source is the model's output format, and the temporary JSON tree is internal. Recommended:
   confirm. This is the main departure from the earlier RFC, which recommended JSON as the model's
   output for v0 and matrix only later. See the departures section.
8. The syntax sketch in the examples. Recommended: treat as a proposal, not frozen. The next
   iteration refines the exact grammar against a token measurement.

## Departures from the earlier UI language RFC

The earlier RFC is `docs/rfcs/ui-language.md`. matrix keeps its research and most of its constraints
and departs in these places. None is a silent deletion. Each is a recorded decision.

| Earlier RFC | matrix | Reason |
|---|---|---|
| Model emits compact JSON for v0; the `.mote` language comes later once validity is proven. | matrix source is the language and the intended model output; the JSON tree is a temporary intermediate. | Owner direction on 2026-10-09. The language is the text syntax. |
| Name is `Mote UI`, file ending `.mote`. | Name is `matrix`. No `.mote`. | The old name collides with the `mote` product and the `moted` daemon. |
| Catalog words in section 7.4 include `page`, `screen`, `card`, `grid`, `dialog`, `sheet`, `chart`, `series`, and more. | Catalog is reorganized and completed in [catalog.md](catalog.md). `chart` and `series` are deferred. `grid` is a collection, not a layout primitive. | Favor a catalog organized by the slice each element needs, and defer charting until a real surface needs it. |
| Semantic color roles `primary`, `danger`, `secondary` and size roles `large`, `small` are modifiers. | Color is never written. `primary`, `confirm`, and `danger` survive only as intent on buttons and actions. Size is never written. | The owner constraint is no style options beyond the theme. |
| Renderer targets named Tauri 2 and Slint, with a license open decision. | No renderer technology is named. matrix assumes a host and a runtime only. | Owner direction on 2026-10-09 that the interpreter is not chosen. The renderer RFC decides targets. |

## Deferred elements and why

These were considered and left out of the closed catalog for now. Each has a reason and a trigger for
reconsidering.

- `chart` and `series`. Charting is a large sub language of its own. Defer until a generated surface
  needs a chart that a `table` or a `stat` cannot carry.
- A free `grid` with custom track sizes. Rejected. The wrapping `grid` collection covers tiles. Add a
  tracked grid only if a required surface cannot be expressed with nested `row` and `col`.
- A rich text editor. Rejected. `area` and `compose` cover text entry. A full editor is an
  application, not a generated surface.
- A web view or embedded script. Rejected permanently. This reopens the validation hole matrix
  exists to close.
- `toast` and transient notifications. Deferred. These are a host concern that may not belong in the
  surface language at all.
- A time grid calendar, where an event's top is its start and its height is its duration. Rejected.
  Proportional placement needs pixels, which matrix forbids. The agenda list in `recipes/calendar.md`
  is the accepted representation. Reconsider only if a required surface cannot be read as an agenda.

## Open questions for the next iteration

1. The exact grammar. The examples sketch it. The next pass freezes it against a token measurement
   on the conformance suite, comparing matrix source to the temporary JSON tree.
2. The exact row type notation a surface relies on, shown in the pull request example as a small
   record. Decide whether the surface declares it or the host publishes it.
3. Whether `key` alone is enough for reordering and animation, or whether a surface ever needs an
   animation intent hint. Prefer `key` alone.
4. Whether `header` and `footer` are enough anchors, or whether a floating action affordance is a
   distinct need.
5. The icon set the `icon` element draws from, and whether it is part of the theme or a separate
   fixed set.
