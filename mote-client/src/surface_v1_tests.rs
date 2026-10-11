use super::*;
use mote_ui::v1::{Alignment, Distribution, Node, Wrapping};

const SIZES: [egui::Vec2; 2] = [egui::vec2(400.0, 800.0), egui::vec2(1200.0, 800.0)];

fn text(value: &str, fill: bool) -> Node {
    Node::Text {
        value: value.into(),
        fill,
    }
}
fn slider(value: f64) -> Node {
    Node::Slider {
        label: "Volume".into(),
        field: "volume".into(),
        value,
        min: 0.0,
        max: 10.0,
        step: Some(2.0),
        fill: false,
    }
}
fn button(label: &str, confirm: bool, disabled: bool) -> Node {
    Node::Button {
        label: label.into(),
        event: label.into(),
        intent: None,
        confirm,
        disabled,
        fill: false,
    }
}
fn screen(children: Vec<Node>) -> Surface {
    Surface {
        version: 1,
        root: Node::Screen {
            title: None,
            children,
        },
    }
}
fn col(children: Vec<Node>) -> Node {
    Node::Col {
        fill: false,
        alignment: None,
        distribution: None,
        children,
    }
}

struct Harness {
    ctx: egui::Context,
    size: egui::Vec2,
    surface: Surface,
}
impl Harness {
    fn new(size: egui::Vec2, surface: Surface) -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_style(crate::theme::style(true));
        Self { ctx, size, surface }
    }
    fn frame(&self, events: Vec<egui::Event>) -> (Vec<Observation>, Vec<Event>, egui::FullOutput) {
        OBSERVATIONS.with(|obs| obs.borrow_mut().clear());
        let mut emitted = Vec::new();
        let output = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| draw(ui, &self.surface, &mut emitted));
            },
        );
        (
            OBSERVATIONS.with(|obs| obs.borrow().clone()),
            emitted,
            output,
        )
    }
    fn click(&self, pos: egui::Pos2) -> Vec<Event> {
        let mut result = self
            .frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ])
            .1;
        result.extend(
            self.frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }])
            .1,
        );
        result
    }
    fn key(&self, key: egui::Key) -> Vec<Event> {
        let result = self
            .frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }])
            .1;
        self.frame(vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Default::default(),
        }]);
        result
    }
}

#[test]
fn disabled_accessibility_has_no_actions_and_changed_confirmation_is_cleared() {
    let mut h = Harness::new(
        SIZES[0],
        screen(vec![
            button("Disabled", false, true),
            button("Delete", true, false),
        ]),
    );
    h.frame(vec![]);
    let (obs, _, out) = h.frame(vec![]);
    let update = out.platform_output.accesskit_update.unwrap();
    let node = &update
        .nodes
        .iter()
        .find(|(_, n)| n.label() == Some("Disabled"))
        .unwrap()
        .1;
    assert!(node.is_disabled());
    assert!(!node.supports_action(accesskit::Action::Focus));
    assert!(!node.supports_action(accesskit::Action::Click));
    h.click(
        obs.iter()
            .find(|o| o.path == vec![1])
            .unwrap()
            .rect
            .center(),
    );
    let mut replacement = button("Other label", true, false);
    if let Node::Button { event, .. } = &mut replacement {
        *event = "Delete".into();
    }
    h.surface = screen(vec![button("Disabled", false, true), replacement]);
    assert!(!h.frame(vec![]).0.iter().any(|o| o.kind == "confirm"));
}

