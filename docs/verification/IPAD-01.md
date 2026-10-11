# IPAD-01: Apple Rust build compatibility

Status: partial. The README criterion remains unchecked.
Date: 2026-10-11, America/Los_Angeles.
Base commit: `4b1f795` on `main`.

## Environment

Commands: `rustc -Vv`, `cargo -V`, `sw_vers`, `xcodebuild -version`,
`xcrun --sdk iphonesimulator --show-sdk-version`, and
`xcrun --sdk iphoneos --show-sdk-version`.

Output:

```text
rustc 1.89.0 (29483883e 2025-08-04)
host: aarch64-apple-darwin
LLVM version: 20.1.7
cargo 1.89.0 (c24e10642 2025-06-23)
macOS 26.6.2 (25G83)
Xcode 27.0 (27A266a)
iOS Simulator SDK 27.0
iPhoneOS SDK 27.0
```

## Pre-change failure

Command: `cargo tree -p mote-client -i glutin`.
Exit: 0.

```text
glutin v0.32.3
├── eframe v0.31.0
│   └── mote-client v0.0.0
└── glutin-winit v0.5.0
    └── eframe v0.31.0
```

Command: `cargo tree -p mote-client -e features -i eframe`.
Exit: 0. The default feature set enabled `accesskit`, `default_fonts`, `glow`, `wayland`,
`web_screen_reader`, and `x11`.

Command: `cargo check -p mote-client --target aarch64-apple-ios-sim`.
The first run exited 101 because the target's `std` and `core` crates were not installed.

Command: `rustup target add aarch64-apple-ios-sim aarch64-apple-ios`.
The restricted attempt could not write the download cache.
The approved unrestricted retry exited 0 and installed both targets.
`rustup target list --installed` then included both target names.

The unchanged simulator check was repeated after installation.
It exited 101 with this renderer failure:

```text
Checking egui_glow v0.31.0
error: Please select at least one api backend
  --> glutin-0.32.3/src/lib.rs:28:1
28 | compile_error!("Please select at least one api backend");
error: could not compile `glutin` (lib) due to 39 previous errors
```

This is the failing build check used before changing the dependency configuration.

## Renderer configuration

The workspace pins eframe and egui to `=0.31.0`.
Eframe now has `default-features = false` and exactly `accesskit`, `default_fonts`, and `wgpu`.
The existing `eframe.workspace = true` entry in `mote-client/Cargo.toml` inherits this configuration.
No source, fixture, golden, layout, hit-region, or event contract was changed.

Commands:

```text
cargo tree -p mote-client -e features -i eframe
cargo tree -p mote-client --target aarch64-apple-ios-sim -i wgpu
cargo tree -p mote-client --target aarch64-apple-ios-sim -i metal
cargo tree -p mote-client --target aarch64-apple-ios-sim -e features -i wgpu-hal
```

All exited 0. Eframe exposes exactly the three selected features.
The Apple dependency path is eframe 0.31.0, egui-wgpu 0.31.0, wgpu 24.0.5,
wgpu-core 24.0.5, wgpu-hal 24.0.4, and metal 0.31.0.
Winit remains 0.30.13. AccessKit remains 0.17.1.
The iOS feature graph enables Metal and does not enable the Glow renderer.

## Simulator compile check

Command: `cargo check -p mote-client --target aarch64-apple-ios-sim`.
Exit: 0.

```text
Checking egui-wgpu v0.31.0
Checking egui-winit v0.31.0
Checking eframe v0.31.0
Checking mote-client v0.0.0
Finished `dev` profile [unoptimized + debuginfo] target(s) in 47.45s
```

The same build command failed at glutin before the dependency change and passed after it.
This verifies compilation only, without an iOS app link or launch.

## Restricted test attempt

Command: `cargo test -p mote-client`.
The restricted run exited 101 with 159 passing tests and five socket permission failures:

```text
app::tests::close_and_reopen_subscribes_from_durable_cursor_and_ignores_stale_replay
app::tests::reconnect_removes_expired_matrix_controls_and_preserves_chat_and_cursor
app::tests::restart_between_status_and_subscription_rechecks_the_watermark
cursor::tests::desktop_reads_the_daemon_public_key_identity_before_migrating
tests::rejected_matrix_submission_still_opens_chat_for_logged_diagnostic
```

Each failed with `Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }`.
The last test reproduced the same failure when run alone with `-- --nocapture`.
The suite was rerun with approved local socket access.

## Mac regression suite

Command: `cargo test -p mote-client`, with approved local socket access.
Exit: 0.

```text
test result: ok. 164 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;
finished in 43.40s
```

The unchanged suite covers all fixture rendering, layout at both golden viewports, 48 point hit
regions, AccessKit role goldens, light and dark themes, typed control events and reconnect behavior.
The pre-existing unused test helper warning for `ObservationHarness::legacy` remains.
No expected shape, role, event, or golden output was changed.

## Workspace verification

Commands: `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`.
Both exited 0 with approved local socket access.
The test output contains 580 passing tests, zero failures and two ignored live Bonjour tests across
48 result summaries, including zero-test documentation suites.
Strict clippy completed without warnings in 7.54 seconds.
The workspace test run retains the pre-existing unused test helper warning for
`ObservationHarness::set_viewport` and `ObservationHarness::legacy`.

The implementation commit is the commit containing this evidence and the WGPU manifest change.
The controller's task report records its exact SHA after creation.

## Remaining acceptance

Task 1 addresses renderer compatibility only.
The shared client core, native iOS shell, both Rust target builds, Xcode application build,
and Simulator launch remain later tasks.
No physical-device, touch, privacy-prompt, VoiceOver, signing, or TestFlight result is claimed.
