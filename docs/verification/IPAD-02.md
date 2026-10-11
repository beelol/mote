# IPAD-02: Offline iPad Simulator gallery

## Post-browser integration smoke, 2026-10-11

At 13:19 America/Los_Angeles, the driver launched the already-installed Mote app on the booted iPad Pro 11-inch (M5) iOS 27.0 Simulator. This used the app installed by the later native browser-approval journey. No Xcode rebuild or app-container replacement was needed.

```text
xcrun simctl launch 43F2F6ED-5C86-4121-B03F-40FE38891C4E local.mote.simulator --mote-offline
local.mote.simulator: 81456
xcrun simctl io 43F2F6ED-5C86-4121-B03F-40FE38891C4E screenshot /private/tmp/mote-gallery-current-settled-20261011.png
Wrote screenshot to: /private/tmp/mote-gallery-current-settled-20261011.png
```

The [settled native capture](assets/IPAD-02/post-browser-11-portrait-gallery.png) shows all eight gallery page selectors, the offline status, the Layout page and its anchored header and footer. The initial immediate screenshot was still in startup and is not used as evidence. This smoke proves current app launch and visible gallery composition after browser integration. It does not repeat the earlier two-size, two-orientation control journey below.

Status: verified for the Simulator acceptance clause. Display scanout is not measured.
Date: 2026-10-11, America/Los_Angeles.
Base: `1e47f41` on `main`.
Delivered through `b17b9f9`; acceptance decision recorded after independent review.

## Environment

The host is Apple Silicon on macOS 26.6.2 (25G83).
Rust is 1.89.0 and Cargo is 1.89.0.
Xcode is 27.0 (27A266a).
The Simulator SDK is 27.0.
The tested runtime is iOS 27.0 (24A434).
The renderer uses egui and eframe 0.31.0, winit 0.30.13 and WGPU 24.0.5 with Metal.

Runtime discovery used `xcrun simctl list devices available`, `xcrun simctl list runtimes`
and `xcodebuild -version`.
The available current runtimes are iOS 26.5 (23F77) and iOS 27.0 (24A434).
The selected destinations are:

| Device | UDID | Logical points in portrait |
| --- | --- | --- |
| iPad Pro 11-inch (M5) | `43F2F6ED-5C86-4121-B03F-40FE38891C4E` | 834 by 1210 |
| iPad Pro 13-inch (M5) | `E1982F50-C8EF-47E9-9854-715268304F2B` | 1032 by 1376 |

Simulator and local socket access required approved unrestricted commands.
The initial restricted `simctl` discovery could not connect to CoreSimulatorService.
Its approved repeat exited 0.

## Interactive shell

The Rust iPad shell exposes Layout, Toggle, Slider, Select, List, Form, Dashboard and Fixture.
Each selection updates the gallery before drawing the Matrix surface in the same frame.
The renderer keeps the same stable namespace for structural transitions.
The shell shows the last local typed interaction.
It has no transport or model runner in this offline phase.
The Preview note composer uses ordinary shell text entry.
It does not add a field type to frozen Matrix v1.
The composer sits above the surface so the software keyboard cannot cover it.
The List sample contains 20 supported rows so scrolling is observable in portrait and landscape.
The existing Mac combined gallery remains unchanged.

Nonsecret deterministic arguments are:

```text
--mote-page layout|toggle|slider|select|list|form|dashboard|fixture
--mote-fixture slider-layout
--mote-reduced-motion
```

Page names are case insensitive.
The fixture argument selects only the checked-in slider layout.
It does not accept a file path.
Unknown arguments and unknown fixture names leave default launch choices unchanged.
No argument adds a host, credential, secret or model operation.

## Red and green checks

Before connecting the page gallery to the shell:

```text
cargo test -p mote-ios shell_exposes_all_eight_gallery_pages
missing page selector: Layout
0 passed; 1 failed; exit 101
```

Before implementing launch argument parsing:

