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

fn live_harness(node: mote_ui::Node) -> ObservationHarness {
    let mut harness = ObservationHarness::default();
    harness.event(&mote_eventlog::Event {
        seq: 17,
        ts: 0,
        scope_id: "main".into(),
        kind: "ui.surface".into(),
        payload: json!({"surface_id":"matrix", "surface":node}),
    });
    harness
}

fn find_node(
    node: accesskit_consumer::Node<'_>,
    role: accesskit::Role,
    name: &str,
) -> Option<accesskit::NodeId> {
    if node.role() == role && node.label().as_deref() == Some(name) {
        return Some(node.id());
    }
    node.children()
        .find_map(|child| find_node(child, role, name))
}

fn action(
    harness: &mut ObservationHarness,
    target: accesskit::NodeId,
    action: accesskit::Action,
    data: Option<accesskit::ActionData>,
) -> egui::FullOutput {
    harness
        .frame_output(vec![egui::Event::AccessKitActionRequest(
            accesskit::ActionRequest { action, target, data },
        )])
        .1
}

#[test]
fn accessibility_click_targets_reach_toggle_and_action_widgets() {
    let mut toggle = live_harness(mote_ui::Node::Toggle {
        label: "Local only".into(),
        name: "local_only".into(),
        value: true,
    });
    let tree = Tree::new(toggle.accesskit_update(), true);
    let toggle_id = crate::surface::observation::take_layout()
        .controls.iter().find(|control| control.kind == "Toggle").unwrap().id;
    let target = find_node(tree.state().root(), accesskit::Role::Switch, "Local only").unwrap();
    action(&mut toggle, target, accesskit::Action::Focus, None);
    assert_eq!(toggle.ctx.memory(|memory| memory.focused()), Some(toggle_id));
    action(&mut toggle, target, accesskit::Action::Click, None);
    assert_eq!(toggle.queued(), [json!({
        "thread":"main", "event":"change", "args":{"field":"local_only", "value":false},
        "surface_seq":17, "node_path":[]
    })]);

    let mut action_harness = live_harness(mote_ui::Node::Action {
        label: "Approve".into(),
        event: "approve".into(),
    });
    let tree = Tree::new(action_harness.accesskit_update(), true);
    let action_id = crate::surface::observation::take_layout()
        .controls.iter().find(|control| control.kind == "Action").unwrap().id;
    let target = find_node(tree.state().root(), accesskit::Role::Button, "Approve").unwrap();
    action(&mut action_harness, target, accesskit::Action::Focus, None);
    assert_eq!(action_harness.ctx.memory(|memory| memory.focused()), Some(action_id));
    action(&mut action_harness, target, accesskit::Action::Click, None);
    assert_eq!(action_harness.queued(), [json!({
        "thread":"main", "event":"approve", "args":{}, "surface_seq":17, "node_path":[]
    })]);
}

#[test]
fn accessibility_click_target_opens_the_select_widget() {
    let mut harness = live_harness(mote_ui::Node::Select {
        label: "Talk model".into(),
        name: "models.talk".into(),
        options: vec!["qwen3:4b".into(), "qwen3:0.6b".into()],
        value: "qwen3:4b".into(),
    });
    let tree = Tree::new(harness.accesskit_update(), true);
    let select_id = crate::surface::observation::take_layout()
        .controls.iter().find(|control| control.kind == "Select").unwrap().id;
    let target = find_node(tree.state().root(), accesskit::Role::ComboBox, "Talk model").unwrap();
    action(&mut harness, target, accesskit::Action::Focus, None);
    assert_eq!(harness.ctx.memory(|memory| memory.focused()), Some(select_id));
    action(&mut harness, target, accesskit::Action::Click, None);
    let layout = crate::surface::observation::take_layout();
    let select = layout.controls.iter().find(|control| control.kind == "Select").unwrap();
    assert!(egui::ComboBox::is_open(&harness.ctx, select.id));
}

#[test]
fn accessibility_field_keeps_real_focus_and_text_selection_actions() {
    let mut harness = live_harness(mote_ui::Node::Field {
        label: "Name".into(),
        name: "display_name".into(),
        value: "owner".into(),
    });
    let update = harness.accesskit_update();
    let layout = crate::surface::observation::take_layout();
    let field_id = layout.controls.iter().find(|control| control.kind == "Field").unwrap().id;
    let tree = Tree::new(update, true);
    let field_id_in_tree = find_node(tree.state().root(), accesskit::Role::TextInput, "Name").unwrap();
    let field = tree.state().node_by_id(field_id_in_tree).unwrap();
    assert!(field.supports_text_ranges());
    let text_run = field.children().find(|child| child.role() == accesskit::Role::TextRun).unwrap();

    action(&mut harness, field_id_in_tree, accesskit::Action::Focus, None);
    assert_eq!(harness.ctx.memory(|memory| memory.focused()), Some(field_id));

    let selection = accesskit::TextSelection {
        anchor: accesskit::TextPosition { node: text_run.id(), character_index: 1 },
        focus: accesskit::TextPosition { node: text_run.id(), character_index: 4 },
    };
    let output = action(
        &mut harness,
        field_id_in_tree,
        accesskit::Action::SetTextSelection,
        Some(accesskit::ActionData::SetTextSelection(selection)),
    );
    let tree = Tree::new(output.platform_output.accesskit_update.unwrap(), true);
    let field_id_in_tree = find_node(tree.state().root(), accesskit::Role::TextInput, "Name").unwrap();
    assert_eq!(tree.state().node_by_id(field_id_in_tree).unwrap().raw_text_selection(), Some(&selection));
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
