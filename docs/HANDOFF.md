# Handoff: Matrix toolchain acceptance closure in progress

Updated 2026-10-10. Replace this file, never append.

## Where

- Branch: `main`.
- Task base: `cb1dd39dd9a3af2c1425a91c4314b7f9e5ba79e8`.
- Preserve untracked `.DS_Store` and `.superpowers/` scratch files.
- The existing remote remains untrusted. Do not bypass its push restriction.

## Selected work

- Task 1 adds static schema, property alias and name case acceptance assertions.
- The controller owns the review after the implementer reports.
- No subagents or reviewers were spawned by the implementer.
- UI-02, UI-03, UI-07 and UI-09 have full evidence and README links.
- The Matrix toolchain umbrella remains unchecked.

## In flight

- Acceptance assertions are added in `mote-ui` and Matrix parser and resolver tests.
- Existing formatter tests passed for design fixtures and 1,000 generated surfaces.
- The UI package and formatter tests passed on the first run.
- Two test expectation errors were corrected after reading parser and resolver diagnostic paths.
- The parser and resolver rerun passed. No production behavior has changed.
- Evidence files and README links are written.
- Self-review found no production changes or missing Task 1 assertions.
- The commit containing this snapshot closes Task 1 and awaits controller review.

## Verification and limits

- UI package: 11 tests passed. Parser: 18 passed. Resolver: 40 passed. Formatter: 7 passed.
- The exact schema command passed with 1 test.
- Initial parser run: 17 passed and one test expectation error.
- Initial resolver run: 39 passed and one test expectation error.
- The initial workspace run failed at the existing context scripted-server test.
  Its local socket bind returned `Operation not permitted` under the sandbox.
- `cargo clippy --workspace -- -D warnings` passed.
- The full workspace rerun passed with local socket access: 254 tests and doc tests.
- The full Matrix package passed: 101 tests and doc tests.
- `git diff --check` passed.
- Workspace checks may need local scripted-server socket access outside the sandbox.
- No live model, native rendering, Pi or iPad evidence is claimed by this task.
- UI-04, UI-05, UI-32 and UI-35 remain for later closure tasks.

## Exact next action

Review `test(matrix): close static toolchain contracts` against the Task 1 brief.
Read `.superpowers/sdd/2026-10-10-matrix-toolchain-closure/task-1-report.md`.
Continue with Task 2 after the controller accepts Task 1.

## Rules

- `docs/rfcs/v0.md` remains authoritative for existing v0 consumer semantics.
- `models.local_only` stays true. Pull only the three approved models.
- Every model call uses the ledger. Redaction runs before storage.
- Structured verdicts fail closed.
