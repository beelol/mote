# matrix closeout goal

This goal drives the whole UI plan to a state the owner can approve. It is broader than
[GOAL.md](GOAL.md), which only iterates on design coverage. Copy the block below into a driver. It is
self contained and does not assume chat history. The end state is `UI-00` ready for an `approve` or a
short list of explicit changes.

```text
Close out the whole mote UI plan, from repository state at ~/projects/mote, without chat history.

Read AGENTS.md first. Then read matrix/README.md, matrix/recipes/README.md, matrix/catalog.md,
matrix/layout-and-theme.md, matrix/bindings-and-state.md, matrix/runtime.md, matrix/examples.md,
and matrix/decisions.md. Read docs/handoff/ui-language/README.md for the original scope, and
docs/backlog.md for the UI-00 through UI-48 block. Read docs/rfcs/renderer.md and
docs/research/renderer-research.md if they exist.

What matrix is. matrix is the text syntax a model emits to draw an interface; the text is the
language. The compact JSON tree in crates/mote-ui is a temporary intermediate and compile target,
not the language, and may change. The runtime is inferred and animated UI on an immediate mode GUI
that redraws each frame, built in Rust for speed: Taffy for flexbox layout, egui for the shell,
wgpu for the GPU draw, rayon for parallel per node measurement, AccessKit for roles. The name is
matrix, never mote, moted, .mote, or matter. matrix/decisions.md holds the settled decisions and
the open questions.

The outcome. When this goal is done, the owner can read matrix/decisions.md and either answer
approve or name a small set of explicit changes, without having to resolve any unstated syntax,
state, rendering, validation, or scope decision. Do not close UI-00; only the owner does.

Do the work in this order, committing and pushing after each step.

1. Finish design coverage. Every common web and app surface an agent would generate has a recipe or
   a worked example in matrix source: the conformance suite, pull requests, settings, dashboard,
   calendar as an agenda, file browser, notification inbox, search results, profile, kanban board,
   multi step wizard, login, media gallery, and a comments thread. Each starts from the question a
   person asks and writes only the slice that answers it. Every catalog element has a recipe and
   appears in at least one example. If a surface needs a word the catalog lacks, record one proposed
   catalog change in matrix/decisions.md with its reason; never silently add a word.

2. Resolve the open questions. Work through every item under open questions in matrix/decisions.md.
   Give each a decision or a recommended default with a reason. Freeze the exact matrix grammar
   against the conformance suite: the declaration shape, the binding notation, the event and
   selection notation, and whether a surface declares its row types or the host publishes them. Move
   anything that still needs the owner into a short, explicit list the owner can answer quickly.

3. Reconcile with the renderer. Make matrix/runtime.md consistent with docs/rfcs/renderer.md if it
   exists: the display list boundary, node identity and keys, the animation hints the runtime needs,
   and the Mac and Pi target constraints. If the renderer RFC does not exist yet, state the
   assumptions matrix depends on and flag them for the renderer work. Do not name a renderer that
   contradicts the Rust immediate mode decision without recording why.

4. Rebuild the acceptance gate. Rewrite the UI-00 through UI-48 block in docs/backlog.md so it
   matches the matrix architecture: text syntax is the language, the tree is a temporary compile
   target, the runtime is immediate mode Rust, the catalog is closed, bindings are typed, streaming
   fails closed. Keep stable ids where the meaning survives, and record every replaced or removed
   criterion in matrix/decisions.md. Each criterion is one sentence plus a complete Verify clause.
   Group later implementation into independently verifiable sets: grammar and lowering to the tree,
   typed bindings and event contracts, streaming parser and diagnostics, runtime conformance,
   theme, and model validity and token measurements. Do not move any criterion into the v0 list. Do
   not check UI-00.

5. Prepare the implementation handoff. Write docs/handoff/ui-language/implementation.md for work
   that starts only after the owner approves UI-00. Name the criteria to move into the active build
   list, their order, the owned files, the test fixtures, the commands, and the stop conditions. It
   must not claim approval has happened.

6. Present the decision summary. Update matrix/decisions.md so its decisions and open questions are
   the exact set the owner confirms to approve UI-00. Keep it short enough to read in one sitting.

Measurement, only when the owner asks. Build a throwaway probe under spikes/ui-language that
tokenizes the conformance suite in matrix source against the temporary JSON tree, using the
tokenizer mote already uses. Report token counts and first try validity. Do not pull a model. Do
not keep the probe as an implementation. Do not build a compiler until the owner approves UI-00.

Boundaries that do not move. One theme per surface, no per element style, no pixels. Three bindings
only: Value, Field, Action. Whole surface replacement; the host diffs; the model never patches.
Fail closed on anything unknown. No expressions, control flow, scripts, or timers in matrix. The
catalog is closed; a model never extends it.

Do not touch these. docs/rfcs/v0.md, the v0 list in README.md, the v0 fixtures and schema under
crates/mote-ui, and any production code. Do not collide with matter (Next Gen Lang, a separate
repository) or the mote and moted product names. Keep matrix design inside matrix/, the acceptance
gate in docs/backlog.md, the implementation handoff in docs/handoff/ui-language/, and a short
pointer in docs/rfcs/ui-language.md.

Keep the docs super readable, per AGENTS.md: plain declarative sentences, one idea per sentence, no
em dashes, no emojis. Commit and push after each step. Use Conventional Commits, imperative
subject, 72 characters, no AI attribution trailers. Concurrent sessions run on this repo and
sometimes commit with git add -A; stage and commit your work promptly and verify it landed. If
history is contended, work in a git worktree and open a PR.

Stop when the owner can approve UI-00 from matrix/decisions.md with no unstated decision left, when
the owner stops you, or when every remaining item needs an owner decision. Say which, and list the
decisions the owner still owes.
```

## What closing out means here

The UI plan closes when four things are true. The design covers the surfaces an agent will generate.
The open questions are decided or carry a recommended default. The acceptance gate in the backlog
matches the matrix architecture. The owner has one short page to approve. Implementation does not
start until the owner closes `UI-00`. This goal reaches that state and no further.
