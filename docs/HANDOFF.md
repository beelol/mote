# Handoff: Matrix v1 consumer contract

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Preserve untracked `.DS_Store` and `.superpowers/`. There is no remote.

The accepted plan is `docs/superpowers/plans/2026-10-10-matrix-v1-consumer-contract.md`.
Task 1 is complete in commits `3cb25d1` and `5ab7641`. It adds the inactive version 1 consumer
contract, strict schema and exact slider, header, footer and fill fixture. The frozen v0 contract is
unchanged.

Task 2 adds a production-unreachable `lower_v1`. It lowers only screen, col, row, header, footer,
text, toggle, select, slider and zero-argument host buttons. It returns the version 1 surface plus a
daemon-only control table keyed by emitted child-index paths. Numeric values must round-trip through
finite `f64` without changing their decimal value. Unsupported catalog elements, option labels,
built-in actions, typed action arguments and lossy numbers fail with `E_LOWER_UNSUPPORTED`.

The exact Task 2 conformance pair is `matrix/conformance/v1/01_slider_layout.matrix` and
`matrix/conformance/v1/01_slider_layout.json`. Production `matrix.render`, daemon emission and the
client remain on v0.

Task 3 is next. It adds version-aware client decoding while treating version 1 as unsupported until
the Task 4 renderer lands. The payload keeps mandatory valid v0 `surface` and snake-case
`matrix_surface`. Restart and binary-upgrade surface restoration remain open and out of scope.
