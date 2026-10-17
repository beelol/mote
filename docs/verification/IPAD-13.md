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

IPAD-13 remains open. The production writable manifest exposes one boolean and three model enums, so it cannot compile an agent-generated numeric slider binding. The real-daemon path above commits trusted source directly and makes no model call. The typed slider path bypasses source generation and production binding resolution. At that run, the live branch returned before applying the existing reduced-motion override and no native system Reduce Motion bridge existed. The system bridge and draw-order defect are addressed by the later automated evidence below. No physical system-preference pass is claimed. The new journey does not fully exercise edited control state across revisions. These remaining behaviors require implementation or separate evidence before completion.

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

## Native system Reduce Motion propagation

Date: 2026-10-17, America/Los_Angeles. The native adapter now reads `UIAccessibilityIsReduceMotionEnabled()`. MoteApp applies the current system value OR the existing explicit override before every draw branch, including live paired rendering and first-run rendering. A system change notification requests a repaint. The app-owned observer is removed on bridge teardown. Unit-only preference injection is excluded from production builds. No Simulator preference override hook was added.

`cargo test -p mote-ios --lib motion -- --nocapture` passed four tests after the new bridge/state/draw-path/lifetime tests were first observed failing. The draw-path test renders a paired host, first-run panel and gallery while changing both inputs and checking the effective transition setting. This is automated propagation evidence. It is not a new paired first/midpoint/settled frame capture and does not demonstrate a physical system preference toggle. Native adapter and build verification is recorded in [IPAD-06](IPAD-06.md#system-reduce-motion-bridge-implementation).

The final draw-path assertion also counts native reads with the explicit override on. It first failed with zero reads. After reading the system value into a local before the OR, all four focused tests passed in `/private/tmp/mote-motion-read-count-green.log`. Both post-correction native bundle builds exited 0 in `/private/tmp/mote-motion-unconditional-final-device-build.log` and `/private/tmp/mote-motion-unconditional-final-sim-build.log`. Earlier bundle logs are superseded.

The fresh post-correction `cargo test --workspace -- --test-threads=4` run exited 0 in `/private/tmp/mote-motion-final-workspace-tests.log`. Its suite summaries total 748 passed, zero failed and 21 ignored. Workspace clippy exited 0 in `/private/tmp/mote-motion-workspace-clippy.log`. The final focused motion rerun passed four tests in `/private/tmp/mote-motion-final-focused-tests.log`. Host all-target clippy and both Apple target builds and clippy checks exited 0. Their exact commands and logs are recorded in [IPAD-06](IPAD-06.md#final-automated-source-verification). Ignored journeys are not acceptance evidence.

The former live-branch override omission and missing system bridge are implemented. The production numeric-binding limitation and the remaining literal IPAD-13 journey requirements above remain open. No acceptance checkbox is changed.

## Production numeric slider journey prepared

Date: 2026-10-17, America/Los_Angeles.

The ignored Simulator journey now replaces the typed slider fixture with four independent real daemon hosts. Each host starts with a trusted baseline that has a `models.local_only` toggle at path `[2]` and no slider. Each host then receives a fresh activation file and an actual scripted surface-model response through the production `turn` inference path. The response replaces that toggle with an exact `budgets.router` slider at path `[2]`.

Each motion mode has separate transition and action runs. The transition run leaves the inferred replacement untouched and supplies the first, midpoint and settled timing evidence. The action run adds the explicit `--mote-live-transition-action` launch argument. The native probe then sends AccessKit `SetValue(160)` to the newly introduced control on its first replacement frame. The action run requires one accepted typed change on the replacement revision. It checks the exact generated node, replacement sequence, node path, field and value. It checks integer `config.changed` persistence from 150 to 160. It checks a later refreshed surface and rendered revision both carry 160. It checks that the later revision emits no stale local edit. It then sends 170 against the replaced revision through another authenticated paired client and requires an `InvalidParams` response with no new event and no config mutation.

The transition run executes once normally and once with `--mote-reduced-motion`. Both preserve external Simulator video recording, binary frame-marker decoding and extraction of the before, first, midpoint and settled PNGs. The baseline has two measured line strokes. The replacement has three because it adds the slider rail. Normal mode requires the changed button border, moved header and footer rules, and newly introduced slider rail to own square particles on the first and midpoint frames, and no particles after 120 ms. The rail observation must have no prior geometry. Reduced motion requires zero particles, immediate changed solid strokes, red two-point pulse paths on the first and midpoint frames, and no pulse after settlement. Both modes require an unchanged stroke throughout. The separate action runs retain external video plus literal before and first-frame PNGs. They do not claim that the superseded first transition settles by 120 ms.

The first live combined run in `/private/tmp/mote-ipad13-slider-20261017.VlCIuN` was red. Its first-frame change immediately committed a refreshed surface revision. That valid second revision restarted changed geometry, leaving 2,179 particles at 133 ms and reaching zero at 150 ms. The failure showed that one host run cannot prove both an untouched 120 ms replacement and first-frame mutation. It is diagnostic evidence only. The split four-host journey has not been run live yet.

Focused non-live verification passed. These commands do not invoke a model or claim the literal Simulator evidence:

```text
cargo test -p mote-ios --lib probe::tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 30 filtered out

cargo test -p mote-ios --test live_transition_simulator --no-run
exit 0

cargo check -p mote-ios --target aarch64-apple-ios-sim
exit 0

cargo clippy -p mote-ios --test live_transition_simulator -- -D warnings
exit 0

cargo clippy -p mote-ios --target aarch64-apple-ios-sim -- -D warnings
exit 0
```

The remaining prerequisite is one live local surface benchmark to issue an activation file whose corpus and model digest match the current tree. This command is intentionally not run as part of the non-live implementation check:

```text
MOTE_HOME=/private/tmp/mote-ipad13-attestation-20261017 \
MOTE_LIVE=1 \
cargo run -p moted -- bench surface
```

After that prerequisite succeeds, the evidence journey itself invokes only the scripted local model server. It does not call a live model:

```text
MOTE_LIVE=1 \
MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E \
MOTE_ARTIFACTS=/private/tmp/mote-ipad13-slider-20261017 \
MOTE_SURFACE_ATTESTATION=/private/tmp/mote-ipad13-attestation-20261017/surface-activation.json \
cargo test -p mote-ios --test live_transition_simulator \
paired_committed_replacement_captures_frames_and_first_frame_action \
-- --ignored --nocapture
```

Expected artifact directories are `normal-transition`, `normal-action`, `reduced-motion-transition` and `reduced-motion-action` under `MOTE_ARTIFACTS`. Every directory contains `journey.mp4`, `events.json`, `capture/report.json`, `video-map.json` and `selected-frames.json`. Transition directories contain the before, first, midpoint and settled PNGs. Action directories contain before and first PNGs. IPAD-13 remains open until this literal paired run passes and its artifacts are retained.

## Production slider journey complete

Date: 2026-10-17, America/Los_Angeles. Environment: iPad Pro 11-inch M5 Simulator `43F2F6ED-5C86-4121-B03F-40FE38891C4E`, iOS 27.0 build 24A434, current production daemon and native client. The activation file was `/private/tmp/mote-ipad13-attestation-20261017.JRF2iJ/surface-activation.json`. Its benchmark recorded 197 of 200 first-try successes, 198 after retry and 3 of 3 slider cases on the first try.

The literal four-host command passed:

```text
MOTE_LIVE=1 \
MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E \
MOTE_ARTIFACTS=/private/tmp/mote-ipad13-slider-20261017-split-green6 \
MOTE_SURFACE_ATTESTATION=/private/tmp/mote-ipad13-attestation-20261017.JRF2iJ/surface-activation.json \
cargo test -p mote-ios --test live_transition_simulator \
paired_committed_replacement_captures_frames_and_first_frame_action \
-- --ignored --nocapture

test paired_committed_replacement_captures_frames_and_first_frame_action ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out
```

Each of the four paths used a fresh scripted model server and production daemon host. The normal transition captured 2,297 square particles on the first frame, 2,289 at 50.019 ms and zero at 133.433 ms. The reduced-motion transition captured zero particles, four red outline pulses on the first frame and at 49.764 ms, and zero pulses at 133.275 ms. Both retained the same unchanged rectangle at `[24, 594.6875, 83.03125, 36]`. The changed button border, header rule, footer rule and new slider rail retained their strict ownership and geometry assertions.

The normal and reduced action paths both changed the new `budgets.router` control from 150 to 160 on its first replacement frame. Each required one typed `ui.event`, integer `config.changed` persistence, a refreshed surface and rendered revision carrying 160, no repeated local action and stale-revision rejection without another event or config change. Normal action owned 2,295 particles on its first frame. Reduced action owned zero particles and four outline pulses.

The production live layout now reserves the chat area's existing 32 percent maximum height and disables vertical auto-shrink. This prevents newly appended chat messages from translating every Matrix stroke while an agent-generated replacement commits. The priming interaction waits for its refreshed surface and a 500 ms settlement window before measurement. Failed attempts under `split-green2` through `split-green5` exposed these harness and live-layout issues and are not passing evidence.

Durable evidence:

- [Normal transition](assets/IPAD-13/production-slider-20261017/normal-transition/selected-frames.json), with [first](assets/IPAD-13/production-slider-20261017/normal-transition/first.png), [midpoint](assets/IPAD-13/production-slider-20261017/normal-transition/midpoint.png) and [settled](assets/IPAD-13/production-slider-20261017/normal-transition/settled.png) frames.
- [Normal action](assets/IPAD-13/production-slider-20261017/normal-action/selected-frames.json), with [first](assets/IPAD-13/production-slider-20261017/normal-action/first.png).
- [Reduced transition](assets/IPAD-13/production-slider-20261017/reduced-motion-transition/selected-frames.json), with [first](assets/IPAD-13/production-slider-20261017/reduced-motion-transition/first.png), [midpoint](assets/IPAD-13/production-slider-20261017/reduced-motion-transition/midpoint.png) and [settled](assets/IPAD-13/production-slider-20261017/reduced-motion-transition/settled.png) frames.
- [Reduced action](assets/IPAD-13/production-slider-20261017/reduced-motion-action/selected-frames.json), with [first](assets/IPAD-13/production-slider-20261017/reduced-motion-action/first.png).

The original H.264 recordings and complete event streams remain under `/private/tmp/mote-ipad13-slider-20261017-split-green6`. IPAD-13 is complete.
