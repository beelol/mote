# Handoff: Matrix toolchain acceptance closure in progress

Updated 2026-10-10. Replace this file, never append.

## Where

- Branch: `main`.
- Task 4 base: `4f5a8eeb8b26f88972ac4866497cb88d268928ed`.
- Task 4 commit subject: `feat(moted): commit finalized Matrix candidates`.
- Preserve untracked `.DS_Store` and `.superpowers/` scratch files.
- The configured remote remains untrusted. Do not push.

## Completed work

- Tasks 1 through 3 were accepted by the controller.
- Task 4 adds consuming stream finalization and the production `matrix.candidate` RPC.
- Required parameters are `thread`, `candidate`, byte-array `chunk` and boolean `finish`.
- Non-final statuses are `incomplete`, `complete` and `rejected`.
- Non-final requests emit no events and never replace retained source.
- Rejection remains latched until finish removes the candidate.
- A successful finish returns `committed` and emits one `ui.surface`.
- Finalized ASTs use original spans for resolution and lowering.
- Successful candidate commits retain canonical source under the existing source lock.
- Legacy `matrix.render` and the shared production envelope helpers remain compatible.
- README links verified UI-32 and leaves the Matrix toolchain umbrella unchecked.

## Verification and limits

- All 11 stream tests passed.
- All nine daemon tests selected by `matrix_` passed.
- The full workspace passed with local socket access: 272 tests and all doc tests.
- The sandboxed attempt failed at the existing context scripted-server socket test.
- `cargo clippy --workspace -- -D warnings` passed.
- `sh matrix/tools/measure.sh --check` passed with unchanged output.
- `git diff --check` passed.
- No models were called or pulled.
- No live-model, native-rendering, Pi or iPad evidence is claimed.
- Each candidate retains at most 256 KiB of source bytes.
- Unfinished candidates remain in daemon memory until finish or daemon shutdown.
- No aggregate candidate count cap or timeout is introduced by this task.
- UI-35 diagnostic events and visible client diagnostics remain in Task 5.

## Exact next action

Review Task 4 against its brief and `.superpowers/sdd/2026-10-10-matrix-toolchain-closure/task-4-report.md`.
Continue with Task 5 after the controller accepts Task 4.
Task 5 may add events on final rejection.
Keep the pre-finalization no-event assertions intact when extending the final rejection tests.

## Rules

- `docs/rfcs/v0.md` remains authoritative for existing v0 consumer semantics.
- `models.local_only` stays true. Pull only the three approved models.
- Every model call uses the ledger. Redaction runs before storage.
- Structured verdicts fail closed.
