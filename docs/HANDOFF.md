# Handoff: Matrix v1 consumer contract

Updated 2026-10-10 in America/Los_Angeles.

Branch: `main`.
Preserve untracked `.DS_Store` and `.superpowers/`.
Do not push.

The accepted plan is `docs/superpowers/plans/2026-10-10-matrix-v1-consumer-contract.md`.
Tasks 1 through 5 are implemented.
Task 5 uses the commit subject `feat(matrix): commit versioned consumer surfaces`.
Its reviewed integer admission fix uses `fix(matrix): reject lossy slider event numbers`.
Its detailed evidence is `.superpowers/sdd/2026-10-10-matrix-v1-consumer-contract/task-5-report.md`.

The daemon resolves each source once.
The frozen v1 slice emits one dual `ui.surface` event.
The v0 fallback is faithful only when the whole resolved source lowers losslessly.
Anchors, fill, slider and other v1-only semantics use the exact approved inert Text fallback.
Valid sources outside the v1 slice retain their lossless v0 emission.
The frozen v0 tree, schema and fixtures remain unchanged.

Each committed sequence owns separate v0 and v1 control tables.
Omitted `surface_contract` admits against v0.
Exact integer `surface_contract: 1` admits against v1.
All other explicit contract values reject.
Every interaction requires the exact committed sequence and structural path.
Slider admission uses the frozen validator's bounded decimal alignment checks.
JSON integers must convert to the frozen f64 value without loss.
Signed and unsigned integer boundaries are checked before range or step validation.
It rejects half steps and nonfinite intermediate arithmetic.
It does not require the total range step count to fit a finite float.
Validation and append failure preserve the prior source, snapshot, both tables and sequence.
Stored `ui.event` bodies contain only `event` and `args`.

The live client forwards v1 renderer events once with contract 1 context.
Rejected changes restore the host value and show the worker error.
Preview and Gallery interactions remain local.
Legacy integration tests explicitly use a legacy client observation mode.
Version-aware checks use the supported v1 renderer.

Verification passed on macOS 26.6.2 arm64 with Rust and Cargo 1.89.0.
The workspace test run passed 471 tests with local socket access.
The focused daemon suite passed 127 tests.
The focused client binary suite passed 156 tests.
Workspace clippy passed with denied warnings.
The diff check passed.

Task 6 remains open.
Update the live dashboard with header, footer and fill.
Keep slider in Gallery until a real numeric host field is published with its write behavior.
The current production manifest intentionally exposes no numeric field.
Slider admission tests use compiled numeric test bindings.
No numeric configuration field or model configuration was added.
Run the gallery and live dashboard and update the relevant evidence.
Surface persistence across restart or binary upgrade remains out of scope.
The later catalog and native Mac, Pi and iPad checks remain open.
