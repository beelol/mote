# Handoff: Matrix v1 consumer contract

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `eeaa277`. Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

The next implementation slice is the separately versioned Matrix consumer contract. The accepted
plan is `docs/superpowers/plans/2026-10-10-matrix-v1-consumer-contract.md`. Keep the v0 tree and
schema unchanged. Add an unreachable v1 contract first, then lower, decode, render, validate and
activate slider plus header, footer and fill in separate verified commits.

The planned compatibility envelope keeps the existing `ui.surface` event and mandatory valid v0
`surface`. It adds a versioned `matrixSurface`. New clients prefer a valid supported version and
fall back to v0. Production v1 emission stays disabled until matching compiler, client,
accessibility, layout and event-admission checks pass.

The first task creates `matrix/consumer-contract-v1.md`, `mote_ui::v1`, a deterministic schema and
the slider/anchor/fill fixture. Full catalog choices that are not frozen remain visible in the
contract artifact. Service uninstall commit `eeaa277` received an independent PASS review.
