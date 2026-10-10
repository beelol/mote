# Matrix host vertical slice

## Goal

Make a small Matrix dashboard a real Mote surface. The daemon owns the binding manifest and current
values. It parses, resolves and lowers Matrix source, commits only a valid compact v0 tree, and
accepts typed changes from the native immediate-mode controls.

## Scope

This slice exposes Mote's existing configuration as trusted Matrix bindings:

- `models.local_only` is a boolean field.
- `models.talk`, `models.router` and `models.think` are model enum fields.
- `model_options` is the fixed list of the three local models allowed by the v0 contract.
- `pipeline.stages` is a list of current stage summaries.
- `refresh` is a zero-argument host action.

The public `matrix.render` request accepts a thread and source text. It never accepts a manifest or
snapshot from the caller. The daemon builds both from its own configuration and pipeline state.

## Commit behavior

The daemon parses, resolves, lowers and validates before appending `ui.surface`. A failure returns
structured diagnostics and leaves the last valid surface unchanged. A successful source is retained
for its thread so a supported field change or refresh can recompile it from fresh host state.

## Events

The client sends the v0 event envelope `{"event": name, "args": {...}}`. Actions use their Matrix
action name. A control edit uses `change` with `field` and `value` arguments. The daemon accepts a
change only for a field in its trusted Matrix manifest, applies it through `ConfigStore`, and then
recompiles the retained source. Unknown fields and wrong value types fail closed.

## Boundary

This is the first useful host slice, not the complete Matrix runtime. Candidate streaming, full
catalog rendering, general action contracts, form submission, surface history and model retry are
still governed by the remaining Matrix host criteria.

