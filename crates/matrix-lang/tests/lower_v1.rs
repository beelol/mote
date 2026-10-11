use matrix_lang::lower_v1::ControlContract;
use matrix_lang::manifest::{ActionContract, EnumDefinition, FieldDefinition};
use matrix_lang::{lower_v1, parse_surface, resolve, BindingSnapshot, Manifest, Type};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn fixture() -> (Manifest, BindingSnapshot) {
    let mut manifest = Manifest::default();
    manifest
        .enums
        .insert("Mode".into(), EnumDefinition::new(["quiet", "normal"]));
    for (name, ty, initial) in [
        ("models.local_only", Type::Boolean, json!(true)),
        ("volume", Type::Number, json!(50)),
        ("mode", Type::Enum("Mode".into()), json!("normal")),
    ] {
        manifest.fields.insert(
            name.into(),
            FieldDefinition {
                binding: ty.into(),
                initial,
            },
        );
    }
    manifest
        .actions
        .insert("Refresh".into(), ActionContract::default());
    (manifest, BindingSnapshot::default())
}

fn lower(
    source: &str,
    manifest: &Manifest,
    snapshot: &BindingSnapshot,
) -> Result<matrix_lang::lower_v1::LoweredSurface, Vec<matrix_lang::Diagnostic>> {
    let parsed = parse_surface(source).unwrap();
    let resolved = resolve(&parsed, manifest, snapshot).unwrap();
    lower_v1(&resolved)
}

#[test]
fn exact_fixture_lowers_with_layout_fill_and_wire_path_controls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source =
        fs::read_to_string(root.join("matrix/conformance/v1/01_slider_layout.matrix")).unwrap();
    let expected =
        fs::read_to_string(root.join("matrix/conformance/v1/01_slider_layout.json")).unwrap();
    let (manifest, snapshot) = fixture();
    let lowered = lower(&source, &manifest, &snapshot).unwrap();

    assert_eq!(
        serde_json::to_string(&lowered.surface).unwrap(),
        expected.trim_end()
    );
    assert_eq!(
        mote_ui::v1::validate(expected.as_bytes()).unwrap(),
        lowered.surface
    );
    assert_eq!(
        lowered.controls,
        BTreeMap::from([
            (
                vec![0, 0, 1],
                ControlContract::Toggle {
                    field: "models.local_only".into(),
                },
            ),
            (
                vec![1, 0, 0],
                ControlContract::Slider {
                    field: "volume".into(),
                    min: 0.0,
                    max: 100.0,
                    step: Some(5.0),
                },
            ),
            (
                vec![1, 0, 1],
                ControlContract::Select {
                    field: "mode".into(),
                    options: vec!["quiet".into(), "normal".into()],
                },
            ),
            (
                vec![2, 0],
                ControlContract::Button {
                    event: "Refresh".into(),
                    disabled: false,
                },
            ),
        ])
    );
}

#[test]
fn gallery_dashboard_matches_trusted_host_lowering() {
    let source = include_str!("../../../matrix/examples/mote-dashboard.matrix");
    let models = ["qwen3:4b", "qwen3:0.6b", "qwen3-coder:30b-64k"];
    let mut manifest = Manifest::default();
    manifest
        .enums
        .insert("Model".into(), EnumDefinition::new(models));
    for (name, ty, initial) in [
        ("models.local_only", Type::Boolean, json!(true)),
        ("models.talk", Type::Enum("Model".into()), json!(models[0])),
        (
            "models.router",
            Type::Enum("Model".into()),
            json!(models[1]),
        ),
        ("models.think", Type::Enum("Model".into()), json!(models[2])),
    ] {
        manifest.fields.insert(
            name.into(),
            FieldDefinition {
                binding: ty.into(),
                initial,
            },
        );
    }
    manifest.values.insert(
        "model_options".into(),
        Type::List(Box::new(Type::Enum("Model".into()))).into(),
    );
    manifest
        .actions
        .insert("refresh".into(), ActionContract::default());
    let snapshot = BindingSnapshot {
        values: BTreeMap::from([("model_options".into(), json!(models))]),
        ..Default::default()
    };
    let lowered = lower(source, &manifest, &snapshot).unwrap();
    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../matrix/examples/mote-dashboard.v1.json");
    let encoded = fs::read(fixture_path).expect("checked-in lowered gallery dashboard");
    assert_eq!(mote_ui::v1::validate(&encoded).unwrap(), lowered.surface);
    assert_eq!(lowered.controls.len(), 5);
}