#[test]
fn slider_accesskit_without_focus_and_non_aligned_upper_bound() {
    let mut control = slider(4.0);
    if let Node::Slider { max, .. } = &mut control {
        *max = 9.0;
    }
    let h = Harness::new(SIZES[0], screen(vec![control]));
    let (obs, _, out) = h.frame(vec![]);
    let id = find(&obs, "slider").id;
    let target = out
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find(|(_, n)| n.role() == accesskit::Role::Slider)
        .unwrap()
        .0;
    let (_, events, _) = h.frame(vec![egui::Event::AccessKitActionRequest(
        accesskit::ActionRequest {
            action: accesskit::Action::SetValue,
            target,
            data: Some(accesskit::ActionData::NumericValue(9.0)),
        },
    )]);
    assert_eq!(
        events,
        vec![Event::Change {
            field: "volume".into(),
            value: serde_json::json!(8.0),
            path: vec![0]
        }]
    );
    h.ctx.memory_mut(|memory| memory.request_focus(id));
    assert_eq!(
        h.key(egui::Key::Home),
        vec![Event::Change {
            field: "volume".into(),
            value: serde_json::json!(0.0),
            path: vec![0]
        }]
    );
    assert_eq!(
        h.key(egui::Key::End),
        vec![Event::Change {
            field: "volume".into(),
            value: serde_json::json!(8.0),
            path: vec![0]
        }]
    );
}

#[test]
fn toggles_selects_and_plain_buttons_emit_typed_local_events() {
    for size in SIZES {
        let h = Harness::new(
            size,
            screen(vec![
                Node::Toggle {
                    label: "Enabled".into(),
                    field: "enabled".into(),
                    value: false,
                    fill: false,
                },
                Node::Select {
                    label: "Priority".into(),
                    field: "priority".into(),
                    options: vec!["Low".into(), "High".into()],
                    value: "Low".into(),
                    fill: false,
                },
                button("Run", false, false),
            ]),
        );
        h.frame(vec![]);
        let (obs, _, _) = h.frame(vec![]);
        assert_eq!(
            h.click(find(&obs, "toggle").rect.center()),
            vec![Event::Change {
                field: "enabled".into(),
                value: serde_json::json!(true),
                path: vec![0]
            }]
        );
        h.click(find(&obs, "select").rect.center());
        let (_, _, out) = h.frame(vec![]);
        let high = out
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find(|(_, n)| n.label() == Some("High"))
            .unwrap()
            .1
            .bounds()
            .unwrap();
        assert_eq!(
            h.click(egui::pos2(
                ((high.x0 + high.x1) / 2.0) as f32,
                ((high.y0 + high.y1) / 2.0) as f32
            )),
            vec![Event::Change {
                field: "priority".into(),
                value: serde_json::json!("High"),
                path: vec![1]
            }]
        );
        h.frame(vec![]);
        assert_eq!(
            h.click(find(&obs, "button").rect.center()),
            vec![Event::Action {
                event: "Run".into(),
                path: vec![2]
            }]
        );
    }
}

#[test]
fn reordered_anchors_keep_source_paths_and_empty_screens_are_bounded() {
    for size in SIZES {
        let h = Harness::new(
            size,
            screen(vec![
                Node::Footer {
                    children: vec![button("Foot", false, false)],
                },
                text("Body", false),
                Node::Header {
                    children: vec![button("Head", false, false)],
                },
            ]),
        );
        h.frame(vec![]);
        let (obs, _, _) = h.frame(vec![]);
        assert_eq!(
            h.click(
                obs.iter()
                    .find(|o| o.path == vec![2, 0])
                    .unwrap()
                    .rect
                    .center()
            ),
            vec![Event::Action {
                event: "Head".into(),
                path: vec![2, 0]
            }]
        );
        for nodes in [
            vec![],
            vec![Node::Header { children: vec![] }],
            vec![Node::Footer { children: vec![] }],
        ] {
            let h = Harness::new(size, screen(nodes));
            let (obs, _, _) = h.frame(vec![]);
            let body = find(&obs, "body");
            assert_eq!(body.rect.width(), size.x);
            assert!(body.rect.height() >= 768.0);
            assert!(body.rect.height() <= 800.0);
        }
    }
}

