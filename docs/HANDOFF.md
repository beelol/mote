# Handoff: Matrix v1 consumer contract

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Preserve untracked `.DS_Store` and `.superpowers/`. `origin` is configured, but the
push was rejected because its trust was not established by trusted user content. Do not push.

The accepted plan is `docs/superpowers/plans/2026-10-10-matrix-v1-consumer-contract.md`.
Task 1 is complete in commits `3cb25d1` and `5ab7641`. Task 2 is complete in commit `f54c1ad`.

Task 3 added one version-aware client decoder. A valid dual payload retains the strict version 1
surface and its mandatory valid v0 fallback as one reduced message. Missing, unknown or malformed
`matrix_surface` data uses the valid v0 fallback. An invalid v0 fallback rejects the update even
when version 1 is valid. Rejected updates preserve the committed display and render revision while
the receipt cursor still advances for the received event.

Task 4 adds the separate renderer in `mote-client/src/surface_v1.rs`.
Live chat and local previews now draw validated version 1 surfaces.
Unsupported or invalid Matrix data still uses the valid version 0 fallback.
The version 0 renderer and gallery catalog remain unchanged.

Screen reserves Header and Footer outside the scrolling body.
The body owns the remaining height and supports horizontal nowrap overflow.
Fill children share remaining main-axis space equally after intrinsic siblings and standard gaps.
Wrap, nowrap, alignment and between or around distribution are implemented.
The Vector slider has a 36 point visual region and a 48 point hit region.
Pointer, arrow, Home, End and AccessKit range input produce bounded local changes.
Contract, type and structural path define runtime identity.
Host value and constraint replacements reset edits.
Removed controls clear state, focus and select popups.
Disabled buttons are inert.
Confirmed buttons require explicit local confirmation.
Changed events, labels or confirmation flags clear an armed confirmation.

The preview-only Gallery variant cannot be produced by decoding.
It keeps the original version 0 catalog followed by a bounded version 1 slice in one outer scroll.
Gallery interactions update local preview status without a daemon.

Version 1 events remain renderer-internal.
Live chat never forwards them to `send_ui_event`.
No `surface_contract` argument is sent.
Production `matrix.render` and daemon emission remain on version 0.
Task 5 must add contract-specific control binding and event admission before activation.
Surface persistence across restart or binary upgrade remains outside this task.

The full implementation and verification report is
`.superpowers/sdd/2026-10-10-matrix-v1-consumer-contract/task-4-report.md`.
It records headless checks at 400 by 800 and 1200 by 800, workspace verification and gallery smoke.