```text
cargo test -p mote-ios simulator
checked_in_fixture_can_be_selected_without_a_path: left None, right Some(Fixture)
explicit_launch_options_choose_page_and_reduced_motion: left None, right Some(Form)
1 passed; 2 failed; exit 101
```

Before extending the List sample:

```text
cargo test -p mote-client-core list_gallery_contains_a_reachable_long_scroll_sample
left 3, right 20
0 passed; 1 failed; exit 101
```

Before moving the composer above the surface:

```text
cargo test -p mote-ios composer_clears_the_keyboard_in_portrait_and_landscape
composer is behind the software keyboard at 1162
0 passed; 1 failed; exit 101
```

After those changes, `cargo test -p mote-ios` passed all nine tests.
The tests select every page with pointer input at both sizes in both orientations.
They inspect all eight navigation hit regions and the composer's actual interaction rectangle.
Each is at least 48 by 48 logical points.
They also inspect every Matrix control allocation for that minimum size.
The last List action scrolls into an unclipped 48 point region and emits `gallery.item.20`.
The composer accepts an `egui::Event::Text` without changing the Matrix contract.
The existing safe-area regression remains green.

The initial last-row synthetic click test missed because it skipped the pointer hover frame.
The corrected driver settles the real pointer position before pressing.
The actual Simulator touch test separately taps the final row after dragging.
Text input accessibility bounds describe the inner text area.
The composer hit test uses the response interaction rectangle instead of that inner text area.

## Simulator interaction driver

The pinned iOS backend exposes one full-window native TextView to XCUITest.
It does not expose individual Matrix buttons as native accessibility elements.
The first probe asserting `app.buttons["Layout"]` failed.
The tree contained Application, Window, Other and TextView.

The checked-in test uses Vision text recognition on orientation-normalized window screenshots.
It converts each observed text target to window-relative point coordinates.
Every tap logs its exact point under `MOTE TAP` or `MOTE SLIDER`.
Assertions inspect the resulting interaction state and edited value in the rendered pixels.
They do not merely assert that a label exists.
OCR whitespace is normalized because a field name can be split around its period.
Each orientation settles before relaunch.
This avoids capturing an intermediate rotation image with final window dimensions.
Expected text uses a bounded predicate wait so kinetic scrolling can finish.

The journey selects all eight pages.
It taps the Layout action.
It edits Toggle, Slider and Select.
It drags the List to its final action and taps it.
It edits all Form controls and submits the form.
It edits the Dashboard toggle and all three model selectors, then refreshes.
It edits the Fixture toggle, slider and selector, then refreshes.
It opens the native keyboard and enters a visible preview note.
The same journey runs in portrait and landscape on both destinations.
The selected-page, list before and after, form submit and text-entry screenshots are retained
as XCUITest attachments.

## Live results

All commands below ran on 2026-10-11 in the environment above.
The successful 11-inch journey finished at 06:40:58 PDT.
The successful 13-inch suite finished at 06:44:07 PDT.

```text
xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' -derivedDataPath target/ipad-derived -resultBundlePath target/ipad-task6-11-journey.xcresult -only-testing:MoteUITests/MoteUITests/testGalleryPagesAndControlsInBothOrientations test
testGalleryPagesAndControlsInBothOrientations passed (227.286 seconds)
Executed 1 test, with 0 failures
TEST SUCCEEDED
exit 0

xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=E1982F50-C8EF-47E9-9854-715268304F2B' -derivedDataPath target/ipad-derived -resultBundlePath target/ipad-task6-13-journey.xcresult test-without-building
testGalleryPagesAndControlsInBothOrientations passed (235.275 seconds)
Executed 3 tests, with 0 failures in 257.685 seconds
TEST EXECUTE SUCCEEDED
exit 0

cargo check -p mote-ios --target aarch64-apple-ios-sim
Finished dev profile in 54.98s
exit 0

cargo test --workspace
613 passed; 0 failed; 2 ignored across 53 result summaries
exit 0

cargo clippy --workspace -- -D warnings
Finished dev profile in 1.21s
exit 0
```