#[test]
fn column_alignment_and_around_distribution_use_remaining_extent() {
    for size in SIZES {
        let h = Harness::new(
            size,
            screen(vec![Node::Col {
                fill: false,
                alignment: Some(Alignment::End),
                distribution: Some(Distribution::Around),
                children: vec![button("A", false, false), button("B", false, false)],
            }]),
        );
        let (obs, _, _) = h.frame(vec![]);
        let a = obs.iter().find(|o| o.path == vec![0, 0]).unwrap();
        let b = obs.iter().find(|o| o.path == vec![0, 1]).unwrap();
        assert_eq!(a.rect.top(), 182.0);
        assert_eq!(b.rect.top(), 570.0);
        assert_eq!(a.rect.right(), size.x - 16.0);
    }
}

#[test]
fn vector_slider_uses_thin_rail_and_accent_outline_in_both_themes() {
    for dark in [true, false] {
        let h = Harness::new(SIZES[0], screen(vec![slider(4.0)]));
        h.ctx.set_style(crate::theme::style(dark));
        let (_, _, out) = h.frame(vec![]);
        let accent = h.ctx.style().visuals.selection.stroke.color;
        assert!(out.shapes.iter().any(
            |shape| matches!(&shape.shape,egui::Shape::LineSegment{stroke,..} if stroke.width==1.0)
        ));
        assert!(out.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Rect(rect) if rect.stroke.color==accent&&rect.stroke.width==1.0&&rect.fill==h.ctx.style().visuals.panel_fill)));
    }
}

#[test]
fn fixed_range_slider_exposes_a_finite_positive_step_and_named_main() {
    let mut surface = screen(vec![Node::Slider {
        label: "Fixed".into(),
        field: "fixed".into(),
        value: 3.0,
        min: 3.0,
        max: 3.0,
        step: None,
        fill: false,
    }]);
    if let Node::Screen { title, .. } = &mut surface.root {
        *title = Some("Settings".into());
    }
    let h = Harness::new(SIZES[0], surface);
    let (_, _, out) = h.frame(vec![]);
    let update = out.platform_output.accesskit_update.unwrap();
    assert!(update
        .nodes
        .iter()
        .any(|(_, n)| n.role() == accesskit::Role::Main && n.label() == Some("Settings")));
    let range = &update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == accesskit::Role::Slider)
        .unwrap()
        .1;
    assert!(
        range.numeric_value_step().unwrap().is_finite()
            && range.numeric_value_step().unwrap() > 0.0
    );
}

#[test]
fn nowrap_body_can_scroll_horizontally_without_moving_anchors() {
    for size in SIZES {
        let row = Node::Row {
            fill: false,
            wrapping: Some(Wrapping::Nowrap),
            alignment: None,
            distribution: None,
            children: (0..20)
                .map(|_| button("Wide button", false, false))
                .collect(),
        };
        let h = Harness::new(
            size,
            screen(vec![
                Node::Header {
                    children: vec![text("Header", false)],
                },
                row,
                Node::Footer {
                    children: vec![text("Footer", false)],
                },
            ]),
        );
        h.frame(vec![]);
        let (before, _, _) = h.frame(vec![]);
        let (after, _, _) = h.frame(vec![
            egui::Event::PointerMoved(find(&before, "body").rect.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(-400.0, 0.0),
                modifiers: Default::default(),
            },
        ]);
        let (settled, _, _) = h.frame(vec![]);
        assert!(
            settled
                .iter()
                .find(|o| o.path == vec![1, 0])
                .unwrap()
                .rect
                .left()
                < before
                    .iter()
                    .find(|o| o.path == vec![1, 0])
                    .unwrap()
                    .rect
                    .left()
        );
        assert_eq!(find(&before, "header").rect, find(&after, "header").rect);
        assert_eq!(find(&before, "footer").rect, find(&after, "footer").rect);
    }
}

#[test]
fn slider_host_range_changes_reset_local_value() {
    let mut h = Harness::new(SIZES[0], screen(vec![slider(4.0)]));
    h.frame(vec![]);
    let (obs, _, _) = h.frame(vec![]);
    h.click(egui::pos2(find(&obs, "slider").rect.right() - 8.0, 24.0));
    let mut replacement = slider(4.0);
    if let Node::Slider { max, .. } = &mut replacement {
        *max = 6.0;
    }
    h.surface = screen(vec![replacement]);
    let (_, _, out) = h.frame(vec![]);
    let update = out.platform_output.accesskit_update.unwrap();
    assert_eq!(
        update
            .nodes
            .iter()
            .find(|(_, n)| n.role() == accesskit::Role::Slider)
            .unwrap()
            .1
            .numeric_value(),
        Some(4.0)
    );
}

