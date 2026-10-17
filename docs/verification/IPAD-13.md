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

The native iPad Pro 11-inch (M5), iOS 27.0 Simulator journey rebuilt and installed the current Rust and Xcode client, then paired it with a scripted local host. It passed generated UI success, one diagnostic retry, preservation of the prior surface on invalid output, a visible terminal diagnostic and prose fallback. The test reported 14 model calls, 14 ledger rows, 14 packs and three authenticated typed refusals.

```text
MOTE_LIVE=1 TMPDIR=/private/tmp \
MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E \
MOTE_ARTIFACTS=/private/tmp/mote-ipad13-native-20261013 \
MOTE_SURFACE_ATTESTATION=/private/tmp/mote-surface-attested.8F07uH/surface-activation.json \
cargo test -p mote-ios --test inferred_surface_simulator \
native_inferred_success_retry_preservation_and_prose_fallback -- --ignored --nocapture
test native_inferred_success_retry_preservation_and_prose_fallback ... ok
test result: ok. 1 passed; 0 failed
```

The local build and XCUITest logs and result bundle are in `/private/tmp/mote-ipad13-native-20261013`. This journey verifies native integration after the transition fix but does not capture the 0, 60 and 120 ms transition frames or immediate interaction on the changed layout.

## Paired replacement frames and immediate action

Date: 2026-10-17, America/Los_Angeles.

Environment: macOS 26.6.2, Rust 1.89.0, Xcode 27.1 (27A9275), Simulator SDK 27.1. The booted iPad Pro 11-inch (M5) Simulator uses iOS 27.0 and identifier `43F2F6ED-5C86-4121-B03F-40FE38891C4E`. The native app was rebuilt from the working tree before this run. No physical iPad was used.

Command:

```text
MOTE_LIVE=1 \
MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E \
MOTE_ARTIFACTS=/private/tmp/mote-live-transition-20261017-f \
cargo test -p mote-ios --test live_transition_simulator \
paired_committed -- --ignored --nocapture
```

Output:

```text
test paired_committed_replacement_captures_frames_and_first_frame_action ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out
```

The test runs two independent authenticated paired hosts. The first uses the real daemon's `matrix.render` compiler and commitment path. It changes a button border, header rule and footer rule. It primes the existing interaction status row before measurement so that an unrelated status-row insertion does not move the entire surface during the measured transition. The replacement button receives a synthetic click through the production AccessKit action path during its first renderer frame. The renderer emits the action in that frame. The real daemon accepts exactly one action after the replacement. Its refresh response commits the same geometry under another control revision without restarting the materialization.

The second host is an explicitly typed LoopbackHost fixture. It adds a slider and moves its rail with the header and footer rules. Its fixture dispatcher validates the authenticated host, thread, latest committed surface revision, contract, bound button path and empty arguments before recording the action. This fixture proves native typed surface rendering and interaction. It does not prove that the production agent can generate a slider.

The explicit Simulator-only `--mote-live-transition-probe` records surface stroke geometry, square-particle counts and first-frame emitted events. It also paints a binary frame marker. External `xcrun simctl io <device> recordVideo --codec=h264 <path>` captures the visible journey. The harness decodes each video frame's marker and matches it to the renderer trace. It extracts the actual first replacement frame, a midpoint between 40 and 100 ms, and a settled frame between 120 and 150 ms. It asserts particles in the first and midpoint frames, no particles in the settled frame, movement of every measured rule and rail, and an unchanged surface border painted throughout. It restores prior Simulator live state and removes its temporary native credentials.

| Paired path | First | Midpoint | Settled | Square particles |
| --- | --- | --- | --- | --- |
| Real daemon | 0 ms | 50.006 ms | 133.362 ms | 2522, 2505, 0 |
| Typed slider fixture | 0 ms | 49.996 ms | 133.347 ms | 2069, 2060, 0 |

