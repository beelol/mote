# matrix closeout goal

This goal drives the whole UI plan to a state the owner can approve. It is broader than
[GOAL.md](GOAL.md), which only iterates on design coverage. Copy the block below into a driver or
set it with `/goal`. It is under 4000 characters, self contained, and does not assume chat history.
The end state is `UI-00` ready for an `approve` or a short list of explicit changes.

```text
Close out the mote UI plan, from ~/projects/mote, no chat history.

Read AGENTS.md, then matrix/ (README, catalog, layout-and-theme, bindings-and-state, runtime, decisions), docs/backlog.md (UI-00..UI-48), and docs/rfcs/renderer.md if present.

matrix is the text syntax a model emits; the text is the language. The JSON tree in crates/mote-ui is a temporary compile target. Runtime: inferred, animated, immediate mode, Rust (Taffy, egui, wgpu, rayon, AccessKit), a consumer process; moted stays headless and streams the validated tree over IPC. Targets Mac, Pi, iPad. Name is matrix, never mote, moted, .mote, matter.

Outcome: the owner reads matrix/decisions.md and answers approve or names a few explicit changes, with no unstated decision left. Do not close UI-00; only the owner does.

Do in order, commit and push after each:
1. Finish design coverage: a recipe or worked example in matrix source for every common surface (pull requests, settings, dashboard, calendar as agenda, file browser, notifications, search, profile, kanban, wizard, login, gallery, comments). Start from the question; write only the slice that answers it. Every catalog element has a recipe and an example. A missing word is one proposed catalog change in decisions.md with a reason, never silent.
2. Materialize the default theme at matrix/themes/vector.md: Vector, thin retro red Atari vector line. Dark base, one red accent, thin strokes not fills, sharp corners, no shadows, mono type scale, tight spacing, six status tones as thin colored strokes, one materialize line on reveal motion with one duration and curve. Render every element group. No new matrix syntax.
3. Resolve every open question in decisions.md with a decision or recommended default, and freeze the grammar against the conformance suite (declaration shape, binding notation, event and selection notation, row-type ownership). Leave only a short owner list.
4. Reconcile with the renderer per matrix/runtime.md: consumer process, one Rust runtime for Mac, Pi, iPad, daemon streams the tree; the tree is the consumer contract so a native SwiftUI consumer stays possible on iOS without changing the language. Build the first milestone, a minimal runtime that draws one validated tree from the stream, then ping the daemon session named Mote handoff priorities to swap it in.
5. Rebuild UI-00..UI-48 in docs/backlog.md to match this architecture. Keep stable ids where meaning survives; record replaced criteria in decisions.md. One sentence plus a full Verify clause each. Group later work: grammar and lowering, bindings and events, streaming and diagnostics, runtime conformance, theme, model validity and tokens. Do not move any into v0. Do not check UI-00.
6. Write docs/handoff/ui-language/implementation.md for post approval work: criteria to activate, order, owned files, fixtures, commands, stop conditions. It must not claim approval.
7. Update decisions.md so decisions and open questions are the exact set to approve UI-00, short enough to read once.

Do not pull a model or build a compiler until UI-00.

Locked: one theme per surface, no per element style, no pixels. Bindings only Value, Field, Action. Whole surface replacement; host diffs; model never patches. Fail closed. No expressions, control flow, scripts, timers. Catalog closed. Layout strict: col, row, header, footer, fill, align roles, auto scroll; no grid tracks, absolute, or z-index; new primitives go to the owner.

Do not touch docs/rfcs/v0.md, the v0 list in README.md, the v0 fixtures and schema in crates/mote-ui, or production code. Keep matrix design in matrix/. Readable prose per AGENTS.md. Conventional Commits, no AI attribution. Concurrent sessions may commit with git add -A; commit promptly and verify; if contended, use a worktree and PR.

Stop when the owner can approve UI-00 with no unstated decision, when the owner stops you, or when all remaining items need the owner. Say which, and list the decisions still owed.
```

## What closing out means here

The UI plan closes when four things are true. The design covers the surfaces an agent will generate.
The open questions are decided or carry a recommended default. The acceptance gate in the backlog
matches the matrix architecture. The owner has one short page to approve. Implementation does not
start until the owner closes `UI-00`. This goal reaches that state and no further.
