# Handoff: surface arrival-to-render latency

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `1c54668`. Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

The client now uses one monotonic clock for stream arrival and rendering. Complete decoded events are
timestamped before queueing. Every successful queue send requests an immediate egui repaint. The
100 ms repaint timer remains as an idle fallback.

Accepted exact surfaces receive a private render revision after validation and replay checks. The
first production Chat draw records that revision once. Rejected, malformed, stale and
superseded-before-frame surfaces do not produce samples. Pipeline frames defer measurement until Chat
actually draws the surface. Render revisions do not alter surface sequence, instance, widget identity
or `ui.event` parameters.

Set `MOTE_UI_LATENCY_REPORT` to a file path before starting the client. Clean shutdown writes a JSON
report with sample count, nearest-rank p50 and p95, boundary, sequence ids and monotonic timing only.
No payload or source is stored. The boundary is complete decoded IPC frame through first egui
render-list construction. GPU and compositor presentation are outside it.

UI-40 remains open. The automated tests are deterministic correctness checks, not native performance
evidence. Completion still requires an unlocked release Mac run with one temporary `MOTE_HOME`, a
visible dashboard, at least 20 Refresh actions and `ui.event` round trips, p50 and p95 below 50 ms,
and close and reopen cursor proof with no duplicate display. Record that run in
`docs/verification/UI-40.md`.

Verification is green. `cargo test -p mote-client` passed 112 tests. `cargo test --workspace`
passed 398 tests plus doc-tests. `cargo clippy --workspace -- -D warnings` and `git diff --check`
passed.