The unchanged header-button border stayed at `[24, 291, 83.03125, 36]` points in both paths. Its structural path and painted geometry remained unchanged. The changed button action used path `[1]` on the replacement revision. The earlier exact 120 ms headless regression supplies the duration boundary. The native settled samples are the first captured frames after that boundary. These captures do not measure physical display scanout.

Durable images and trace metadata:

- [Real daemon first](assets/IPAD-13/live-20261017/real-daemon/first.png), [midpoint](assets/IPAD-13/live-20261017/real-daemon/midpoint.png), [settled](assets/IPAD-13/live-20261017/real-daemon/settled.png), [selected trace](assets/IPAD-13/live-20261017/real-daemon/selected-frames.json).
- [Typed slider first](assets/IPAD-13/live-20261017/typed-slider-fixture/first.png), [midpoint](assets/IPAD-13/live-20261017/typed-slider-fixture/midpoint.png), [settled](assets/IPAD-13/live-20261017/typed-slider-fixture/settled.png), [selected trace](assets/IPAD-13/live-20261017/typed-slider-fixture/selected-frames.json).

Each directory also contains the before image, full native trace and video marker map. The original videos, build output, host events and cleanup evidence remain in `/private/tmp/mote-live-transition-20261017-f`.

Earlier capture attempts failed strict assertions. Continuous in-app GPU readback reduced update cadence to about 10 fps and missed the midpoint. Sparse readback captured the second replacement frame and interrupted later cadence. External video avoids that interference. Another attempt correctly showed a second transition when the first accepted action inserted the live status row. The final fixture primes that row before measurement. The typed fixture also required a private SQLite file and the gateway's actual `interaction` dispatch method. These were fixture defects. They are not accepted product failures or passing evidence.

IPAD-13 remains open. The production writable manifest exposes one boolean and three model enums, so it cannot compile an agent-generated numeric slider binding. The real-daemon path above commits trusted source directly and makes no model call. The typed slider path bypasses source generation and production binding resolution. The live branch also returns before applying the existing reduced-motion override, and no native system Reduce Motion bridge exists. No reduced-motion pass is claimed. The new journey does not fully exercise edited control state across revisions. These remaining behaviors require implementation or separate evidence before completion.

Verification after the harness change:

```text
cargo test --workspace -- --test-threads=4
exit 0
cargo clippy --workspace -- -D warnings
exit 0
cargo clippy -p mote-ios --all-targets -- -D warnings
exit 0
cargo clippy -p mote-ios --target aarch64-apple-ios-sim -- -D warnings
exit 0
cargo test -p mote-ios --lib --offline
test result: ok. 32 passed; 0 failed
```

The workspace and library tests ran with local fixture socket access. An earlier sandboxed library run had 31 passes and failed `live::recovery_tests::saved_host_recovers_after_unavailable_app_launch` at its localhost listener with `PermissionDenied`. The identical library command passed with fixture access. The new paired timeline test was first observed failing its replacement-trigger assertion and then passing after implementation.

## Changed button border ownership

Date: 2026-10-17. Environment and Simulator are the same as the paired replacement run above.

Review found that aggregate particle counts and moved rules could pass even if the changed button border did not materialize. The native journey now queries the exact structural button stroke at path `[1]`. A test-only and Simulator-only observation hook tags the actual transition paint groups with their owning structural IDs. It records the button's current points, previous points, solid-stroke visibility and owned square-particle count. Normal production builds do not compile the hook. The Simulator records observations only when the explicit probe enables them.

The native assertions require a five-point button border that moves below the taller header. They require its previous points to match the before frame and its current points to match the settled border. In both first and midpoint frames, that exact border must have its solid stroke hidden and its own square particles present. At settlement, its moved solid border must be painted and its owned particle count must be zero. Header, footer and slider particles cannot satisfy these checks.

The new assertion first failed with `missing path [1] button materialization observation` in `/private/tmp/mote-live-transition-border-red`. The completed hook passed this command:

