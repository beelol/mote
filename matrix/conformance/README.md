# matrix conformance fixtures

The ten design cases are the first ten cases in [../examples.md](../examples.md).
Their source is measured directly from that document.
Case 10 is an intentional scope rejection.
The toolchain tests parse and resolve all ten cases.
Cases 1 through 9 resolve against their host snapshots.
Case 6 lowers to the unchanged list fixture.
The other positive cases reject unsupported v0 semantics.

The `v0/` fragments document exact resolved snapshots of nine existing JSON fixtures.
They use the lowering subset in [../runtime.md](../runtime.md).
Their matching consumer JSON files stay unchanged in `crates/mote-ui/fixtures/`.
The compact `v0/*.json` snapshots pin canonical lowering output.
Tests compare each snapshot with both the compiler output and the unchanged consumer fixture.
The host fixture manifest supplies the data below.
Source measurements exclude that manifest in the same way ordinary Matrix source excludes host contracts.

| Matrix file | Matching JSON | Host manifest snapshot |
|---|---|---|
| `v0/01_text.matrix` | `01_text.json` | No binding. |
| `v0/02_stack.matrix` | `02_stack.json` | No binding. |
| `v0/03_row.matrix` | `03_row.json` | No binding. |
| `v0/04_list.matrix` | `04_list.json` | `fixture.items: Value<List<{value: str}>>` contains first, second and third. |
| `v0/05_table.matrix` | `05_table.json` | `fixture.models: Value<List<{name: str, value: str}>>` contains talk/qwen3:4b and router/qwen3:0.6b. |
| `v0/06_field.matrix` | `06_field.json` | `display_name: Field<str>` starts at owner. |
| `v0/07_toggle.matrix` | `07_toggle.json` | `local_only: Field<bool>` starts at true. |
| `v0/08_select.matrix` | `08_select.json` | `models.talk: Field<Model>` starts at qwen3:4b and accepts both listed model names. |
| `v0/09_action.matrix` | `09_action.json` | `approve: Action` has no arguments. |

Design case 6 resolves to `04_list.json` when `mail.unread` contains three records with subjects
first, second and third.
That comparison covers a loaded snapshot only.
The v0 JSON does not encode a live binding or its loading transitions.
Design cases 1 through 5 and 7 through 9 need semantics absent from the v0 schema.
Case 10 has no valid lowered JSON.
The existing `10_chat_progress.json` has arbitrary chat children without message roles.
It has no exact source equivalent under the Matrix chat signature.
Determinate unlabeled progress has a separate lowering test.

The negative cases in `diagnostics.json` pin every diagnostic field for lexer, parser, resolver
and unsupported-lowering failures.
The v0 consumer's rejected-JSON text node remains unchanged.

Run the measurement command in [../measurement.md](../measurement.md).
The command measures source and compact JSON rather than validating Matrix grammar or lowering.
