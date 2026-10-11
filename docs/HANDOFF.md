# Handoff: Matrix v1 consumer contract

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Preserve untracked `.DS_Store` and `.superpowers/`. `origin` is configured, but the
push was rejected because its trust was not established by trusted user content. Do not push.

The accepted plan is `docs/superpowers/plans/2026-10-10-matrix-v1-consumer-contract.md`.
Task 1 is complete in commits `3cb25d1` and `5ab7641`. Task 2 is complete in commit `f54c1ad`.

Task 3 adds one version-aware client decoder. A valid dual payload retains the strict version 1
surface and its mandatory valid v0 fallback as one reduced message. Missing, unknown or malformed
`matrix_surface` data uses the valid v0 fallback. An invalid v0 fallback rejects the update even
when version 1 is valid. Rejected updates preserve the committed display and render revision while
the receipt cursor still advances for the received event.

Version 1 remains unsupported for drawing until Task 4. The app draws only the retained v0 fallback.
Live messages and local previews both cross the version-aware surface boundary. Fixture and gallery
previews are wrapped as version 0 and keep their existing rendering, accessibility and layout
behavior. Production `matrix.render` and daemon emission remain on v0. Surface persistence across a
restart or binary upgrade remains open and out of scope.

Task 4 is next. It adds the separate anchored version 1 renderer and keeps production emission
disabled until the later daemon task.
