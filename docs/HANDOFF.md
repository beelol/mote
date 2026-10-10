# Handoff: Matrix runtime layout verification

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Task 5 base: `a5c286d`.
Task 5 implementation is complete in `docs/superpowers/plans/2026-10-10-matrix-host-runtime-v0.md`.
Commit subject: `fix(client): enforce runtime layout invariants`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

The new headless observations distinguish response rectangles, hit rectangles, painted bounds,
paint clips, scroll viewport bounds, content extents and scroll offsets.
They exercise the real client view methods and existing surface renderer.
The ten unchanged fixtures run at 800 by 480 and 1280 by 800 in preview and chat.
The 200-item List and 500-row Table reach their final rows in each existing outer scroll area.
No nested collection scroll region is needed.

RED measurements found 36-point actual hit heights for Field, Toggle, Select trigger and Action.
Existing popup option targets were already 48 points high.
Chat previously consumed the viewport and placed its composer below the screen.
The implementation explicitly registers padded hit regions and reserves 72 points for composition.
The theme's surface control visuals remain unchanged.
The composer now uses a 36-point input with a 48-point hit region.

Six layout tests cover fixture geometry, control measurements, center and both padded vertical edges,
adjacent controls, the composer and Send, and collection scrolling.
The focused client run passed 89 tests and the measurement check passed.
Workspace clippy passed.
An earlier sandboxed client run failed three local socket tests with PermissionDenied.
The client rerun passed with local socket access.
The first workspace run exposed an old isolation-helper assumption that one Tab focuses Action.
A diagnostic can make chat scrollable, so egui exposes the outer scroll region to keyboard focus.
The helper now traverses with Tab until the observed prior Action ID is focused.
Every traversal step still asserts unchanged display bounds and no event.
The focused daemon test passes all 173 prefixes, 297 rejected finalizations and 48 switches.
The workspace passed 323 tests and all doc tests.
The final pre-commit rerun also passed with the completed paint-clip assertions.

Task 1 admission, Task 2 decoder and candidate isolation, Task 3 literal semantics and Task 4
structural state and focus must remain intact.
The schema, ten consumer fixtures and PNG goldens, source fixtures and deterministic measurements
remain unchanged.

The UI-15/UI-17/UI-43 evidence files and README links are written.
The task report is `.superpowers/sdd/2026-10-10-matrix-host-runtime-v0/task-5-report.md`.
Next: independently review Task 5, then continue Task 6 host snapshot refresh.
UI-15 native Mac captures and later target repetition remain open.
UI-17 later approved touch targets remain open.
UI-43 is verified for the current approved headless path and checked in README.
No Taffy path is approved or implemented.

UI-35 native capture remains pending. The prior Mac capture session was locked.
No model pull, model call, remote write or push has occurred.
The client still persists only its cursor, not chat history.
`docs/rfcs/v0.md` remains authoritative. Production `models.local_only` stays true.
