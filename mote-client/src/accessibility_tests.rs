use super::*;
use accesskit_consumer::{common_filter, FilterResult, Tree};
use serde_json::{json, Value};
use std::path::PathBuf;

const VIEWPORTS: [egui::Vec2; 2] = [egui::vec2(800.0, 480.0), egui::vec2(1280.0, 800.0)];

fn nested(node: accesskit_consumer::Node<'_>) -> Value {
    let children = node
        .filtered_children(common_filter)
        .map(nested)
        .collect::<Vec<_>>();
    let mut value = json!({"role": format!("{:?}", node.role())});
    if let Some(name) = node.label() {
        value["name"] = name.into();
    }
    if node.role() != accesskit::Role::Label {
        if let Some(current) = node.value() {
            value["value"] = current.into();
        }
    }
    if let Some(toggled) = node.toggled() {
        value["toggled"] = format!("{toggled:?}").into();
    }
    if let Some(numeric_value) = node.numeric_value() {
        value["numeric_value"] = numeric_value.into();
    }
    if !children.is_empty() {
        value["children"] = children.into();
    }
    value
}

fn observed(node: mote_ui::Node, viewport: egui::Vec2) -> Value {
    let mut harness = ObservationHarness::default();
    harness.preview(node, viewport);
    let tree = Tree::new(harness.accesskit_update(), true);
    let root = tree.state().root();
    assert_eq!(common_filter(&root), FilterResult::Include);
    nested(root)
}

fn fixtures() -> Vec<(String, mote_ui::Node)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates/mote-ui/fixtures");
    let mut paths = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths.len(), 10);
    paths
        .into_iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_str().unwrap().to_owned();
            let node = mote_ui::validate(&std::fs::read(path).unwrap()).unwrap();
            (name, node)
        })
        .chain(std::iter::once(("gallery".into(), crate::gallery::tree())))
        .collect()
}

#[test]
fn accessibility_all_fixtures_and_gallery_match_exact_role_trees() {
    let golden_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/roles");
    let mut comparisons = 0;
    for (name, node) in fixtures() {
        let expected: Value = serde_json::from_slice(
            &std::fs::read(golden_root.join(format!("{name}.json"))).unwrap(),
        )
        .unwrap();
        let mut first = None;
        for viewport in VIEWPORTS {
            let actual = observed(node.clone(), viewport);
            assert_eq!(actual, expected, "{name} at {viewport:?}");
            if let Some(first) = &first {
                assert_eq!(&actual, first, "{name} changed across approved viewports");
            } else {
                first = Some(actual);
            }
            comparisons += 1;
        }
    }
    eprintln!("{comparisons} exact production-shell AccessKit role-tree comparisons");
    assert_eq!(comparisons, 22);
}
