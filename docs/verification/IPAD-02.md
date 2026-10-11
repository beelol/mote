# IPAD-02: Offline iPad Simulator gallery

Status: partial. The checkbox remains open.
Date: 2026-10-11, America/Los_Angeles.
Base: `1e47f41` on `main`.

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

## Transition evidence gap

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
No first, approximately 60 ms and 120 ms Simulator frame sequence at verified 60 fps is claimed.
No actual presented-frame timing measurement or temporally resolved Reduced Motion pulse is claimed.
This missing temporal evidence keeps IPAD-02 open.

There is no physical touch, privacy-prompt, VoiceOver, signing or TestFlight evidence here.