The 11-inch deterministic fixture and Reduced Motion launch check passed in 9.155 seconds.
The foreground check passed in 11.692 seconds.
Those two results are in `target/ipad-task6-11-acceptance2.xcresult`.
That earlier suite had one journey failure caused by OCR splitting `models.local_only`.
It is not reported as a green suite.
The corrected whitespace driver passed the full later 11-inch journey above.

The 13-inch suite ran the final journey, deterministic launch and foreground checks together.
Its portrait List last action was tapped at (174.97, 1291.50) points.
Its landscape List last action was tapped at (174.97, 947.50) points.
Both produced `Action: gallery.item.20`.
The portrait composer was tapped at (97.86, 203.03) points.
The landscape composer was tapped at (97.83, 201.82) points.
Both opened the native keyboard and displayed `iPad preview` after text entry.
Other exact point targets remain in the test log under `MOTE TAP` and `MOTE SLIDER`.
The List gesture uses the window-relative points (0.85, 0.70) to (0.85, 0.25).
The shell status remained on the List page before the gesture.
The final action state was captured after the gesture.

The two journey result bundles retain 24 selected-page and interaction attachments each.
The 13-inch suite also retains the deterministic launch and foreground attachments.
Raw landscape XCTest application attachments can contain transformed black bars.
The committed landscape screenshot therefore uses native Simulator capture:

```text
xcrun simctl io E1982F50-C8EF-47E9-9854-715268304F2B screenshot /private/tmp/mote-task6-13-landscape-live.png
Detected file type PNG from extension
Wrote screenshot to: /private/tmp/mote-task6-13-landscape-live.png
exit 0
```

The native screenshot is 2752 by 2064 pixels.
It shows the selected Fixture page and safe-area placement during the successful landscape run.

| Retained screenshot | Observed state |
| --- | --- |
| [11-inch portrait List before](assets/IPAD-02/11-portrait-list-before.png) | Selected List with the first row visible |
| [11-inch portrait List after](assets/IPAD-02/11-portrait-list-after.png) | Last row visible and `gallery.item.20` action |
| [11-inch portrait text](assets/IPAD-02/11-portrait-text.png) | Visible entered note above the native keyboard |
| [13-inch portrait text](assets/IPAD-02/13-portrait-text.png) | Visible entered note above the native keyboard |
| [13-inch landscape Fixture](assets/IPAD-02/13-landscape-fixture.png) | All eight selectors, safe area and fixture controls |

Detailed local command logs are in `.superpowers/sdd/2026-10-11-ipad-simulator-ui/`.
The final logs are `task-6-11-journey.log`, `task-6-13-journey.log`,
`task-6-ios-check.log`, `task-6-workspace-final.log` and `task-6-clippy-final.log`.

## Review follow-up

The interactive commit is `42d50448387ad413f66c3c958022ba8fa7fdd281`.
Review found no Critical or Important issue.
Three Minor robustness findings received focused regressions on 2026-10-11.

```text
cargo test -p mote-ios simulator
Before: 3 passed; 2 failed; exit 101
missing_values_do_not_swallow_later_recognized_flags: reduced_motion was false
invalid_later_values_do_not_erase_a_valid_page_selection: None instead of Some(Form)
After: 5 passed; 0 failed; exit 0
```

The parser now leaves recognized flags available when a preceding option has no value.
An unknown later page value cannot erase an earlier valid selection.
The native text-target retry regression injects two recognition misses at the Vision boundary.
It still uses a real app screenshot and a real Toggle selection.
Its first run failed with one failure in 4.574 seconds.
The bounded eight-second observation retry passed in 10.452 seconds.
The tap uses the coordinates retained from the successful observation.

