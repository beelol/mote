//! Surface tree: the compact JSON UI the model emits, its validator, and the generated JSON
//! schema. See docs/rfcs/v0.md section 12. Node types are `type` first, `children` last. The
//! renderer lives in [`render`].

pub mod render;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Limits from the contract.
pub const MAX_DEPTH: usize = 12;
pub const MAX_NODES: usize = 2000;
pub const MAX_BYTES: usize = 256 * 1024;

/// A surface tree node. Internally tagged by `type`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    Text { value: String },
    Stack { #[serde(default)] children: Vec<Node> },
    Row { #[serde(default)] children: Vec<Node> },
    List { #[serde(default)] children: Vec<Node> },
    Table { columns: Vec<String>, rows: Vec<Vec<String>> },
    Field { label: String, name: String, #[serde(default)] value: String },
    Toggle { label: String, name: String, #[serde(default)] value: bool },
    Select { label: String, name: String, options: Vec<String>, #[serde(default)] value: String },
    Action { label: String, event: String },
    Progress { value: f32 },
    Chat { #[serde(default)] children: Vec<Node> },
}

/// Allowed JSON keys for a node type, including `type`.
fn allowed_keys(ty: &str) -> Option<&'static [&'static str]> {
    Some(match ty {
        "text" => &["type", "value"],
        "stack" | "row" | "list" | "chat" => &["type", "children"],
        "table" => &["type", "columns", "rows"],
        "field" => &["type", "label", "name", "value"],
        "toggle" => &["type", "label", "name", "value"],
        "select" => &["type", "label", "name", "options", "value"],
        "action" => &["type", "label", "event"],
        "progress" => &["type", "value"],
        _ => return None,
    })
}

fn child_arrays(ty: &str) -> &'static [&'static str] {
    match ty {
        "stack" | "row" | "list" | "chat" => &["children"],
        _ => &[],
    }
}

/// A validation failure with a human diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic(pub String);

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Validate raw tree JSON. On success returns the typed tree; on failure a named diagnostic.
pub fn validate(bytes: &[u8]) -> Result<Node, Diagnostic> {
    if bytes.len() > MAX_BYTES {
        return Err(Diagnostic(format!(
            "tree exceeds 256 KB ({} bytes, limit {})",
            bytes.len(),
            MAX_BYTES
        )));
    }
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| Diagnostic(format!("tree is not valid JSON: {e}")))?;

    let mut count = 0usize;
    check_value(&value, 1, &mut count)?;

    serde_json::from_value::<Node>(value)
        .map_err(|e| Diagnostic(format!("tree does not match the surface schema: {e}")))
}

/// Validate, returning a `text` node carrying the diagnostic on failure, as the contract
/// specifies for a rejected tree.
pub fn validate_to_node(bytes: &[u8]) -> Node {
    match validate(bytes) {
        Ok(node) => node,
        Err(d) => Node::Text { value: d.0 },
    }
}

fn check_value(value: &Value, depth: usize, count: &mut usize) -> Result<(), Diagnostic> {
    if depth > MAX_DEPTH {
        return Err(Diagnostic(format!("tree exceeds max depth {MAX_DEPTH}")));
    }
    let obj = value
        .as_object()
        .ok_or_else(|| Diagnostic("a node must be a JSON object".to_string()))?;
    let ty = obj
        .get("type")
        .and_then(|t| t.as_str())
        .ok_or_else(|| Diagnostic("node is missing a string 'type'".to_string()))?;
    let allowed =
        allowed_keys(ty).ok_or_else(|| Diagnostic(format!("unknown node type: {ty}")))?;

    *count += 1;
    if *count > MAX_NODES {
        return Err(Diagnostic(format!("tree exceeds {MAX_NODES} nodes")));
    }

    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(Diagnostic(format!("unknown key '{key}' on node type '{ty}'")));
        }
    }

    for arr_key in child_arrays(ty) {
        if let Some(children) = obj.get(*arr_key).and_then(|c| c.as_array()) {
            for child in children {
                check_value(child, depth + 1, count)?;
            }
        }
    }
    Ok(())
}

/// The JSON schema for the surface tree, generated from [`Node`].
pub fn tree_schema() -> Value {
    let schema = schemars::schema_for!(Node);
    serde_json::to_value(schema).expect("schema serializes")
}

/// Pretty-printed schema, as committed at `schemas/tree.v0.json`.
pub fn tree_schema_pretty() -> String {
    serde_json::to_string_pretty(&tree_schema()).expect("schema pretty")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn fixtures_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
    }

    fn fixture_files() -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(fixtures_dir())
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
            .collect();
        files.sort();
        files
    }

    #[test]
    fn accepts_all_ten_fixtures() {
        let files = fixture_files();
        assert_eq!(files.len(), 10, "expected ten fixtures, found {}", files.len());
        for f in files {
            let bytes = std::fs::read(&f).unwrap();
            validate(&bytes).unwrap_or_else(|d| panic!("fixture {f:?} rejected: {d}"));
        }
    }

    #[test]
    fn rejects_unknown_type() {
        let d = validate(br#"{"type":"blink","value":"x"}"#).unwrap_err();
        assert!(d.0.contains("unknown node type: blink"), "{d}");
    }

    #[test]
    fn rejects_unknown_key() {
        let d = validate(br#"{"type":"text","value":"x","foo":1}"#).unwrap_err();
        assert!(d.0.contains("unknown key 'foo'"), "{d}");
    }

    #[test]
    fn rejects_depth_13() {
        // 13 nested stacks exceed the depth-12 limit.
        let mut tree = String::new();
        for _ in 0..13 {
            tree.push_str(r#"{"type":"stack","children":["#);
        }
        tree.push_str(r#"{"type":"text","value":"deep"}"#);
        for _ in 0..13 {
            tree.push_str("]}");
        }
        let d = validate(tree.as_bytes()).unwrap_err();
        assert!(d.0.contains("max depth"), "{d}");
    }

    #[test]
    fn rejects_2001_nodes() {
        // One stack plus 2000 text children = 2001 nodes.
        let mut children = Vec::new();
        for i in 0..2000 {
            children.push(format!(r#"{{"type":"text","value":"{i}"}}"#));
        }
        let tree = format!(r#"{{"type":"stack","children":[{}]}}"#, children.join(","));
        let d = validate(tree.as_bytes()).unwrap_err();
        assert!(d.0.contains("2000 nodes"), "{d}");
    }

    #[test]
    fn rejects_257kb() {
        let value = "x".repeat(257 * 1024);
        let tree = format!(r#"{{"type":"text","value":"{value}"}}"#);
        let d = validate(tree.as_bytes()).unwrap_err();
        assert!(d.0.contains("256 KB"), "{d}");
    }

    #[test]
    fn rejected_tree_becomes_text_node_with_diagnostic() {
        let node = validate_to_node(br#"{"type":"blink"}"#);
        match node {
            Node::Text { value } => assert!(value.contains("unknown node type")),
            other => panic!("expected text node, got {other:?}"),
        }
    }

    #[test]
    fn fixtures_render_to_matching_golden_pngs() {
        let files = fixture_files();
        assert_eq!(files.len(), 10);
        for f in files {
            let bytes = std::fs::read(&f).unwrap();
            let node = validate(&bytes).unwrap();
            let png = render::render_png(&node);
            let golden_path = f.with_extension("png");
            let golden = std::fs::read(&golden_path)
                .unwrap_or_else(|_| panic!("missing golden {golden_path:?}; run gen_goldens"));
            assert_eq!(png, golden, "render drifted from golden for {f:?}");
        }
    }

    #[test]
    fn schema_file_matches_generated() {
        let committed_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/tree.v0.json");
        let committed = std::fs::read_to_string(&committed_path)
            .expect("schemas/tree.v0.json must exist; run the schema generator");
        assert_eq!(
            committed.trim_end(),
            tree_schema_pretty().trim_end(),
            "schemas/tree.v0.json drifted from the Rust types; regenerate it"
        );
    }
}
