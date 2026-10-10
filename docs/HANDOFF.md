# Handoff: Matrix host snapshot refresh

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Task 6 base: `36639ad`.
Requested subject: `feat(matrix): refresh validated host snapshots`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

The daemon now retains one accepted typed `HostSnapshot` per Matrix thread.
The fixed host snapshot contains a numeric progress Value and a list of string-labelled rows.
The daemon adds those bindings to its owned manifest and binding snapshot.
No RPC accepts a manifest or host snapshot from the client.
The client still consumes only validated `ui.surface` events.

Compilation is factored around the proposed host snapshot.
Resolution and supported v0 lowering complete before the existing atomic event commitment path.
The exact manifest used for lowering also builds the committed control contract.
Successful commitment updates the surface, optional canonical source and accepted host snapshot.
Rejection changes none of those three values.

Focused acceptance starts from progress 0.25 and one populated list row.
One host refresh changes progress to 0.75 and replaces the list with three rows.
The newly lowered JSON has the exact values and rows.
The real decoder, reducer and renderer observe one complete replacement.
No displayed node contains a source path.
The scripted Ollama request count and context ledger count remain unchanged.

An out-of-range progress update fails before commitment.
Pending, failure, key and selection declarations are valid full-catalog source semantics but remain
unsupported by v0 lowering.
Each returns a structured diagnostic and preserves the prior surface, source and accepted snapshot.

UI-30 remains partial.
Loaded progress and populated-list updates are covered.
Pending and failed collection transitions and revised-consumer key and selection behavior remain
contract-gated.
No native screenshot is claimed.

The focused client suite passed 91 tests.
The focused daemon suite passed 65 unit tests and two integration tests.
The final workspace test passed 327 tests and all doc tests.
The deterministic Matrix measurement check passed with the updated host manifest and snapshot
baseline.
Workspace clippy passed with warnings denied.
The final diff check passed.
The Task 6 report belongs at
`.superpowers/sdd/2026-10-10-matrix-host-runtime-v0/task-6-report.md` after the commit exists.

Tasks 1 through 5 must remain intact.
UI-35 native capture remains pending because the prior Mac session was locked.
No model pull, model call, remote write or push has occurred.
The client still persists only its cursor, not chat history.
`docs/rfcs/v0.md` remains authoritative.
Production `models.local_only` stays true.
