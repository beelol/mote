# matrix

matrix is a high level GUI language for agents. A model writes a short matrix program. A runtime
reads it and draws a working interface. The point of matrix is that a model spends almost no tokens
and almost no attention describing a screen, because matrix infers everything a normal interface
needs.

This folder is the design home for matrix. It is isolated on purpose. It does not depend on the
rest of mote, and it does not decide its own interpreter technology yet.

## What matrix is, and what it is not

matrix is the text syntax. That text is the language. An agent emits matrix source, not JSON.

The compact JSON surface tree that already exists in `crates/mote-ui` is a temporary thing. It is a
compile target and an intermediate form, not the language. matrix lowers to a tree today because a
validated tree is easy to render and easy to check. The tree may change or be replaced. Nothing in
matrix's design treats the current tree shape as permanent. When this folder says "the tree," it
means that temporary intermediate, never the language.

So the layers are:

1. matrix source. The durable language. What an agent writes. Covered by this folder.
2. A typed intermediate tree. Temporary. What a runtime renders. Today this is the v0 JSON tree.
3. A runtime that reads the tree and runs the underlying behavior. It is an immediate mode GUI that
   redraws every frame, inferred and animated. The fastest technology for this is Rust, with Taffy
   for flexbox layout, egui for the shell, wgpu for the GPU draw, and rayon for parallel per node
   measurement. See [runtime.md](runtime.md).

## Naming

The language is `matrix`. It is not `mote`, `moted`, or `.mote`. It is not `matter`, which is the
working name of Next Gen Lang in a separate repository. Keep these three names apart in all prose
and all file names. matrix borrows syntax doctrine from matter, but it is its own small language
with no expressions and no control flow.

## Design goals

1. Minimal tokens. A screen a human would sketch in a sentence should be a few lines of matrix.
2. Inferred interface. Scrolling, focus, keyboard, touch targets, loading, empty, error, light and
   dark theme, and accessibility are implicit. A model cannot forget them and cannot break them.
3. Closed and complete. The element catalog is a short fixed list. It is chosen to cover the common
   web and app surfaces an agent needs to generate, not to be extensible by the model.
4. Standard by default. Heights, spacing, and padding are standard values from one theme. The model
   never writes a pixel. A theme may override the standard values. The model never picks a theme.
5. Typed meaning. Every data reference, editable value, and action is typed and checked before the
   interface renders. A reference that does not type check never reaches a screen.

## Why inferred UI needs this

The target use is converting an intent into a working interface with no hand layout. A person asks
"what are my pull requests," and an agent answers with a real list it can act on. The agent should
not reason about padding, scroll containers, or focus rings. It should name the data, name the
shape, and name the actions. matrix is the vocabulary for exactly that and nothing more. See
[examples.md](examples.md) for the pull request case worked end to end.

## Read in this order

1. [recipes/](recipes/README.md). One concept per file, each a short snippet of how you would make
   that thing, with what the runtime gives you for free and what it animates. Start here to build.
2. [catalog.md](catalog.md). Every element, grouped, with the slice of data and bindings each one
   needs. The whole closed catalog in one place.
3. [layout-and-theme.md](layout-and-theme.md). The layout vocabulary, automatic scrolling, standard
   heights and padding, and the one default theme with its override rules.
4. [bindings-and-state.md](bindings-and-state.md). The three typed bindings, who owns which state,
   how events travel, and how a streamed surface renders.
5. [runtime.md](runtime.md). The immediate mode GUI model, why affordances are free and animated,
   and the fastest technology to build the interpreter.
6. [examples.md](examples.md). A conformance suite and real world surfaces, each shown as matrix
   source. Includes the GitHub pull request surface.
7. [decisions.md](decisions.md). The decisions an owner must confirm, and where this design departs
   from the earlier UI language RFC.
8. [GOAL.md](GOAL.md). A resumable goal for continuing to iterate on matrix design coverage.
9. [CLOSEOUT.md](CLOSEOUT.md). A single goal that drives the whole UI plan to a state the owner can
   approve, through `UI-00`.

## Status

Design only. Date 2026-10-09. No interpreter exists. No matrix parser exists. The catalog, the
bindings, and the examples are proposals open to owner revision. Nothing here changes the v0 build
contract or the v0 surface tree. The formal acceptance gate is `UI-00` in `docs/backlog.md`, which
only the owner closes.