#[test]
fn disabled_button_rejects_keyboard_and_accesskit_clicks() {
    let h = Harness::new(
        SIZES[0],
        screen(vec![
            button("Disabled", false, true),
            button("Active", false, false),
        ]),
    );
    let (obs, _, out) = h.frame(vec![]);
    let id = obs.iter().find(|o| o.path == vec![0]).unwrap().id;
    let target = out
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find(|(_, n)| n.label() == Some("Disabled"))
        .unwrap()
        .0;
    h.ctx.memory_mut(|memory| memory.request_focus(id));
    assert!(h.key(egui::Key::Space).is_empty());
    assert!(h.key(egui::Key::Enter).is_empty());
    assert!(h
        .frame(vec![egui::Event::AccessKitActionRequest(
            accesskit::ActionRequest {
                action: accesskit::Action::Click,
                target,
                data: None
            }
        )])
        .1
        .is_empty());
}

#[test]
fn button_intents_keep_actions_and_accent_labels() {
    for intent in [
        None,
        Some(mote_ui::v1::ButtonIntent::Primary),
        Some(mote_ui::v1::ButtonIntent::Danger),
    ] {
        let mut action = button("Run", false, false);
        if let Node::Button { intent: value, .. } = &mut action {
            *value = intent;
        }
        let h = Harness::new(SIZES[0], screen(vec![action]));
        h.frame(vec![]);
        let (obs, _, out) = h.frame(vec![]);
        if intent.is_some() {
            assert!(out.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.text()=="Run"&&text.fallback_color==h.ctx.style().visuals.selection.stroke.color)));
        }
        assert_eq!(
            h.click(find(&obs, "button").rect.center()),
            vec![Event::Action {
                event: "Run".into(),
                path: vec![0]
            }]
        );
    }
}

#[test]
fn confirmation_cancel_and_removal_never_emit() {
    let mut h = Harness::new(SIZES[0], screen(vec![button("Delete", true, false)]));
    h.frame(vec![]);
    let (obs, _, _) = h.frame(vec![]);
    assert!(h.click(find(&obs, "button").rect.center()).is_empty());
    h.frame(vec![]);
    assert!(h.key(egui::Key::Escape).is_empty());
    assert!(!h.frame(vec![]).0.iter().any(|o| o.kind == "confirm"));
    h.frame(vec![]);
    h.click(find(&obs, "button").rect.center());
    h.surface = screen(vec![]);
    assert!(h.frame(vec![]).1.is_empty());
    h.surface = screen(vec![button("Delete", true, false)]);
    assert!(!h.frame(vec![]).0.iter().any(|o| o.kind == "confirm"));
}

#[test]
fn select_retains_local_value_but_replacement_options_reset_it() {
    let select = |options: Vec<String>| Node::Select {
        label: "Priority".into(),
        field: "priority".into(),
        options,
        value: "Low".into(),
        fill: false,
    };
    let mut h = Harness::new(
        SIZES[0],
        screen(vec![select(vec!["Low".into(), "High".into()])]),
    );
    h.frame(vec![]);
    let (obs, _, _) = h.frame(vec![]);
    h.click(find(&obs, "select").rect.center());
    let (_, _, out) = h.frame(vec![]);
    let bounds = out
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .into_iter()
        .find(|(_, n)| n.label() == Some("High"))
        .unwrap()
        .1
        .bounds()
        .unwrap();
    h.click(egui::pos2(
        ((bounds.x0 + bounds.x1) / 2.0) as f32,
        ((bounds.y0 + bounds.y1) / 2.0) as f32,
    ));
    let (_, _, out) = h.frame(vec![]);
    assert!(out
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .iter()
        .any(|(_, n)| n.role() == accesskit::Role::ComboBox && n.value() == Some("High")));
    h.surface = screen(vec![select(vec!["Low".into(), "Normal".into()])]);
    let (_, _, out) = h.frame(vec![]);
    assert!(out
        .platform_output
        .accesskit_update
        .unwrap()
        .nodes
        .iter()
        .any(|(_, n)| n.role() == accesskit::Role::ComboBox && n.value() == Some("Low")));
}

