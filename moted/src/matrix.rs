use crate::pipeline::PipelineSnapshot;
use matrix_lang::manifest::{ActionContract, EnumDefinition, FieldDefinition, RecordDefinition};
use matrix_lang::{
    lower_v0, parse_surface, resolve, BindingSnapshot, Diagnostic, Manifest, Surface, Type,
};
use mote_ui::Node;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MODELS: [&str; 3] = ["qwen3:4b", "qwen3:0.6b", "qwen3-coder:30b-64k"];

pub fn accepts_change(field: &str, value: &Value) -> bool {
    match field {
        "models.local_only" => value.is_boolean(),
        "models.talk" | "models.router" | "models.think" => {
            value.as_str().is_some_and(|model| MODELS.contains(&model))
        }
        _ => false,
    }
}

#[derive(Clone, Debug, PartialEq)]
enum ControlContract {
    Field {
        name: String,
        ty: Type,
        choices: Vec<String>,
    },
    Action {
        name: String,
        contract: ActionContract,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommittedMatrixSurface {
    pub seq: i64,
    surface: Node,
    controls: BTreeMap<Vec<usize>, ControlContract>,
}

impl CommittedMatrixSurface {
    pub fn new(surface: Node, manifest: &Manifest) -> Self {
        let mut controls = BTreeMap::new();
        collect_controls(&surface, manifest, &mut Vec::new(), &mut controls);
        Self {
            seq: 0,
            surface,
            controls,
        }
    }

    pub fn accepts(&self, params: &Value, event: &str, args: &Value) -> bool {
        if params.get("surface_seq").and_then(Value::as_i64) != Some(self.seq) {
            return false;
        }
        let Some(path) = params
            .get("node_path")
            .and_then(Value::as_array)
            .and_then(|path| {
                path.iter()
                    .map(|index| index.as_u64().and_then(|i| usize::try_from(i).ok()))
                    .collect::<Option<Vec<_>>>()
            })
        else {
            return false;
        };
        let Some(args) = args.as_object() else {
            return false;
        };
        match self.controls.get(&path) {
            Some(ControlContract::Action { name, contract }) => {
                event == name && contract.parameters.is_empty() && args.is_empty()
            }
            Some(ControlContract::Field { name, ty, choices }) => {
                if event != "change"
                    || args.len() != 2
                    || args.get("field").and_then(Value::as_str) != Some(name)
                {
                    return false;
                }
                args.get("value").is_some_and(|value| match ty {
                    Type::Boolean => value.is_boolean(),
                    Type::Enum(_) => value
                        .as_str()
                        .is_some_and(|value| choices.iter().any(|choice| choice == value)),
                    _ => false,
                })
            }
            None => false,
        }
    }
}

fn collect_controls(
    node: &Node,
    manifest: &Manifest,
    path: &mut Vec<usize>,
    controls: &mut BTreeMap<Vec<usize>, ControlContract>,
) {
    match node {
        Node::Stack { children }
        | Node::Row { children }
        | Node::List { children }
        | Node::Chat { children } => {
            for (index, child) in children.iter().enumerate() {
                path.push(index);
                collect_controls(child, manifest, path, controls);
                path.pop();
            }
        }
        Node::Toggle { name, .. } | Node::Select { name, .. } | Node::Field { name, .. } => {
            if let Some(field) = manifest.fields.get(name) {
                let choices = match &field.binding.ty {
                    Type::Enum(name) => manifest
                        .enums
                        .get(name)
                        .map(|definition| definition.members.clone())
                        .unwrap_or_default(),
                    _ => Vec::new(),
                };
                controls.insert(
                    path.clone(),
                    ControlContract::Field {
                        name: name.clone(),
                        ty: field.binding.ty.clone(),
                        choices,
                    },
                );
            }
        }
        Node::Action { event, .. } => {
            if let Some(contract) = manifest.actions.get(event) {
                controls.insert(
                    path.clone(),
                    ControlContract::Action {
                        name: event.clone(),
                        contract: contract.clone(),
                    },
                );
            }
        }
        _ => {}
    }
}

pub struct HostBindings {
    pub manifest: Manifest,
    pub snapshot: BindingSnapshot,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum HostSnapshot {
    Ready { progress: f64, items: Vec<String> },
    Pending,
    Failed,
}

impl Default for HostSnapshot {
    fn default() -> Self {
        Self::ready(0.0, std::iter::empty::<String>())
    }
}

#[allow(dead_code)]
impl HostSnapshot {
    pub fn ready(
        progress: f64,
        items: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Ready {
            progress,
            items: items.into_iter().map(Into::into).collect(),
        }
    }

    pub fn pending() -> Self {
        Self::Pending
    }

    pub fn failed() -> Self {
        Self::Failed
    }
}

pub fn surface_payload(surface: &Value) -> Value {
    json!({"surface": surface, "surface_id": "matrix"})
}

pub fn render_success(valid: bool) -> Value {
    json!({"ok": true, "valid": valid})
}

pub fn host_bindings(
    config: &BTreeMap<String, Value>,
    pipeline: &PipelineSnapshot,
    host: &HostSnapshot,
) -> Result<HostBindings, Vec<Diagnostic>> {
    let HostSnapshot::Ready { progress, items } = host else {
        let state = match host {
            HostSnapshot::Pending => "pending",
            HostSnapshot::Failed => "failed",
            HostSnapshot::Ready { .. } => unreachable!(),
        };
        return Err(vec![Diagnostic {
            code: matrix_lang::DiagnosticCode::LowerUnsupported,
            severity: matrix_lang::Severity::Error,
            span: matrix_lang::Span::new(0, 0),
            declaration_path: vec!["screen".into()],
            message: format!("The v0 consumer cannot represent a {state} host snapshot"),
            hint: Some("Wait for a loaded snapshot before committing this surface".into()),
        }]);
    };
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
    manifest.records.insert(
        "HostItem".into(),
        RecordDefinition {
            fields: BTreeMap::from([("label".into(), Type::String.into())]),
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
    manifest
        .values
        .insert("host.progress".into(), Type::Number.into());
    manifest.values.insert(
        "host.items".into(),
        Type::List(Box::new(Type::Record("HostItem".into()))).into(),
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
    snapshot
        .values
        .insert("model_options".into(), json!(MODELS));
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
    snapshot
        .values
        .insert("host.progress".into(), json!(progress));
    snapshot.values.insert(
        "host.items".into(),
        Value::Array(
            items
                .iter()
                .map(|label| json!({"label": label}))
                .collect(),
        ),
    );
    for name in manifest.fields.keys() {
        if let Some(value) = config.get(name) {
            snapshot.fields.insert(name.clone(), value.clone());
        }
    }
    Ok(HostBindings { manifest, snapshot })
}

pub fn compile(source: &str, bindings: &HostBindings) -> Result<Value, Vec<Diagnostic>> {
    let parsed = parse_surface(source)?;
    compile_surface(&parsed, bindings)
}

pub fn compile_surface(
    parsed: &Surface,
    bindings: &HostBindings,
) -> Result<Value, Vec<Diagnostic>> {
    let resolved = resolve(parsed, &bindings.manifest, &bindings.snapshot)?;
    let node = lower_v0(&resolved)?;
    serde_json::to_value(node).map_err(|error| {
        vec![Diagnostic {
            code: matrix_lang::DiagnosticCode::LowerLimit,
            severity: matrix_lang::Severity::Error,
            span: parsed.screen.span,
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
