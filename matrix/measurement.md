# matrix grammar measurement

Date 2026-10-10.
Environment is macOS on the shared Mote workspace.
The measurement uses the repository's approved `tiktoken-rs` 0.6 `o200k_base` tokenizer.
The cached local Rust libraries were already built.
Rust is 1.89.0 and Cargo is 1.89.0.
The host is macOS 26.6.2 (25G83), arm64.
The shell wrapper uses the installed `jq` 1.7.1 to select Cargo artifacts.
No dependency was installed and no model was pulled or called.

## Command and method

Run from the repository root.

```sh
sh matrix/tools/measure.sh
sh matrix/tools/measure.sh --check
```

The command compiles a measurement utility in a fresh temporary directory.
It first runs an offline, locked workspace build.
It selects matching library paths from Cargo's artifact report.
This avoids mixing cached libraries from different feature sets.
Check mode compares the complete stdout TSV with `matrix/measurement-output.tsv`.
Any missing row or changed count fails the command.
It reads the first source fence of each numbered case in `matrix/examples.md`.
It also reads the nine `.matrix` fragments in `matrix/conformance/v0/`.
It reads their matching unchanged `.json` files in `crates/mote-ui/fixtures/`.
JSON is parsed and serialized without indentation or spaces.
Matrix source keeps its authoring whitespace and final newline.
The design and v0 fragment counts include neither Markdown fences nor host manifests.
Tokens use `encode_ordinary` and add no prompt or special tokens.
Counts are independent of local Ollama model tokenization.
They are comparable to the existing ledger estimator rather than claimed model input counts.

The fragment comparisons measure source and existing consumer snapshots.
The separate dashboard measurement compiles and validates the current trusted host contract.
The fixture manifest and lowering rules document the exact snapshot comparisons.
No first try validity rate can be measured before a parser exists.

## Design conformance suite

Source file is `matrix/examples.md` for every row.

| Case | Matrix bytes | Matrix tokens | Compact JSON bytes | JSON tokens |
|---|---|---|---|---|
| 1. Static text | 106 | 29 | No exact v0 equivalent | No equivalent |
| 2. Wrapping tags | 77 | 24 | No exact v0 equivalent | No equivalent |
| 3. Typed controls | 196 | 67 | No exact v0 equivalent | No equivalent |
| 4. Form action | 207 | 58 | No exact v0 equivalent | No equivalent |
| 5. List and empty override | 100 | 28 | No exact v0 equivalent | No equivalent |
| 6. Loaded list snapshot | 51 | 14 | 125 | 33 |
| 7. Chat card | 131 | 33 | No exact v0 equivalent | No equivalent |
| 8. Keyed reorder | 56 | 15 | No exact v0 equivalent | No equivalent |
| 9. Anchors | 217 | 56 | No exact v0 equivalent | No equivalent |
| 10. Invalid selection | 41 | 12 | Intentionally invalid | No equivalent |
| Total source | 1182 | 336 | Partial comparison only | Partial comparison only |

Case 6 compares to `crates/mote-ui/fixtures/04_list.json`.
Its host snapshot is documented in [conformance/README.md](conformance/README.md).
The comparison covers loaded data and does not prove loading, pending or failure semantics.
The remaining positive cases use meanings that the v0 schema cannot preserve.
Adding invented JSON properties or discarding those meanings would give a misleading comparison.

## Exact v0 snapshot fragments

Each Matrix file below lives in `matrix/conformance/v0/`.
Its JSON file has the same stem under `crates/mote-ui/fixtures/`.

| File stem | Matrix bytes | Matrix tokens | Compact JSON bytes | JSON tokens |
|---|---|---|---|---|
| `01_text` | 23 | 6 | 41 | 11 |
| `02_stack` | 44 | 15 | 99 | 27 |
| `03_row` | 37 | 13 | 90 | 25 |
| `04_list` | 51 | 13 | 125 | 33 |
| `05_table` | 76 | 22 | 96 | 37 |
| `06_field` | 26 | 7 | 69 | 18 |
| `07_toggle` | 31 | 8 | 71 | 19 |
| `08_select` | 67 | 27 | 114 | 44 |
| `09_action` | 28 | 7 | 53 | 13 |
| Total | 383 | 118 | 758 | 227 |

The comparable fragments use 49.5 percent fewer bytes and 48.0 percent fewer estimator tokens.
The result excludes host contracts and data snapshots from the Matrix side.
It measures authoring syntax rather than total transport or storage cost.
The original v0 chat and progress fixture lacks Matrix message roles and has no exact source equivalent.
The full catalog requires a future owner approved consumer contract revision.

## Trusted dashboard host and transport

