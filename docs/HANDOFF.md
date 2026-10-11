# Handoff: Surface accessibility roles

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `895cbb1`.
Commit: `fix(client): expose surface accessibility roles`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

The production egui surface renderer now emits current-v0 AccessKit roles, names and values from
the existing surface namespace and structural path. The semantic hierarchy covers text, lists,
tables, fields, toggles, selects, actions, progress and chat. Visual labels and controls are hidden
from AccessKit after their semantics are assigned, so the filtered tree has no duplicate nodes.

The observation harness captures the real AccessKit update through `MoteApp::draw_shell` and
traverses an `accesskit_consumer::Tree` with the macOS adapter's `common_filter`. Eleven checked-in
goldens cover all ten fixtures and the gallery. Both 800 by 480 and 1280 by 800 compare exactly, for
22 production-shell comparisons.

UI-22 remains open. It still needs an unlocked native NSAccessibility dump and the other target.
Typed alt text, live status and overlay focus remain outside the current consumer contract.

Focused accessibility passed 22 comparisons. The client passed 101 tests. The workspace passed 383
tests. Workspace clippy and `git diff --check` passed. No remote write or push occurred.
