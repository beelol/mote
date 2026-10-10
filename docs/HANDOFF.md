# Handoff: Matrix native diagnostic capture pending

Updated 2026-10-10. Replace this file, never append.

## Where

- Branch: `main`.
- Task 5 base: `c4d9ee4aae1314da0acdf2fc361f13937706e454`.
- Task 5 commit subject: `feat(client): show Matrix rejection diagnostics`.
- Preserve untracked `.DS_Store` and `.superpowers/` scratch files.
- The configured remote remains untrusted. Do not push.

## Completed work

- Tasks 1 through 4 were accepted by the controller.
- Terminal whole-source and candidate rejections append one redacted `ui.diagnostic` event.
- Non-final candidate pushes still emit no events.
- Diagnostic events retain structured fields and candidate identity without full source.
- Client system messages display code, byte span, message and optional hint beside the prior surface.
- Diagnostic replay is deterministic and duplicate diagnostic sequences are ignored.
- Structured `--matrix` rejection continues into the client instead of exiting before display.
- Five exact malformed inputs pass through both production entry points.
- Every case preserves the prior dashboard and retained Refresh source.
- The shared transport envelope helpers and measurements remain unchanged.

## Verification

- Eleven focused daemon Matrix tests passed.
- All 62 client tests passed.
- The full workspace passed with local socket access: 277 tests and all doc tests.
- `cargo clippy --workspace -- -D warnings` passed.
- `sh matrix/tools/measure.sh --check` passed.
- `git diff --check` passed.
- No model calls or pulls occurred.
- No native screenshot, Pi or iPad evidence is claimed.

## Native capture blocker and exact next action

Capture the real diagnostic and retained dashboard when an unlocked GUI session exists.
Code acceptance can be independently reviewed now.
The exact attempt and blocker are in `docs/verification/UI-35.md`.
UI-35 and the Matrix toolchain umbrella remain unchecked.

The safe isolated fixture remains at `/private/tmp/mote-ui35.XJxtru`.
Its `submit.py` script commits the dashboard and rejects the `live-invalid` final candidate.
Stored sequence 3 is the dashboard and sequence 4 is the diagnostic.
The actual client reduced the events and saved cursor 4.
The UI tool reported that the Mac was locked and automatic unlock failed.
Lookup by executable path and process name also reported `Invalid app` for the unbundled client.
No native pixels could be inspected.
The isolated daemon and client processes were stopped after the attempt.

Restart the daemon with that `MOTE_HOME` and `HF_HUB_OFFLINE=1`.
Rerun `submit.py` so retained source exists in the new daemon process.
Move only this fixture's `client_cursor` aside before launching its client.
Capture the window containing both the prior dashboard and diagnostic.
Then update UI-35 evidence and check the umbrella if its remaining Verify clauses are satisfied.

## Limits

The client still persists only its cursor, not message history.
This task proves deterministic replay and in-session deduplication.
It does not claim process-restart history persistence.
Unfinished candidate state remains bounded per candidate as in Task 4.
There is no new aggregate candidate cap or timeout.

## Rules

- `docs/rfcs/v0.md` remains authoritative for existing consumer behavior.
- `models.local_only` stays true.
- Every model call uses the ledger. Redaction runs before event storage.
- Structured verdicts fail closed.
