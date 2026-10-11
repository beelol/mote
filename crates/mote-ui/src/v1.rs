//! Versioned Matrix consumer contract.
//!
//! This module is intentionally not connected to production emission. The legacy [`crate::Node`]
//! contract remains the only production surface until the compiler and consumer are activated
//! together.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_DEPTH: usize = crate::MAX_DEPTH;
pub const MAX_NODES: usize = crate::MAX_NODES;
pub const MAX_BYTES: usize = crate::MAX_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic(pub String);

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractStatus {
    Frozen,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    pub name: &'static str,
    pub status: ContractStatus,
}

const FROZEN: ContractStatus = ContractStatus::Frozen;
const UNRESOLVED: ContractStatus = ContractStatus::Unresolved;

pub const CATALOG_INVENTORY: &[CatalogEntry] = &[
    CatalogEntry {
        name: "screen",
        status: FROZEN,
    },
    CatalogEntry {
        name: "section",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "card",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "col",
        status: FROZEN,
    },
    CatalogEntry {
        name: "row",
        status: FROZEN,
    },
    CatalogEntry {
        name: "tabs",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "tab",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "header",
        status: FROZEN,
    },
    CatalogEntry {
        name: "footer",
        status: FROZEN,
    },
    CatalogEntry {
        name: "title",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "text",
        status: FROZEN,
    },
    CatalogEntry {
        name: "caption",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "label",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "code",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "md",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "link",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "badge",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "tag",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "stat",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "kv",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "icon",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "image",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "avatar",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "timestamp",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "divider",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "field",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "area",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "toggle",
        status: FROZEN,
    },
    CatalogEntry {
        name: "check",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "choice",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "select",
        status: FROZEN,
    },
    CatalogEntry {
        name: "multiselect",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "segmented",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "slider",
        status: FROZEN,
    },
    CatalogEntry {
        name: "stepper",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "date",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "time",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "search",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "button",
        status: FROZEN,
    },
    CatalogEntry {
        name: "form",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "list",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "item",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "table",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "column",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "grid",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "tile",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "progress",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "steps",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "status",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "banner",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "empty",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "error",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "typing",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "chat",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "message",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "compose",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "dialog",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "sheet",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "menu",
        status: UNRESOLVED,
    },
    CatalogEntry {
        name: "tooltip",
        status: UNRESOLVED,
    },
];

