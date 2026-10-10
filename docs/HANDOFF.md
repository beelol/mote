# Handoff: Matrix toolchain acceptance closure in progress

Updated 2026-10-10. Replace this file, never append.

## Where

- Branch: `main`.
- Task 2 base: `9bd59c7c925928dc5b92aa847e4b46809257b0ec`.
- Preserve untracked `.DS_Store` and `.superpowers/` scratch files.
- The existing remote remains untrusted. Do not bypass its push restriction.

## Completed work

- Task 1 was accepted independently by the controller.
- UI-02, UI-03, UI-07 and UI-09 have evidence and README links.
- Task 2 adds reproducible source, host and transport measurements for UI-04.
- The dashboard source and compiled consumer tree use the production trusted host contract.
- The manifest and snapshot rows are declared canonical measurement encodings of in-memory structures.
- The request, response and event rows include the current IPC envelope fields and newline framing.
- Shared production builders link the measurements to the client, server and daemon serialization.
- Six independent production drift probes fail the check, including daemon result and surface payload changes.
- These rows are deterministic samples, not live traffic captures.
- UI-04 has evidence and a README link.
- No Matrix language or runtime behavior changed.
- The Matrix toolchain umbrella remains unchecked.

## Verification and limits

- The required-row check failed before the utility emitted host and transport rows.
- Temporarily omitting the event row also failed the completed check.
- Two checked complete TSV outputs were byte-identical.
- The full workspace passed with local socket access: 254 tests and doc tests.
- The sandboxed attempt failed at the existing context scripted-server socket test.
- `cargo clippy --workspace -- -D warnings` passed.
- `git diff --check` passed.
- Measurement uses local locked `tiktoken-rs` 0.6.0 with `o200k_base` and ordinary encoding.
- The shell wrapper needs installed jq and offline cached workspace dependencies.
- No model, native rendering, Pi or iPad evidence is claimed by this task.
- UI-05, UI-32 and UI-35 remain for later closure tasks.

## Exact next action

Review `docs(matrix): measure host transport costs` against the Task 2 brief.
Read `.superpowers/sdd/2026-10-10-matrix-toolchain-closure/task-2-report.md`.
Continue with Task 3 after the controller accepts Task 2.

## Rules

- `docs/rfcs/v0.md` remains authoritative for existing v0 consumer semantics.
- `models.local_only` stays true. Pull only the three approved models.
- Every model call uses the ledger. Redaction runs before storage.
- Structured verdicts fail closed.
