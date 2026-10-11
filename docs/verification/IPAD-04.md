# IPAD-04 verification

Date: 2026-10-11.
Implementation commit: `6c8d6ac923436761325648e8b9a0439a725a18bd`.
Status: Verified. The live model validity gate and paired native Simulator journey pass.

## Environment and boundaries

macOS 26.6.2, build 25G83, arm64.
Rust and Cargo 1.89.0.
Ollama 0.35.0.
Surface model: `qwen3:4b`, Q4_K_M, 2,497,293,931 bytes.
Model digest: `359d7dd4bcdab3d86b87d73ac27966f4dbb9f5efdfcc75d34a8764a09474fae7`.
Compiler contract version: `1`.
Fixed corpus hash: `447a7a73f4669a6450bf88a9e1ab5bc03f914c2ab70d213f3d398e49980a5d2d`.

The corpus contains 20 prose-only requests and 18 requests for each of ten surface categories.
The checked-in corpus and its hash test did not change.
Every generation uses `generate_surface`, the strict JSON parser, the host manifest, Matrix parser, resolver and v0 lowerer.
Every generation writes one pack and one ledger row.
Every generation uses `think=false`, temperature zero and `num_ctx=32768`.
No model was pulled.
Only `qwen3:4b` generated responses in these live checks.
All live data is isolated under `/private/tmp`.
The default user data directory was not changed.
The isolated configuration retains `models.local_only=true`.

Ollama metadata was read without generation:

```text
curl -fsS http://127.0.0.1:11434/api/version
{"version":"0.35.0"}
curl -fsS http://127.0.0.1:11434/api/tags
qwen3:4b digest=359d7dd4bcdab3d86b87d73ac27966f4dbb9f5efdfcc75d34a8764a09474fae7
qwen3:4b size=2497293931 quantization=Q4_K_M
```

## Diagnosis and controlled experiments

The initial run at `70e213a280d5c157f5a85520efab52c5b009a83e` used `/private/tmp/mote-surface-gate.ik2fVn`.
Its 200 valid JSON envelopes all chose `none`.
Only the 20 prose-only cases were correct.
It wrote 200 packs and 200 `surface|qwen3:4b|ok` ledger rows.
There was no activation attestation.

Changing only the schema to a root object with the Matrix branch first did not change that result.
This run used `/private/tmp/mote-surface-schema.YzjiaG`.
It returned 20 first-try and final valid cases, 200 valid envelopes, zero Matrix parses and zero retries.

The next prompt gave complete syntax examples for the host-owned declarations.
It clarified that completed prose and empty diagnostics do not cancel the original request.
The first run used `/private/tmp/mote-surface-prompt.flKYjF`.
The diagnostic repeat used `/private/tmp/mote-surface-diagnostics.WLU1w1`.
Both returned 175 first-try and 176 final valid cases with 220 attempts and 20 retries.
All 18 table cases failed with `E_LEX_INCOMPLETE_STRING`.
The diagnostic repeat also recorded one unresolved list projection and five unexpected `none` text decisions.

A ledgered diagnostic of the unchanged first table case used `/private/tmp/mote-surface-source.tGPWLo`:

```text
MOTE_HOME=/private/tmp/mote-surface-source.tGPWLo MOTE_LIVE=1 cargo test -p moted live_surface_table_diagnostic -- --ignored --nocapture
samples=1 attempts=2 first_attempt_valid=0 after_retry_valid=0 retries=1
rejected source: screen { table from pipeline.stages { column "Stage" .stage column "Calls" .calls column "Latency" .lat" } }
```

The general table example was shortened to a complete Stage and Calls table.
No corpus request or expected result changed.
Repeating the diagnostic on the same case returned one valid first attempt and no retry.
The data directory contains three packs and three surface ledger rows for these two diagnostic runs.
This single-case diagnostic did not write an activation attestation.

## Final fixed-corpus gate

The first full run with the compact table example used `/private/tmp/mote-surface-compact.tNngJw`.
It reached 192 first-try and 194 final valid cases.
The following fresh run repeated those counts against the committed implementation:

```text
MOTE_HOME=/private/tmp/mote-surface-attested.8F07uH MOTE_LIVE=1 cargo run -p moted -- bench surface
exit 0
```

Its complete `bench.jsonl` result is:

```json
{"name":"surface","revision":"6c8d6ac923436761325648e8b9a0439a725a18bd","model":"qwen3:4b","model_digest":"359d7dd4bcdab3d86b87d73ac27966f4dbb9f5efdfcc75d34a8764a09474fae7","compiler_contract_version":1,"corpus_hash":"447a7a73f4669a6450bf88a9e1ab5bc03f914c2ab70d213f3d398e49980a5d2d","category_counts":{"action":18,"field":18,"list":18,"none":20,"progress":18,"row":18,"select":18,"stack":18,"table":18,"text":18,"toggle":18},"status":"complete","reason":null,"activation_passed":true,"metrics":{"samples":200,"attempts":202,"envelope_valid":202,"matrix_parsed":176,"host_resolved":174,"v0_lowered":174,"first_attempt_valid":192,"after_retry_valid":194,"retries":2,"first_attempt_rate":0.96,"after_retry_rate":0.97,"first_try_by_category":{"action":18,"field":18,"list":17,"none":20,"progress":18,"row":18,"select":18,"stack":18,"table":18,"text":12,"toggle":17},"terminal_failures":{"text: unexpected_none":6},"rejected_sources":{}}}
```

All 20 prose-only cases still abstain correctly.
The two retries recover the list and toggle cases.
The six terminal failures return `none` for requested literal-text surfaces.
Activation depends on 192 first-try valid cases, not the 194 final valid cases.

The isolated durable attestation is `/private/tmp/mote-surface-attested.8F07uH/surface-activation.json`:

```json
{"model":"qwen3:4b","model_digest":"359d7dd4bcdab3d86b87d73ac27966f4dbb9f5efdfcc75d34a8764a09474fae7","corpus_hash":"447a7a73f4669a6450bf88a9e1ab5bc03f914c2ab70d213f3d398e49980a5d2d","compiler_contract_version":1,"first_try_valid":192,"sample_count":200}
```

The file is mode `0600`.
It is written through a temporary file, synced and atomically renamed.
Failed, unavailable or changed-digest evidence removes any prior attestation.
The production reader still refuses absent, malformed or stale identity, corpus, compiler or score fields.
No activation file was written to the default data directory.

Ledger and event accounting:

```text
sqlite3 /private/tmp/mote-surface-attested.8F07uH/mote.db 'select lane,model,status,count(*) from ledger group by lane,model,status; select count(*) from packs; select kind,count(*) from events group by kind;'
surface|qwen3:4b|ok|202
202
model.request|202
model.response|202
```

## Production and failure verification

The committed daemon also completed one live production turn in `/private/tmp/mote-surface-production.6yXCJb`.
Its isolated configuration sets talk, router and think to `qwen3:4b` and retains `local_only=true`.
The exact passing attestation was copied only into that isolated directory with mode `0600`.
The existing embedding cache was available.
The daemon ran with `HF_HUB_OFFLINE=1`.

```text
MOTE_HOME=/private/tmp/mote-surface-production.6yXCJb HF_HUB_OFFLINE=1 target/debug/moted
python3 /private/tmp/mote-surface-production.6yXCJb/drive_turn.py
matrix.render: {"ok":true,"valid":true}
turn request: Hello. Show a compact panel with a refresh button.
turn reply: {"ledger_id":1,"pack_id":1,"status":"done"}
```

The prior surface at seq 3 contained `Prior surface` and a Refresh button.
The talk stream contained 1,053 deltas totaling 4,426 characters.
It included explanatory text and an HTML/CSS code sample.
That answer stayed in the talk stream.
`talk.done` at seq 1061 preceded one inferred surface at seq 1066.
The accepted Matrix surface contained literal `Mote` text and the host-owned Refresh button.
No second inferred surface was committed.
The router verdict at seq 1063 arrived after the completed talk answer and reported `think`.
No think harness or tool call ran in this check.
The late router outcome is recorded without changing routing behavior in this task.

