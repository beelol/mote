use super::*;
use crate::surface::observation::{self, Control, Layout};

const VIEWPORTS: [egui::Vec2; 2] = [egui::vec2(800.0, 480.0), egui::vec2(1280.0, 800.0)];

fn fixtures() -> Vec<(String, Node)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates/mote-ui/fixtures");
    let mut paths: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 10);
    paths
        .into_iter()
        .map(|p| {
            (
                p.file_stem().unwrap().to_str().unwrap().into(),
                mote_ui::validate(&std::fs::read(p).unwrap()).unwrap(),
            )
        })
        .collect()
}

fn harness(node: &Node, viewport: egui::Vec2, preview: bool) -> ObservationHarness {
    let mut harness = ObservationHarness {
        viewport,
        ..Default::default()
    };
    if preview {
        harness.app.fixture = Some(node.clone());
    } else {
        harness.event(&Event {
            seq: 17,
            ts: 0,
            scope_id: "main".into(),
            kind: "ui.surface".into(),
            payload: serde_json::json!({"surface_id":"matrix", "surface":node}),
        });
    }
    harness
}

fn measured(harness: &mut ObservationHarness) -> (Vec<DisplayedNode>, egui::FullOutput, Layout) {
    let (nodes, output) = harness.frame_output(vec![]);
    (nodes, output, observation::take_layout())
}

fn targets(layout: &Layout) -> Vec<String> {
    layout
        .controls
        .iter()
        .filter_map(|c| {
            (c.hit.width() < 48.0 || c.hit.height() < 48.0).then(|| {
                format!(
                    "{} visual={:?}, actual hit={:?}, clip={:?}",
                    c.kind, c.visual, c.hit, c.clip
                )
            })
        })
        .collect()
}

