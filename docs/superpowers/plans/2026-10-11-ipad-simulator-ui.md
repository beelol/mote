# iPad Simulator Rust UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver IPAD-01 through IPAD-03 as a Rust immediate-mode Mote app running in the iPad Simulator and connected through the paired localhost host transport.

**Architecture:** Move platform-neutral egui rendering and state into `mote-client-core`; keep the Mac eframe shell as one adapter and add a Rust static library plus minimal Objective-C entry point for iOS. Use eframe with WGPU and Metal because the current Glow backend cannot target iOS. The Simulator consumes the host abstractions from the host-foundation plan and never uses a Unix socket directly.

**Tech Stack:** Rust 2021, egui and eframe 0.31.0, winit 0.30.13, WGPU/Metal, Xcode 27, iOS Simulator SDK 27, Objective-C entry point.

**Spec:** `docs/superpowers/specs/2026-10-11-ipad-host.md`

## Global Constraints

- All product UI, layout, state and transitions remain in Rust immediate-mode code.
- The Objective-C shell only enters Rust and supplies Apple lifecycle metadata.
- Winit owns `UIApplicationMain`; do not start SwiftUI or a second UIKit application loop.
- Use a temporary Simulator-only identity such as `local.mote.simulator`; do not configure signing or create an App Store record.
- Simulator traffic goes to `moted` at localhost through the paired Noise transport, never through the Unix socket.
- Do not claim physical touch, local-network privacy, VoiceOver, signing or TestFlight evidence.
- Preserve the existing Mac client behavior and exact Matrix event contracts.
- Run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before every commit.

## Review Focus

- Switching from Glow to WGPU must not change the Mac client's validated shapes, hit regions or typed events. Task 1 reruns the full rendering suite.
- The iOS entry point must let winit start UIKit exactly once. Task 3 proves launch and process survival in the Simulator.
- New gallery hit targets must become active on the first replacement frame even while old pixels dissolve. Task 5 pins `page_switch_installs_new_hit_targets_on_first_frame`.
- Unchanged structural borders must not restart or flicker across page replacement. Task 5 pins identity and geometry equality.
- Relaunch and host switching must not share cursors, surfaces or approvals. Task 7 exercises colliding fixture hosts and a real loopback reconnect.

---

### Task 1: WGPU compatibility and Apple targets

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `mote-client/Cargo.toml`
- Test: existing `mote-client` rendering, layout, accessibility and theme tests

**Interfaces:**
- Produces: one workspace eframe configuration with `default-features = false` and features `accesskit`, `default_fonts`, and `wgpu`.

- [ ] **Step 1: Record the current backend failure boundary**

Run the current dependency tree and `cargo check -p mote-client --target aarch64-apple-ios-sim`. Record the Glow/glutin iOS failure in `docs/verification/IPAD-01.md` as pre-change evidence.

- [ ] **Step 2: Install the two Apple Rust targets**

Run: `rustup target add aarch64-apple-ios-sim aarch64-apple-ios`
Expected: both targets appear in `rustup target list --installed`.

- [ ] **Step 3: Change eframe to the WGPU feature set**

Keep the exact eframe and egui versions. Do not add a second renderer feature.

- [ ] **Step 4: Verify desktop and Apple core compilation**

Run: `cargo test -p mote-client && cargo check -p mote-client --target aarch64-apple-ios-sim`
Expected: the Mac suite passes and compilation proceeds past the old glutin boundary.

- [ ] **Step 5: Run workspace verification and commit**

Run workspace test and clippy, then commit `build(ui): use WGPU for Apple targets`.

### Task 2: Shared Rust client core

