# Handoff: Inferred surface validity gate

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `59bfedb`.
Requested subject: `docs(matrix): correct inferred surface evidence`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

Task 4 adds `MOTE_LIVE=1 moted bench surface`. The benchmark uses a fixed 200-prompt corpus with
SHA-256 `447a7a73f4669a6450bf88a9e1ab5bc03f914c2ab70d213f3d398e49980a5d2d`.
The corpus covers text, stack, row, list, table, field, toggle, select, action, determinate progress
and no-surface turns.

Every benchmark attempt uses the real Task 1 surface model boundary. Each attempt writes one pack,
one `surface` ledger row, one `model.request` event and one `model.response` event. The result line
reports strict envelope, Matrix parse, host resolution, v0 lowering, first-attempt and after-retry
counts and rates. Production activation requires at least 95 percent first-attempt validity.

The live benchmark used the already installed local `qwen3:4b`. No model was pulled. All 200
responses passed the strict envelope and chose `none`. The result was 20 of 200 valid cases, or
10 percent, both before and after retry. No response entered Matrix parsing and no retry was
eligible. The isolated database contains exactly 200 surface ledger rows and matching request and
response events.

The activation gate failed. Production `handle_turn` remains disconnected from automatic surface
inference. UI-24 remains open. UI-36 remains partial because retry input uses the last committed
canonical source instead of a valid prefix recovered from the rejected candidate. A second failure
preserves the prior surface and prose instead of rendering a recovered prefix with fallback prose.
UI-37 is verified only for the permitted local Ollama v0 path. UI-38 remains open because the owner
has not accepted the two-call composition as the final framing.

The live benchmark called the surface model boundary directly with canned completed prose. It
captured strict-envelope behavior only. It did not run after a completed live prose answer through
the internal orchestration and does not prove live prose-to-surface ordering.

The isolated live evidence remains at `/private/tmp/mote-surface-task4.EH1JJG`. The exact output is
copied into `docs/verification/UI-24.md`.

The focused benchmark suite passes 6 tests. The full workspace passes 373 tests. Workspace clippy
and `git diff --check` pass. Matrix measurements were not affected because this task does not
change the language, resolver, lowerer, consumer or transport representation.

No remote write or push occurred.
