use matrix_lang::lower_v1::ControlContract;
use matrix_lang::manifest::ActionContract;
use matrix_lang::{lower_v1, parse_surface, resolve, BindingSnapshot, Manifest, Type};
use serde_json::json;

#[test]
fn voice_surface_lowers_status_and_toggle_through_the_public_matrix_contract() {
    let source = include_str!("../../matrix/examples/voice-status.matrix");
    let mut manifest = Manifest::default();
    let mut snapshot = BindingSnapshot::default();
    for (name, value) in [
        ("voice.label", "Hearing you"),
        ("voice.detail", "Transcribing locally."),
        ("voice.partial", "book the flight"),
        ("voice.speaking", ""),
    ] {
        manifest.values.insert(name.into(), Type::String.into());
        snapshot.values.insert(name.into(), json!(value));
    }
    manifest
        .actions
        .insert("voice_toggle".into(), ActionContract::default());

    let parsed = parse_surface(source).unwrap();
    let resolved = resolve(&parsed, &manifest, &snapshot).unwrap();
    let lowered = lower_v1(&resolved).unwrap();
    lowered.surface.validate().unwrap();

    assert_eq!(
        lowered.controls.get(&vec![0, 0, 1]),
        Some(&ControlContract::Button {
            event: "voice_toggle".into(),
            disabled: false,
        })
    );
    let encoded = serde_json::to_value(lowered.surface).unwrap();
    assert_eq!(
        encoded["root"]["children"][0]["children"][0]["children"][0]["value"],
        "Hearing you"
    );
    assert_eq!(
        encoded["root"]["children"][1]["children"][1]["value"],
        "book the flight"
    );
    assert!(!encoded.to_string().contains("model_path"));
    assert!(!encoded.to_string().contains("pcm"));
}
