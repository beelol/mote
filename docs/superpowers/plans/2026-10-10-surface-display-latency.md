# Surface arrival-to-render latency plan

## Goal

Wake the egui client immediately when IPC delivers an event and measure accepted `ui.surface`
arrival through its first real production render pass. Keep UI-40 open until an unlocked native run
records at least 20 visible refreshes below 50 ms at p50 and p95.

## Task 1: Timestamp stream arrival and wake the renderer

**Files:**
- Create: `mote-client/src/display_latency.rs`
- Modify: `mote-client/src/main.rs`
- Modify: `mote-client/src/stream.rs`
- Modify: `mote-client/src/app.rs`
- Modify: `mote-client/src/state.rs`

- [ ] **Step 1: Add a shared monotonic clock**

Use one Instant origin in production and a deterministic manual clock in tests. Timestamp a complete
decoded event immediately after `EventStream::read_event` and before queueing it.

- [ ] **Step 2: Request immediate repaint**

Start the stream thread with the real egui context. After every successful queue send call
`request_repaint()`. Keep the 100 ms repaint timer only as an idle fallback. Test the wake callback.

- [ ] **Step 3: Track accepted surface revisions**

Assign a private render revision only after the exact `ui.surface` passes validation and reduction.
Keep it separate from surface sequence, instance and control identity. Rejected, stale and malformed
events must create no latency sample.

## Task 2: Record first production render exactly once

**Files:**
- Modify: `mote-client/src/app.rs`
- Modify: `mote-client/src/observation.rs`

- [ ] **Step 1: Return drawn surface revisions from the real shell**

Have the production chat/surface draw path report revisions traversed in the frame. Match a pending
arrival immediately after `draw_shell`. Record only the first frame and remove it.

- [ ] **Step 2: Cover accepted and excluded paths**

With a manual clock, accepted arrival at 100 ms and first real draw at 111 ms records one 11 ms
sample; later frames record none. Rejected, malformed, stale and superseded-before-frame surfaces
record none. Pipeline defers the sample until Chat actually renders the surface.

- [ ] **Step 3: Pin summary arithmetic**

Use 20 explicit fake samples for nearest-rank p50/p95 tests. Do not call them Mac performance
evidence.

## Task 3: Add an opt-in report and partial UI-40 evidence

**Files:**
- Create or modify: `docs/verification/UI-40.md`
- Replace: `docs/HANDOFF.md`

When `MOTE_UI_LATENCY_REPORT` names a path, write one JSON report on clean client shutdown. Include
sample count, p50 and p95 plus the declared timestamp boundary. Store only sequence ids and monotonic
offsets/durations, never payload or source.

State clearly that the boundary is complete decoded IPC frame through first egui render-list
construction. It includes queueing, validation, reduction and CPU render. It does not prove GPU or
compositor presentation.

Keep UI-40 open. Native completion needs a release daemon/client under one temporary MOTE_HOME,
visible dashboard, at least 20 Refresh actions with ui.event round trips, report p50/p95 below 50 ms,
and close/reopen cursor proof without duplicate display.

## Verification

Run focused client tests, then:

```text
cargo test -p mote-client
cargo test --workspace
cargo clippy --workspace -- -D warnings
git diff --check
```

Commit: `perf(client): measure surface arrival to render`