The exact fixture slider regression failed when the old target produced `Changed volume: 5.0`.
The corrected point target lies four points after the observed label's left edge.
It produces exactly `Changed volume: 0.0`.
The visible minimum thumb is probed in a small pixel region below the label.
The unedited 50 value has zero red pixels in that region.
The edited minimum has 44 red pixels at scale 2.
The probe excludes the ordinary track and focused control border.
Every gallery and fixture slider assertion checks the exact zero event and visible minimum thumb.
Every Form, Dashboard and Fixture select assertion checks both the exact event value
and the visible selected control text.

The Xcode debugger diagnostics `debugger version lookup failed for path '<nil>': noURL`
and `no debugger version` occur in successful tests.
They are non-failing tooling noise, not an app failure.

The first strengthened 13-inch run passed five tests with zero failures in 401.932 seconds.
Later full runs added a focused landscape model-value check.
The 13-inch run then passed five tests and failed one journey at a redundant post-edit Volume lookup.
The exact zero event had passed before that lookup.
The pixel assertion now retains the successful pre-edit label coordinates.
The 11-inch run passed five tests and failed one journey when Vision split the exact Think event
into separate observations and placed the navigation label `Slider` between them.
The assertion now groups only fragments within four points of the status row and orders them by x.
The out-of-order fragment regression failed in 0.087 seconds before this fix.
It passed in 0.042 seconds after the fix.
No recognized characters or expected values are substituted.

Vision read the tiny landscape `qwen3:0.6b` control glyph as `awen3:0.6b` at native resolution.
Vocabulary hints and language correction did not resolve that observation and were removed.
A bounded control-row crop enlarged three times passed the exact model-value regression
in 21.585 seconds with language correction disabled.
Two isolated failing landscape runs and the later failing 11-inch run stalled after their tests ended.
Only those exact Xcode processes were stopped after several minutes without finalization progress.
Their logs and partial result bundles remain local under `.superpowers` and `target`.

Final strengthened runs completed on 2026-10-11 at 07:33 and 07:34 PDT.
They used the environment and device identifiers recorded above.

```text
xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' -derivedDataPath target/ipad-derived -resultBundlePath target/ipad-task6-review-verified-11.xcresult test
Executed 7 tests, with 0 failures in 409.434 seconds
** TEST SUCCEEDED **
exit 0

xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=E1982F50-C8EF-47E9-9854-715268304F2B' -derivedDataPath target/ipad-derived -resultBundlePath target/ipad-task6-review-verified-13.xcresult test-without-building
Executed 7 tests, with 0 failures in 419.476 seconds
** TEST EXECUTE SUCCEEDED **
exit 0

cargo test --workspace
615 passed; 0 failed; 2 ignored across 53 result summaries
exit 0
cargo clippy --workspace -- -D warnings
Finished dev profile in 0.58s
exit 0
cargo check -p mote-ios --target aarch64-apple-ios-sim
Finished dev profile in 0.24s
exit 0
```

Both complete journeys passed exact Form, Dashboard and Fixture selection values,
exact slider minimum events, visible selected control rows and the edited thumb pixel checks.
Both journeys passed text entry with the native keyboard in portrait and landscape.
The native logs record each actual point target from the successful observation.
The result bundles retain screenshots for each selected page and both scrolling states.
IPAD-02 remains open for the temporal evidence described below.

A final review caught a numeric-prefix assertion gap on 2026-10-11.
The focused negative case rejected `Changed volume: 0.01` when expecting `Changed volume: 0.0`.
It failed in 0.081 seconds against substring matching.
Complete normalized status-row equality fixed the gap.
Exact zero and whitespace-normalized zero remain accepted.
Trailing text and longer numeric tokens are rejected without value substitution.

```text
xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' -derivedDataPath target/ipad-derived -resultBundlePath target/ipad-task6-numeric-green.xcresult -only-testing:MoteUITests/MoteUITests/testExactStatusRejectsLongerNumericTokens -only-testing:MoteUITests/MoteUITests/testFixtureSliderReachesExactMinimum -only-testing:MoteUITests/MoteUITests/testStatusRowOrdersSplitExactEventFragments test
Executed 3 tests, with 0 failures in 10.722 seconds
** TEST SUCCEEDED **
exit 0
```