```text
MOTE_LIVE=1 \
MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E \
MOTE_ARTIFACTS=/private/tmp/mote-live-transition-border-green \
cargo test -p mote-ios --test live_transition_simulator \
paired_committed -- --ignored --nocapture

test paired_committed_replacement_captures_frames_and_first_frame_action ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out
```

| Paired path | Button before | Button settled | Midpoint | Settled | Button-owned particles |
| --- | --- | --- | --- | --- | --- |
| Real daemon | `[8, 379.5, 818, 36]` | `[8, 428.5, 818, 36]` | 49.999 ms | 133.329 ms | 854, 854, 0 |
| Typed slider fixture | `[8, 379.5, 818, 36]` | `[8, 412.5, 818, 36]` | 49.991 ms | 133.331 ms | 854, 854, 0 |

Both first replacement frames were captured at 0 ms. The strict 40 through 100 ms midpoint and 120 through 150 ms settled windows remain unchanged. Both journeys also retain their immediate action, unchanged stroke, moved rule and rail, and single accepted effect assertions.

- [Daemon first](assets/IPAD-13/live-border-20261017/real-daemon/first.png), [midpoint](assets/IPAD-13/live-border-20261017/real-daemon/midpoint.png), [settled](assets/IPAD-13/live-border-20261017/real-daemon/settled.png), [path-owned observations](assets/IPAD-13/live-border-20261017/real-daemon/selected-frames.json).
- [Slider fixture first](assets/IPAD-13/live-border-20261017/typed-slider-fixture/first.png), [midpoint](assets/IPAD-13/live-border-20261017/typed-slider-fixture/midpoint.png), [settled](assets/IPAD-13/live-border-20261017/typed-slider-fixture/settled.png), [path-owned observations](assets/IPAD-13/live-border-20261017/typed-slider-fixture/selected-frames.json).

Each directory also retains the before image, full trace and video marker map. Original videos and host events remain in `/private/tmp/mote-live-transition-border-green`. The focused command `cargo test -p mote-client-core transition::tests --offline` passed all 18 transition tests. The remaining IPAD-13 product and evidence gaps listed above remain open.

Follow-up regression commands also passed:

```text
cargo test --workspace -- --test-threads=4
exit 0
cargo clippy --workspace -- -D warnings
exit 0
cargo clippy -p mote-ios --target aarch64-apple-ios-sim -- -D warnings
exit 0
```

The complete workspace output is retained in `/private/tmp/mote-transition-border-workspace.log`. The run briefly waited in the macOS loader before its first test binary started, then completed without test failures.

## Explicit observation gate

Date: 2026-10-17. Review found that owner tags were still collected for ordinary Simulator transitions. The renderer now reads the explicit observation flag before materialization and collects owner tags only while it is enabled. Both the normal and reduced-motion paint paths leave the owner vector empty with zero capacity while observation is disabled. The observation map remains gated by the same flag.

The new `ordinary_materialization_records_no_paint_owners` test first failed with `disabled observation must not collect owners`. After the fix, it passed for both motion modes. The test also enables observation and requires identical paint and hidden-stroke output. All 19 focused transition tests passed with `cargo test -p mote-client-core transition::tests --offline`. The final focused rerun of `cargo test -p mote-client-core ordinary_materialization_records_no_paint_owners --offline` passed after adding the output equality checks.

The explicit probe path was rerun with the same native command above and `MOTE_ARTIFACTS=/private/tmp/mote-live-transition-observation-gate`. It passed both paired hosts with the strict first-frame, timing, button-owned particle, unchanged-stroke, rule, rail and action assertions intact. Output was `1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out`. Workspace clippy and Simulator-target clippy both exited 0 after this gate change. No production rendering behavior or reduced-motion support was changed.

The full `cargo test --workspace -- --test-threads=4` rerun also exited 0. Its output is retained in `/private/tmp/mote-transition-observation-gate-workspace.log`.
