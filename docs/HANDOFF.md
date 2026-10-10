# Handoff: Matrix committed event admission

Updated 2026-10-10. Replace this file, never append.

Branch: `main`.
Task 1 base: `2b1e97b`.
Commit subject: `fix(matrix): validate committed surface events`.
Preserve untracked `.DS_Store` and `.superpowers/`.
Do not push.

Task 1 adds per-thread Matrix contracts installed after successful `ui.surface` storage.
Their revision is the stored event sequence.
The client sends `surface_seq` and a zero-based structural `node_path` outside consumer JSON.
Root controls use an empty path.
Controls must be exposed by the current committed surface and match trusted types and exact args.
Refresh must be exposed and takes empty args.
Legacy approval cards use their explicit approve/deny route with existing approval checks.
Arbitrary non-Matrix action names are rejected.

The real renderer, reducer and ordered worker carry the new context.
The production surface payload, transport helpers and measurement outputs remain unchanged.
Whole-source rendering, candidate commitment and Refresh share contract installation.
The retained-source lock serializes admission with replacement.

Verification passed: 22 focused daemon tests, 63 client tests, 284 workspace tests and doc tests,
workspace clippy, deterministic Matrix measurements and diff whitespace checks.
Socket tests ran with local socket access.
No live model call or pull occurred.
Evidence is in `docs/verification/UI-29-v0.md`.
Exact RED/GREEN output is in `.superpowers/sdd/2026-10-10-matrix-host-runtime-v0/task-1-report.md`.

Next: independently review Task 1, then continue Task 2 of
`docs/superpowers/plans/2026-10-10-matrix-host-runtime-v0.md`.
Task 2 must drive the real reducer, renderer and outbound queue with the new sequence/path envelope.
Keep incomplete candidate controls inert and preserve the previous committed contract.

UI-29 remains partial for full-catalog action arguments and activation context.
The older native diagnostic capture remains pending in `docs/verification/UI-35.md`.
Its isolated fixture is `/private/tmp/mote-ui35.XJxtru`.
The Mac was locked during that capture attempt.
No native, Pi or iPad evidence is claimed.
The client still persists only its cursor, not chat history.

`docs/rfcs/v0.md` remains authoritative.
Production `models.local_only` stays true.
Every model call uses the ledger and every stored event passes through redaction.
