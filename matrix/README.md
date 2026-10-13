# matrix

matrix is a high level GUI language for agents. A model writes a short matrix program. A runtime
reads it and draws a working interface. The point of matrix is that a model spends almost no tokens
and almost no attention describing a screen, because matrix infers everything a normal interface
needs.

This folder is the design home for matrix.
It keeps the language design separate from production code.
The interim consumer contract uses the existing v0 tree.
The recommended Rust runtime is described in [runtime.md](runtime.md).

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
   [grammar.md](grammar.md) freezes the exact declaration and binding forms.
8. [GOAL.md](GOAL.md). A resumable goal for continuing to iterate on matrix design coverage.
9. [CLOSEOUT.md](CLOSEOUT.md). A single goal that drives the whole UI plan to a state the owner can
   approve, through `UI-00`.

## Status

The grammar is frozen and owner-approved. `crates/matrix-lang` now lexes, parses, formats, resolves
and lowers the supported subset into the version 1 consumer tree and a validated v0 fallback. `moted` owns the first trusted
manifest and snapshot for configuration and pipeline data. `mote-client --matrix <path>` submits a
source file and displays the compiled surface through the native immediate-mode renderer.

The full catalog and general action contracts remain open. The Mac host stream and iPad consumer
exist for the version 1 slice. The root README is the completion record.

## One working program, from source to pixels

The checked-in [dashboard source](examples/mote-dashboard.matrix) is a runnable Matrix program:

```matrix
screen {
  header {
    row {
      text "Agent dashboard" fill
      button "Refresh" -> refresh
    }
  }
  col {
    col fill {
      toggle "Local only" models.local_only
      select "Talk model" models.talk options model_options
      select "Router model" models.router options model_options
      select "Think model" models.think options model_options
    }
    text "Changes apply on the next turn."
  }
  footer {
    text "Local model settings"
  }
}
```

`screen`, `header`, `row` and `col` describe structure. `fill` gives a child the remaining space on its
parent's main axis. `models.local_only` and `models.talk` are typed host-owned fields, not values
supplied by the model. `model_options` is a host-owned list. `refresh` is a host-declared action.
The agent does not specify pixel positions, colors, touch targets or arbitrary code.

The compiler resolves that source against the daemon's manifest and current snapshot. For
example, the toggle becomes this version 1 consumer node when `models.local_only` is true:

```json
{"type":"toggle","label":"Local only","field":"models.local_only","value":true}
```

The select becomes a node with a concrete `options` array and the current `value`. The complete
checked-in [version 1 JSON result](examples/mote-dashboard.v1.json) shows the entire translation.
It is an intermediate representation, not a second language that an agent must write. Its root
is a `screen` with nested `header`, `row`, `col`, `toggle`, `select`, `text` and `footer` nodes.
The daemon commits this validated tree in a `ui.surface` event. A version 1 client validates the
tree again and draws it through the Rust immediate-mode renderer. The renderer computes geometry,
scrolling, hit regions, focus and accessibility. It redraws from the current tree each frame.
Changed structural strokes materialize as red square pixels. The checked-in JSON records the
dashboard's example host values; a live compilation reads the current host values instead.

The pipeline in code is:

```text
Matrix source (.matrix)
  -> lexer and parser (crates/matrix-lang/src/lexer.rs, parser.rs)
  -> typed resolver against host manifest and snapshot (resolve.rs)
  -> version 1 lowering plus validated v0 fallback (lower_v1.rs, lower_v0.rs)
  -> daemon commit and ui.surface event (moted/src/matrix.rs, moted/src/main.rs)
  -> client validation and immediate-mode drawing (crates/mote-client-core/src/surface_v1.rs)
  -> typed ui.event back to the daemon, checked against the committed surface
```

For example, changing the Talk model select sends a `change` event with
`{"field":"models.talk","value":"qwen3:0.6b"}` and the control's structural path, contract
version and committed surface sequence. The daemon checks those against the exact committed
surface before accepting the change. See [consumer-contract-v1.md](consumer-contract-v1.md) for
the wire format and [grammar.md](grammar.md) for the syntax. The larger
[examples](examples.md) document contains proposed catalog forms too. Not every form in that
design document is implemented in version 1.

To try the actual dashboard, run these from the repository root in separate terminals:

```sh
cargo run -p moted
cargo run -p mote-client -- --matrix matrix/examples/mote-dashboard.matrix
```

For an offline tour of the rendered controls, run
`cargo run -p mote-client -- --gallery`. The gallery does not call the daemon or a model.
