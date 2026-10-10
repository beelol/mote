# Surface accessibility implementation plan

## Goal

Expose the current v0 surface consumer as a stable, named AccessKit role tree through the production
egui renderer. Verify all ten fixtures and the gallery at both approved viewports. Keep UI-22 open
until the unlocked native macOS accessibility tree is captured.

## Task 1: Observe the production AccessKit tree

**Files:**
- Modify: `Cargo.toml`
- Modify: `mote-client/Cargo.toml`
- Modify: `mote-client/src/observation.rs`
- Modify: `mote-client/src/app.rs`
- Create: `mote-client/src/accessibility_tests.rs`

- [ ] **Step 1: Capture the real egui update**

Enable AccessKit on the observation context, render through `MoteApp::draw_shell`, and return
`FullOutput.platform_output.accesskit_update`. Build an `accesskit_consumer::Tree` and traverse it
with `common_filter`, matching the filter used by the macOS adapter.

- [ ] **Step 2: Add role-tree goldens**

Create nested JSON goldens for all ten fixtures and the gallery. Omit unstable ids, bounds and focus.
Run every case at 800 by 480 and 1280 by 800. Require identical filtered trees across viewports and
22 exact comparisons. Exact equality is the duplicate semantic-node assertion.

## Task 2: Add current-v0 semantics to the production renderer

**Files:**
- Create: `mote-client/src/accessibility.rs`
- Modify: `mote-client/src/lib.rs`
- Modify: `mote-client/src/surface.rs`

- [ ] **Step 1: Add stable semantic helpers**

Use egui AccessKit node builders and accessibility parents. Derive ids from the existing surface
namespace, structural path and fixed semantic salts. Do not introduce a parallel retained tree.

- [ ] **Step 2: Assign roles, names and values**

Use Label for text; List/ListItem; Table/Row/ColumnHeader/Cell; TextInput with field label and value;
Switch with toggle label and state; ComboBox with select label and value; Button with action label;
ProgressIndicator with percentage name and numeric value; Log/ListItem for chat. Generic Stack and
Row containers may be filtered by the adapter.

- [ ] **Step 3: Remove duplicate visual semantics**

Keep Field and Select labels visually present but hidden from AccessKit after naming the control.
Hide the visual progress percentage after exposing it through the progress node.

Do not add roles for unsupported catalog nodes, alt text, live regions or overlays.

## Task 3: Record partial UI-22 evidence

**Files:**
- Create: eleven `golden/roles/*.json` files
- Create: `docs/verification/UI-22.md`
- Modify: `README.md`
- Replace: `docs/HANDOFF.md`

Record egui, AccessKit, Rust and macOS versions, exact commands and 22 production-shell comparisons.
Keep UI-22 open. The remaining verification is an unlocked native Mac dump through the eframe/winit
adapter. Later alt text, live status and overlay focus remain consumer-contract gated.

## Verification

Run:

```text
cargo test -p mote-client accessibility_ -- --nocapture
cargo test -p mote-client
cargo test --workspace
cargo clippy --workspace -- -D warnings
git diff --check
```

Commit: `fix(client): expose surface accessibility roles`
