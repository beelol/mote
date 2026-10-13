# IPAD-13: Agent layout transition

Date: 2026-10-13, America/Los_Angeles.

Environment: Apple Silicon Mac, Rust 1.89.0, egui 0.31.0. The focused check uses the production Matrix renderer with a headless egui context. It is not an iPad Simulator capture.

The live iPad surface used the surface revision in its UI identity. A replacement revision therefore started a new transition cache and could not compare its strokes with the prior revision. The renderer now accepts a stable transition identity while keeping revision-specific control identity. The live view keys that transition by host and thread.

The renderer captures header and footer rules and slider rails as line strokes. A focused regression draws two revisions with different UI identities and one shared transition identity. It changes the header height, which moves the header rule and the slider rail.

```text
cargo test -p mote-client-core live_surface_revision_materializes_rules_with_new_control_identity
test transition::tests::live_surface_revision_materializes_rules_with_new_control_identity ... ok
test result: ok. 1 passed; 0 failed
```

This proves square particles appear on the changed revision, remain at 60 ms and finish at 120 ms in the production renderer. Existing transition tests cover unchanged strokes, first-frame hit targets and reduced-motion behavior. The live paired Simulator capture and first-frame control journey in the Verify clause remain open. Do not mark IPAD-13 complete from this headless evidence.

The full workspace checks passed after this change:

```text
cargo test --workspace
exit 0
cargo clippy --workspace -- -D warnings
exit 0
```

The first sandboxed workspace test run failed because a browser fixture could not bind a localhost listener. The same command passed with local socket access outside the restricted sandbox. This was an environment permission failure, not a test assertion failure.
