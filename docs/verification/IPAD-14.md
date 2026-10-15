# IPAD-14 verification in progress

Date: 2026-10-14, America/Los_Angeles.

The reusable bridge is implemented in `mote-client-core::native_text`. The iOS shell uses one
transparent `UITextField` over the active egui field. Egui remains the visual renderer. The native
field supplies first-responder behavior, the system keyboard, UIKit selection and edit menus,
marked-text composition, dictation and hardware-keyboard events. UTF-16 selection offsets are
converted at the Rust boundary. Unconsumed native updates and accessibility focus requests expire
after the rendered frame. Return events are accepted only for the currently focused field.

All production single-line fields in the shared Matrix renderer, the offline gallery, first-run
pairing and live chat use the shared bridge. The only remaining direct `TextEdit` construction is in
the theme test that checks selection contrast.

## Focused Rust verification

Command:

```text
cargo test -p mote-client-core native_text
cargo test -p mote-ios native_text
cargo test -p mote-ios native_text_focus_targets_any_generated_field_id
```

Output: six shared bridge tests passed. Five iOS bridge tests passed. The generated-field focus test
passed. The tests cover UTF-16 emoji selection, reversed selection, 48-point hit bounds, applied
native edits, published active state, frame expiry, invalid and empty callbacks, targeted Return and
an arbitrary generated Matrix field identity.

## iPad Simulator verification

Environment: iPad Pro 11-inch (M5) Simulator, device
`43F2F6ED-5C86-4121-B03F-40FE38891C4E`, iOS 27.0, Xcode 27.0.

Command:

```text
xcodebuild test -project ios/Mote/Mote.xcodeproj -scheme Mote -configuration Debug \
  -destination 'platform=iOS Simulator,id=43F2F6ED-5C86-4121-B03F-40FE38891C4E' \
  -derivedDataPath /private/tmp/mote-native-text-test14 \
  -resultBundlePath /private/tmp/mote-native-text-test14.xcresult \
  CODE_SIGNING_ALLOWED=NO \
  -only-testing:MoteUITests/MoteUITests/testNativeTextBridgeSupportsEdgeFocusHardwareEditingAndEditMenu
```

Output: the selected test passed in 26.264 seconds. It tapped the right edge of the 48-point field,
observed the system keyboard, typed `abcd`, moved the cursor left with a hardware-key event, inserted
`X` to produce `abcXd`, and exercised the native Paste, Copy and Cut menu items. Cut cleared the
field. A long press exposed Paste and restored `pasted`. The result bundle contains the attachment
`native-text-edit-menu`.

## Physical iPad verification

Environment: iPad Pro 11-inch (M4), UDID `00008132-001104661AB9001C`, iPadOS 26.5, Xcode 27.0.

The signed Debug app built successfully with team `FQ6YGD7554`, bundle ID `com.beelol.mote`, and the
Mote development profile. `xcrun devicectl device install app` installed it successfully. A separate
Mote-only test identity `com.beelol.mote.xctrunner` and development profile
`Mote UI Tests Development`, profile ID `4DQ37J5P2N`, were created without changing another app.
The signed physical UI test runner built successfully with that profile.

The physical test is waiting for the iPad to be unlocked. Marked-text composition and dictation
still require observed runs on the physical device. IPAD-14 remains unchecked until those checks and
the queued physical flow pass.