```text
sqlite3 /private/tmp/mote-surface-production.6yXCJb/mote.db 'select lane,model,status,count(*) from ledger group by lane,model,status; select count(*) from packs;'
router|qwen3:4b|ok|1
surface|qwen3:4b|ok|1
talk|qwen3:4b|ok|1
3
python3 /private/tmp/mote-surface-production.6yXCJb/check_surface.py
matrix.candidate: rejected with E_RESOLVE_ACTION, Unknown action
stale surface action: invalid_params
invented action: invalid_params
action with invented arguments: invalid_params
current Refresh action: {"ok":true,"rendered":{"ok":true,"valid":true}}
Prior surface preserved; stale and invented actions refused; valid refresh made zero model calls.
```

The live safety check compared the exact committed payload before and after each refusal.
The valid Refresh preserved that payload and left the ledger and pack counts at three.
The isolated daemon was stopped after these checks.

The production turn tests use the real dispatcher with a scripted local-model boundary.
They prove that prose precedes one committed surface, one invalid candidate receives one retry, a second rejection preserves the prior surface, transport failure preserves prose and prior surface, and stale activation is refused.
They also verify model ledger counts and stale or invented typed interaction rejection.

```text
cargo test -p moted surface_
35 passed; 0 failed
cargo test -p moted inferred_production_turn_
4 passed; 0 failed
cargo test -p moted surface_report_records_only_complete_exact_digest_evidence
1 passed; 0 failed
cargo test --workspace
exit 0; no failed tests
cargo clippy --workspace -- -D warnings
exit 0
```

The schema expectation failed before the schema change.
The prompt-example test failed before complete examples were added.
The durable recorder and report tests failed before their implementations existed.
The compact table test failed with three columns before the example changed to two.
The rejected-source test failed before the benchmark retained redacted compiler evidence.
The focused tests then passed.
The final workspace and lint outputs are retained at `/private/tmp/mote-surface-final-tests.log` and `/private/tmp/mote-surface-final-clippy.log`.
The production-turn output is retained at `/private/tmp/mote-surface-production-tests.log`.

## Paired native Simulator verification

Date: 2026-10-11, 10:48 to 10:49 PDT.
Source base: `61f6bfd64850b8333627f25dea8155b1af123e1a`.
Device: iPad Pro 11-inch (M5), iOS 27.0.
UDID: `43F2F6ED-5C86-4121-B03F-40FE38891C4E`.
Viewport: 834 by 1210 points in portrait.
Native capture: 1668 by 2420 pixels.
Xcode 27.0, build 27A266a, iPhoneSimulator 27.0 SDK.
The macOS and Rust versions remain those recorded above.

The harness builds and installs the exact native app before its first launch.
It starts the real daemon dispatcher and Noise gateway in isolated `/private/tmp` storage.
It generates a new host identity and pairs the app using a real single-use MOTE1 offer.
The harness confirms the pending native device on the host.
There is no authentication bypass or hard-coded host key.
The final host is `bb39981f246e887b6d5f7996520c5b27c6bb50cd521f3fa940f4f3fefc44e354`.

The local scripted model boundary returns four ordinary talk answers and six structured surface replies.
The surface and talk lanes use the approved `qwen3:4b` name.
The router uses the approved `qwen3:0.6b` name with deterministic talk verdicts.
No model is loaded or pulled for this deterministic failure journey.
These scripted replies test production behavior, not live model quality.
The original 192-of-200 live gate remains the model-quality evidence.
The harness copies its passing attestation only to the temporary host directory with mode 0600.
The scripted inventory returns that exact model digest.
The daemon still applies its normal identity, corpus and compiler activation checks.
`models.local_only` remains true.
The default MOTE_HOME is untouched.

Command:

