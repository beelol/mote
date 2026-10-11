# IPAD-04 verification

Date: 2026-10-11.
Implementation commit: `6c8d6ac923436761325648e8b9a0439a725a18bd`.
Status: The live model validity gate passes. The criterion remains open pending Simulator captures.

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

## Remaining verification

The Simulator must still capture success, one-retry recovery, stable prior surface and final prose fallback through the paired client.
This file does not claim those UI captures.
IPAD-04 remains unchecked until that remaining Verify clause is covered.
