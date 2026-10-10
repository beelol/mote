# Handoff: Matrix production-shell layout verification

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Task 5 original base: `a5c286d`. Fix Round 1 base: `2a9b490`.
Task 5 implementation is complete in `docs/superpowers/plans/2026-10-10-matrix-host-runtime-v0.md`.
Fix Round 1 subject: `fix(client): measure the production app layout`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

The new headless observations distinguish response rectangles, hit rectangles, painted bounds,
paint clips, scroll viewport bounds, content extents and scroll offsets.
They exercise the shared production shell, real client view methods and existing surface renderer.
Production update and ObservationHarness call the same `MoteApp::draw_shell` method.
The shell owns 16-point margins and the actual chat navigation panel.
The original standalone 8-point harness geometry is superseded by corrected evidence.
The ten unchanged fixtures run at 800 by 480 and 1280 by 800 in preview and chat.
The 200-item List and 500-row Table reach their final rows in each existing outer scroll area.
No nested collection scroll region is needed.

RED measurements found 36-point actual hit heights for Field, Toggle, Select trigger and Action.
Existing popup option targets were already 48 points high.
Chat previously consumed the viewport and placed its composer below the screen.
The implementation explicitly registers padded hit regions and reserves 72 points for composition.
The theme's surface control visuals remain unchanged.
The composer now uses a 36-point input with a 48-point hit region.

Seven layout tests cover production shell geometry, fixture geometry, control measurements, center and both padded vertical edges,
adjacent controls, the composer and Send, and collection scrolling.
The focused client run passed 90 tests. The measurement check passed unchanged.
Workspace clippy passed.
An earlier sandboxed client run failed three local socket tests with PermissionDenied.
The client rerun passed with local socket access.
The first workspace run exposed an old isolation-helper assumption that one Tab focuses Action.
A diagnostic can make chat scrollable, so egui exposes the outer scroll region to keyboard focus.
The helper now traverses with Tab until the observed prior Action ID is focused.
Every traversal step still asserts unchanged display bounds and no event.
The focused daemon test passes all 173 prefixes, 297 rejected finalizations and 48 switches.
The current workspace rerun passed 324 tests and all doc tests.
Workspace clippy and diff checks passed.
Keyboard setup explicitly traverses and verifies the Chat and Pipeline navigation focus stops.
The candidate helper passes unchanged with the production shell.
The composed Stack at 800 by 480 needs outer scrolling to expose its full Action target.
Initial and final clips and overlap checks pass. All four full targets are reachable.
No further production layout fix was needed.

Correct chat scroll viewports are (16,84) to (784,392) and (16,84) to (1264,712).
Their clips are (0,81) to (800,395) and (0,81) to (1280,715).
Preview viewports are (16,16) to (784,464) and (16,16) to (1264,784).
Composer and Send hit y bounds are 414 to 462 and 734 to 782.

Task 1 admission, Task 2 decoder and candidate isolation, Task 3 literal semantics and Task 4
structural state and focus must remain intact.
The schema, ten consumer fixtures and PNG goldens, source fixtures and deterministic measurements
remain unchanged.

The UI-15/UI-17/UI-43 evidence files and README links are written.
The task report is `.superpowers/sdd/2026-10-10-matrix-host-runtime-v0/task-5-report.md`.
Next: independently review Fix Round 1, then continue Task 6 host snapshot refresh.
UI-15 native Mac captures and later target repetition remain open.
UI-17 later approved touch targets remain open.
UI-43 is verified for the current approved headless path and checked in README.
No Taffy path is approved or implemented.

UI-35 native capture remains pending. The prior Mac capture session was locked.
No model pull, model call, remote write or push has occurred.
The client still persists only its cursor, not chat history.
`docs/rfcs/v0.md` remains authoritative. Production `models.local_only` stays true.