```text
MOTE_LIVE=1 TMPDIR=/private/tmp MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E MOTE_ARTIFACTS=/private/tmp/mote-inferred-native-attempt3 MOTE_SURFACE_ATTESTATION=/private/tmp/mote-surface-attested.8F07uH/surface-activation.json cargo test -p mote-ios --test inferred_surface_simulator native_inferred -- --ignored --nocapture
```

The harness runs:

```text
xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -destination id=43F2F6ED-5C86-4121-B03F-40FE38891C4E -derivedDataPath /private/tmp/mote-inferred-derived build
xcrun simctl install 43F2F6ED-5C86-4121-B03F-40FE38891C4E /private/tmp/mote-inferred-derived/Build/Products/Debug-iphonesimulator/Mote.app
xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -destination id=43F2F6ED-5C86-4121-B03F-40FE38891C4E -derivedDataPath /private/tmp/mote-inferred-derived -resultBundlePath /private/tmp/mote-inferred-native-attempt3/native.xcresult -only-testing:MoteUITests/MoteUITests/testInferredSurfaceSuccessRetryPreservationAndFallback test
```

Output:

```text
** BUILD SUCCEEDED **
Executed 1 test, with 0 failures (0 unexpected) in 49.920 seconds
** TEST SUCCEEDED **
Native inferred success, one retry, stable prior surface, visible terminal diagnostic and prose fallback passed. 14 calls, 14 ledger rows, 14 packs; 3 authenticated typed refusals.
test result: ok. 1 passed; 0 failed; 0 ignored; finished in 72.60s
exit 0
```

XCUITest enters each natural-language request in the actual native composer and taps Send.
It uses native screenshots and on-screen text recognition to assert each resulting state.
All four final screenshots were directly inspected.

| Native state | Event evidence | Visible capture |
| --- | --- | --- |
| Success | User seq 2, talk.done seq 7, one generated surface seq 12. | [Success](artifacts/ipad-inferred-2026-10-11/success.png) shows ordinary prose and Success panel with Refresh. |
| One retry recovery | User seq 15, talk.done seq 21, two surface calls, one accepted surface seq 27. | [Recovery](artifacts/ipad-inferred-2026-10-11/one-retry.png) shows Recovery prose and Recovered panel. |
| Two invalid replies | User seq 28, talk.done seq 35, two surface calls, terminal diagnostic seq 40, no new surface. | [Preservation](artifacts/ipad-inferred-2026-10-11/prior-preserved.png) shows Preservation prose, E_RESOLVE_ACTION and the same Recovered panel. |
| Ordinary prose fallback | User seq 41, talk.done seq 47, one none decision, cursor 50, no new surface. | [Fallback](artifacts/ipad-inferred-2026-10-11/prose-fallback.png) shows Ordinary prose fallback and the preserved panel. |

Both retry requests carry the actual structured `E_RESOLVE_PATH` and `Unknown Value path` diagnostic.
The first invalid source uses an unresolved Value path.
The second invalid source attempts an invented action.
The host rejects it with `E_RESOLVE_ACTION` and `Unknown action`.
No invalid source or invented control reaches the consumer.
The saved paired client retains the exact committed surface sequence 27 through final cursor 50.
The terminal diagnostic appears as a delivered System message rather than replacing that surface.

The native Refresh tap creates one typed ui.event at seq 13 and a host re-render at seq 14.
It is not another inference call.
The host receives at most one generated surface for each completed turn.
The complete log has four surfaces: initial fixture, success, typed Refresh and recovery.
It has four turn.user events, four ledger-linked talk.done events and one terminal ui.diagnostic.

A second real paired authenticated client submits three invalid typed interactions.
A stale surface sequence, an invented action and invented action arguments each return InvalidParams.
They add zero model calls.
No rejected generated action is activated.

Ledger query and output:

```text
select lane,model,status,count(*) from ledger group by lane,model,status;
select count(*) from ledger;
select count(*) from packs;
router|qwen3:0.6b|ok|4
surface|qwen3:4b|ok|6
talk|qwen3:4b|ok|4
14
14
```

