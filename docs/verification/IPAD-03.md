# IPAD-03 verification

Date: 2026-10-11, America/Los_Angeles.
Status: verified after independent recovery review and final native rerun.
The native Simulator completes the README Verify journey below.
The README checkbox is closed by the evidence commit.

## Environment

macOS 26.6.2, build 25G83, Apple Silicon.
Rust 1.89.0, commit 29483883e, LLVM 20.1.7, Rust 2021.
Xcode 27.0, build 27A266a, iPhoneSimulator 27.0 SDK.
Device: iPad Pro 11-inch (M5), iOS 27.0.
UDID: `43F2F6ED-5C86-4121-B03F-40FE38891C4E`.
Viewport: 834 by 1210 points, portrait.
Base: `33e52a6`.
Shared command-channel foundation: `9cb9d94`.
Native integration: `4d74826`.
Dependencies are pinned in Cargo.lock.
No physical iPad, Apple account, provisioning profile, distribution signing or TestFlight is used.
Local socket, Bonjour and Simulator commands required approved execution outside the filesystem sandbox.

## Native live journey

The harness starts two independent real moted loopback hosts and one impersonator.
Each host has a newly generated cryptographic identity.
Both genuine hosts use thread `main`, surface sequence 1 and the same `models.talk` binding.
The hosts commit their dashboards through the ordinary Matrix parser and compiler.
The controlled local model HTTP fixture returns deterministic streamed chat content.
This test verifies transport and shell behavior, not model quality or IPAD-04.
There is no hard-coded host key and no loopback Noise bypass.
The short-lived MOTE1 offers enter the app through the Simulator launch environment.
The harness confirms the pending device on each host.
The saved host metadata deliberately omits pairing offers and private credentials.

Commands:

```text
xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -destination 'id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' -derivedDataPath /private/tmp/mote-task7-derived build
xcrun simctl install 43F2F6ED-5C86-4121-B03F-40FE38891C4E /private/tmp/mote-task7-derived/Build/Products/Debug-iphonesimulator/Mote.app
MOTE_LIVE=1 MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E MOTE_ARTIFACTS=/private/tmp/mote-task7-live-attempt8 cargo test -p mote-ios --test simulator_live -- --ignored --nocapture
```

The harness runs this XCUITest command from the repository root:

```text
xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -destination id=43F2F6ED-5C86-4121-B03F-40FE38891C4E -derivedDataPath /private/tmp/mote-task7-derived -resultBundlePath /private/tmp/mote-task7-live-attempt8/native.xcresult -only-testing:MoteUITests/MoteUITests/testLivePairedHostsChatDashboardAndRelaunch test
```

Output:

```text
** BUILD SUCCEEDED **
Executed 1 test, with 0 failures (0 unexpected)
** TEST SUCCEEDED **
Simulator paired two hosts, completed native chat and typed dashboard edit, resumed cursor without duplicates, moved 127.0.0.1:61551 to 127.0.0.1:61630, rejected 127.0.0.1:61553, restored and deleted native Keychain credentials. Home cursor 1 -> 12.
test native_pair_chat_edit_relaunch_discover_and_keychain_delete ... ok
test result: ok. 1 passed; 0 failed; 0 ignored
exit 0
```

The native run finishes in 64.66 seconds.
Home's identity is `ac92420fa323d2ea83ac86d1750a4237aa34f36a38c27809c99d998a61721a65`.
Office's identity is `72a1ba1927e1a241788d544db1e2cab7e271232212c7d2e39c786547d77462da`.
Office stays at `127.0.0.1:61552`.
The impersonator advertises Home's fingerprint but has a different Noise key.
Home moves to `127.0.0.1:61630` only after the pinned identity handshake succeeds.

XCUITest selects `qwen3:0.6b` on Home's committed dashboard.
The host acknowledges the typed change and commits the updated surface.
The test enters `hello` and taps the visible Send button.
It observes both the user message and streamed assistant reply.
It terminates and relaunches the app without a new offer.
It observes the edited dashboard and existing chat again.
It selects Office and verifies that Home's chat is absent.
Office retains its own dashboard and original `qwen3:4b` value.
It selects Home again and verifies that Office's dashboard is absent.

The durable event log has exactly one `turn.user`, one typed `ui.event` and one ledger-linked `talk.done`.
The saved Home state contains exactly one `hello` message.
Office contains no `hello` message.
Home's cursor advances from 1 to 12 and is restored at 12 after relaunch.
No unacknowledged action is automatically replayed on relaunch.
The authoritative snapshot and shared reducer suppress old replayed events.

## Real Simulator Keychain

The native bridge stores credentials under service `local.mote.paired-host` and cryptographic HostId account.
The item uses `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`.
The credential counter is durably advanced before each authenticated session attempt.
Private device keys are not stored in Documents or diagnostic reports.

