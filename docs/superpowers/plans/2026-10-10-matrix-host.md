# Matrix host vertical slice plan

1. Correct resolver budget accounting for fields under empty nested templates and preserve it with
   an acceptance regression.
2. Teach the native renderer to emit typed action and control-change envelopes.
3. Add a daemon-owned Matrix manifest and snapshot built from Mote configuration and pipeline state.
4. Add atomic `matrix.render` compilation and structured diagnostics without replacing the last
   valid surface on failure.
5. Route trusted `change` and `refresh` events, recompile retained sources, and cover the full flow
   with daemon dispatch tests.
6. Add the runnable example, verification evidence and handoff state. Run workspace tests and
   clippy, request independent review, then commit each verified change.