The input source is `matrix/examples/mote-dashboard.matrix` including its final newline.
The utility imports the production host binding and pipeline modules from `moted/src/`.
It builds an empty pipeline snapshot from in-memory event and context stores.
It passes the nine exact v0 config defaults to the production snapshot builder.
The four model defaults are `models.local_only=true`, `models.talk="qwen3:4b"`,
`models.router="qwen3:0.6b"` and `models.think="qwen3-coder:30b-64k"`.
The remaining defaults are `budgets.talk_first_token=300`, `budgets.router=150`,
`budgets.think_first_token=3000`, `packs.talk=600` and `packs.think=8000`.
Pipeline numbers, events and requests are empty.
The generated time is zero.
The dashboard uses the first frozen versioned consumer slice.
Its three selects have the three approved model options.
Its toggle is true and its action is `refresh`.
The production compiler prepares one dual payload with the exact inert v0 fallback and contract 1.
The strict versioned envelope validator validates both representations before measurement.
The consumer JSON row measures the complete `matrix_surface` object.

| Row | Encoding | Bytes | Tokens |
|---|---|---|---|
| `dashboard_source` | Matrix source | 463 | 110 |
| `dashboard_consumer_json` | Compact consumer JSON | 935 | 299 |
| `host_manifest` | Canonical measurement JSON | 1367 | 374 |
| `binding_snapshot` | Canonical measurement JSON | 313 | 114 |
| `matrix_render_request` | IPC JSON line | 572 | 145 |
| `matrix_render_response` | IPC JSON line | 43 | 15 |
| `ui_surface_event` | IPC JSON line | 1138 | 349 |

The manifest and snapshot are in-memory Rust structures without a production JSON serializer.
Their rows are deterministic measurement encodings.
They are neither transmitted bytes nor Rust memory sizes.
The encoding retains every manifest field: `version`, `records`, `enums`, `values`, `fields` and `actions`.
Records encode their `fields`; enums encode `members` and `labels`.
Bindings encode `ty` and `display`; mutable fields also encode `initial`.
Actions encode `parameters`.
Types use objects with a `kind` string.
Enum and record types add `name`; list and option types add `item`.
Display formats use a `kind` string and any `symbol` or `decimals` values.
The snapshot encodes `manifest_version`, `values`, `fields`, `row_fields` and `selections`.
Row fields and selections are empty objects for this sample.
A nonempty row field or selection map rejects instead of being omitted.
Both encodings use compact JSON with lexicographically sorted object keys.
Array order is preserved.
Neither encoding adds a newline.
Changing this declared encoding changes its measurement cost.

The IPC rows call the production `mote-ipc` request, success and event envelope builders.
They call the same `encode_frame` function as the socket writer.
The client and server use these builders for real traffic.
The daemon and measurement call the same Matrix success result and surface payload builders.
Each row includes one terminating newline, as required by its framing.
They are deterministic samples encoded by the production wire contract, not a live traffic capture.
Request id is `1` and thread is `main`.
The request is `{"id":1,"method":"matrix.render","params":{"source":"...","thread":"main"}}`.
The source string is JSON escaped inside this request.
The reply is `{"id":1,"result":{"ok":true,"valid":true}}`.
The event is `{"event":{"kind":"ui.surface","payload":{"matrix_surface":...,"surface":...,"surface_id":"matrix"},"scope_id":"main","seq":1,"ts":0}}`.
The event payload passes through the production redacting event log before measurement.
Its first stored sequence is `1`.
Its timestamp is normalized to `0` for reproducibility.
These metadata values are declared samples, not live measurements.

Source crosses IPC in the render request.
The unary reply carries only the success and validity result.
The subscribed event carries the compiled tree and surface identity.
The host manifest and binding snapshot remain inside the daemon during this render.
The consumer JSON row is already contained in the event row and must not be added again.
One render request, reply and subscribed event total 1753 bytes for these sample values.
Their separately counted tokens sum to 509.
This sum is not a model prompt count or the tokenization of concatenated frames.
Subscriptions, later refreshes, config events, socket overhead and database storage are excluded.
Populated pipeline rows change the internal binding snapshot, not the emitted dashboard surface.
Real timestamps change the event cost.
The source savings above do not establish total transport savings.

Two checked runs produced byte-identical complete TSV output on 2026-10-10.
`cmp` exited zero.
The current SHA-256 is recorded in [UI-04](../docs/verification/UI-04.md).
The complete output is committed in [measurement-output.tsv](measurement-output.tsv).
The acceptance evidence is [UI-04](../docs/verification/UI-04.md).

The check is linked to production serialization rather than independent envelope replicas.
Adding a field to the production successful render result fails the checked response count.
Adding a field to the production surface payload fails the checked event count.
Changing each production outer envelope or its framing also fails the check.
All six temporary drift probes were restored after confirming these failures.
The restored production output proves the committed counts above without changing the fixture.