pub fn catalog_inventory() -> &'static [CatalogEntry] {
    CATALOG_INVENTORY
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Alignment {
    Start,
    Center,
    End,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Distribution {
    Between,
    Around,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Wrapping {
    Wrap,
    Nowrap,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ButtonIntent {
    Primary,
    Danger,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Surface {
    #[schemars(range(min = 1, max = 1))]
    pub version: u32,
    pub root: Node,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SurfaceEnvelope {
    pub surface_id: String,
    pub surface: crate::Node,
    pub matrix_surface: Surface,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Node {
    Screen {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default)]
        children: Vec<Node>,
    },
    Col {
        #[serde(default, skip_serializing_if = "is_false")]
        fill: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alignment: Option<Alignment>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        distribution: Option<Distribution>,
        #[serde(default)]
        children: Vec<Node>,
    },
    Row {
        #[serde(default, skip_serializing_if = "is_false")]
        fill: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wrapping: Option<Wrapping>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alignment: Option<Alignment>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        distribution: Option<Distribution>,
        #[serde(default)]
        children: Vec<Node>,
    },
    Header {
        #[serde(default)]
        children: Vec<Node>,
    },
    Footer {
        #[serde(default)]
        children: Vec<Node>,
    },
    Text {
        value: String,
        #[serde(default, skip_serializing_if = "is_false")]
        fill: bool,
    },
    Toggle {
        label: String,
        field: String,
        value: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        fill: bool,
    },
    Select {
        label: String,
        field: String,
        options: Vec<String>,
        value: String,
        #[serde(default, skip_serializing_if = "is_false")]
        fill: bool,
    },
    Slider {
        label: String,
        field: String,
        value: f64,
        min: f64,
        max: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<f64>,
        #[serde(default, skip_serializing_if = "is_false")]
        fill: bool,
    },
    Button {
        label: String,
        event: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        intent: Option<ButtonIntent>,
        #[serde(default, skip_serializing_if = "is_false")]
        confirm: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        disabled: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        fill: bool,
    },
}

impl Surface {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self.version != CONTRACT_VERSION {
            return Err(Diagnostic(format!(
                "Matrix consumer contract requires version {CONTRACT_VERSION}"
            )));
        }
        if !matches!(self.root, Node::Screen { .. }) {
            return Err(Diagnostic("Matrix consumer root must be a screen".into()));
        }
        let mut nodes = 0;
        check_node(&self.root, Parent::Root, 1, &mut nodes)?;
        let bytes = serde_json::to_vec(self).map_err(|error| {
            Diagnostic(format!("Matrix consumer tree cannot serialize: {error}"))
        })?;
        if bytes.len() > MAX_BYTES {
            return Err(Diagnostic(format!(
                "Matrix consumer tree exceeds 256 KB ({} bytes, limit {MAX_BYTES})",
                bytes.len()
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Parent {
    Root,
    Screen,
    Row,
    Col,
    Other,
}

fn required(value: &str, what: &str) -> Result<(), Diagnostic> {
    if value.is_empty() {
        Err(Diagnostic(format!("{what} cannot be empty")))
    } else {
        Ok(())
    }
}

fn check_slider(value: f64, min: f64, max: f64, step: Option<f64>) -> Result<(), Diagnostic> {
    if !value.is_finite()
        || !min.is_finite()
        || !max.is_finite()
        || step.is_some_and(|n| !n.is_finite())
    {
        return Err(Diagnostic("slider numbers must be finite".into()));
    }
    if min > max {
        return Err(Diagnostic("slider min cannot exceed max".into()));
    }
    if !(min..=max).contains(&value) {
        return Err(Diagnostic("slider value must be inside its range".into()));
    }
    if let Some(step) = step {
        if step <= 0.0 {
            return Err(Diagnostic("slider step must be positive".into()));
        }
        let quotient = (value - min) / step;
        let tolerance = 1e-9 * quotient.abs().max(1.0);
        if (quotient - quotient.round()).abs() > tolerance {
            return Err(Diagnostic(
                "slider value must be aligned to step from min".into(),
            ));
        }
    }
    Ok(())
}

fn check_node(
    node: &Node,
    parent: Parent,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), Diagnostic> {
    if depth > MAX_DEPTH {
        return Err(Diagnostic(format!(
            "Matrix consumer tree exceeds depth {MAX_DEPTH}"
        )));
    }
    *nodes += 1;
    if *nodes > MAX_NODES {
        return Err(Diagnostic(
            "Matrix consumer tree exceeds 2,000 nodes".into(),
        ));
    }
    if node.fill() && !matches!(parent, Parent::Row | Parent::Col) {
        return Err(Diagnostic(
            "fill requires a direct row or col parent".into(),
        ));
    }
    match node {
        Node::Screen { children, .. } => {
            if parent != Parent::Root {
                return Err(Diagnostic("a screen cannot be nested".into()));
            }
            let headers = children
                .iter()
                .filter(|node| matches!(node, Node::Header { .. }))
                .count();
            let footers = children
                .iter()
                .filter(|node| matches!(node, Node::Footer { .. }))
                .count();
            if headers > 1 || footers > 1 {
                return Err(Diagnostic(
                    "header and footer must be unique direct screen children".into(),
                ));
            }
            check_children(children, Parent::Screen, depth, nodes)
        }
        Node::Col { children, .. } => check_children(children, Parent::Col, depth, nodes),
        Node::Row { children, .. } => check_children(children, Parent::Row, depth, nodes),
        Node::Header { children, .. } | Node::Footer { children, .. } => {
            if parent != Parent::Screen {
                return Err(Diagnostic(
                    "header and footer must be unique direct screen children".into(),
                ));
            }
            check_children(children, Parent::Other, depth, nodes)
        }
        Node::Text { .. } => Ok(()),
        Node::Toggle { label, field, .. } => {
            required(label, "toggle label")?;
            required(field, "toggle field")
        }
        Node::Select {
            label,
            field,
            options,
            value,
            ..
        } => {
            required(label, "select label")?;
            required(field, "select field")?;
            if options.is_empty() || options.iter().any(String::is_empty) {
                return Err(Diagnostic("select options must be nonempty strings".into()));
            }
            let unique: BTreeSet<_> = options.iter().collect();
            if unique.len() != options.len() {
                return Err(Diagnostic("select options must be unique".into()));
            }
            if !options.contains(value) {
                return Err(Diagnostic("select value must name one option".into()));
            }
            Ok(())
        }
        Node::Slider {
            label,
            field,
            value,
            min,
            max,
            step,
            ..
        } => {
            required(label, "slider label")?;
            required(field, "slider field")?;
            check_slider(*value, *min, *max, *step)
        }
        Node::Button { label, event, .. } => {
            required(label, "button label")?;
            required(event, "button event")
        }
    }
}

fn check_children(
    children: &[Node],
    parent: Parent,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), Diagnostic> {
    for child in children {
        check_node(child, parent, depth + 1, nodes)?;
    }
    Ok(())
}

impl Node {
    fn fill(&self) -> bool {
        match self {
            Self::Screen { .. } | Self::Header { .. } | Self::Footer { .. } => false,
            Self::Col { fill, .. }
            | Self::Row { fill, .. }
            | Self::Text { fill, .. }
            | Self::Toggle { fill, .. }
            | Self::Select { fill, .. }
            | Self::Slider { fill, .. }
            | Self::Button { fill, .. } => *fill,
        }
    }
}

fn allowed_keys(ty: &str) -> Option<&'static [&'static str]> {
    Some(match ty {
        "screen" => &["type", "title", "children"],
        "col" => &["type", "fill", "alignment", "distribution", "children"],
        "row" => &[
            "type",
            "fill",
            "wrapping",
            "alignment",
            "distribution",
            "children",
        ],
        "header" | "footer" => &["type", "children"],
        "text" => &["type", "value", "fill"],
        "toggle" => &["type", "label", "field", "value", "fill"],
        "select" => &["type", "label", "field", "options", "value", "fill"],
        "slider" => &[
            "type", "label", "field", "value", "min", "max", "step", "fill",
        ],
        "button" => &[
            "type", "label", "event", "intent", "confirm", "disabled", "fill",
        ],
        _ => return None,
    })
}

fn child_arrays(ty: &str) -> &'static [&'static str] {
    match ty {
        "screen" | "col" | "row" | "header" | "footer" => &["children"],
        _ => &[],
    }
}

fn check_raw_node(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), Diagnostic> {
    if depth > MAX_DEPTH {
        return Err(Diagnostic(format!(
            "Matrix consumer tree exceeds depth {MAX_DEPTH}"
        )));
    }
    let object = value
        .as_object()
        .ok_or_else(|| Diagnostic("a Matrix consumer node must be an object".into()))?;
    let ty = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| Diagnostic("Matrix consumer node is missing a string 'type'".into()))?;
    let allowed = allowed_keys(ty).ok_or_else(|| Diagnostic(format!("unknown node type: {ty}")))?;
    *nodes += 1;
    if *nodes > MAX_NODES {
        return Err(Diagnostic(
            "Matrix consumer tree exceeds 2,000 nodes".into(),
        ));
    }
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(Diagnostic(format!(
                "unknown key '{key}' on node type '{ty}'"
            )));
        }
    }
    for child_key in child_arrays(ty) {
        if let Some(children) = object.get(*child_key).and_then(Value::as_array) {
            for child in children {
                check_raw_node(child, depth + 1, nodes)?;
            }
        }
    }
    Ok(())
}

pub fn validate(bytes: &[u8]) -> Result<Surface, Diagnostic> {
    if bytes.len() > MAX_BYTES {
        return Err(Diagnostic(format!(
            "Matrix consumer tree exceeds 256 KB ({} bytes, limit {MAX_BYTES})",
            bytes.len()
        )));
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| Diagnostic(format!("Matrix consumer tree is not valid JSON: {error}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| Diagnostic("Matrix consumer surface must be an object".into()))?;
    for key in object.keys() {
        if !["version", "root"].contains(&key.as_str()) {
            return Err(Diagnostic(format!(
                "unknown key '{key}' on Matrix consumer surface"
            )));
        }
    }
    let version = object.get("version").and_then(Value::as_u64);
    if version != Some(u64::from(CONTRACT_VERSION)) {
        return Err(Diagnostic(format!(
            "Matrix consumer contract requires version {CONTRACT_VERSION}"
        )));
    }
    let root = object
        .get("root")
        .ok_or_else(|| Diagnostic("Matrix consumer surface is missing root".into()))?;
    check_raw_node(root, 1, &mut 0)?;
    let surface: Surface = serde_json::from_value(value).map_err(|error| {
        Diagnostic(format!(
            "Matrix consumer tree does not match version 1: {error}"
        ))
    })?;
    surface.validate()?;
    Ok(surface)
}

pub fn validate_envelope(bytes: &[u8]) -> Result<SurfaceEnvelope, Diagnostic> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        Diagnostic(format!(
            "Matrix surface envelope is not valid JSON: {error}"
        ))
    })?;
    let object = value
        .as_object()
        .ok_or_else(|| Diagnostic("Matrix surface envelope must be an object".into()))?;
    for key in object.keys() {
        if !["surface_id", "surface", "matrix_surface"].contains(&key.as_str()) {
            return Err(Diagnostic(format!(
                "unknown key '{key}' on Matrix surface envelope"
            )));
        }
    }
    if object.get("surface_id").and_then(Value::as_str) != Some("matrix") {
        return Err(Diagnostic(
            "Matrix surface envelope requires surface_id 'matrix'".into(),
        ));
    }
    let fallback = object
        .get("surface")
        .ok_or_else(|| Diagnostic("Matrix surface envelope is missing v0 fallback".into()))?;
    let fallback_bytes = serde_json::to_vec(fallback)
        .map_err(|error| Diagnostic(format!("v0 fallback cannot serialize: {error}")))?;
    let surface = crate::validate(&fallback_bytes)
        .map_err(|error| Diagnostic(format!("invalid v0 fallback: {error}")))?;
    let matrix = object
        .get("matrix_surface")
        .ok_or_else(|| Diagnostic("Matrix surface envelope is missing matrix_surface".into()))?;
    let matrix_bytes = serde_json::to_vec(matrix)
        .map_err(|error| Diagnostic(format!("matrix_surface cannot serialize: {error}")))?;
    let matrix_surface = validate(&matrix_bytes)?;
    Ok(SurfaceEnvelope {
        surface_id: "matrix".into(),
        surface,
        matrix_surface,
    })
}

pub fn tree_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(Surface)).expect("v1 schema serializes")
}

pub fn tree_schema_pretty() -> String {
    serde_json::to_string_pretty(&tree_schema()).expect("v1 schema formats")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::path::Path;

    fn fixture_bytes() -> Vec<u8> {
        std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures-v1/01_slider_layout.json"),
        )
        .expect("v1 slider layout fixture")
    }

    fn fixture_value() -> Value {
        serde_json::from_slice(&fixture_bytes()).unwrap()
    }

    fn slider_mut(value: &mut Value) -> &mut serde_json::Map<String, Value> {
        value["root"]["children"][1]["children"][0]["children"][0]
            .as_object_mut()
            .unwrap()
    }

    #[test]
    fn exact_slider_anchor_fill_fixture_round_trips() {
        let bytes = fixture_bytes();
        let surface = validate(&bytes).unwrap();
        assert_eq!(
            serde_json::to_vec(&surface).unwrap(),
            bytes.strip_suffix(b"\n").unwrap()
        );
    }

    #[test]
    fn dual_envelope_requires_valid_v0_fallback_and_snake_case_v1() {
        let matrix_surface = fixture_value();
        let envelope = json!({
            "surface_id":"matrix",
            "surface":{"type":"text","value":"This Matrix surface requires a newer Mote client."},
            "matrix_surface":matrix_surface,
        });
        let decoded = validate_envelope(&serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert_eq!(decoded.surface_id, "matrix");
        assert!(matches!(decoded.surface, crate::Node::Text { .. }));

        let invalid_fallback = json!({
            "surface_id":"matrix",
            "surface":{"type":"blink"},
            "matrix_surface":fixture_value(),
        });
        let error = validate_envelope(&serde_json::to_vec(&invalid_fallback).unwrap()).unwrap_err();
        assert!(error.0.contains("v0 fallback"), "{error}");

        let camel_case = json!({
            "surface_id":"matrix",
            "surface":{"type":"text","value":"fallback"},
            "matrixSurface":fixture_value(),
        });
        let error = validate_envelope(&serde_json::to_vec(&camel_case).unwrap()).unwrap_err();
        assert!(error.0.contains("matrixSurface"), "{error}");
    }

    #[test]
    fn rejects_unknown_version_type_and_key() {
        for (candidate, expected) in [
            (
                json!({"version":2,"root":{"type":"screen","children":[]}}),
                "version 1",
            ),
            (
                json!({"version":1,"root":{"type":"blink"}}),
                "unknown node type",
            ),
            (
                json!({"version":1,"root":{"type":"screen","color":"red","children":[]}}),
                "unknown key 'color'",
            ),
        ] {
            let error = validate(&serde_json::to_vec(&candidate).unwrap()).unwrap_err();
            assert!(error.0.contains(expected), "{error}");
        }
    }

    #[test]
    fn enforces_byte_depth_and_node_limits() {
        let oversized = json!({
            "version": 1,
            "root": {"type":"screen","children":[{"type":"text","value":"x".repeat(257 * 1024)}]}
        });
        let error = validate(&serde_json::to_vec(&oversized).unwrap()).unwrap_err();
        assert!(error.0.contains("256 KB"), "{error}");

        let mut nested = json!({"type":"text","value":"leaf"});
        for _ in 0..12 {
            nested = json!({"type":"col","children":[nested]});
        }
        let error = validate(&serde_json::to_vec(&json!({"version":1,"root":nested})).unwrap())
            .unwrap_err();
        assert!(error.0.contains("depth 12"), "{error}");

        let children: Vec<_> = (0..2_000)
            .map(|index| json!({"type":"text","value":index.to_string()}))
            .collect();
        let error = validate(
            &serde_json::to_vec(&json!({"version":1,"root":{"type":"screen","children":children}}))
                .unwrap(),
        )
        .unwrap_err();
        assert!(error.0.contains("2,000 nodes"), "{error}");
    }

    #[test]
    fn enforces_slider_range_step_and_finite_values() {
        let cases = [
            ("value", json!(101), "inside its range"),
            ("min", json!(101), "min cannot exceed max"),
            ("step", json!(0), "step must be positive"),
            ("value", json!(52), "aligned to step"),
        ];
        for (key, replacement, expected) in cases {
            let mut candidate = fixture_value();
            slider_mut(&mut candidate).insert(key.into(), replacement);
            let error = validate(&serde_json::to_vec(&candidate).unwrap()).unwrap_err();
            assert!(error.0.contains(expected), "{key}: {error}");
        }

        let mut surface = validate(&fixture_bytes()).unwrap();
        let Node::Screen { children, .. } = &mut surface.root else {
            unreachable!()
        };
        let Node::Col { children, .. } = &mut children[1] else {
            unreachable!()
        };
        let Node::Row { children, .. } = &mut children[0] else {
            unreachable!()
        };
        let Node::Slider { value, .. } = &mut children[0] else {
            unreachable!()
        };
        *value = f64::NAN;
        let error = surface.validate().unwrap_err();
        assert!(error.0.contains("finite"), "{error}");
    }

    #[test]
    fn fill_is_only_valid_on_direct_row_or_col_children() {
        let candidate = json!({"version":1,"root":{"type":"screen","children":[
            {"type":"text","value":"bad","fill":true}
        ]}});
        let error = validate(&serde_json::to_vec(&candidate).unwrap()).unwrap_err();
        assert!(
            error.0.contains("fill requires a direct row or col parent"),
            "{error}"
        );

        let anchor = json!({"version":1,"root":{"type":"screen","children":[
            {"type":"header","fill":true,"children":[]}
        ]}});
        let error = validate(&serde_json::to_vec(&anchor).unwrap()).unwrap_err();
        assert!(error.0.contains("unknown key 'fill'"), "{error}");
    }

    #[test]
    fn anchors_are_unique_direct_screen_children() {
        for candidate in [
            json!({"version":1,"root":{"type":"screen","children":[
                {"type":"header","children":[]},{"type":"header","children":[]}
            ]}}),
            json!({"version":1,"root":{"type":"screen","children":[
                {"type":"col","children":[{"type":"footer","children":[]}]}
            ]}}),
        ] {
            let error = validate(&serde_json::to_vec(&candidate).unwrap()).unwrap_err();
            assert!(error.0.contains("header and footer"), "{error}");
        }
    }

    #[test]
    fn catalog_inventory_names_every_frozen_and_unresolved_word() {
        let names: Vec<_> = catalog_inventory().iter().map(|entry| entry.name).collect();
        assert_eq!(
            names,
            [
                "screen",
                "section",
                "card",
                "col",
                "row",
                "tabs",
                "tab",
                "header",
                "footer",
                "title",
                "text",
                "caption",
                "label",
                "code",
                "md",
                "link",
                "badge",
                "tag",
                "stat",
                "kv",
                "icon",
                "image",
                "avatar",
                "timestamp",
                "divider",
                "field",
                "area",
                "toggle",
                "check",
                "choice",
                "select",
                "multiselect",
                "segmented",
                "slider",
                "stepper",
                "date",
                "time",
                "search",
                "button",
                "form",
                "list",
                "item",
                "table",
                "column",
                "grid",
                "tile",
                "progress",
                "steps",
                "status",
                "banner",
                "empty",
                "error",
                "typing",
                "chat",
                "message",
                "compose",
                "dialog",
                "sheet",
                "menu",
                "tooltip",
            ]
        );
        let frozen: Vec<_> = catalog_inventory()
            .iter()
            .filter(|entry| entry.status == ContractStatus::Frozen)
            .map(|entry| entry.name)
            .collect();
        assert_eq!(
            frozen,
            [
                "screen", "col", "row", "header", "footer", "text", "toggle", "select", "slider",
                "button"
            ]
        );
    }

    #[test]
    fn schema_is_deterministic_and_matches_the_committed_file() {
        let first = tree_schema_pretty();
        let second = tree_schema_pretty();
        assert_eq!(first, second);
        let committed = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/tree.matrix.v1.json"),
        )
        .expect("schemas/tree.matrix.v1.json must exist; run gen_matrix_v1_schema");
        assert_eq!(committed.trim_end(), first.trim_end());
    }

    #[test]
    fn schema_closes_version_and_node_inventory() {
        let schema = tree_schema();
        assert_eq!(schema["properties"]["version"]["minimum"], json!(1.0));
        assert_eq!(schema["properties"]["version"]["maximum"], json!(1.0));
        assert_eq!(schema["additionalProperties"], json!(false));
        let variants = schema["definitions"]["Node"]["oneOf"].as_array().unwrap();
        let types: Vec<_> = variants
            .iter()
            .map(|variant| {
                assert_eq!(variant["additionalProperties"], json!(false));
                variant["properties"]["type"]["enum"][0].as_str().unwrap()
            })
            .collect();
        assert_eq!(
            types,
            [
                "screen", "col", "row", "header", "footer", "text", "toggle", "select", "slider",
                "button"
            ]
        );
        for anchor_type in ["header", "footer"] {
            let anchor = variants
                .iter()
                .find(|variant| variant["properties"]["type"]["enum"][0] == anchor_type)
                .unwrap();
            assert!(anchor["properties"].get("fill").is_none());
        }
    }
}