**Files:**
- Create: `crates/mote-client-core/Cargo.toml`
- Create: `crates/mote-client-core/src/lib.rs`
- Create: `crates/mote-client-core/src/contract.rs`
- Create: `crates/mote-client-core/src/state.rs`
- Create: `crates/mote-client-core/src/accessibility.rs`
- Create: `crates/mote-client-core/src/surface.rs`
- Create: `crates/mote-client-core/src/surface_v1.rs`
- Create: `crates/mote-client-core/src/theme.rs`
- Create: `crates/mote-client-core/src/gallery.rs`
- Create: `crates/mote-client-core/src/transition.rs`
- Create: `crates/mote-client-core/src/observation.rs`
- Modify: `Cargo.toml`
- Modify: `mote-client/Cargo.toml`
- Modify: `mote-client/src/{app,contract,state,accessibility,surface,surface_v1,theme,gallery}.rs`
- Modify: `mote-client/src/observation.rs`
- Move/adapt tests: `mote-client/src/{surface_v1_tests,layout_tests,accessibility_tests}.rs`

**Interfaces:**
- Produces: `Surface`, `ChatState`, `SurfaceEvent`, `Gallery`, `GalleryPage`, `install_theme`, platform-neutral reducers and draw functions.
- Constraint: this crate depends on egui, accesskit, mote-ui, mote-protocol and serde only. Event DTOs
  live in `mote-protocol`; the UI core does not depend on mote-eventlog, rusqlite, eframe, mote-ipc,
  filesystem paths, sockets or UIKit.

- [ ] **Step 1: Add a dependency-boundary test**

Add a workspace check that rejects `eframe`, `mote-ipc` and Apple framework dependencies in `mote-client-core` and verifies the core can compile for `aarch64-apple-ios-sim`.

- [ ] **Step 2: Run the boundary check and confirm failure**

Run: `cargo check -p mote-client-core --target aarch64-apple-ios-sim`
Expected: FAIL because the crate does not exist.

- [ ] **Step 3: Move the platform-neutral modules without behavior changes**

Preserve state IDs, structural paths, role names, colors, hit regions and event payloads. Move the
platform-neutral observation harness and renderer probes into the core while leaving the desktop
eframe adapter, Unix workers, cursor files and pipeline code in `mote-client`.

- [ ] **Step 4: Make the Mac shell consume the shared core**

Replace local module access with core imports. Keep current public observation helpers available under the existing test feature.

- [ ] **Step 5: Run parity tests**

Run: `cargo test -p mote-client-core && cargo test -p mote-client`
Expected: all moved tests and existing desktop tests pass unchanged.

- [ ] **Step 6: Run workspace verification and commit**

Run workspace test and clippy, then commit `refactor(ui): share the Matrix client core`.

### Task 3: Minimal native iOS shell

**Files:**
- Create: `mote-ios/Cargo.toml`
- Create: `mote-ios/src/lib.rs`
- Create: `mote-ios/src/app.rs`
- Create: `ios/Mote/Mote.xcodeproj/project.pbxproj`
- Create: `ios/Mote/main.m`
- Create: `ios/Mote/mote_ios.h`
- Create: `ios/Mote/Info.plist`
- Create: `ios/Mote/Assets.xcassets/**`
- Create: `ios/Mote/build-rust.sh`
- Create: `ios/MoteUITests/MoteUITests.m`
- Modify: `Cargo.toml`

**Interfaces:**
- Consumes: `mote-client-core`.
- Produces: `extern "C" fn mote_ios_main() -> !` and an Xcode Simulator app that links `libmote_ios.a`.

- [ ] **Step 1: Add the failing Apple build check**

Run: `cargo build -p mote-ios --target aarch64-apple-ios-sim`
Expected: FAIL because `mote-ios` does not exist.

- [ ] **Step 2: Implement the Rust entry and offline app state**

The Rust entry calls eframe/winit before UIKit starts elsewhere. Start in offline gallery mode with no transport or model activity.

- [ ] **Step 3: Add the checked-in Xcode project**

Use the temporary bundle ID `local.mote.simulator`. The build phase compiles the Rust static library
for the active Simulator architecture, then links it into the application. Add
`NSLocalNetworkUsageDescription` and `_mote._tcp` under `NSBonjourServices` to `Info.plist`. Add an
XCUITest target only as a repeatable acceptance driver. Do not configure a development team or device
signing.

- [ ] **Step 4: Build the Rust targets**

Run: `cargo check -p mote-protocol --target aarch64-apple-ios-sim && cargo check -p mote-protocol --target aarch64-apple-ios && cargo check -p mote-client-core --target aarch64-apple-ios-sim && cargo check -p mote-client-core --target aarch64-apple-ios && cargo build -p mote-ios --target aarch64-apple-ios-sim && cargo build -p mote-ios --target aarch64-apple-ios`
Expected: PASS.