#[test]
fn layout_actual_control_measurements_include_popup_options() {
    let mut failures = Vec::new();
    for viewport in VIEWPORTS {
        for (name, node) in fixtures() {
            let mut h = harness(&node, viewport, true);
            measured(&mut h);
            let (_, _, layout) = measured(&mut h);
            for control in &layout.controls {
                eprintln!("{name} {viewport:?}: {control:?}");
            }
            failures.extend(targets(&layout));
            if matches!(node, Node::Select { .. }) {
                tap(&mut h, &layout.controls[0], false);
                let (_, _, popup) = measured(&mut h);
                for control in &popup.controls {
                    eprintln!("popup {viewport:?}: {control:?}");
                }
                failures.extend(targets(&popup));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn assert_geometry(layout: &Layout, screen: egui::Rect) {
    assert_eq!(
        layout.scrolls.len(),
        1,
        "only the existing outer scroll is intended"
    );
    let scroll = &layout.scrolls[0];
    assert!(matches!(scroll.name, "preview" | "chat"));
    assert!(screen.contains_rect(scroll.viewport));
    assert!(screen.contains_rect(scroll.clip));
    assert!(scroll.clip.top() >= scroll.viewport.top() - 3.0);
    assert!(scroll.clip.bottom() <= scroll.viewport.bottom() + 3.0);
    for (bounds, clip) in &layout.paints {
        assert!(
            scroll.clip.contains_rect(*clip),
            "paint escaped scroll clip: {bounds:?} {clip:?} {scroll:?}"
        );
        let visible = bounds.intersect(*clip);
        if visible.is_positive() {
            assert!(screen.contains_rect(visible));
        }
    }
    for (i, control) in layout.controls.iter().enumerate() {
        assert!(
            control.clip.contains_rect(control.hit),
            "hit escapes clip: {control:?}"
        );
        assert!(
            screen.contains_rect(control.hit),
            "control outside screen: {control:?}"
        );
        for other in &layout.controls[i + 1..] {
            if control.layer == other.layer {
                assert!(
                    !control.hit.intersect(other.hit).is_positive(),
                    "overlapping hits: {control:?} {other:?}"
                );
            }
        }
    }
    for (bounds, clip, popup_clip) in &layout.popup_paints {
        assert!(
            popup_clip.contains_rect(*clip),
            "popup paint escaped its clip"
        );
        assert!(
            screen.contains_rect(*popup_clip),
            "popup clip outside screen"
        );
        let visible = bounds.intersect(*clip);
        if visible.is_positive() {
            assert!(screen.contains_rect(visible));
        }
    }
}

#[test]
fn layout_all_ten_fixtures_at_both_golden_viewports() {
    let mut failures = Vec::new();
    let mut cases = 0;
    let mut controls = 0;
    for viewport in VIEWPORTS {
        for preview in [true, false] {
            for (name, node) in fixtures() {
                let mut h = harness(&node, viewport, preview);
                measured(&mut h);
                let (first, _, layout) = measured(&mut h);
                controls += layout.controls.len();
                failures.extend(
                    targets(&layout)
                        .into_iter()
                        .map(|f| format!("{name} {viewport:?} preview={preview}: {f}")),
                );
                assert_geometry(
                    &layout,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, viewport),
                );
                let (second, _, _) = measured(&mut h);
                assert_eq!(first, second, "unstable reading order or bounds: {name}");
                let paths: Vec<_> = first
                    .iter()
                    .filter(|n| {
                        !matches!(
                            n.node,
                            Node::Stack { .. }
                                | Node::Row { .. }
                                | Node::List { .. }
                                | Node::Chat { .. }
                        )
                    })
                    .map(|n| n.path.clone())
                    .collect();
                let mut sorted = paths.clone();
                sorted.sort();
                assert_eq!(paths, sorted, "structural reading order: {name}");
                cases += 1;
            }
        }
    }
    eprintln!("{cases} fixture/view/mode cases; {controls} actual control measurements");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn layout_200_list_items_and_500_table_rows_reach_the_final_row() {
    for viewport in VIEWPORTS {
        for preview in [true, false] {
            for (name, node, last) in [
                (
                    "List",
                    Node::List {
                        children: (0..200)
                            .map(|i| Node::Text {
                                value: format!("item {i:03}"),
                            })
                            .collect(),
                    },
                    "item 199",
                ),
                (
                    "Table",
                    Node::Table {
                        columns: vec!["name".into(), "value".into()],
                        rows: (0..500)
                            .map(|i| vec![format!("row {i:03}"), format!("value {i:03}")])
                            .collect(),
                    },
                    "row 499",
                ),
            ] {
                let mut h = harness(&node, viewport, preview);
                measured(&mut h);
                let (before, _, layout) = measured(&mut h);
                let scroll = &layout.scrolls[0];
                assert!(scroll.content.y > scroll.viewport.height());
                assert_eq!(scroll.offset.y, 0.0);
                assert_geometry(
                    &layout,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, viewport),
                );
                h.frame(vec![
                    egui::Event::PointerMoved(scroll.viewport.center()),
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0.0, -100_000.0),
                        modifiers: Default::default(),
                    },
                ]);
                for _ in 0..30 {
                    measured(&mut h);
                }
                let (after, output, layout) = measured(&mut h);
                let scroll = &layout.scrolls[0];
                assert_geometry(
                    &layout,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, viewport),
                );
                assert!(
                    (scroll.offset.y - (scroll.content.y - scroll.viewport.height())).abs() < 1.0,
                    "did not reach end: {scroll:?}"
                );
                let last_bounds = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Text(t) if t.galley.text() == last => {
                            Some(t.galley.rect.translate(t.pos.to_vec2()))
                        }
                        _ => None,
                    })
                    .expect("final row painted");
                assert!(
                    scroll.viewport.contains_rect(last_bounds),
                    "final row not fully reachable: {last_bounds:?} {scroll:?}"
                );
                assert_eq!(
                    before.iter().map(|n| &n.path).collect::<Vec<_>>(),
                    after.iter().map(|n| &n.path).collect::<Vec<_>>()
                );
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        egui::Shape::Text(t)
                            if t.galley.text().starts_with("row ")
                                || t.galley.text().starts_with("item ") =>
                        {
                            Some(t.galley.text())
                        }
                        _ => None,
                    })
                    .collect();
                assert!(
                    labels.windows(2).all(|pair| pair[0] < pair[1]),
                    "visual reading order"
                );
                assert!(h.queued().is_empty());
                eprintln!(
                    "{name} {viewport:?} preview={preview}: {scroll:?}; final={last_bounds:?}"
                );
            }
        }
    }
}

fn position(control: &Control, edge: i32) -> egui::Pos2 {
    control.hit.center() + egui::vec2(0.0, edge as f32 * (control.hit.height() / 2.0 - 1.0))
}

fn tap(h: &mut ObservationHarness, control: &Control, edge: bool) {
    h.click(position(control, i32::from(edge)));
}

