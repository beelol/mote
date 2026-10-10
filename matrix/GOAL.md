# matrix goal

Copy the block below into a driver to keep iterating on matrix. It is self contained and does not
assume chat history. The block is under 4000 characters.

```text
Iterate on matrix, the high level GUI language for agents, from repository state at
~/projects/mote, without chat history.

Read matrix/README.md, then matrix/recipes/README.md, matrix/catalog.md,
matrix/layout-and-theme.md, matrix/bindings-and-state.md, matrix/runtime.md,
matrix/examples.md, and matrix/decisions.md. Read AGENTS.md for the rules. Treat
matrix/decisions.md as the settled decisions and open questions.

What matrix is. matrix is the text syntax a model emits to draw an interface. The text is the
language. The compact JSON tree in crates/mote-ui is a temporary intermediate and compile target,
not the language, and may change. The runtime is inferred and animated UI on an immediate mode GUI
that redraws each frame; animation lives in the runtime, never in matrix source. The fastest
interpreter is Rust: Taffy for flexbox layout, egui for the shell, wgpu for the GPU draw, rayon for
parallel per node measurement, AccessKit for roles. The name is matrix, never mote, moted, .mote,
or matter.

The job. Make matrix cover every surface an agent needs to turn an intent into an interface, so a
person can convert common web interfaces into matrix. Work from real questions, not APIs. For each
surface: start from the question, pick the smallest slice of data and actions it needs, write it in
matrix, and confirm the catalog has the words. If not, propose one catalog change in
matrix/decisions.md with its reason. Never silently add a word.

Keep the docs super readable. One concept per recipe in matrix/recipes/, each with the four parts:
Make it, Slice, Free, Animates. Plain declarative sentences, one idea per sentence, no em dashes, no
emojis, per AGENTS.md. Every catalog element has a recipe and appears in at least one example.
Prefer adding a worked real world surface over adding prose.

Surfaces to work next, each as a recipe or example with its data slice: calendar day and week, file
browser, notification inbox, search results, profile page, kanban board, metrics dashboard from
stat and table, multi step wizard with steps, login and settings flows, media gallery with grid and
tile, and a thread of comments. Write only the slice the question needs.

Boundaries that do not move. One theme per surface, no per element style, no pixels. Three bindings
only: Value, Field, Action. Whole surface replacement; the host diffs; the model never patches.
Fail closed on anything unknown. No expressions, control flow, scripts, or timers in matrix. The
catalog is closed; a model never extends it.

Do not touch these. docs/rfcs/v0.md, the v0 list in README.md, the v0 fixtures and schema under
crates/mote-ui, and any production code. Do not collide with matter (Next Gen Lang, a separate
repository) or the mote and moted product names. Keep all matrix work inside matrix/, except the
short pointer already in docs/rfcs/ui-language.md.

When a decision needs the owner, record it in matrix/decisions.md under open questions with a
recommended answer, and keep going on the rest. Do not close UI-00; only the owner does.

Measurement, only when the owner asks. Build a throwaway probe under spikes/ui-language that
tokenizes the conformance suite in matrix source against the temporary JSON tree, using the
tokenizer mote already uses. Report token counts and first try validity. Do not pull a model. Do
not keep the probe as an implementation. Do not build a compiler until the owner approves UI-00.

Commits. Do not commit unless the owner asks. If asked: Conventional Commits, imperative subject, 72
characters, no AI attribution trailers, small verified changes on main.

Stop when the surfaces above are worked and the catalog covers them, when the owner stops you, or
when every remaining item needs an owner decision. Say which.
```

## Why this goal is shaped this way

The goal keeps matrix a design effort until the owner approves `UI-00`. It drives coverage through
real questions, because the point of matrix is inferred UI from intent. It fixes the recipe shape so
the docs stay readable. It records the runtime as an immediate mode GUI built in Rust for speed. It
protects the v0 contract and the product naming, and defers any measurement or compiler work to an
explicit owner go ahead.