- [ ] **Step 5: Build and launch the Simulator app**

Use `xcodebuild` with an available iPad Simulator destination, install with `simctl`, launch `local.mote.simulator`, and confirm the process remains alive with the gallery visible.

- [ ] **Step 6: Verify and close IPAD-01**

Record toolchain, SDK, target, Xcode build and launch evidence in `docs/verification/IPAD-01.md`. Run workspace test and clippy, update the README checkbox, and commit `feat(ipad): launch the Rust client in Simulator`.

### Task 4: Page-based gallery data

**Files:**
- Modify: `crates/mote-client-core/src/gallery.rs`
- Modify: `crates/mote-client-core/src/contract.rs`
- Test: `crates/mote-client-core/src/gallery.rs`

**Interfaces:**
- Produces: `GalleryPage::{Layout, Toggle, Slider, Select, List, Form, Dashboard, Fixture}`, `Gallery::select` and one validated supported surface per page.

- [ ] **Step 1: Write failing gallery-page tests**

Add `gallery_pages_cover_required_names_and_validate`. Assert each required page is selectable, validates, uses only the supported consumer contract and exposes its expected control.

- [ ] **Step 2: Run the focused test and confirm failure**

Run: `cargo test -p mote-client-core gallery_pages_cover_required_names_and_validate`
Expected: FAIL because the page model is absent.

- [ ] **Step 3: Implement the page model**

Build List and Form from supported v1 rows, columns and controls. Do not enable unresolved Matrix catalog elements. Load the checked-in dashboard through trusted host lowering, not by parsing untrusted source on the iPad.

- [ ] **Step 4: Run core gallery and validation tests**

Run: `cargo test -p mote-client-core gallery`
Expected: PASS.

- [ ] **Step 5: Commit the gallery pages**

Run workspace test and clippy, then commit `feat(ui): add Matrix gallery pages`.

### Task 5: Vector pixel materialization

**Files:**
- Modify: `crates/mote-client-core/src/transition.rs`
- Modify: `crates/mote-client-core/src/surface.rs`
- Modify: `crates/mote-client-core/src/surface_v1.rs`
- Modify: `crates/mote-client-core/src/theme.rs`
- Modify: `matrix/themes/vector.md` only if implementation discovers a documented-token mismatch

**Interfaces:**
- Produces: `StructuralId`, `StructuralStroke`, `MaterializeState::replace` and `MaterializeState::frame`.
- Fixed values: deterministic 4 logical point square particles, 120 ms ease-out, immediate new layout and hit testing, reduced-motion immediate swap plus brief outline pulse.

- [ ] **Step 1: Write failing transition tests**

Add `page_switch_installs_new_hit_targets_on_first_frame`, `changed_strokes_materialize_at_0_60_120_ms`, `unchanged_strokes_do_not_restart`, `particle_frames_are_deterministic` and `reduced_motion_swaps_and_pulses`.

- [ ] **Step 2: Run the focused tests and confirm failure**

Run: `cargo test -p mote-client-core transition`
Expected: FAIL because structural transition behavior is absent.

- [ ] **Step 3: Register structural strokes in production rendering**

Derive identity from namespace, node path and kind. Preserve strokes only when identity and geometry match. Do not infer transition geometry from test-only observation data.

- [ ] **Step 4: Implement deterministic materialization**

Install the new resolved surface before starting paint animation. Split only changed red borders into 4 point squares, order by a stable hash and ease to the new strokes at exactly 120 ms.

- [ ] **Step 5: Apply the approved Vector tokens**

Use dark line `#681D24`, dark accent `#F0444C`, light line `#9F3538` and light accent `#C8322E`. Rebaseline only intentional theme goldens and explain each changed pixel in evidence.

- [ ] **Step 6: Run transition, layout and theme tests**

Run: `cargo test -p mote-client-core && cargo test -p mote-client`
Expected: PASS with first-frame hit targets and unchanged-node stability.

