# Matrix v1 consumer contract implementation plan

Date: 2026-10-10

## Goal

Add a versioned Matrix consumer contract without changing the frozen v0 tree. Version 1 is the
immutable first slice for slider, header, footer and fill plus the v0-equivalent elements needed to
compose them. Later catalog families use later contract versions unless their complete semantics are
resolved before version 1 is activated.

## Compatibility contract

- Keep `mote_ui::Node`, `mote_ui::validate`, the v0 fixtures and `schemas/tree.v0.json` unchanged.
- Add `mote_ui::v1::Surface` and `schemas/tree.matrix.v1.json` for the exact first slice only.
- Keep the `ui.surface` event kind and durable cursor behavior unchanged.
- Carry a mandatory valid v0 fallback in `surface` beside the versioned Matrix surface in
  `matrix_surface` in one event payload.
- Use the exact faithful v0 lowering only when the complete source is lossless in v0. Otherwise use
  one approved v0 Text fallback that says the Matrix surface needs a newer Mote client. Never
  flatten unsupported structure or controls into an approximate interactive tree.
- Let new clients prefer a valid supported Matrix surface. Fall back to v0 when the Matrix surface
  is missing, malformed or has an unknown version.
- Add an optional `surface_contract` number to the `ui.event` RPC context. Omission means v0 for
  legacy clients. Bind the value to `surface_seq` and the matching committed control table. Keep the
  stored `ui.event` body unchanged. Fallback and Matrix paths are not assumed to be isomorphic.
- Keep production v1 emission disabled until the compiler, client, accessibility, layout and event
  admission checks for the emitted variants pass together.

## Task 1: Freeze the contract

Create the approval artifact, typed contract, strict validator and deterministic schema for the
exact first slice. Include exact slider, header, footer and fill fixtures. Record unresolved later
catalog wire choices explicitly. Freeze and test the dual payload shape. Do not change production
emission.

Files:

- Create `matrix/consumer-contract-v1.md`.
- Create `crates/mote-ui/src/v1.rs`.
- Create `crates/mote-ui/examples/gen_matrix_v1_schema.rs`.
- Create `schemas/tree.matrix.v1.json`.
- Create `crates/mote-ui/fixtures-v1/01_slider_layout.json`.
- Modify `crates/mote-ui/src/lib.rs` only to export the new module.

Verify strict version, type, key, bound, depth, node, byte, slider and layout validation. Verify the
v0 schema and fixtures remain unchanged. Verify schema generation is deterministic. Verify strict
validation of the dual envelope, including its valid v0 `surface` and version 1 `matrix_surface`.

Commit: `feat(ui): add Matrix v1 consumer contract`

## Task 2: Add the first v1 lowerer

Create `lower_v1` for screen, col, row, header, footer, text, toggle, select, button and slider. Carry
fill and the explicit row layout semantics. Preserve numbers without narrowing to `f32`. Return the
surface and a daemon-only control table keyed by emitted wire path. Keep production on v0.

Files:

- Create `crates/matrix-lang/src/lower_v1.rs`.
- Create `crates/matrix-lang/tests/lower_v1.rs`.
- Create the matching source and JSON conformance fixture under `matrix/conformance/v1/`.
- Modify `crates/matrix-lang/src/lib.rs` to export the new lowerer.

Commit: `feat(matrix): lower slider and anchored layout to v1`

## Task 3: Decode versioned surfaces safely

Add a client surface enum and one payload decoder. Preserve all v0 behavior. Parse version 1 but
treat it as unsupported and display v0 until the renderer lands in Task 4. After Task 4, prefer a
valid supported version 1 and otherwise validate and display the v0 fallback. Reject both-invalid
updates without replacing the prior committed surface.

Files:

- Create `mote-client/src/contract.rs`.
- Modify client state, app, fixture, gallery and existing test harnesses to use the enum.

Verify the same dual event is independently consumable by the legacy decoder from `surface` and the
version-aware decoder from `matrix_surface` at receipt time. Verify the event reduces once, advances
one durable cursor once and does not duplicate either representation. Surface persistence across a
restart or binary upgrade is explicitly out of scope and remains open. The current connection flow
invalidates surfaces and subscribes after the durable cursor, so reopening does not restore that
surface.

Commit: `refactor(client): decode versioned surface contracts`

## Task 4: Render anchored slider layouts

Add a separate v1 renderer. Keep header and footer outside the scrolling body. Give the body the
remaining height. Implement direct-child fill on the main axis. Implement the Vector slider with a
36 point visual region, a 48 point hit region, AccessKit range semantics, pointer and keyboard
input, bounded stepping and stable immediate-mode identity.

Verify at 400 by 800 and 1200 by 800. Check anchor stability during body scrolling, clips, remaining
space, overlap, focus order, slider pointer and keyboard input, event values and state retention.

Commit: `feat(client): render anchored Matrix slider layouts`

## Task 5: Commit and validate the dual contract

Compile the supported v1 slice in the daemon. Commit the mandatory v0 fallback and the v1 tree
atomically. Use faithful v0 lowering only for a wholly lossless source. Otherwise emit the approved
noninteractive v0 Text fallback. Add `surface_contract` to the RPC interaction context, bind it to
the committed sequence and save separate matching control tables rather than assuming fallback and
v1 paths match. Admit a slider change only when version, sequence, path, field, numeric type, range
and step match. Reject every invalid event before a stored event, configuration write or model call.

Commit: `feat(matrix): commit versioned consumer surfaces`

## Task 6: Activate and demonstrate the slice

Update the live dashboard with header, footer and fill. Keep slider in the gallery until a real host
numeric field is published. Update the relevant evidence without claiming the full catalog or
hardware-only checks. Run focused tests, the whole workspace tests, workspace clippy, the gallery
and the live dashboard.

Commit: `feat(matrix): activate anchored v1 dashboard surfaces`

## Full catalog continuation

Deliver the remaining closed catalog in five families. Each later immutable version is a cumulative
superset, so one surface can combine earlier layout and input nodes with later nodes. Freeze each
family in a later contract version unless its complete semantics were resolved before version 1
activation. Each family requires a source fixture, JSON fixture, schema inventory, lowerer check,
client rendering, accessibility, layout, daemon event admission, gallery coverage, evidence,
workspace tests and clippy before production emission.

1. Text and media. Freeze constrained Markdown, asset fetching and status tone mapping first.
2. Inputs and forms. Include typed action arguments and pending action state.
3. Collections. Add loading, empty, failure, stable keys and scoped selection after the host snapshot
   represents pending and failed state explicitly.
4. Feedback and chat.
5. Overlays and built-in actions, including focus trapping and contextual dismissal.

## Completion boundary

Do not mark the Matrix host/runtime umbrella complete until every catalog element is represented by
the approved contract and passes its full compiler, consumer, interaction and evidence gate. Native
Mac, Pi and iPad checks remain open when the required session or hardware is unavailable.