The first native attempt returned Keychain status -34018.
The linker-only application signature lacked the required application identifier entitlement.
The controller authorized a Simulator-only local ad-hoc stamp.
The checked-in Simulator.entitlements contains only `application-identifier` and `keychain-access-groups` for `local.mote.simulator`.
Xcode embeds the simulated entitlement section and stamps the app locally:

```text
/usr/bin/codesign --force --sign - --entitlements /tmp/mote-task7-derived/Build/Intermediates.noindex/Mote.build/Debug-iphonesimulator/Mote.build/Mote.app.xcent --timestamp=none --generate-entitlement-der /tmp/mote-task7-derived/Build/Products/Debug-iphonesimulator/Mote.app
Signing Identity: "Sign to Run Locally"
```

All added build settings are conditional on `sdk=iphonesimulator*`.
Command: `xcodebuild -project ios/Mote/Mote.xcodeproj -target Mote -sdk iphoneos -showBuildSettings`.
The physical settings still report `CODE_SIGNING_ALLOWED = NO`, no entitlement file and `SUPPORTED_PLATFORMS = iphonesimulator`.
No team, Apple ID, certificate or provisioning profile is selected.

The relaunched app reports `keychain_restored` for Home with saved cursor 12 and for Office with saved cursor 1.
The harness then relaunches with `SIMCTL_CHILD_MOTE_SIMULATOR_KEYCHAIN_DELETE` set to Home's HostId.
The actual native SecItemDelete operation succeeds.
An immediate SecItemCopyMatching returns absence.
The report records `keychain_deleted` with `missing: true`.
The screen requires a new owner-present pairing and disables the stale dashboard controls.
This is a real Simulator Keychain lifecycle check, not the in-memory test store.

## Test-first and compatibility checks

The initial live-loopback test failed because the shared command/update and live adapter APIs were absent.
The moved-port command-session assertion later failed before shared endpoint propagation was implemented.
The Unix replacement-host test failed before the adapter rechecked identity on each local connection.
The shell refusal-isolation test failed before status messages were keyed by HostId.
These regressions now pass.

Commands:

```text
cargo test -p mote-ios --test live_loopback --test security -- --nocapture
cargo test -p mote-client unix_and_fixture_frames
cargo test -p mote-ios --lib live_shell_labels
cargo check -p mote-ios --target aarch64-apple-ios-sim
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

The focused real-daemon suite passes 2 tests.
The credential suite passes 2 tests.
The fixture/Unix parity test and host-isolated shell test pass.
The real-daemon test revokes the paired device in moted's actual DeviceStore and rejects a new Noise session.
It also proves that a typed model change creates no generation call.
Model inventory `/api/tags` is counted separately from `/api/chat` and `/api/generate`.
The Apple Silicon Simulator target check exits 0.
The supplementary `cargo check -p mote-ios --target aarch64-apple-ios` also exits 0 in 1 minute 20 seconds.
This is a Rust compile check, not a physical-device run or signing claim.
Workspace run 5 reports 638 passed, 3 intentionally ignored live checks and no failures across 57 suites.
Workspace run 6 reports 640 passed, 3 ignored and no failures across 57 suites.
It precedes the native integration commit with strict clippy run 9 also exiting 0.
Strict workspace clippy exits 0.
The workspace second-instance fixture had one transient startup timeout and passed its focused retry and the subsequent full run.

The Mac shell retains its existing StreamUpdate timing and replay-watermark projection.
Its chat and typed event request helpers use the shared host-bound command worker.
Fixture and Unix events reduce to identical HostDirectory state in the parity test.
Transport work, Bonjour browsing, Keychain calls and durable state writes run off the render thread.

## Evidence files

The [artifact directory](artifacts/ipad-live-2026-10-11/) contains exact logs, public host metadata, durable state and redacted daemon events.
The native result bundle remains at `/private/tmp/mote-task7-live-attempt8/native.xcresult`.
Command: `xcrun xcresulttool export attachments --path /private/tmp/mote-task7-live-attempt8/native.xcresult --output-path /private/tmp/mote-task7-live-attempt8/attachments`.
It exported 6 named native screenshots.
All six and the Keychain deletion screenshot were visually inspected.

| Native capture | Observable result |
| --- | --- |
| [Home paired](artifacts/ipad-live-2026-10-11/home-paired.png) | Online, cursor 1, committed Home dashboard. |
| [Typed edit](artifacts/ipad-live-2026-10-11/typed-edit.png) | Host acknowledgement, cursor 4, updated local model. |
| [Chat](artifacts/ipad-live-2026-10-11/chat.png) | One user message and one streamed fixture reply, cursor 12. |
| [Relaunched](artifacts/ipad-live-2026-10-11/relaunched.png) | Online, cursor 12, preserved chat and edited dashboard. |
| [Office](artifacts/ipad-live-2026-10-11/office.png) | Separate cursor 1 and dashboard with no Home chat. |
| [Home restored](artifacts/ipad-live-2026-10-11/home-restored.png) | Home's cursor, chat and edited dashboard return. |
| [Credential deleted](artifacts/ipad-live-2026-10-11/credential-deleted.png) | New pairing required, Home offline, stale controls disabled. |

The native harness preserves displaced prior Documents files in its `prior-live-state` artifact directory.
It leaves the final ephemeral Home credential deleted and the final app screen offline.
Existing offline gallery tests explicitly launch with `--mote-offline`.
No default Chrome profile or browser credential is opened.
Physical privacy prompts, sleep/wake, model quality and browser approval delivery remain outside this criterion.

## Recovery review verification

Date: 2026-10-11, 09:55 to 09:56 PDT.
Environment: the same iPad Pro 11-inch (M5), iOS 27.0, Xcode 27.0 and macOS environment above.
The review fixes invalidate stale command connections after same-address restarts.
Indeterminate submitted commands return their error and are not replayed automatically.
Saved hosts unavailable at app launch retry with a pinned identity and 250-millisecond to 5-second capped backoff.
The command receiver is handed off only after authenticated attach succeeds.
The Mac shell retains one shared Unix host pin across actions and stream reconnects.
Its local HostDirectory projection is explicitly ephemeral and cannot serialize as remote paired-host credentials.
The Unix client authorizes the server peer UID before learning hello identity.
The existing Mac arrival timestamps, ChatState delivery and replay-watermark semantics are unchanged.

Commands and output:

```text
cargo test -p mote-ios --test live_loopback same_address
1 passed; 0 failed
cargo test -p mote-ios saved_host_recovers
1 passed; 0 failed; finished in 2.35s
cargo test -p mote-client shell_session_rejects
1 passed; 0 failed
cargo test -p mote-client pinned_shell_stream
1 passed; 0 failed
cargo test --workspace
exit 0, all final doctests completed
cargo clippy --workspace -- -D warnings
exit 0, finished in 19.15s
cargo check -p mote-ios --target aarch64-apple-ios-sim
exit 0, finished in 21.11s
xcodebuild -project ios/Mote/Mote.xcodeproj -scheme Mote -destination 'id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' -derivedDataPath /private/tmp/mote-task7-derived build
** BUILD SUCCEEDED **
xcrun simctl install 43F2F6ED-5C86-4121-B03F-40FE38891C4E /private/tmp/mote-task7-derived/Build/Products/Debug-iphonesimulator/Mote.app
exit 0
MOTE_LIVE=1 MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E MOTE_ARTIFACTS=/private/tmp/mote-task7-live-attempt9 cargo test -p mote-ios --test simulator_live -- --ignored --nocapture
** TEST SUCCEEDED **
Simulator paired two hosts, completed native chat and typed dashboard edit, resumed cursor without duplicates, moved 127.0.0.1:57459 to 127.0.0.1:58022, rejected 127.0.0.1:57461, restored and deleted native Keychain credentials. Home cursor 1 -> 12.
1 passed; 0 failed; finished in 69.80s
exit 0
xcrun xcresulttool export attachments --path /private/tmp/mote-task7-live-attempt9/native.xcresult --output-path /private/tmp/mote-task7-live-attempt9/attachments
exit 0, six native attachments exported
```

The exact logs and regenerated public state are in [review artifacts](artifacts/ipad-live-review-2026-10-11/).
The full workspace log is `mote-task7-workspace12.log`.
The strict clippy log is `mote-task7-clippy15.log`.
Earlier failing red logs are included beside their green regressions.
One earlier workspace run hit the existing second_instance startup timeout.
Its focused retry and the final full workspace run passed.
The review native run uses new generated identities.
Home is `de5483a58111ab957af962e24432494f2519570d9c8caab813614dae3072ca71`.
Office is `cbfb08de29f0f0f96c0a1718a993b6919c7e48261d43c6b028caed7398437178`.
The network log proves Home's Keychain credential restored at saved cursor 12 and later returned missing after actual native deletion.
No offers or private credential material are exported.
All seven review screenshots were visually inspected.

| Native review capture | Observable result |
| --- | --- |
| [Home paired](artifacts/ipad-live-review-2026-10-11/home-paired.png) | Online, cursor 1, committed Home dashboard. |
| [Typed edit](artifacts/ipad-live-review-2026-10-11/typed-edit.png) | Cursor 4, host accepted interaction, Talk model changed. |
| [Chat](artifacts/ipad-live-review-2026-10-11/chat.png) | One user hello and one fixture reply, cursor 12. |
| [Relaunched](artifacts/ipad-live-review-2026-10-11/relaunched.png) | Online at cursor 12, chat and edited dashboard retained. |
| [Office](artifacts/ipad-live-review-2026-10-11/office.png) | Separate cursor 1 and unedited dashboard, no Home chat. |
| [Home restored](artifacts/ipad-live-review-2026-10-11/home-restored.png) | Original Home chat and edited dashboard restored. |
| [Credential deleted](artifacts/ipad-live-review-2026-10-11/credential-deleted.png) | Home offline, new owner-present pairing required, stale controls disabled. |

## Final reviewed-source verification

Date: 2026-10-11, 10:22 to 10:23 PDT.
Environment: the same native iPad Pro 11-inch Simulator and toolchain listed above.
Source HEAD: `6131d0c`, with Task 7 recovery tip `8003027`.
The independent review approved all three recovery findings without new breakage.
Same-address recovery also invalidates commands after a fresh authenticated event subscription when Bonjour is unavailable.
The focused regression first failed with a stale WebSocket reset and then passed both recovery variants.
The command is not automatically resubmitted after an indeterminate failure.

Commands and final output:

```text
MOTE_LIVE=1 MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E MOTE_ARTIFACTS=/private/tmp/mote-task7-live-attempt10 cargo test -p mote-ios --test simulator_live -- --ignored --nocapture
Executed 1 test, with 0 failures (0 unexpected) in 44.342 seconds
** TEST SUCCEEDED **
Simulator paired two hosts, completed native chat and typed dashboard edit, resumed cursor without duplicates, moved 127.0.0.1:62723 to 127.0.0.1:62792, rejected 127.0.0.1:62725, restored and deleted native Keychain credentials. Home cursor 1 -> 12.
test result: ok. 1 passed; 0 failed; 0 ignored; finished in 80.71s
exit 0
xcrun xcresulttool export attachments --path /private/tmp/mote-task7-live-attempt10/native.xcresult --output-path /private/tmp/mote-task7-live-attempt10/attachments
Exported 6 attachments
exit 0
```

The harness rebuilds the Rust Simulator target and native app before this XCUITest.
The final generated Home identity is `952ccc20a0c663433b2fec355fe7e2bd357c646650f24e0ed05cdb719e9b19ea`.
The final Office identity is `7a67a5f31d1db291cc29ac9c12458bd59568b9766bfa53cccfd95aee15540ed0`.
Office stays at `127.0.0.1:62724` and cursor 1.
Home commits its typed change at surface sequence 4 and restores cursor 12 after app termination and relaunch.
The durable Home state has one user hello and one assistant reply.
Office has no Home message and retains its unedited dashboard.
The native Keychain report records Home restoration at saved cursor 12 and actual deletion with `missing: true`.
All seven final screenshots were directly inspected.
The exact output, redacted daemon events, public host state and native discovery report are in [final artifacts](artifacts/ipad-live-final-2026-10-11/).
The result bundle remains at `/private/tmp/mote-task7-live-attempt10/native.xcresult`.
No pairing offer, private credential or displaced prior Documents state is checked in.

| Final native capture | Observable result |
| --- | --- |
| [Home paired](artifacts/ipad-live-final-2026-10-11/home-paired.png) | Home online, cursor 1, committed dashboard. |
| [Typed edit](artifacts/ipad-live-final-2026-10-11/typed-edit.png) | Host accepted the typed change, cursor 4, updated model value. |
| [Chat](artifacts/ipad-live-final-2026-10-11/chat.png) | One user hello and one fixture reply at cursor 12. |
| [Relaunched](artifacts/ipad-live-final-2026-10-11/relaunched.png) | Home online at cursor 12 with unchanged chat and edited dashboard. |
| [Office](artifacts/ipad-live-final-2026-10-11/office.png) | Office online at cursor 1, unedited dashboard and no Home chat. |
| [Home restored](artifacts/ipad-live-final-2026-10-11/home-restored.png) | Home chat and edited dashboard restored after host switching. |
| [Credential deleted](artifacts/ipad-live-final-2026-10-11/credential-deleted.png) | Home offline, owner-present pairing required and stale controls disabled. |

Final pre-commit gates ran on the controller-held source after the native journey:

```text
cargo test --workspace > /private/tmp/mote-task7-workspace17.log 2>&1
exit 0, final mote_ui doctest completed
667 passed; 0 failed; 7 intentionally ignored live checks; 60 suites
cargo clippy --workspace -- -D warnings > /private/tmp/mote-task7-clippy20.log 2>&1
exit 0, finished in 0.25s
```

Both exact logs are included in the final artifact directory.