The exact event positive and visible edited fixture thumb check both passed in that native run.
The unchanged full seven-test journeys on both sizes are the broader evidence above.
Fresh `cargo test --workspace` passed 615 tests with zero failures and two ignored tests.
Fresh `cargo clippy --workspace -- -D warnings` exited zero in 0.31 seconds.
The negative-case Xcode process stalled after its completed failing test.
Only that process was stopped after more than three minutes, preserving its log and partial bundle.

## Simulator timing diagnostic

The opt-in `--mote-timing-probe` is compiled only for the Apple iOS Simulator.
It does not change production defaults or the frozen Matrix contract.
The app starts from Layout and changes to Form using the real animation clock.
It never replaces `ctx.input.time`.
It then alternates Layout and Form every half second during the sustained interval.
The native test waits without screenshot polling during measurement.
The two Simulator sizes run sequentially to avoid competing during that interval.

Foundation resolves the app's Documents directory through `NSDocumentDirectory`.
The bridge copies no environment path and does not assume `HOME`.
Every launch creates a fresh directory with raw timestamps and original GPU PNGs.
The final diagnostic retains one warmup image and three transition images.
Compression and file writes occur after the measured interval, on a worker thread.
The assets here were exported explicitly from the app container located with
`xcrun simctl get_app_container <device> local.mote.simulator data`.

The diagnostic distinguishes four boundaries.

| Boundary | Measurement |
| --- | --- |
| App update | Unique natural input time, plus monotonic `Instant` milliseconds. Duplicate egui passes are excluded. |
| Renderer paint | A no-draw `egui_wgpu::CallbackTrait::paint` timestamps an actual render-pass encoding invocation. |
| GPU image | `ViewportCommand::Screenshot` returns an actual WGPU texture copy through `Event::Screenshot`. |
| Presentation and scanout | Unknown. The report stores null timestamps and makes no display scanout claim. |

The screenshot command is consumed on the following render pass in the pinned eframe backend.
A 32-bit red and green marker in each actual image independently identifies its rendered sequence.
The report retains the request sequence, decoded image sequence, later delivery time and actual
new-layout-relative update time.
The marker samples lie at three-point intervals near the upper-right safe area.
The literal-pixel decoder test rejects an absent marker.
The reported cadence is render-pass encoding cadence, not nominal display refresh rate.
The ten-second measurement window is 4000 through 14000 ms after launch.
Raw tuples in `app_updates` are sequence, monotonic milliseconds and natural input seconds.
Raw tuples in `render_pass_encodings` are sequence and monotonic milliseconds.
Summary tuples are measured fps, p95 interval milliseconds and maximum interval milliseconds.

Four diagnostic regressions failed before implementation, then passed.
They cover duplicate passes, prearmed capture timing, literal marker pixels and measured intervals.
The opt-in launch regression also failed before implementation, then passed.
All 16 iPad Rust tests passed after implementation.
Strict Apple Simulator target clippy passed with no warnings.

The Debug native diagnostic completed four runs and passed its completion test in 100.188 seconds.
This is not a passing transition-timing claim.
First-time GPU readback introduced a 346 ms update gap.
Its normal portrait image times were 0, 443.839 and 450.057 ms.
Other Debug runs produced midpoint images around 133 ms and final images around 150 ms.
The selected [Debug report](assets/IPAD-02/timing/debug-11-portrait/report.json) and all three GPU images
preserve that failed temporal trial.
The other Debug trials remain in `target/ipad-probe-debug-11`.

The first Release Xcode build failed before app launch because an uncached host build script
reported `ld: library 'System' not found`.
The existing locked Rust Release build succeeded outside the Xcode shell in 12.47 seconds.
The subsequent Xcode Release build succeeded without changing the app or build script.