#[test]
fn layout_fixture_controls_activate_at_center_and_padded_edge() {
    for viewport in VIEWPORTS {
        for edge in [-1, 0, 1] {
            for (name, node) in fixtures().into_iter().filter(|(_, n)| {
                matches!(
                    n,
                    Node::Field { .. }
                        | Node::Toggle { .. }
                        | Node::Select { .. }
                        | Node::Action { .. }
                )
            }) {
                let mut h = harness(&node, viewport, false);
                measured(&mut h);
                let (_, _, layout) = measured(&mut h);
                let control = &layout.controls[0];
                assert!(control.path.is_empty());
                assert!(
                    targets(&Layout {
                        controls: vec![control.clone()],
                        ..Default::default()
                    })
                    .is_empty(),
                    "{name}: {control:?}"
                );
                h.click(position(control, edge));
                match &node {
                    Node::Field { .. } => {
                        assert_eq!(h.ctx.memory(|m| m.focused()), Some(control.id));
                        h.edit("edge edit");
                        let (_, output, _) = measured(&mut h);
                        assert!(output.shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(t) if t.galley.text() == "edge edit")));
                        assert!(h.queued().is_empty());
                    }
                    Node::Toggle { .. } | Node::Action { .. } => {
                        let events = h.queued();
                        assert_eq!(events.len(), 1);
                        assert_eq!(events[0]["surface_seq"], 17);
                        assert_eq!(events[0]["node_path"], serde_json::json!([]));
                    }
                    Node::Select { options, .. } => {
                        assert!(h.queued().is_empty());
                        let (_, _, popup) = measured(&mut h);
                        let choices: Vec<_> = popup
                            .controls
                            .iter()
                            .filter(|c| c.kind == "Option")
                            .collect();
                        assert_eq!(choices.len(), options.len(), "trigger did not open popup");
                        assert!(!popup.popup_paints.is_empty());
                        assert!(targets(&popup).is_empty(), "{:?}", targets(&popup));
                        assert_geometry(
                            &popup,
                            egui::Rect::from_min_size(egui::Pos2::ZERO, viewport),
                        );
                        for pair in choices.windows(2) {
                            assert!(pair[0].hit.bottom() <= pair[1].hit.top());
                        }
                        h.click(position(choices[0], edge));
                        assert!(
                            h.queued().is_empty(),
                            "reselecting the current value emits no event"
                        );
                        let (_, _, closed) = measured(&mut h);
                        assert!(!closed.controls.iter().any(|c| c.kind == "Option"));
                        let trigger = closed.controls.iter().find(|c| c.kind == "Select").unwrap();
                        h.click(position(trigger, edge));
                        let (_, _, reopened) = measured(&mut h);
                        let second = &reopened
                            .controls
                            .iter()
                            .filter(|c| c.kind == "Option")
                            .collect::<Vec<_>>()[1];
                        h.click(position(second, edge));
                        assert_eq!(
                            h.queued(),
                            [
                                serde_json::json!({"thread":"main", "event":"change", "args":{"field":"models.talk", "value":"qwen3:0.6b"}, "surface_seq":17, "node_path":[]})
                            ]
                        );
                    }
                    _ => unreachable!(),
                }
            }
        }
    }
}

#[test]
fn layout_adjacent_controls_preserve_hit_separation_and_reading_order() {
    let children: Vec<_> = fixtures()
        .into_iter()
        .map(|(_, n)| n)
        .filter(|n| {
            matches!(
                n,
                Node::Field { .. }
                    | Node::Toggle { .. }
                    | Node::Select { .. }
                    | Node::Action { .. }
            )
        })
        .collect();
    for viewport in VIEWPORTS {
        for node in [
            Node::Stack {
                children: children.clone(),
            },
            Node::Row {
                children: children.clone(),
            },
        ] {
            for preview in [true, false] {
                let mut h = harness(&node, viewport, preview);
                measured(&mut h);
                let (_, _, layout) = measured(&mut h);
                assert!(targets(&layout).is_empty(), "{:?}", targets(&layout));
                assert_geometry(
                    &layout,
                    egui::Rect::from_min_size(egui::Pos2::ZERO, viewport),
                );
                let surface_controls: Vec<_> = layout
                    .controls
                    .iter()
                    .filter(|c| !c.path.is_empty())
                    .collect();
                assert_eq!(
                    surface_controls.iter().map(|c| c.kind).collect::<Vec<_>>(),
                    ["Field", "Toggle", "Select", "Action"]
                );
                for (index, control) in surface_controls.iter().enumerate() {
                    assert_eq!(control.path, [index]);
                }
                for pair in surface_controls.windows(2) {
                    assert!(
                        pair[1].hit.top() > pair[0].hit.top()
                            || pair[1].hit.left() > pair[0].hit.right(),
                        "visual reading order"
                    );
                }
            }
        }
    }
}

#[test]
fn layout_chat_composer_and_send_respond_at_center_and_padded_edges() {
    for viewport in VIEWPORTS {
        for edge in [-1, 0, 1] {
            let mut h = harness(
                &Node::Text {
                    value: "chat".into(),
                },
                viewport,
                false,
            );
            measured(&mut h);
            let (_, _, layout) = measured(&mut h);
            let composer = layout
                .controls
                .iter()
                .find(|c| c.kind == "Composer")
                .unwrap();
            h.click(position(composer, edge));
            assert_eq!(h.ctx.memory(|m| m.focused()), Some(composer.id));
            h.edit("local edit");
            assert_eq!(h.app.input, "local edit");
            h.edit("");
            h.key(egui::Key::Backspace, false);
            assert!(h.app.input.is_empty());
            let send = layout.controls.iter().find(|c| c.kind == "Send").unwrap();
            let pos = position(send, edge);
            let (_, output) = h.frame_output(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                },
            ]);
            assert!(
                output.platform_output.events.iter().any(|e| matches!(e,
                egui::output::OutputEvent::Clicked(info) if info.label.as_deref() == Some("Send")))
            );
            assert!(h.queued().is_empty());
            assert_eq!(h.app.chat.messages.len(), 1, "empty Send creates no turn");
        }
    }
}