- [ ] **Step 7: Commit the transition**

Run workspace test and clippy, then commit `feat(ui): materialize Vector page transitions`.

### Task 6: Offline Simulator acceptance

**Files:**
- Create: `mote-ios/src/simulator.rs`
- Modify: `mote-ios/src/app.rs`
- Create: `docs/verification/IPAD-02.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: page gallery and materialization from Tasks 4 and 5.
- Produces: deterministic Simulator launch arguments for gallery page, reduced motion and fixture selection.

- [ ] **Step 1: Add Simulator test launch modes**

Expose only nonsecret launch arguments for initial page, fixture and reduced motion. Keep production defaults unchanged.

- [ ] **Step 2: Drive two iPad Simulator sizes and orientations**

Discover available runtimes at execution time. Use the checked-in XCUITest target to exercise every
page, scrolling, text entry, toggle, slider, select and form in portrait and landscape on two iPad
sizes. Assert every new control retains its 48 by 48 point hit region.

- [ ] **Step 3: Capture the transition evidence**

Use a screen recording or frame capture to retain first, approximately 60 ms and 120 ms frames. Record sustained frame timing and the reduced-motion outline pulse.

- [ ] **Step 4: Verify and close IPAD-02**

Record device runtimes, commands, screenshots, recordings and observed interactions in `docs/verification/IPAD-02.md`. Run workspace test and clippy, update the README checkbox, and commit `test(ipad): verify the offline Matrix gallery`.

### Task 7: Live localhost host adapter

**Files:**
- Create: `mote-ios/src/live.rs`
- Create: `mote-ios/src/keychain.rs`
- Create: `mote-ios/src/discovery.rs`
- Modify: `mote-ios/src/app.rs`
- Modify: `mote-client/src/app.rs`
- Modify: `mote-client/src/stream.rs`
- Create: `mote-ios/tests/live_loopback.rs`
- Create: `docs/verification/IPAD-03.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: `HostTransport`, `HostEventStream`, `HostDirectory`, `DiscoveryBrowser` and paired loopback harness from the host plan.
- Produces: core-facing `HostCommand` and `HostUpdate` channels used by both Mac and iPad shells plus
  a Simulator Keychain-backed `CredentialStore`.

- [ ] **Step 1: Write failing transport-parity tests**

Assert the same reducer sees identical updates for a fixture transport and the Unix adapter. Add live
pair, chat, surface, typed change acknowledgement, relaunch, credential deletion, revocation and
two-host collision tests.

- [ ] **Step 2: Run the live-loopback test and confirm failure**

Run: `cargo test -p mote-ios --test live_loopback`
Expected: FAIL because the live adapter is absent.

- [ ] **Step 3: Implement shell-independent command and update channels**

Run blocking transport work off the render thread. Every command carries explicit host identity. Feed replies, events and connection states into the same shared reducer.

- [ ] **Step 4: Configure Simulator pairing over localhost**

Accept the ephemeral localhost endpoint and short-lived `MOTE1` offer through test configuration.
Persist paired credentials per cryptographic host identity in the Simulator Keychain. Do not
hard-code a host key or bypass Noise because the address is loopback.

- [ ] **Step 5: Drive Bonjour discovery from the Simulator**

Publish `_mote._tcp` on the Mac with only version and fingerprint TXT fields. Discover it from the
Simulator, pair, rotate the loopback port, rediscover and reconnect only after Noise proves the same
key. Reject an impersonator. Record the checked-in purpose text, but do not claim the physical privacy
prompt.

- [ ] **Step 6: Run automated and live journeys**

Pair, send a chat, render a committed Matrix surface, edit a control, kill and relaunch from the saved per-host cursor, and switch between two colliding fixture hosts without mixed state.

- [ ] **Step 7: Verify and close IPAD-03 and HOST-04 when complete**

Record commands, endpoint, Simulator runtime, Keychain persistence checks, discovery evidence,
screenshots, cursor comparison and event log in `docs/verification/IPAD-03.md` and
`docs/verification/HOST-04.md`. Run workspace test and clippy. Update each README checkbox only when
its full clause passes, and commit `feat(ipad): connect Simulator to Mote localhost`.