```text
cargo build --locked -p mote-ios --target aarch64-apple-ios-sim --release
Finished release profile in 12.47s
exit 0

xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Release -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' -derivedDataPath target/ipad-release-derived -resultBundlePath target/ipad-task6-probe-release-11-retry.xcresult -only-testing:MoteUITests/MoteUITests/testOptInTimingProbeInBothOrientations test
Executed 1 test, with 0 failures in 99.309 seconds
** TEST SUCCEEDED **
exit 0
```

The Release measurements occurred on 2026-10-11, starting at 07:56 PDT.
They use the same SDK, OS, Rust and iPad runtimes recorded above.
The initial four 11-inch runs captured 0, approximately 66.7 and approximately 133.3 ms.
Their measured render-pass encoding cadence was approximately 60 fps.
The superseded successful initial Release exports remain in `target/ipad-probe-before-warmup`.
The final eight runs and both failed temporal trials are retained under `assets/IPAD-02/timing`.

The first 13-inch Release portrait trial exposed a cold capture-pipeline cost.
Its three image times were 0, 350.060 and 366.682 ms despite a later encoding cadence of 60.002 fps.
The [cold 13-inch report](assets/IPAD-02/timing/release-13-portrait/report.json) and its original images
are retained as a failed temporal trial.
The other three initial 13-inch runs captured 0, approximately 66.7 and approximately 133.3 ms.
The completion-only native test passed in 99.814 seconds.
That test result alone does not establish acceptable capture timing.

The probe now requests one warmup GPU image while Layout remains selected.
It waits for an actual `Event::Screenshot` with a valid decoded marker before arming the transition.
The two regressions for first-frame prewarming and the GPU completion barrier failed before the fix.
They passed after the fix, along with all 18 iPad Rust tests.
The warmup PNG and its update and delivery timestamps are retained separately in `warmup_capture`.
This is probe warmup, not a production performance fix.
No first-use cost is hidden or counted as successful transition timing.

The repeated native tests ran sequentially on 2026-10-11 from 08:06 through 08:11 PDT.
The 11-inch completion test passed in 100.287 seconds.
The 13-inch completion test passed in 100.015 seconds.

```text
xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Release -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' -derivedDataPath target/ipad-release-derived -resultBundlePath target/ipad-task6-probe-warm-11.xcresult -only-testing:MoteUITests/MoteUITests/testOptInTimingProbeInBothOrientations test
Executed 1 test, with 0 failures in 100.287 seconds
** TEST SUCCEEDED **
exit 0

xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Release -destination 'platform=iOS Simulator,id=E1982F50-C8EF-47E9-9854-715268304F2B' -derivedDataPath target/ipad-release-derived -resultBundlePath target/ipad-task6-probe-warm-13.xcresult -only-testing:MoteUITests/MoteUITests/testOptInTimingProbeInBothOrientations test-without-building
Executed 1 test, with 0 failures in 100.015 seconds
** TEST EXECUTE SUCCEEDED **
exit 0
```

The warmup-barrier reports show these measured results.

| Repeated Release run | Actual image times, ms | Render encoding fps | p95 interval, ms | Maximum interval, ms | Warmup delivered, ms after launch |
| --- | --- | --- | --- | --- | --- |
| [11 portrait](assets/IPAD-02/timing/warm-11-portrait/report.json) | 0, 66.680, 133.356 | 60.001 | 16.761 | 17.102 | 302.714 |
| [11 portrait Reduced Motion](assets/IPAD-02/timing/warm-11-portrait-reduced/report.json) | 0, 66.661, 133.321 | 60.000 | 16.736 | 16.785 | 310.760 |
| [11 landscape](assets/IPAD-02/timing/warm-11-landscape/report.json) | 0, 66.663, 133.337 | 60.001 | 16.745 | 16.982 | 347.367 |
| [11 landscape Reduced Motion](assets/IPAD-02/timing/warm-11-landscape-reduced/report.json) | 0, 66.659, 133.337 | 60.001 | 16.741 | 16.788 | 318.958 |
| [13 portrait](assets/IPAD-02/timing/warm-13-portrait/report.json) | 0, 66.680, 133.352 | 60.001 | 16.758 | 16.994 | 335.377 |
| [13 portrait Reduced Motion](assets/IPAD-02/timing/warm-13-portrait-reduced/report.json) | 0, 66.667, 133.336 | 60.001 | 16.737 | 16.890 | 314.414 |
| [13 landscape](assets/IPAD-02/timing/warm-13-landscape/report.json) | 0, 66.671, 133.341 | 60.001 | 16.756 | 17.066 | 325.696 |
| [13 landscape Reduced Motion](assets/IPAD-02/timing/warm-13-landscape-reduced/report.json) | 0, 66.674, 133.334 | 60.000 | 16.739 | 16.828 | 327.771 |