#[test]
fn removing_select_closes_its_popup_and_clears_widget_focus() {
    let mut h = Harness::new(
        SIZES[0],
        screen(vec![Node::Select {
            label: "Priority".into(),
            field: "priority".into(),
            options: vec!["Low".into(), "High".into()],
            value: "Low".into(),
            fill: false,
        }]),
    );
    h.frame(vec![]);
    let (obs, _, _) = h.frame(vec![]);
    h.click(find(&obs, "select").rect.center());
    assert!(h.ctx.memory(|memory| memory.any_popup_open()));
    h.surface = screen(vec![]);
    h.frame(vec![]);
    assert!(!h.ctx.memory(|memory| memory.any_popup_open()));
    assert!(h.ctx.memory(|memory| memory.focused()).is_none());
}

#[test]
fn finite_extreme_slider_ranges_stay_finite_during_input_and_paint() {
    for step in [None, Some(f64::MAX)] {
        let surface = screen(vec![Node::Slider {
            label: "Wide range".into(),
            field: "wide".into(),
            value: 0.0,
            min: -f64::MAX,
            max: f64::MAX,
            step,
            fill: false,
        }]);
        surface.validate().unwrap();
        let h = Harness::new(SIZES[0], surface);
        h.frame(vec![]);
        let (obs, _, out) = h.frame(vec![]);
        assert!(out.shapes.iter().all(|shape| match &shape.shape {
            egui::Shape::Rect(rect) => rect.rect.is_finite(),
            egui::Shape::LineSegment { points, .. } => points.iter().all(|point| point.is_finite()),
            egui::Shape::Text(text) => text.pos.is_finite(),
            _ => true,
        }));
        let control = find(&obs, "slider");
        let events = h.click(egui::pos2(control.rect.right() - 8.0, 24.0));
        assert_eq!(
            events,
            vec![Event::Change {
                field: "wide".into(),
                value: serde_json::json!(f64::MAX),
                path: vec![0]
            }]
        );
        assert!(h.key(egui::Key::End).is_empty());
    }
}
fn find<'a>(obs: &'a [Observation], kind: &str) -> &'a Observation {
    obs.iter().find(|o| o.kind == kind).unwrap()
}

#[test]
fn anchors_own_remaining_body_height_and_stay_fixed_during_scroll() {
    for size in SIZES {
        let h = Harness::new(
            size,
            screen(vec![
                Node::Header {
                    children: vec![button("Head", false, false)],
                },
                col((0..80).map(|_| text("Long body", false)).collect()),
                Node::Footer {
                    children: vec![button("Foot", false, false)],
                },
            ]),
        );
        h.frame(vec![]);
        let (before, _, _) = h.frame(vec![]);
        let header = find(&before, "header");
        let footer = find(&before, "footer");
        let body = find(&before, "body");
        assert_eq!(
            header.rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(size.x, 80.0))
        );
        assert_eq!(
            footer.rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 720.0), egui::pos2(size.x, 800.0))
        );
        assert_eq!(
            body.rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 80.0), egui::pos2(size.x, 720.0))
        );
        let (after, _, _) = h.frame(vec![
            egui::Event::PointerMoved(body.rect.center()),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -400.0),
                modifiers: Default::default(),
            },
        ]);
        let (after_settled, _, _) = h.frame(vec![]);
        assert_eq!(find(&after, "header").rect, header.rect);
        assert_eq!(find(&after, "footer").rect, footer.rect);
        assert!(
            after_settled
                .iter()
                .find(|o| o.path == vec![1, 0])
                .unwrap()
                .rect
                .top()
                < before
                    .iter()
                    .find(|o| o.path == vec![1, 0])
                    .unwrap()
                    .rect
                    .top()
        );
        for item in after_settled.iter().filter(|o| o.path.first() == Some(&1)) {
            assert!(item.clip.top() >= body.rect.top());
            assert!(item.clip.bottom() <= body.rect.bottom());
        }
    }
}

