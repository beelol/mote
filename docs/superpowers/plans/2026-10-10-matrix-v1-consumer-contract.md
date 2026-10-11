# Matrix v1 consumer contract implementation plan

Date: 2026-10-10

## Goal

Add a versioned Matrix consumer contract without changing the frozen v0 tree. Deliver slider,
header, footer and fill through the real immediate-mode client first. Continue through the closed
catalog in bounded families after the first production slice is verified.

## Compatibility contract

- Keep `mote_ui::Node`, `mote_ui::validate`, the v0 fixtures and `schemas/tree.v0.json` unchanged.
- Add `mote_ui::v1::Surface` and `schemas/tree.matrix.v1.json`.
- Keep the `ui.surface` event kind and durable cursor behavior unchanged.
- Carry a mandatory valid v0 fallback beside the versioned Matrix surface in one event payload.
- Let new clients prefer a valid supported Matrix surface. Fall back to v0 when the Matrix surface
  is missing, malformed or has an unknown version.
- Keep the `ui.event` request and stored event body unchanged.
- Keep production v1 emission disabled until the compiler, client, accessibility, layout and event
  admission checks for the emitted variants pass together.

## Task 1: Freeze the contract

Create the approval artifact, typed contract, strict validator, deterministic schema and catalog
inventory. Include exact slider, header, footer and fill fixtures. Record unresolved full-catalog
wire choices explicitly. Do not change production emission.

Files:

- Create `matrix/consumer-contract-v1.md`.
- Create `crates/mote-ui/src/v1.rs`.
- Create `crates/mote-ui/examples/gen_matrix_v1_schema.rs`.
- Create `schemas/tree.matrix.v1.json`.
- Create `crates/mote-ui/fixtures-v1/01_slider_layout.json`.
- Modify `crates/mote-ui/src/lib.rs` only to export the new module.

Verify strict version, type, key, bound, depth, node, byte, slider and layout validation. Verify the
v0 schema and fixtures remain unchanged. Verify schema generation is deterministic.

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

Add a client surface enum and one payload decoder. Preserve all v0 behavior. Prefer valid version 1
and otherwise validate and display the v0 fallback. Reject both-invalid updates without replacing
the prior committed surface.

Files:

- Create `mote-client/src/contract.rs`.
- Modify client state, app, fixture, gallery and existing test harnesses to use the enum.

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
atomically. Save the exact matching control table at the event sequence. Admit a slider change only
when sequence, path, field, numeric type, range and step match. Reject every invalid event before a
stored event, configuration write or model call.

Commit: `feat(matrix): commit versioned consumer surfaces`

## Task 6: Activate and demonstrate the slice

Update the live dashboard with header, footer and fill. Keep slider in the gallery until a real host
numeric field is published. Update the relevant evidence without claiming the full catalog or
hardware-only checks. Run focused tests, the whole workspace tests, workspace clippy, the gallery
and the live dashboard.

Commit: `feat(matrix): activate anchored v1 dashboard surfaces`

## Full catalog continuation

Deliver the remaining closed catalog in five families. Each family requires a source fixture, JSON
fixture, schema inventory, lowerer check, client rendering, accessibility, layout, daemon event
admission, gallery coverage, evidence, workspace tests and clippy before production emission.

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