These are three images from the same natural transition in each run.
No frames from separate transitions are spliced together.
The final image is the first sampled frame after the 120 ms duration, at approximately 133 ms.
The original PNGs show new Form text immediately at the first frame.
Normal midpoint images show square-pixel borders moving to the new control bounds.
Normal final images show settled borders with no transition particles.
Reduced Motion swaps the layout immediately with no square particles.
Its outline is stronger at the first frame, fades by the midpoint and is absent at the final frame.
The unchanged navigation and composer retain their bounds throughout the captures.

The following independent raw-report check passed for all eight final runs.
The midpoint tolerance is 40 through 90 ms and the final tolerance is 120 through 150 ms.
The measured ten-second render cadence must be at least 60 fps with p95 at most 20 ms.
These bounds are diagnostic checks, not a substitute for presentation timestamps.

```sh
for report in docs/verification/assets/IPAD-02/timing/warm-*/report.json; do
  jq -e '(.captures | length) == 3 and .warmup_capture.decoded_render_sequence != null and .warmup_capture.gpu_event_received_ms < .transition_changed_at_ms and .captures[0].transition_elapsed_ms == 0 and .captures[1].transition_elapsed_ms >= 40 and .captures[1].transition_elapsed_ms <= 90 and .captures[2].transition_elapsed_ms >= 120 and .captures[2].transition_elapsed_ms <= 150 and .render_encoding_cadence_fps_p95_ms_max_ms[0] >= 60 and .render_encoding_cadence_fps_p95_ms_max_ms[1] <= 20 and .present_timestamps == null and .scanout_timestamps == null' "$report"
done
```

Output was eight `true` lines with exit zero.

The read-only [pixel comparison](../../ios/compare-probe-frames.swift) uses the original GPU images.
It defines red as R greater than 20, R greater than twice G and R greater than twice B.
It excludes the top and bottom 400 physical pixel rows from each two-times-scale image.
That central region excludes the diagnostic marker, navigation and composer.
It counts red pixels outside an eight-pixel dilation of the final red border.
It also sums the red-channel intensity over red pixels to quantify the fading outline.
The final-frame outside count is zero by construction and is not independent evidence.
The first and midpoint comparisons are independent of that final mask.

```sh
xcrun swiftc -O -module-cache-path target/ipad-probe-swift-cache ios/compare-probe-frames.swift -o target/ipad-probe-compare
target/ipad-probe-compare docs/verification/assets/IPAD-02/timing/warm-*
```

```text
warm-11-landscape red=[46518, 34001, 17873] outsideFinalBorder8px=[28370, 0, 0] redEnergy=[4532422, 4317204, 2241537]
warm-11-landscape-reduced red=[24065, 24026, 17873] outsideFinalBorder8px=[0, 0, 0] redEnergy=[4337790, 3212645, 2241537]
warm-11-portrait red=[40644, 34390, 18204] outsideFinalBorder8px=[22352, 2, 0] redEnergy=[3938120, 4350545, 2275751]
warm-11-portrait-reduced red=[24396, 24357, 18204] outsideFinalBorder8px=[0, 0, 0] redEnergy=[4372008, 3246857, 2275751]
warm-13-landscape red=[50502, 35330, 19201] outsideFinalBorder8px=[31026, 1, 0] redEnergy=[4946758, 4455241, 2379649]
warm-13-landscape-reduced red=[25393, 25354, 19201] outsideFinalBorder8px=[0, 0, 0] redEnergy=[4475902, 3350757, 2379649]
warm-13-portrait red=[42246, 32577, 16449] outsideFinalBorder8px=[25522, 0, 0] redEnergy=[4088134, 4169051, 2093441]
warm-13-portrait-reduced red=[22641, 22602, 16449] outsideFinalBorder8px=[0, 0, 0] redEnergy=[4189694, 3064549, 2093441]
exit 0
```

