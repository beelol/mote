# IPAD-05 verification

Date: 2026-10-11.
Status: Verified. The paired native journey, source gates and harness commit passed.

## Environment and isolation

The live client ran in the iPad Pro 11-inch (M5) Simulator with iOS 27.0.
The Mac ran macOS 26.6.2, Xcode 27.0 and Rust 1.89.0.
The browser was Google Chrome for Testing 155.0.8059.39.
The test fixture listened only on a temporary `127.0.0.1` port.
The Mac host used a new temporary host identity, database and per-character Mote browser profile.
The native host asserted `--user-data-dir=<temporary data>/browser/mote/local`, `--remote-debugging-address=127.0.0.1`, `--remote-debugging-port=0` and `--use-mock-keychain` against the actual Chrome process arguments.
The headed runtime log independently records an isolated `<temporary data>/browser/test/local` profile with the same loopback and mock-keychain flags.
The process did not use the default Chrome profile.
The test did not load or pull a model.
The deterministic browser journey wrote zero model ledger rows and zero packs.

## Commands and output

The headed fixtures ran with these commands. `MOTE_CHROME` pointed to the pinned Chrome for Testing executable in each command.

```text
MOTE_LIVE=1 MOTE_CHROME='/private/tmp/mote-browser-runtime-155/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing' cargo test -p mote-browser --test runtime -- --ignored --nocapture
MOTE_LIVE=1 MOTE_CHROME='/private/tmp/mote-browser-runtime-155/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing' cargo test -p moted --test browser_fixture -- --ignored --nocapture
```

Their final outputs are [headed-runtime.log](artifacts/ipad-browser-2026-10-11/headed-runtime.log) and [headed-daemon.log](artifacts/ipad-browser-2026-10-11/headed-daemon.log).
The runtime suite passed eight headed cases.
The daemon suite passed five headed cases twice.

The native journey used this command:

```text
MOTE_LIVE=1 MOTE_CHROME='/private/tmp/mote-browser-runtime-155/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing' MOTE_SIMULATOR_ID=43F2F6ED-5C86-4121-B03F-40FE38891C4E MOTE_ARTIFACTS=/private/tmp/mote-ipad-browser-20261011-1240-11 cargo test -p mote-ios --test browser_approval native_paired -- --ignored --nocapture
```

It built and installed the native Xcode app, paired it through the real localhost Noise gateway, and ran `MoteUITests/testBrowserApprovalDenyApproveRepeatAndNaturalExpiry`.
The [native harness](artifacts/ipad-browser-2026-10-11/native-harness.log) passed in 185.34 seconds.
The [XCUITest](artifacts/ipad-browser-2026-10-11/native-xctest.log) passed one test with zero failures in 156.228 seconds.
The XCUITest used actual Simulator taps for Deny and two rapid Approve taps.
It relaunched the app while the first request was pending.

The final source gates ran after the native harness commit `cd2ff0f`:

```text
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo check -p mote-ios --target aarch64-apple-ios-sim
```

The [workspace test output](artifacts/ipad-browser-2026-10-11/workspace.log) records 709 passed, zero failed and 20 ignored across 64 result summaries.
The [strict clippy output](artifacts/ipad-browser-2026-10-11/clippy.log) and [Apple Simulator check](artifacts/ipad-browser-2026-10-11/apple-check.log) both exited zero.

## Browser boundary

The runtime test covers stable references, snapshot, read, fill, stale-reference refusal and page-state revalidation.
It checks that a full navigation and a same-document navigation invalidate pending authority.
The paired daemon tests cover delivery grants, incorrect host, thread and action bindings, duplicate decisions, worker restart and file-write approval coexistence.
The live fixture covers submit, purchase, message, download, OAuth and non-GET triggers.
The fixture base was a temporary `http://127.0.0.1:<port>` origin. Its tested paths included `/read`, `/navigation`, `/injection`, `/redirect`, `/off-origin`, `/trigger/<effect>` and `/effect/<effect>`.
Every protected trigger created a durable card while its fixture effect remained zero.
The fixture also refused an unapproved origin and off-origin redirect.
Malformed and unknown outward-action verdicts classify as protected commits in the policy test.
The injected page text stayed inside an untrusted-page boundary and did not authorize a purchase.
The browser exposes named controls through the daemon and MCP bridge.
It does not expose raw CDP or arbitrary JavaScript execution.

The native journey used the stable snapshot reference to read and fill a local form.
It refused the reference after navigation, as recorded in [stale-ref.json](artifacts/ipad-browser-2026-10-11/stale-ref.json).
The denied request had zero fixture effects.
The approved request made exactly one POST despite the repeated tap.
The third request expired naturally after 127,357 milliseconds with no additional effect.
No clock or database value was changed to produce this expiry.
The [effect record](artifacts/ipad-browser-2026-10-11/effects.json) and [expiry record](artifacts/ipad-browser-2026-10-11/natural-expiry.json) retain these counts and timing.

The [event log](artifacts/ipad-browser-2026-10-11/events.json) contains three `approval.created` and three `approval.updated` events.
Each native card has one terminal status: denied, approved or expired.
It contains 22 `browser_step` events and one `browser_step.completed` event.
The [ledger output](artifacts/ipad-browser-2026-10-11/ledger.txt) contains one `browser.approved_request|ok` row and no model ledger rows or packs.
The host validated and committed the final Matrix surface.
The paired client retained that same surface sequence and displayed the observed POST, site and HTTP 200 response.

The [HAR](artifacts/ipad-browser-2026-10-11/network.har) contains the approved POST with HTTP 200.
Its request body is `[redacted]`.
Its cookies and query strings are empty.
Authorization headers, the fixture secret and the filled form text are absent from the HAR and stored events.
The browser saved [before](artifacts/ipad-browser-2026-10-11/browser-before.png) and [after](artifacts/ipad-browser-2026-10-11/browser-after.png) captures.
The page looks unchanged because the approved effect is the fixture's server-side counter.

## Simulator captures and cleanup

The native captures show [pending](artifacts/ipad-browser-2026-10-11/pending-deny.png), [pending after relaunch](artifacts/ipad-browser-2026-10-11/pending-reconnected.png), [denied](artifacts/ipad-browser-2026-10-11/denied.png), [pending approval](artifacts/ipad-browser-2026-10-11/pending-approve.png), [approved with the Matrix result](artifacts/ipad-browser-2026-10-11/approved-matrix-result.png), [pending expiry](artifacts/ipad-browser-2026-10-11/pending-expiry.png) and [expired](artifacts/ipad-browser-2026-10-11/expired.png).
The pending card shows the host, local site, protected action class and two touch controls.
The terminal cards have no decision controls.
The expired view still shows the prior validated Matrix result.

Xcode replaced the app data container during an earlier successful UI run.
That run's Rust evidence check used the old container path and failed.
The test recovered all three prior Documents files byte-for-byte in the current container and confirmed deletion of its temporary Keychain credential.
The final passing run re-resolved the current bundle container before reading state and during cleanup.
It restored the prior files and recorded the temporary credential as missing in [credential-deletion.json](artifacts/ipad-browser-2026-10-11/credential-deletion.json).
No pairing offer, private credential, persisted client state, typed form snapshot, host database or Xcode build directory is checked in.
No physical iPad, signing, App Store Connect or TestFlight action was taken.
