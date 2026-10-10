# matrix grammar measurement

Date 2026-10-10.
Environment is macOS on the shared Mote workspace.
The measurement uses the repository's approved `tiktoken-rs` 0.6 `o200k_base` tokenizer.
The cached local Rust libraries were already built.
No dependency was installed and no model was pulled or called.

## Command and method

Run from the repository root.

```sh
sh matrix/tools/measure.sh
```

The command compiles a measurement utility in a fresh temporary directory.
It reads the first source fence of each numbered case in `matrix/examples.md`.
It also reads the nine `.matrix` fragments in `matrix/conformance/v0/`.
It reads their matching unchanged `.json` files in `crates/mote-ui/fixtures/`.
JSON is parsed and serialized without indentation or spaces.
Matrix source keeps its authoring whitespace and final newline.
Counts include neither Markdown fences nor host manifests.
Tokens use `encode_ordinary` and add no prompt or special tokens.
Counts are independent of local Ollama model tokenization.
They are comparable to the existing ledger estimator rather than claimed model input counts.

The tool measures source and snapshots.
It is not a Matrix parser, compiler or proof of semantic equivalence.
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