#[test]
fn fill_reserves_trailing_siblings_and_splits_remaining_space() {
    for size in SIZES {
        let h = Harness::new(
            size,
            screen(vec![col(vec![
                text("A", true),
                text("B", true),
                button("Tail", false, false),
            ])]),
        );
        let (obs, _, _) = h.frame(vec![]);
        let a = obs.iter().find(|o| o.path == vec![0, 0]).unwrap();
        let b = obs.iter().find(|o| o.path == vec![0, 1]).unwrap();
        let tail = obs.iter().find(|o| o.path == vec![0, 2]).unwrap();
        assert_eq!(a.rect.height(), 352.0);
        assert_eq!(b.rect.height(), 352.0);
        assert_eq!(tail.rect.bottom(), 784.0);
        assert_eq!(b.rect.top() - a.rect.bottom(), 8.0);
        assert_eq!(tail.rect.top() - b.rect.bottom(), 8.0);
    }
}

#[test]
fn row_wrap_alignment_and_distribution_follow_contract() {
    for size in SIZES {
        let row = |wrapping, alignment, distribution| Node::Row {
            fill: false,
            wrapping: Some(wrapping),
            alignment: Some(alignment),
            distribution,
            children: vec![
                button("A long first button", false, false),
                button("A long second button", false, false),
                button("A long third button", false, false),
            ],
        };
        let h = Harness::new(
            size,
            screen(vec![
                row(Wrapping::Wrap, Alignment::End, None),
                row(
                    Wrapping::Nowrap,
                    Alignment::Center,
                    Some(Distribution::Between),
                ),
            ]),
        );
        let (obs, _, _) = h.frame(vec![]);
        let children: Vec<_> = obs
            .iter()
            .filter(|o| o.path.len() == 2 && o.path[0] == 0)
            .collect();
        if size.x == 400.0 {
            assert!(children[1].rect.top() > children[0].rect.top());
        } else {
            assert_eq!(children[0].rect.top(), children[2].rect.top());
        }
        let nowrap: Vec<_> = obs
            .iter()
            .filter(|o| o.path.len() == 2 && o.path[0] == 1)
            .collect();
        assert_eq!(nowrap[0].rect.top(), nowrap[2].rect.top());
        assert!(nowrap[0].rect.right() <= nowrap[1].rect.left());
        if size.x == 1200.0 {
            assert_eq!(nowrap[2].rect.right(), 1184.0);
        }
    }
}

#[test]
fn slider_has_range_semantics_36_visual_and_48_hit_region() {
    for size in SIZES {
        let h = Harness::new(size, screen(vec![slider(4.0)]));
        let (obs, _, out) = h.frame(vec![]);
        let control = find(&obs, "slider");
        assert_eq!(control.rect.height(), 48.0);
        let node = out
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find(|(_, node)| node.role() == accesskit::Role::Slider)
            .unwrap()
            .1;
        assert_eq!(node.label(), Some("Volume"));
        assert_eq!(node.numeric_value(), Some(4.0));
        assert_eq!(node.min_numeric_value(), Some(0.0));
        assert_eq!(node.max_numeric_value(), Some(10.0));
        assert_eq!(node.numeric_value_step(), Some(2.0));
        assert!(node.supports_action(accesskit::Action::SetValue));
        assert!(out.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text()=="Volume" && text.pos.y==control.rect.top()+6.0)));
        assert!(out.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::LineSegment { points, .. } if points[0].y==control.rect.top()+34.0)));
    }
}