The scripted server recorded exactly 14 POST /api/chat calls.
The redacted host event log contains exactly 14 model.request and 14 model.response events.
Each call has one ledger row and one pack.
The six surface calls are distributed 1, 2, 2 and 1 across the four native turns.
The ordinary native typed tap and all three refused interactions make no extra model call.

The [artifact directory](artifacts/ipad-inferred-2026-10-11/) contains the exact build, native and harness logs, public host state, redacted events, ledger counts and four PNG captures.
No pairing offer, private credential, host database or displaced prior Documents state is checked in.
The harness deletes only its temporary native Keychain credential and confirms that it is missing.
The [deletion record](artifacts/ipad-inferred-2026-10-11/credential-deletion.json) comes from a
second complete paired native journey after the cleanup guard was added.
It stops the app and restores all displaced prior Documents files, including on test failure.
The XCUITest child is killed and waited on if the harness exits early.
The prior-state backup is preserved if restoration fails.
Temporary host storage is then removed by the fixture lifecycle.
No physical iPad, Apple account, provisioning, distribution signing or TestFlight work occurs.

## Diagnostic regression and final gates

The paired ThreadProjection previously ignored ui.diagnostic while advancing its cursor.
The focused terminal_inference_diagnostic_remains_visible_in_paired_thread test failed before the fix.
It now passes and verifies the projected code, message and optional hint.
The projection fix does not mutate committed surfaces or authorize an interaction.

The first native attempt activated the previously installed running app before rebuilding and failed the visible-diagnostic assertion.
Its host diagnostic and preserved surface were intact.
The harness now terminates, rebuilds and installs before pairing.
The second native XCUITest passed all four states but its post-run assertion incorrectly expected the rejected identifier in a compiler diagnostic.
The captured request contained the correct structured diagnostic without that identifier.
The corrected structured-field assertion and every post-run check passed in the third complete journey above.
The red native logs remain under `/private/tmp/mote-inferred-native-attempt1` and `/private/tmp/mote-inferred-native-attempt2`.
The focused red and green diagnostic logs are included in the artifact directory.

The final frozen-source workspace gate ran after the complete native journey:

```text
cargo test --workspace > /private/tmp/mote-inferred-final-workspace.log 2>&1
exit 0
61 test and documentation suites; 668 passed; 0 failed; 9 ignored

cargo clippy --workspace -- -D warnings > /private/tmp/mote-inferred-final-clippy.log 2>&1
exit 0
Finished dev profile in 13.85 seconds
```

The nine ignored tests require explicitly enabled live or native boundaries.
The native inference test was separately enabled and passed above.
The exact final outputs are included as [workspace.log](artifacts/ipad-inferred-2026-10-11/workspace.log) and [clippy.log](artifacts/ipad-inferred-2026-10-11/clippy.log).
Every IPAD-04 Verify clause now has evidence.
The README acceptance checkbox is checked.

## Cleanup hardening review

Independent review found that the original harness restored displaced Simulator Documents files
only after all assertions. A failure could leave fixture state in the Simulator.
The cleanup guard is installed before the first rename and restores the prior files on unwind.
The focused `native_state_guard_restores_prior_files_after_failure` and
`native_state_guard_preserves_backup_when_restore_fails` tests pass.
It also runs temporary credential deletion on failure and preserves its backup if restoration fails.
The second complete native journey passed with the hardened harness in the same iOS 27 iPad Pro
11-inch Simulator. It took 111.94 seconds and again recorded four turns, 14 model calls, 14 ledger
rows, 14 packs, one visible terminal diagnostic and three typed refusals.
Its `credential-deletion.json` records `keychain_deleted` with `missing: true`.
The first attempt at this rerun failed before pairing because the restricted shell could not
contact CoreSimulator. An unrestricted local Simulator-service check found the booted device, and
the identical test binary then passed with service access. No application code changed between
those attempts.