Normal first images have 22352 through 31026 red pixels away from the final border region.
Normal midpoint images have more red border pixels than the settled final image.
All Reduced Motion first and midpoint images have zero red pixels away from that region.
Their red-channel energy strictly decreases across the first, midpoint and final images.
Those measurements support particles in the normal transition and an outline-only fading pulse
in Reduced Motion within the analyzed region.
The original full-frame PNGs remain available for independent inspection.

Final verification on 2026-10-11 from 08:14 through 08:15 PDT used the environment recorded above.

```text
cargo test --workspace
53 test result summaries: 622 passed, 0 failed, 2 ignored
exit 0

cargo clippy --workspace -- -D warnings
Finished dev profile in 0.47s
exit 0

cargo check -p mote-ios --target aarch64-apple-ios-sim
Finished dev profile in 10.55s
exit 0

cargo clippy -p mote-ios --target aarch64-apple-ios-sim -- -D warnings
Finished dev profile in 1.01s
exit 0
```

## Presentation-timing boundary and acceptance decision

The core transition regression suite runs as part of the workspace tests.
It covers changed square border particles, unchanged structural identity, immediate new hit targets,
120 ms completion and the immediate Reduced Motion outline pulse.
Those controlled Rust frames are not a measurement of Simulator presentation timing.

A real Simulator capture was attempted with:

```text
xcrun simctl io 43F2F6ED-5C86-4121-B03F-40FE38891C4E recordVideo --codec=h264 /private/tmp/mote-task6-transition.mp4
Recording started
Recording completed. Writing to disk.
Wrote video to: /private/tmp/mote-task6-transition.mp4

ffprobe -v error -show_entries stream=r_frame_rate,avg_frame_rate,nb_frames,duration,width,height -of default=noprint_wrappers=1 /private/tmp/mote-task6-transition.mp4
width=1668
height=2420
r_frame_rate=600/1
avg_frame_rate=1415/141
duration=84.600000
nb_frames=849
```

The recording is variable rate.
The nominal timestamp denominator is not a measured 600 fps.
The approximately 10 fps whole-recording average includes idle frames.
Short timestamp intervals include repeated surface commits and cannot prove sustained 60 fps.
The capture remains local in `/private/tmp`.
The newer opt-in probe provides actual GPU images and about 60 render-pass encodings per second.
It also provides a temporally resolved native Reduced Motion outline pulse.
It does not provide `SurfaceTexture::present` completion or actual scanout timestamps.
The final GPU sample is approximately 133 ms rather than an exact 120 ms image.
No stronger presented-frame timing claim is made.

The driver accepts IPAD-02 on 2026-10-11 after independent review. The controlled renderer
regression proves the border has settled at exactly 120 ms and remains identical afterward.
The native 60 Hz paint series has samples around 116.7 and 133.3 ms, so 120 ms falls between
available frames. The captured first, midpoint and first post-120 ms final GPU images come from
one transition. They show the expected particle progression, settled border and Reduced Motion
outline-only pulse. Eight native reports verify renderer encoding cadence at or above 60 fps.
The two-device interaction suites verify every requested page, control, scroll and text-entry
path in both orientations. This combination covers the README Verify clause without asserting
unmeasured display scanout latency or an exact 120 ms native screenshot.

There is no physical touch, privacy-prompt, VoiceOver, signing or TestFlight evidence here.
