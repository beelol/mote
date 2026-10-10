# Handoff: Matrix candidate display isolation

Updated 2026-10-10. Replace this file, never append.

Branch: `main`.
Task 2 base: `77b58ef`.
Task 2 commit subject: `test(client): verify committed candidate isolation`.
Preserve untracked `.DS_Store` and `.superpowers/`.
Do not push.

Task 2 adds a reusable test-only observation harness around the real IPC decoder,
application reducer, chat view, `surface::draw` and outbound event queue.
The `test-observation` feature exposes it only for the daemon's development dependency.
Passive draw hooks observe structural paths, egui namespaces, nodes and painted rectangles.
They do not add controls or change layout.

The ten unchanged JSON fixtures cover 1,963 framed byte splits, 1,963 malformed frames
and 20 malformed complete payloads.
The daemon action/form regression covers 173 byte prefixes, 297 rejected finalizations
and 48 single whole-surface switches.
Candidate-only positions emit nothing on taps or Enter.
Prior controls retain their sequence, path and exact args.
The form parses but remains unsupported by v0 lowering.
No form approximation or consumer metadata was added.

Tests exposed two framing defects.
`EventDecoder` now retains bytes across timeouts, including splits inside UTF-8.
Malformed complete frames are skipped without invalidating the committed display.
EOF before a newline is rejected.
Hard disconnect and resync behavior remain unchanged.
Each reconnect still reads status and subscribes on the same socket.
The replay cutoff still suppresses historical Matrix surfaces and clears expired interaction state.

Task 1 admission contracts, event retention atomicity, diagnostics and source ownership remain intact.
The sequence and zero-based path still travel outside the v0 Node payload.
Schema, fixtures and deterministic measurement outputs are unchanged.
No live model call, model pull, source persistence, remote write or push occurred.

Verification passed: 9 IPC tests, 67 client tests, 25 focused daemon tests,
296 workspace tests and all doc tests, workspace clippy, deterministic measurements and diff checks.
Socket-dependent suites ran with local socket access after sandbox permission failures.
Evidence: `docs/verification/UI-31.md`, `UI-33.md` and `UI-34.md`.
The full report is `.superpowers/sdd/2026-10-10-matrix-host-runtime-v0/task-2-report.md`.

Next: independently review Task 2, then continue Task 3 in
`docs/superpowers/plans/2026-10-10-matrix-host-runtime-v0.md`.
Task 3 pins literal text rendering and forbidden Matrix source semantics.

UI-29 remains partial for full-catalog actions and activation context.
UI-35 native capture remains pending in `docs/verification/UI-35.md`.
Its isolated fixture is `/private/tmp/mote-ui35.XJxtru`.
The Mac was locked during that capture attempt.
Headless observations do not replace native captures or claim Pi or iPad acceptance.
The client still persists only its cursor, not chat history.

`docs/rfcs/v0.md` remains authoritative.
Production `models.local_only` stays true.
Every model call uses the ledger and every stored event passes through redaction.
