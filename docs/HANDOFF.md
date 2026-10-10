# Handoff: Matrix dashboard connected to Mote

Updated 2026-10-10. Replace this file, never append.

## Where

- Branch: `main`.
- Latest implementation commit: `6e1ac31`. The current HEAD also contains the verification
  documentation recorded with this handoff.
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

1. Implement the production Matrix candidate stream so every prefix preserves the committed
   surface. Then close UI-32 and the commitment part of UI-35.
2. Add a visible in-app diagnostic artifact and one bounded diagnostic-bearing retry.
3. Version the full-catalog consumer contract. Add slider, strict layout, anchors, fill, collection
   selection, general action arguments, accessibility roles and host surface history.
4. Keep current v0 JSON surfaces and approval cards compatible while each replacement is verified.

## Rules

- `docs/rfcs/v0.md` remains authoritative for behavior not replaced by a verified Matrix contract.
- `models.local_only` stays true. Pull only the three approved models.
- Every model call uses the ledger. Redaction runs before storage. Structured verdicts fail closed.
- Run workspace tests and clippy with warnings denied before every commit.
