use crate::pipeline::PipelineSnapshot;
use matrix_lang::manifest::{
    ActionContract, EnumDefinition, FieldDefinition, RecordDefinition,
};
use matrix_lang::{
    lower_v0, parse_surface, resolve, BindingSnapshot, Diagnostic, Manifest, Type,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MODELS: [&str; 3] = ["qwen3:4b", "qwen3:0.6b", "qwen3-coder:30b-64k"];

pub fn accepts_change(field: &str, value: &Value) -> bool {
    match field {
        "models.local_only" => value.is_boolean(),
        "models.talk" | "models.router" | "models.think" => value
            .as_str()
            .is_some_and(|model| MODELS.contains(&model)),
        _ => false,
    }
}

pub struct HostBindings {
    pub manifest: Manifest,
    pub snapshot: BindingSnapshot,
}

pub fn host_bindings(
    config: &BTreeMap<String, Value>,
    pipeline: &PipelineSnapshot,
) -> HostBindings {
    let mut manifest = Manifest::default();
    manifest
        .enums
        .insert("Model".into(), EnumDefinition::new(MODELS));
    manifest.records.insert(
        "PipelineStage".into(),
        RecordDefinition {
            fields: BTreeMap::from([
                ("stage".into(), Type::String.into()),
                ("calls".into(), Type::Integer.into()),
                ("latency".into(), Type::String.into()),
            ]),
        },
    );
    manifest.values.insert(
        "model_options".into(),
        Type::List(Box::new(Type::Enum("Model".into()))).into(),
    );
    manifest.values.insert(
        "pipeline.stages".into(),
        Type::List(Box::new(Type::Record("PipelineStage".into()))).into(),
    );
    for (name, ty) in [
        ("models.local_only", Type::Boolean),
        ("models.talk", Type::Enum("Model".into())),
        ("models.router", Type::Enum("Model".into())),
        ("models.think", Type::Enum("Model".into())),
    ] {
        let initial = config.get(name).cloned().unwrap_or(Value::Null);
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
        .insert("refresh".into(), ActionContract::default());

    let mut snapshot = BindingSnapshot::default();
    snapshot.values.insert("model_options".into(), json!(MODELS));
    snapshot.values.insert(
        "pipeline.stages".into(),
        Value::Array(
            pipeline
                .numbers
                .iter()
                .map(|stage| {
                    let latency = match (stage.latency_p50_ms, stage.latency_p95_ms) {
                        (Some(p50), Some(p95)) => format!("p50 {p50} ms, p95 {p95} ms"),
                        _ => "No latency samples".into(),
                    };
                    json!({
                        "stage": stage.stage,
                        "calls": stage.calls,
                        "latency": latency,
                    })
                })
                .collect(),
        ),
    );
    for name in manifest.fields.keys() {
        if let Some(value) = config.get(name) {
            snapshot.fields.insert(name.clone(), value.clone());
        }
    }
    HostBindings { manifest, snapshot }
}

pub fn compile(source: &str, bindings: &HostBindings) -> Result<Value, Vec<Diagnostic>> {
    let parsed = parse_surface(source)?;
    let resolved = resolve(&parsed, &bindings.manifest, &bindings.snapshot)?;
    let node = lower_v0(&resolved)?;
    serde_json::to_value(node).map_err(|error| {
        vec![Diagnostic {
            code: matrix_lang::DiagnosticCode::LowerLimit,
            severity: matrix_lang::Severity::Error,
            span: matrix_lang::Span::new(0, source.len()),
            declaration_path: vec!["screen".into()],
            message: format!("Could not encode the compact surface: {error}"),
            hint: None,
        }]
    })
}

pub fn diagnostics(errors: &[Diagnostic]) -> Value {
    Value::Array(
        errors
            .iter()
            .map(|error| {
                json!({
                    "code": error.code.as_str(),
                    "severity": "error",
                    "span": {"start": error.span.start, "end": error.span.end},
                    "declaration_path": error.declaration_path,
                    "message": error.message,
                    "hint": error.hint,
                })
            })
            .collect(),
    )
}

