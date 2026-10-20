use crate::pipeline::PipelineSnapshot;
use matrix_lang::manifest::{ActionContract, EnumDefinition, FieldDefinition, RecordDefinition};
use matrix_lang::{
    lower_v0, lower_v1, parse_surface, resolve, BindingSnapshot, Diagnostic, Manifest, Surface, Type,
};
use mote_ui::Node;
use serde_json::{json, Value};
use std::collections::BTreeMap;

const MODELS: [&str; 3] = ["qwen3:4b", "qwen3:0.6b", "qwen3-coder:30b-64k"];
const HARNESSES: [&str; 2] = ["opencode", "mote"];
const ROUTER_BUDGET_MIN: i64 = 50;
const ROUTER_BUDGET_MAX: i64 = 1_000;
const ROUTER_BUDGET_STEP: i64 = 10;

pub fn canonical_change(field: &str, value: &Value) -> Option<Value> {
    match field {
        "models.local_only" => value.is_boolean().then(|| value.clone()),
        "models.talk" | "models.router" | "models.think" => {
            value
                .as_str()
                .filter(|model| MODELS.contains(model))
                .map(|_| value.clone())
        }
        "agent.harness" => value
            .as_str()
            .filter(|harness| HARNESSES.contains(harness))
            .map(|_| value.clone()),
        "budgets.router" => {
            let number = value.as_f64()?;
            if !number.is_finite() || number.fract() != 0.0 {
                return None;
            }
            let integer = number as i64;
            (integer as f64 == number
                && (ROUTER_BUDGET_MIN..=ROUTER_BUDGET_MAX).contains(&integer)
                && (integer - ROUTER_BUDGET_MIN) % ROUTER_BUDGET_STEP == 0)
                .then(|| json!(integer))
        }
        _ => None,
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
    matrix: Option<matrix_lang::lower_v1::LoweredSurface>,
}

impl CommittedMatrixSurface {
    pub fn new(surface: Node, manifest: &Manifest) -> Self {
        let mut controls = BTreeMap::new();
        collect_controls(&surface, manifest, &mut Vec::new(), &mut controls);
        Self {
            seq: 0,
            surface,
            controls,
            matrix: None,
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
        match params.get("surface_contract") {
            None => {}
            Some(version) if version.as_u64() == Some(1) => {
                return self.matrix.as_ref().is_some_and(|matrix| {
                    matrix
                        .controls
                        .get(&path)
                        .is_some_and(|control| accepts_v1(control, event, args))
                });
            }
            Some(_) => return false,
        }
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

fn accepts_v1(
    control: &matrix_lang::lower_v1::ControlContract,
    event: &str,
    args: &serde_json::Map<String, Value>,
) -> bool {
    use matrix_lang::lower_v1::ControlContract;
    let field = match control {
        ControlContract::Button {
            event: name,
            disabled,
        } => {
            return !disabled && event == name && args.is_empty();
        }
        ControlContract::Toggle { field }
        | ControlContract::Select { field, .. }
        | ControlContract::Slider { field, .. } => field,
    };
    if event != "change"
        || args.len() != 2
        || args.get("field").and_then(Value::as_str) != Some(field)
    {
        return false;
    }
    let Some(value) = args.get("value") else {
        return false;
    };
    match control {
        ControlContract::Toggle { .. } => value.is_boolean(),
        ControlContract::Select { options, .. } => value
            .as_str()
            .is_some_and(|value| options.iter().any(|option| option == value)),
        ControlContract::Slider { min, max, step, .. } => slider_number(value).is_some_and(|value| {
            mote_ui::v1::Surface {
                version: 1,
                root: mote_ui::v1::Node::Screen {
                    title: None,
                    children: vec![mote_ui::v1::Node::Slider {
                        label: field.clone(),
                        field: field.clone(),
                        value,
                        min: *min,
                        max: *max,
                        step: *step,
                        fill: false,
                    }],
                },
            }
            .validate()
            .is_ok()
        }),
        ControlContract::Button { .. } => unreachable!(),
    }
}

fn slider_number(value: &Value) -> Option<f64> {
    let number = value.as_f64()?;
    let integer = value
        .as_i64()
        .map(i128::from)
        .or_else(|| value.as_u64().map(i128::from));
    if integer.is_some_and(|integer| number as i128 != integer) {
        return None;
    }
    Some(number)
}

pub struct CompiledSurface {
    pub fallback: Value,
    matrix: Option<matrix_lang::lower_v1::LoweredSurface>,
}

impl CompiledSurface {
    pub fn prepare(&self, manifest: &Manifest) -> Result<(Value, CommittedMatrixSurface), String> {
        let surface = mote_ui::validate(&serde_json::to_vec(&self.fallback).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let mut committed = CommittedMatrixSurface::new(surface, manifest);
        let mut payload = surface_payload(&self.fallback);
        if let Some(matrix) = &self.matrix {
            payload["matrix_surface"] =
                serde_json::to_value(&matrix.surface).map_err(|e| e.to_string())?;
            mote_ui::v1::validate_envelope(&serde_json::to_vec(&payload).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            committed.matrix = Some(matrix.clone());
        }
        Ok((payload, committed))
    }
}

pub fn compile_contract(
    source: &str,
    bindings: &HostBindings,
) -> Result<CompiledSurface, Vec<Diagnostic>> {
    let parsed = parse_surface(source)?;
    compile_surface_contract(&parsed, bindings)
}

pub fn compile_surface_contract(
    parsed: &Surface,
    bindings: &HostBindings,
) -> Result<CompiledSurface, Vec<Diagnostic>> {
    let resolved = resolve(parsed, &bindings.manifest, &bindings.snapshot)?;
    let matrix = lower_v1(&resolved);
    if let Ok(matrix) = &matrix {
        validate_router_budget_sliders(matrix, parsed.screen.span)
            .map_err(|error| vec![error])?;
    }
    let fallback = lower_v0(&resolved);
    let (fallback, matrix) = match (fallback, matrix) {
        (Ok(fallback), Ok(matrix)) => (fallback, Some(matrix)),
        (Ok(fallback), Err(_)) => (fallback, None),
        (Err(_), Ok(matrix)) => (
            Node::Text {
                value: "This Matrix surface requires a newer Mote client.".into(),
            },
            Some(matrix),
        ),
        (Err(errors), Err(_)) => return Err(errors),
    };
    let fallback = serde_json::to_value(fallback).map_err(|error| vec![Diagnostic {
        code: matrix_lang::DiagnosticCode::LowerLimit,
        severity: matrix_lang::Severity::Error,
        span: parsed.screen.span,
        declaration_path: vec!["screen".into()],
        message: format!("Could not encode the compact surface: {error}"),
        hint: None,
    }])?;
    Ok(CompiledSurface { fallback, matrix })
}

pub(crate) fn has_exact_router_budget_slider(
    matrix: &matrix_lang::lower_v1::LoweredSurface,
) -> bool {
    let mut found = false;
    for control in matrix.controls.values() {
        let matrix_lang::lower_v1::ControlContract::Slider {
            field,
            min,
            max,
            step,
        } = control
        else {
            continue;
        };
        if field != "budgets.router" {
            continue;
        }
        if *min != ROUTER_BUDGET_MIN as f64
            || *max != ROUTER_BUDGET_MAX as f64
            || *step != Some(ROUTER_BUDGET_STEP as f64)
        {
            return false;
        }
        found = true;
    }
    found
}

pub(crate) fn validate_router_budget_sliders(
    matrix: &matrix_lang::lower_v1::LoweredSurface,
    span: matrix_lang::Span,
) -> Result<(), Diagnostic> {
    for (path, control) in &matrix.controls {
        let matrix_lang::lower_v1::ControlContract::Slider {
            field,
            min,
            max,
            step,
        } = control
        else {
            continue;
        };
        if field == "budgets.router"
            && (*min != ROUTER_BUDGET_MIN as f64
                || *max != ROUTER_BUDGET_MAX as f64
                || *step != Some(ROUTER_BUDGET_STEP as f64))
        {
            return Err(Diagnostic {
                code: matrix_lang::DiagnosticCode::ResolveConstraint,
                severity: matrix_lang::Severity::Error,
                span,
                declaration_path: std::iter::once("screen".into())
                    .chain(path.iter().map(usize::to_string))
                    .collect(),
                message: "budgets.router requires min 50, max 1000 and step 10".into(),
                hint: Some(
                    "Use slider \"Router budget\" budgets.router min 50 max 1000 step 10"
                        .into(),
                ),
            });
        }
    }
    Ok(())
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
    manifest
        .enums
        .insert("Harness".into(), EnumDefinition::new(HARNESSES));
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
        "harness_options".into(),
        Type::List(Box::new(Type::Enum("Harness".into()))).into(),
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
    manifest
        .values
        .insert("harness.active".into(), Type::String.into());
    manifest
        .values
        .insert("harness.pending".into(), Type::String.into());
    for (name, ty) in [
        ("models.local_only", Type::Boolean),
        ("models.talk", Type::Enum("Model".into())),
        ("models.router", Type::Enum("Model".into())),
        ("models.think", Type::Enum("Model".into())),
        ("budgets.router", Type::Integer),
        ("agent.harness", Type::Enum("Harness".into())),
    ] {
        let initial = if name == "agent.harness" {
            json!(pipeline.harness.configured)
        } else {
            config.get(name).cloned().unwrap_or(Value::Null)
        };
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
    snapshot
        .values
        .insert("harness_options".into(), json!(HARNESSES));
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
    snapshot.values.insert(
        "harness.active".into(),
        json!(pipeline.harness.active),
    );
    snapshot.values.insert(
        "harness.pending".into(),
        json!(pipeline.harness.pending.as_deref().unwrap_or("none")),
    );
    for name in manifest.fields.keys() {
        if name == "agent.harness" {
            snapshot
                .fields
                .insert(name.clone(), json!(pipeline.harness.configured));
        } else if let Some(value) = config.get(name) {
            snapshot.fields.insert(name.clone(), value.clone());
        }
    }
    Ok(HostBindings { manifest, snapshot })
}

#[allow(dead_code)]
pub fn compile(source: &str, bindings: &HostBindings) -> Result<Value, Vec<Diagnostic>> {
    let parsed = parse_surface(source)?;
    compile_surface(&parsed, bindings)
}

#[allow(dead_code)]
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

#[cfg(test)]
mod contract_tests {
    use super::*;
    use matrix_lang::manifest::FieldDefinition;

    fn bindings(value: Value) -> HostBindings {
        let mut manifest = Manifest::default();
        manifest.fields.insert("volume".into(), FieldDefinition {
            binding: Type::Number.into(), initial: value,
        });
        manifest.fields.insert("enabled".into(), FieldDefinition {
            binding: Type::Boolean.into(), initial: json!(true),
        });
        manifest.enums.insert("Mode".into(), EnumDefinition::new(["quiet", "normal"]));
        manifest.fields.insert("mode".into(), FieldDefinition {
            binding: Type::Enum("Mode".into()).into(), initial: json!("quiet"),
        });
        manifest.actions.insert("refresh".into(), ActionContract::default());
        HostBindings { manifest, snapshot: BindingSnapshot::default() }
    }

    fn committed(source: &str, value: Value) -> (Value, CommittedMatrixSurface) {
        let bindings = bindings(value);
        let compiled = compile_contract(source, &bindings).unwrap();
        let (payload, mut committed) = compiled.prepare(&bindings.manifest).unwrap();
        committed.seq = 37;
        (payload, committed)
    }

    #[test]
    fn host_router_budget_slider_compiles_with_integer_range_and_canonical_changes() {
        let config = BTreeMap::from([
            ("models.local_only".to_string(), json!(true)),
            ("models.talk".to_string(), json!("qwen3:4b")),
            ("models.router".to_string(), json!("qwen3:0.6b")),
            ("models.think".to_string(), json!("qwen3-coder:30b-64k")),
            ("budgets.router".to_string(), json!(150)),
        ]);
        let pipeline = PipelineSnapshot {
            generated_at: 0,
            event_limit: crate::pipeline::PIPELINE_EVENT_LIMIT,
            request_limit: crate::pipeline::PIPELINE_REQUEST_LIMIT,
            window_ms: 3_600_000,
            config: config.clone(),
            harness: crate::pipeline::HarnessSnapshot {
                configured: "opencode".into(),
                active: "opencode".into(),
                pending: None,
            },
            voice: super::super::voice::VoiceStatus::default(),
            events: Vec::new(),
            numbers: Vec::new(),
            requests: Vec::new(),
        };
        let bindings = host_bindings(&config, &pipeline, &HostSnapshot::default()).unwrap();
        let compiled = compile_contract(
            "screen { slider \"Router budget\" budgets.router min 50 max 1000 step 10 }",
            &bindings,
        )
        .unwrap();
        let (payload, mut committed) = compiled.prepare(&bindings.manifest).unwrap();
        committed.seq = 41;
        assert_eq!(
            payload["matrix_surface"]["root"]["children"][0],
            json!({
                "type":"slider",
                "label":"Router budget",
                "field":"budgets.router",
                "value":150.0,
                "min":50.0,
                "max":1000.0,
                "step":10.0
            })
        );
        let params = json!({"surface_contract":1,"surface_seq":41,"node_path":[0]});
        assert!(committed.accepts(
            &params,
            "change",
            &json!({"field":"budgets.router","value":160.0})
        ));
        assert_eq!(canonical_change("budgets.router", &json!(160.0)), Some(json!(160)));
        for value in [json!(49), json!(55), json!(1001), json!(150.5), json!("150")] {
            assert!(canonical_change("budgets.router", &value).is_none(), "{value}");
        }
    }

    #[test]
    fn host_router_budget_slider_rejects_any_advertised_contract_drift() {
        let config = BTreeMap::from([
            ("models.local_only".to_string(), json!(true)),
            ("models.talk".to_string(), json!("qwen3:4b")),
            ("models.router".to_string(), json!("qwen3:0.6b")),
            ("models.think".to_string(), json!("qwen3-coder:30b-64k")),
            ("budgets.router".to_string(), json!(150)),
        ]);
        let pipeline = PipelineSnapshot {
            generated_at: 0,
            event_limit: crate::pipeline::PIPELINE_EVENT_LIMIT,
            request_limit: crate::pipeline::PIPELINE_REQUEST_LIMIT,
            window_ms: 3_600_000,
            config: config.clone(),
            harness: crate::pipeline::HarnessSnapshot {
                configured: "opencode".into(),
                active: "opencode".into(),
                pending: None,
            },
            voice: super::super::voice::VoiceStatus::default(),
            events: Vec::new(),
            numbers: Vec::new(),
            requests: Vec::new(),
        };
        let bindings = host_bindings(&config, &pipeline, &HostSnapshot::default()).unwrap();
        for source in [
            "screen { slider \"Router budget\" budgets.router min 40 max 1000 step 10 }",
            "screen { slider \"Router budget\" budgets.router min 50 max 1010 step 10 }",
            "screen { slider \"Router budget\" budgets.router min 50 max 1000 step 5 }",
            "screen { slider \"Router budget\" budgets.router min 50 max 1000 }",
        ] {
            let errors = compile_contract(source, &bindings).err().unwrap();
            assert_eq!(errors[0].code, matrix_lang::DiagnosticCode::ResolveConstraint);
        }
    }

    #[test]
    fn slider_dual_surface_and_decimal_admission_follow_frozen_validator() {
        let (payload, committed) = committed("screen { slider \"Volume\" volume min 0 max 1 step 0.1 }", json!(0.3));
        assert_eq!(payload["surface"], json!({"type":"text","value":"This Matrix surface requires a newer Mote client."}));
        assert_eq!(payload["matrix_surface"], json!({"version":1,"root":{"type":"screen","children":[
            {"type":"slider","label":"Volume","field":"volume","value":0.3,"min":0.0,"max":1.0,"step":0.1}
        ]}}));
        mote_ui::v1::validate_envelope(&serde_json::to_vec(&payload).unwrap()).unwrap();
        let params = json!({"surface_contract":1,"surface_seq":37,"node_path":[0]});
        for value in [json!(0), json!(0.3), json!(0.30000000000000004), json!(1)] {
            assert!(committed.accepts(&params, "change", &json!({"field":"volume","value":value})), "{value}");
        }
        for value in [json!("0.3"), json!(true), Value::Null, json!([]), json!({}), json!(-0.1), json!(1.1), json!(0.35)] {
            assert!(!committed.accepts(&params, "change", &json!({"field":"volume","value":value})), "{value}");
        }
    }

    #[test]
    fn slider_admission_rejects_integer_rounding_across_range_and_step_boundaries() {
        for max in ["9007199254740992", "18014398509481984"] {
            let source = format!("screen {{ slider \"Volume\" volume min 0 max {max} step 2 }}");
            let (_, committed) = committed(&source, json!(0));
            let params = json!({"surface_contract":1,"surface_seq":37,"node_path":[0]});
            assert!(committed.accepts(&params, "change", &json!({"field":"volume","value":9007199254740992_u64})));
            assert!(!committed.accepts(&params, "change", &json!({"field":"volume","value":9007199254740993_u64})), "{source}");
        }
    }

    #[test]
    fn slider_admission_preserves_exact_signed_unsigned_and_float_values() {
        let (_, committed) = committed("screen { slider \"Volume\" volume min -1e20 max 1e20 }", json!(0));
        let params = json!({"surface_contract":1,"surface_seq":37,"node_path":[0]});
        for value in [
            json!(0), json!(-42), json!(42_u64), json!(9007199254740992_u64),
            json!(9007199254740994_u64), json!(i64::MIN),
            json!(9223372036854774784_i64), json!(18446744073709549568_u64),
            json!(0.3), json!(0.30000000000000004),
        ] {
            assert!(committed.accepts(&params, "change", &json!({"field":"volume","value":value})), "{value}");
        }
        for value in [
            json!(9007199254740993_u64), json!(-9007199254740993_i64),
            json!(i64::MAX), json!(i64::MIN + 1), json!(u64::MAX),
        ] {
            assert!(!committed.accepts(&params, "change", &json!({"field":"volume","value":value})), "{value}");
        }
    }

    #[test]
    fn slider_admission_handles_extreme_scales_and_non_aligned_maxima() {
        for (min, max, step, valid, invalid) in [
            ("0", "1.05", "0.1", json!(1.0), json!(1.05)),
            ("0", "1000000001", "2", json!(1000000000), json!(1000000001)),
            ("0", "1e308", "1e-308", json!(1e-308), json!(1e308)),
            ("1e308", "1.7e308", "1e308", json!(1e308), json!(1.7e308)),
            ("0", "1e-300", "1e-301", json!(3e-301), json!(3.5e-301)),
        ] {
            let source = format!("screen {{ slider \"Volume\" volume min {min} max {max} step {step} }}");
            let (_, committed) = committed(&source, json!(min.parse::<f64>().unwrap()));
            let params = json!({"surface_contract":1,"surface_seq":37,"node_path":[0]});
            assert!(committed.accepts(&params, "change", &json!({"field":"volume","value":valid})), "{source}: {valid}");
            assert!(!committed.accepts(&params, "change", &json!({"field":"volume","value":invalid})), "{source}: {invalid}");
        }
    }

    #[test]
    fn contract_tables_require_exact_context_and_compiled_control_types() {
        let (_, committed) = committed("screen { col { toggle \"Enabled\" enabled select \"Mode\" mode options [\"quiet\",\"normal\"] slider \"Volume\" volume min 0 max 10 step 2 button \"Refresh\" -> refresh button \"Disabled\" disabled -> refresh } }", json!(2));
        let params = json!({"surface_contract":1,"surface_seq":37,"node_path":[0,2]});
        let args = json!({"field":"volume","value":4});
        assert!(committed.accepts(&params, "change", &args));
        for (key, value) in [
            ("surface_contract", json!(0)), ("surface_contract", json!(2)),
            ("surface_contract", Value::Null), ("surface_contract", json!("1")),
            ("surface_contract", json!(true)), ("surface_contract", json!(1.0)),
            ("surface_seq", json!(36)), ("surface_seq", json!(38)),
            ("surface_seq", Value::Null), ("surface_seq", json!(37.0)),
            ("node_path", json!([])), ("node_path", json!([2])),
            ("node_path", json!([0,2.0])), ("node_path", json!([0,-1])),
        ] {
            let mut invalid = params.clone(); invalid[key] = value;
            assert!(!committed.accepts(&invalid, "change", &args), "{invalid}");
        }
        for key in ["surface_contract", "surface_seq", "node_path"] {
            let mut invalid = params.clone(); invalid.as_object_mut().unwrap().remove(key);
            assert!(!committed.accepts(&invalid, "change", &args));
        }
        for (event, args) in [("refresh", json!({})), ("change", json!({"field":"enabled","value":4})), ("change", json!({"field":"volume","value":4,"extra":0})), ("change", json!({"field":"volume"})), ("change", json!([]))] {
            assert!(!committed.accepts(&params, event, &args));
        }
        for (index, event, args) in [
            (0, "change", json!({"field":"enabled","value":false})),
            (1, "change", json!({"field":"mode","value":"normal"})),
            (2, "change", json!({"field":"volume","value":4})),
            (3, "refresh", json!({})),
        ] {
            let params = json!({"surface_contract":1,"surface_seq":37,"node_path":[0,index]});
            assert!(committed.accepts(&params, event, &args));
            for wrong in [json!({"field":"enabled","value":"false"}), json!({"field":"mode","value":true}), json!({"field":"mode","value":"unknown"}), json!({"extra":0})] {
                assert!(!committed.accepts(&params, event, &wrong));
            }
        }
        assert!(!committed.accepts(&json!({"surface_contract":1,"surface_seq":37,"node_path":[0,4]}), "refresh", &json!({})));
    }
}
