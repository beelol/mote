# Handoff: Matrix literal and forbidden source semantics

Updated 2026-10-10. Replace this file, never append.

Branch: `main`.
Task 3 base: `042a4cc`.
Task 3 commit subject: `test(matrix): pin literal and forbidden semantics`.
Preserve untracked `.DS_Store` and `.superpowers/`.
Do not push.

Task 3 adds exact characterization tests.
No production parser, resolver, lowerer or renderer behavior changed.
The test-only observation harness now returns egui output as well as displayed nodes.
The feature-gated test library exposes the shape type for daemon integration tests.

HTML-looking strings, a script-tag string and Markdown punctuation lower to exact Text nodes.
The real decoder, reducer, renderer and outbound queue preserve their literal characters.
The client observes one style section per literal.
Clicks and Enter create no surface action.
No URL or clipboard command is emitted on clicks.

The checked diagnostic corpus now has 25 cases.
Its 17 new cases cover forbidden presentation, state, navigation and layout source forms.
They include an undeclared state read and a valid catalog collection grid.
Tracked layout syntax rejects at the parser.
The valid collection grid resolves and retains `E_LOWER_UNSUPPORTED`.
All diagnostic fields are exact.
Both whole-source render and final candidate submission preserve the prior source and contract.
Each of 34 terminal rejections emits one redacted diagnostic without a source literal.
The real prior Refresh control still renders the prior source in all 34 cases.
Configuration, packs, ledger and model-request counts stay unchanged.

CandidateStream retains its first terminal prefix rejection.
Submitting an invalid identifier one byte at a time may therefore name an earlier prefix.
The exact shared corpus supplies the whole source before finalization.
It does not claim diagnostic equality across every malformed-source fragmentation.

Partial evidence: `docs/verification/UI-13.md`, `UI-21.md`, `UI-23.md`,
`UI-26.md` and `UI-27.md`.
All five criteria remain open.
Markdown, broader theme groups, layout acceptance and host history still need their full clauses.
README links the evidence without claiming closure.

Task 1 admission and retention atomicity remain intact.
Task 2's decoder fixes and UI-31, UI-33 and UI-34 evidence remain unchanged.
The schema, ten consumer fixtures, nine source fixtures and deterministic measurements are unchanged.
No live model call, model pull, remote write or push occurred.

Verification passed: 117 Matrix tests, 68 client tests, 62 daemon unit tests and
two daemon integration tests in the focused run.
The full workspace run passed 301 tests and all doc tests.
Workspace clippy, the deterministic measurement check and diff checks passed.
Socket-dependent suites ran with local socket access.

Next: independently review Task 3, then continue Task 4 in
`docs/superpowers/plans/2026-10-10-matrix-host-runtime-v0.md`.
Task 4 reconciles interaction state by live structural identity.
The full Task 3 report is
`.superpowers/sdd/2026-10-10-matrix-host-runtime-v0/task-3-report.md`.

UI-29 remains partial for full-catalog actions and activation context.
UI-35 native capture remains pending in `docs/verification/UI-35.md`.
Its isolated fixture is `/private/tmp/mote-ui35.XJxtru`.
The Mac was locked during that capture attempt.
Headless observations do not replace native captures or claim Pi or iPad acceptance.
The client still persists only its cursor, not chat history.

`docs/rfcs/v0.md` remains authoritative.
Production `models.local_only` stays true.
Every model call uses the ledger and every stored event passes through redaction.
