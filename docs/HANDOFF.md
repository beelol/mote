# Handoff: Matrix toolchain acceptance closure in progress

Updated 2026-10-10. Replace this file, never append.

## Where

- Branch: `main`.
- Current HEAD: `47019ac`.
- The configured remote is still untrusted by the execution environment. Push was refused earlier.
  Do not bypass that restriction. Preserve the untracked `.DS_Store`.

## Completed slice

- `matrix-lang` lexes, parses, formats, streams, resolves and lowers the supported v0 subset.
- The final resolver budget overcount is fixed in `833d4a4`.
- `mote-client --gallery` opens one scrollable native page containing all eleven v0 Node variants.
- `moted` owns a trusted Matrix manifest and snapshot for the four model settings, the three
  approved local model choices, current pipeline-stage summaries and Refresh.
- `matrix.render` atomically compiles and commits complete valid source. Invalid source returns
  structured diagnostics and leaves the previous surface unchanged.
- `mote-client --matrix matrix/examples/mote-dashboard.matrix` opens the working native dashboard.
- Toggle and select changes use typed `event` and `args` envelopes. The daemon rejects values
  outside the trusted contract. Successful changes recompile the retained source.
- Matrix surfaces use stable identity and update in place. Cached immediate-mode control state
  yields to changed host values. Failed events show an error and roll back optimistic state.
- One ordered event worker keeps the render thread responsive and preserves click order.
- UI-10 and V0-45 are verified. UI-29 and UI-30 have partial evidence and remain open.

## Verification

- Final focused client run: 59 tests passed.
- Final focused Matrix daemon dispatch run: 10 tests passed.
- `cargo test --workspace` passed after the integration fixes.
- `cargo clippy --workspace -- -D warnings` passed.
- `git diff --check` passed after documentation whitespace cleanup.
- An isolated live run under `/tmp/mote-matrix-live2` displayed one native dashboard. Clicking Local
  only changed the daemon config, emitted `ui.event`, recompiled the surface and updated the same
  dashboard. The temporary value was restored to true.
- Final screenshot: `/tmp/mote-matrix-dashboard-final.png`.
- Independent final review approved the complete slice with no remaining findings.

## Next actions

1. Reconcile UI-02, UI-03, UI-04, UI-05, UI-07 and UI-09 against current tests and add only the
   missing explicit contract tests, transport measurement and evidence.
2. Integrate `CandidateStream` into the daemon commitment path so incomplete and rejected chunks
   preserve the last committed surface. Close UI-32 only when its full Verify clause passes.
3. Emit a visible client diagnostic artifact for rejected Matrix source without replacing the last
   valid surface. Close UI-35 only when its full Verify clause passes.
4. Continue into the versioned full-catalog consumer contract after the toolchain criterion is
   fully checked.

## In flight

- The read-only gap audit is complete at
  `.superpowers/sdd/2026-10-10-matrix-toolchain/gap-audit.md`.
- The extension plan is `docs/superpowers/plans/2026-10-10-matrix-toolchain-closure.md`.
- Workspace tests and clippy passed before the plan checkpoint. Task 1 is the next dispatch.
- No product edit has started for this slice. The untracked `.DS_Store` remains untouched.

## Rules

- `docs/rfcs/v0.md` remains authoritative for behavior not replaced by a verified Matrix contract.
- `models.local_only` stays true. Pull only the three approved models.
- Every model call uses the ledger. Redaction runs before storage. Structured verdicts fail closed.
- Run workspace tests and clippy with warnings denied before every commit.
