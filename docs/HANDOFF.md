# Handoff: Pipeline configuration client closure

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `287312f`.
Commit: `fix(client): apply pipeline config asynchronously`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

The production Pipeline shell has a populated headless regression for all five stages and all four
tabs. The test enters Pipeline through the real shell and clicks each stage and tab at 1280 by 800.
All 20 states retain complete navigation. Now, Numbers and Requests contain stage-distinct data.
Config contains all nine v0 keys.

Config Apply no longer performs socket work on the egui render thread. A persistent background
worker submits only dirty typed keys in `CONFIG_KEYS` order. Invalid local boolean and integer text
does not enter IPC. Edit then revert and repeated Apply while pending send nothing.

Batch outcomes are reconciled by edit revision. Failed keys stay dirty and show their error.
Successful keys clear only when the submitted revision is still current. One snapshot refresh starts
after the complete batch when at least one key succeeded. Snapshot installation updates clean keys
and preserves failed or concurrently edited keys. Configuration errors survive successful snapshot
installation.

V0-47 and V0-48 remain open. V0-47 still requires live-turn screenshots of the native Pipeline
view. V0-48 still requires a native Config click correlated with the already verified before-and-after
ledger rows.

The focused client suite passes 98 tests. The full workspace passes 380 tests. Workspace clippy and
`git diff --check` pass.

No remote write or push occurred.