#[test]
fn slider_pointer_keys_bounds_and_local_state_reset() {
    for size in SIZES {
        let mut h = Harness::new(size, screen(vec![slider(4.0)]));
        h.frame(vec![]);
        let (obs, _, _) = h.frame(vec![]);
        let control = find(&obs, "slider");
        let events = h.click(egui::pos2(
            control.rect.right() - 8.0,
            control.rect.top() + 2.0,
        ));
        assert_eq!(
            events,
            vec![Event::Change {
                field: "volume".into(),
                value: serde_json::json!(10.0),
                path: vec![0]
            }]
        );
        assert!(h.key(egui::Key::ArrowRight).is_empty());
        assert_eq!(
            h.key(egui::Key::ArrowLeft)[0],
            Event::Change {
                field: "volume".into(),
                value: serde_json::json!(8.0),
                path: vec![0]
            }
        );
        let (_, _, out) = h.frame(vec![]);
        assert!(out
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .any(|(_, n)| n.numeric_value() == Some(8.0)));
        h.surface = screen(vec![slider(2.0)]);
        let (new, _, out) = h.frame(vec![]);
        assert_eq!(find(&new, "slider").id, control.id);
        assert!(out
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .any(|(_, n)| n.numeric_value() == Some(2.0)));
        h.surface = screen(vec![text("Removed", false)]);
        h.frame(vec![]);
        h.surface = screen(vec![slider(4.0)]);
        let (_, _, out) = h.frame(vec![]);
        assert!(out
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .iter()
            .any(|(_, n)| n.numeric_value() == Some(4.0)));
    }
}

#[test]
fn disabled_buttons_are_inert_and_confirmation_emits_once() {
    for size in SIZES {
        let h = Harness::new(
            size,
            screen(vec![
                button("Disabled", false, true),
                button("Delete", true, false),
            ]),
        );
        h.frame(vec![]);
        let (obs, _, _) = h.frame(vec![]);
        assert!(h
            .click(
                obs.iter()
                    .find(|o| o.path == vec![0])
                    .unwrap()
                    .rect
                    .center()
            )
            .is_empty());
        assert!(h
            .click(
                obs.iter()
                    .find(|o| o.path == vec![1])
                    .unwrap()
                    .rect
                    .center()
            )
            .is_empty());
        let (obs, _, _) = h.frame(vec![]);
        assert_eq!(
            h.click(find(&obs, "confirm").rect.center()),
            vec![Event::Action {
                event: "Delete".into(),
                path: vec![1]
            }]
        );
        assert!(h.frame(vec![]).1.is_empty());
        assert!(!h.frame(vec![]).0.iter().any(|o| o.kind == "confirm"));
    }
}

#[test]
fn focus_order_uses_header_body_footer_and_skips_disabled() {
    for size in SIZES {
        let h = Harness::new(
            size,
            screen(vec![
                Node::Header {
                    children: vec![button("Head", false, false)],
                },
                col(vec![
                    slider(4.0),
                    Node::Toggle {
                        label: "Enabled".into(),
                        field: "enabled".into(),
                        value: false,
                        fill: false,
                    },
                    Node::Select {
                        label: "Priority".into(),
                        field: "priority".into(),
                        options: vec!["Low".into(), "High".into()],
                        value: "Low".into(),
                        fill: false,
                    },
                    button("Disabled", false, true),
                    button("Body", false, false),
                ]),
                Node::Footer {
                    children: vec![button("Foot", false, false)],
                },
            ]),
        );
        h.frame(vec![]);
        let (obs, _, _) = h.frame(vec![]);
        let expected: Vec<_> = [
            vec![0, 0],
            vec![1, 0],
            vec![1, 1],
            vec![1, 2],
            vec![1, 4],
            vec![2, 0],
        ]
        .into_iter()
        .map(|path| obs.iter().find(|o| o.path == path).unwrap().id)
        .collect();
        for id in expected {
            h.key(egui::Key::Tab);
            assert_eq!(h.ctx.memory(|m| m.focused()), Some(id));
        }
    }
}
