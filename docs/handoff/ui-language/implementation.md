# Handoff: implement Matrix after approval

For a fresh agent. Nothing here assumes chat history. Date: 2026-10-10.

## Gate

Do not start this handoff until the owner closes `UI-00` and records a response to all four items
in `matrix/decisions.md`. Owner approval selects the criteria to move from `docs/backlog.md` into
an active README list. This document does not approve Matrix, activate a criterion or authorize a
consumer contract revision.

Read `AGENTS.md`, `docs/rfcs/v0.md`, the root README, `matrix/grammar.md`, `matrix/runtime.md`,
`matrix/decisions.md`, `matrix/catalog.md` and the selected acceptance criteria before editing.
The v0 contract remains authoritative.

## Activation order

Move only owner-selected criteria into active scope. Preserve their identifiers and full Verify
clauses. Use this order unless the recorded approval changes it.

1. Freeze the source toolchain with UI-06 through UI-12, UI-32, UI-35 and UI-45. Build parsing,
   formatting, scope resolution, type checking and structured diagnostics without a renderer.
2. Implement the current v0 lowering subset with UI-01 through UI-05, UI-13, UI-31, UI-33 and
   UI-34. Reject every unsupported catalog feature with `E_LOWER_UNSUPPORTED`.
3. Connect approved host behavior with UI-26 through UI-30, UI-36, UI-38 and UI-40. Keep moted
   headless. Send only complete validated trees to the consumer and validate events before storage.
4. Propose a separate consumer contract revision before activating UI-15 through UI-23 or UI-25.
   That revision must carry every identity, selection, binding, accessibility and layout semantic
   the selected criteria require.
5. Run model, target and performance work only when separately selected. This covers UI-14, UI-24,
   UI-37, UI-41 through UI-44 and UI-46 through UI-48.

## Owned files

- Put the approved source implementation in a new `crates/matrix-lang/` workspace crate. Keep its
  parser, formatter, resolver, type checker, lowerer and diagnostic types independent of egui.
- Keep source fixtures and language documentation under `matrix/`.
- Change `crates/mote-ui/` only for an explicitly approved consumer contract revision. Never add
  arbitrary JSON keys to bypass the versioned schema.
- Put host manifests, resolved binding snapshots and candidate commitment in moted. Keep all model
  calls on the existing ledger path and redact before event storage.
- Keep drawing and transient interaction state in `mote-client/`. The client must not parse Matrix
  source or own durable binding data.
- Put criterion evidence in `docs/verification/<ID>.md`. Update the README checkbox only after its
  full Verify clause passes.

## Fixtures

- Treat `matrix/conformance/v0/*.matrix` as the positive lowering baseline. Add canonical expected
  JSON beside each source only after compiler approval. Compare with the matching unchanged v0
  fixture.
- Add positive source fixtures for every selected grammar form. Add negative fixtures for every
  rejection named by the selected criteria.
- Keep `crates/mote-ui/fixtures/*.json` and their PNG files unchanged until an approved contract
  revision names a migration and new golden set.
- Record host manifests and binding snapshots explicitly. A test must not hide host resolution in
  an ad hoc mock.
- Cut streaming fixtures at token, UTF-8 and brace boundaries. Assert that no incomplete candidate
  replaces the last committed surface or emits an action.

## Commands

Run the narrow test first while developing. Run the full contract before every commit.

```text
cargo test -p matrix-lang
cargo test -p mote-ui
cargo test -p mote-client
cargo test --workspace
cargo clippy --workspace -- -D warnings
sh matrix/tools/measure.sh
git diff --check
```

Record the exact command, output, environment and date in the selected criterion's evidence file.
Model experiments also record every call in the ledger. Pull only a model named in
`docs/rfcs/v0.md` section 0.

## Stop conditions

Stop without inventing a default when any of these is true.

- UI-00 is open or the owner's four answers are incomplete.
- Work requires a backlog criterion the owner did not move into active scope.
- Lowering requires a semantic absent from the unchanged v0 tree and no consumer contract revision
  is approved.
- A source feature cannot be represented losslessly by the approved host and consumer contracts.
- A model, target, dependency or network action falls outside the approved local-only policy.
- A Verify clause needs unavailable hardware, an unlocked desktop or an owner action. Record the
  exact remaining evidence and continue only with independent selected work.
- Workspace tests or clippy fail. Fix the failure before committing.

Commit small verified changes to `main` with Conventional Commits and push after each accepted
slice. Never close UI-00 on the owner's behalf.