#[test]
fn unsupported_catalog_option_labels_and_nonzero_argument_actions_fail_closed() {
    let (mut manifest, snapshot) = fixture();
    manifest
        .enums
        .get_mut("Mode")
        .unwrap()
        .labels
        .insert("normal".into(), "Normal mode".into());
    manifest.actions.insert(
        "Save".into(),
        ActionContract {
            parameters: BTreeMap::from([("volume".into(), Type::Number)]),
        },
    );
    for source in [
        "screen { section { text \"later\" } }",
        "screen { select \"Mode\" mode options [\"quiet\", \"normal\"] }",
        "screen { slider \"Volume\" volume min 0 max 100 button \"Save\" -> Save{volume} }",
        "screen { button \"Back\" -> Back }",
    ] {
        let error = lower(source, &manifest, &snapshot).unwrap_err();
        assert_eq!(error.len(), 1, "{source}");
        assert_eq!(error[0].code.as_str(), "E_LOWER_UNSUPPORTED", "{source}");
    }
}

#[test]
fn numeric_values_reject_f64_overflow_underflow_and_precision_loss() {
    let (mut manifest, snapshot) = fixture();
    let cases = [
        "screen { slider \"Volume\" volume min 0 max 1 step 1e-400 }",
        "screen { slider \"Volume\" volume min 0 max 9007199254740993 step 1 }",
    ];
    for source in cases {
        let error = lower(source, &manifest, &snapshot).unwrap_err();
        assert_eq!(error[0].code.as_str(), "E_LOWER_UNSUPPORTED", "{source}");
        assert_eq!(error[0].declaration_path, ["screen", "0"], "{source}");
    }

    let error =
        parse_surface("screen { slider \"Volume\" volume min 0 max 1e400 step 1 }").unwrap_err();
    assert_eq!(error[0].code.as_str(), "E_LEX_NUMBER");

    manifest.fields.get_mut("volume").unwrap().initial = json!(9_007_199_254_740_993_u64);
    let error = lower(
        "screen { slider \"Volume\" volume min 0 max 9007199254740994 step 1 }",
        &manifest,
        &snapshot,
    )
    .unwrap_err();
    assert_eq!(error[0].code.as_str(), "E_LOWER_UNSUPPORTED");
}

#[test]
fn slider_current_value_must_satisfy_the_frozen_range_and_step() {
    let (mut manifest, snapshot) = fixture();
    for (value, source) in [
        (
            json!(11),
            "screen { slider \"Volume\" volume min 0 max 10 step 1 }",
        ),
        (
            json!(0.3),
            "screen { slider \"Volume\" volume min 0 max 1 step 0.2 }",
        ),
    ] {
        manifest.fields.get_mut("volume").unwrap().initial = value;
        let error = lower(source, &manifest, &snapshot).unwrap_err();
        assert_eq!(error[0].code.as_str(), "E_LOWER_UNSUPPORTED", "{source}");
        assert_eq!(error[0].declaration_path, ["screen", "0"], "{source}");
    }

    manifest.fields.get_mut("volume").unwrap().initial = json!(0.3);
    lower(
        "screen { slider \"Volume\" volume min 0 max 1 step 0.1 }",
        &manifest,
        &snapshot,
    )
    .unwrap();
}
