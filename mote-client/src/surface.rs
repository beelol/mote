use eframe::egui;
use mote_ui::Node;
use std::collections::HashMap;

use crate::accessibility::{self, Value as AccessibilityValue};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceContext {
    pub surface_seq: Option<i64>,
    pub surface_contract: Option<u64>,
    pub node_path: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SurfaceEvent {
    Action {
        event: String,
        context: SurfaceContext,
    },
    Change {
        field: String,
        value: serde_json::Value,
        state_id: egui::Id,
        context: SurfaceContext,
    },
}

impl SurfaceEvent {
    pub fn context(&self) -> &SurfaceContext {
        match self {
            Self::Action { context, .. } | Self::Change { context, .. } => context,
        }
    }

    pub fn set_surface_seq(&mut self, seq: Option<i64>) {
        match self {
            Self::Action { context, .. } | Self::Change { context, .. } => context.surface_seq = seq,
        }
    }

    pub fn event(&self) -> &str {
        match self {
            Self::Action { event, .. } => event,
            Self::Change { .. } => "change",
        }
    }

    pub fn args(&self) -> serde_json::Value {
        match self {
            Self::Action { .. } => serde_json::json!({}),
            Self::Change { field, value, .. } => {
                serde_json::json!({ "field": field, "value": value })
            }
        }
    }
}

pub fn discard_change(ctx: &egui::Context, event: &SurfaceEvent) {
    let SurfaceEvent::Change { state_id, .. } = event else {
        return;
    };
    if event.context().surface_contract == Some(1) {
        crate::surface_v1::discard_change(ctx, *state_id);
        return;
    }
    ctx.data_mut(|data| {
        data.remove::<(bool, bool)>(*state_id);
        data.remove::<(String, String)>(*state_id);
    });
}

#[derive(Clone, Copy, Hash)]
enum ControlKind {
    Field,
    Toggle,
    Select,
    Action,
}

pub fn draw(ui: &mut egui::Ui, node: &Node, events: &mut Vec<SurfaceEvent>) {
    let cache = ui.id().with("live-controls");
    let mut live = HashMap::new();
    draw_at(ui, node, events, &mut Vec::new(), ui.id(), &mut live);
    let previous = ui
        .data_mut(|data| data.get_temp::<HashMap<egui::Id, egui::Id>>(cache))
        .unwrap_or_default();
    for (id, widget) in previous.iter().filter(|(id, _)| !live.contains_key(id)) {
        ui.data_mut(|data| {
            data.remove::<(String, String)>(*id);
            data.remove::<(bool, bool)>(*id);
            data.remove::<egui::text_edit::TextEditState>(*id);
        });
        let popup_open = egui::ComboBox::is_open(ui.ctx(), *widget);
        ui.memory_mut(|memory| {
            memory.surrender_focus(*widget);
            if popup_open {
                memory.close_popup();
            }
        });
    }
    ui.data_mut(|data| data.insert_temp(cache, live));
}

fn draw_at(
    ui: &mut egui::Ui,
    node: &Node,
    events: &mut Vec<SurfaceEvent>,
    path: &mut Vec<usize>,
    namespace: egui::Id,
    live: &mut HashMap<egui::Id, egui::Id>,
) {
    #[cfg(any(test, feature = "test-observation"))]
    let first_shape = observation::shape_count(ui);
    match node {
        Node::Text { value } => {
            let id = accessibility::id(namespace, path, "text");
            accessibility::parent(ui, id, accesskit::Role::Label, |ui| {
                let response = ui.label(value);
                accessibility::hide(&response);
                accessibility::leaf(ui, id, accesskit::Role::Label, Some(value), AccessibilityValue::None, Some(&response));
            });
        }
        Node::Stack { children } => {
            let id = accessibility::id(namespace, path, "stack");
            accessibility::parent(ui, id, accesskit::Role::GenericContainer, |ui| {
                egui::Frame::new().inner_margin(16).show(ui, |ui| {
                    for (index, child) in children.iter().enumerate() {
                        path.push(index);
                        ui.push_id(index, |ui| draw_at(ui, child, events, path, namespace, live));
                        path.pop();
                    }
                });
            });
        }
        Node::List { children } | Node::Chat { children } => {
            let (salt, role) = match node {
                Node::List { .. } => ("list", accesskit::Role::List),
                Node::Chat { .. } => ("chat", accesskit::Role::Log),
                _ => unreachable!(),
            };
            let id = accessibility::id(namespace, path, salt);
            accessibility::parent(ui, id, role, |ui| {
                egui::Frame::new().inner_margin(16).show(ui, |ui| {
                    for (index, child) in children.iter().enumerate() {
                        if index > 0 { ui.separator(); }
                        path.push(index);
                        let item = accessibility::id(namespace, path, "item");
                        accessibility::parent(ui, item, accesskit::Role::ListItem, |ui| {
                            ui.push_id(index, |ui| draw_at(ui, child, events, path, namespace, live));
                        });
                        path.pop();
                    }
                });
            });
        }
        Node::Row { children } => {
            let id = accessibility::id(namespace, path, "row-layout");
            accessibility::parent(ui, id, accesskit::Role::GenericContainer, |ui| {
                egui::Frame::new().inner_margin(16).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (index, child) in children.iter().enumerate() {
                            path.push(index);
                            ui.push_id(index, |ui| draw_at(ui, child, events, path, namespace, live));
                            path.pop();
                        }
                    });
                });
            });
        }
        Node::Table { columns, rows } => {
            let table = accessibility::id(namespace, path, "table");
            accessibility::parent(ui, table, accesskit::Role::Table, |ui| {
                egui::Grid::new(ui.next_auto_id()).striped(false).show(ui, |ui| {
                    let header = accessibility::id(namespace, path, "table-header");
                    accessibility::parent(ui, header, accesskit::Role::Row, |ui| {
                        for (column_index, column) in columns.iter().enumerate() {
                            let cell = table_semantic_id(namespace, path, "column-header", 0, column_index);
                            table_cell(ui, column, accesskit::Role::ColumnHeader, cell);
                        }
                    });
                    ui.end_row();
                    for (row_index, row) in rows.iter().enumerate() {
                        let row_id = table_semantic_id(namespace, path, "row", row_index, 0);
                        accessibility::parent(ui, row_id, accesskit::Role::Row, |ui| {
                            for (column_index, cell_text) in row.iter().enumerate() {
                                let cell = table_semantic_id(namespace, path, "cell", row_index, column_index);
                                table_cell(ui, cell_text, accesskit::Role::Cell, cell);
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        }
        Node::Field { label, value, .. } => {
            let id = namespace.with((ControlKind::Field, &*path));
            live.insert(id, id);
            let mut edited = ui
                .data_mut(|data| data.get_temp::<(String, String)>(id))
                .filter(|(host, _)| host == value)
                .map(|(_, edited)| edited)
                .unwrap_or_else(|| value.clone());
            control(ui, |ui| {
                let label_response = ui.label(label);
                accessibility::hide(&label_response);
                let response = ui.add_sized([ui.available_width().min(320.0), 36.0],
                    egui::TextEdit::singleline(&mut edited).id(id));
                let response = padded_response(ui, response);
                if ui.input(|input| input.has_accesskit_action_request(response.id, accesskit::Action::Click)) {
                    response.request_focus();
                }
                accessibility::leaf(ui, response.id, accesskit::Role::TextInput, Some(label), AccessibilityValue::Text(&edited), Some(&response));
                #[cfg(any(test, feature = "test-observation"))]
                observation::control(ui, "Field", path, &response);
            });
            ui.data_mut(|data| data.insert_temp(id, (value.clone(), edited)));
        }
        Node::Toggle { label, name, value } => {
            let id = namespace.with((ControlKind::Toggle, &*path));
            let mut edited = ui
                .data_mut(|data| data.get_temp::<(bool, bool)>(id))
                .filter(|(host, _)| host == value)
                .map(|(_, edited)| edited)
                .unwrap_or(*value);
            let response = identified_control(ui, id, label, Some(&mut edited));
            accessibility::leaf(ui, response.id, accesskit::Role::Switch, Some(label), AccessibilityValue::Toggle(edited), Some(&response));
            #[cfg(any(test, feature = "test-observation"))]
            observation::control(ui, "Toggle", path, &response);
            live.insert(id, response.id);
            if response.changed() {
                events.push(SurfaceEvent::Change {
                    context: SurfaceContext { surface_seq: None, surface_contract: None, node_path: path.clone() },
                    field: name.clone(),
                    value: serde_json::Value::Bool(edited),
                    state_id: id,
                });
            }
            ui.data_mut(|data| data.insert_temp(id, (*value, edited)));
        }
        Node::Select {
            label,
            name,
            options,
            value,
        } => {
            let id = namespace.with((ControlKind::Select, &*path));
            let mut selected = ui
                .data_mut(|data| data.get_temp::<(String, String)>(id))
                .filter(|(host, _)| host == value)
                .map(|(_, selected)| selected)
                .unwrap_or_else(|| value.clone());
            let response = control(ui, |ui| {
                let label_response = ui.label(label);
                accessibility::hide(&label_response);
                let mut combo = egui::ComboBox::from_id_salt(id)
                    .selected_text(&selected)
                    .show_ui(ui, |ui| {
                        for option in options {
                            control(ui, |ui| {
                                #[cfg(any(test, feature = "test-observation"))]
                                let first_shape = observation::shape_count(ui);
                                let response =
                                    crate::theme::selectable_label(ui, selected == *option, option);
                                let response = padded_response(ui, response);
                                #[cfg(any(test, feature = "test-observation"))]
                                observation::control(ui, "Option", path, &response);
                                #[cfg(any(test, feature = "test-observation"))]
                                observation::popup(ui, first_shape);
                                let activated = response.clicked();
                                if activated {
                                    ui.memory_mut(|memory| memory.close_popup());
                                }
                                if activated && selected != *option {
                                    selected = option.clone();
                                    events.push(SurfaceEvent::Change {
                                        context: SurfaceContext { surface_seq: None, surface_contract: None, node_path: path.clone() },
                                        field: name.clone(),
                                        value: serde_json::Value::String(selected.clone()),
                                        state_id: id,
                                    });
                                }
                            });
                        }
                    });
                combo.response = padded_response(ui, combo.response);
                combo
            });
            accessibility::leaf(ui, response.response.id, accesskit::Role::ComboBox, Some(label), AccessibilityValue::Text(&selected), Some(&response.response));
            live.insert(id, response.response.id);
            #[cfg(any(test, feature = "test-observation"))]
            observation::control(ui, "Select", path, &response.response);
            ui.data_mut(|data| data.insert_temp(id, (value.clone(), selected)));
        }
        Node::Action { label, event } => {
            let id = namespace.with((ControlKind::Action, &*path));
            let response = identified_control(ui, id, label, None);
            accessibility::leaf(ui, response.id, accesskit::Role::Button, Some(label), AccessibilityValue::None, Some(&response));
            #[cfg(any(test, feature = "test-observation"))]
            observation::control(ui, "Action", path, &response);
            live.insert(id, response.id);
            if response.clicked() {
                events.push(SurfaceEvent::Action {
                    context: SurfaceContext { surface_seq: None, surface_contract: None, node_path: path.clone() },
                    event: event.clone(),
                });
            }
        }
        Node::Progress { value } => {
            let value = value.clamp(0.0, 1.0);
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width().min(320.0), 8.0),
                egui::Sense::hover(),
            );
            ui.painter().rect_stroke(
                rect,
                0,
                ui.visuals().widgets.inactive.bg_stroke,
                egui::StrokeKind::Inside,
            );
            let y = rect.center().y;
            ui.painter().line_segment(
                [
                    egui::pos2(rect.left(), y),
                    egui::pos2(rect.left() + rect.width() * value, y),
                ],
                ui.visuals().selection.stroke,
            );
            let percentage = format!("{:.0}%", value * 100.0);
            let caption = crate::theme::caption(ui, &percentage);
            accessibility::hide(&caption);
            accessibility::leaf(ui, response.id, accesskit::Role::ProgressIndicator, Some(&percentage), AccessibilityValue::Progress(f64::from(value)), Some(&response));
        }
    }
    #[cfg(any(test, feature = "test-observation"))]
    observation::record(ui, node, path, first_shape);
}

#[cfg(any(test, feature = "test-observation"))]
pub(crate) mod observation {
    use super::*;
    use std::cell::RefCell;

    #[derive(Clone, Debug, PartialEq)]
    pub struct DisplayedNode {
        pub namespace: egui::Id,
        pub path: Vec<usize>,
        pub node: Node,
        pub rect: egui::Rect,
    }

    thread_local! {
        static DISPLAYED: RefCell<Vec<DisplayedNode>> = const { RefCell::new(Vec::new()) };
        static LAYOUT: RefCell<Layout> = RefCell::new(Layout::default());
    }

    #[derive(Clone, Debug)]
    pub struct Control {
        pub kind: &'static str,
        pub path: Vec<usize>,
        pub id: egui::Id,
        pub layer: egui::LayerId,
        pub visual: egui::Rect,
        pub hit: egui::Rect,
        pub clip: egui::Rect,
    }

    #[derive(Clone, Debug)]
    pub struct Scroll {
        pub name: &'static str,
        pub viewport: egui::Rect,
        pub clip: egui::Rect,
        pub content: egui::Vec2,
        pub offset: egui::Vec2,
    }

    #[derive(Default, Debug)]
    pub struct Layout {
        pub controls: Vec<Control>,
        pub scrolls: Vec<Scroll>,
        pub paints: Vec<(egui::Rect, egui::Rect)>,
        pub popup_paints: Vec<(egui::Rect, egui::Rect, egui::Rect)>,
    }

    pub fn take_layout() -> Layout {
        LAYOUT.with(|layout| std::mem::take(&mut *layout.borrow_mut()))
    }

    pub fn control(ui: &egui::Ui, kind: &'static str, path: &[usize], response: &egui::Response) {
        LAYOUT.with(|layout| layout.borrow_mut().controls.push(Control {
            kind, path: path.to_vec(), id: response.id, layer: response.layer_id,
            visual: response.rect, hit: response.interact_rect, clip: ui.clip_rect(),
        }));
    }

    pub fn scroll<R>(name: &'static str, clip: egui::Rect, output: &egui::scroll_area::ScrollAreaOutput<R>) {
        LAYOUT.with(|layout| layout.borrow_mut().scrolls.push(Scroll {
            name, viewport: output.inner_rect, clip, content: output.content_size, offset: output.state.offset,
        }));
    }

    pub fn take() -> Vec<DisplayedNode> {
        DISPLAYED.with(|nodes| std::mem::take(&mut *nodes.borrow_mut()))
    }

    pub fn shape_count(ui: &egui::Ui) -> usize {
        let mut count = 0;
        ui.painter().for_each_shape(|_| count += 1);
        count
    }

    fn shape_bounds(shape: &egui::Shape) -> egui::Rect {
        match shape {
            egui::Shape::Rect(shape) => shape.rect,
            egui::Shape::Text(shape) => shape.galley.rect.translate(shape.pos.to_vec2()),
            shape => shape.visual_bounding_rect(),
        }
    }

    pub fn popup(ui: &egui::Ui, start: usize) {
        let mut index = 0;
        ui.painter().for_each_shape(|shape| {
            if index >= start {
                LAYOUT.with(|layout| layout.borrow_mut().popup_paints.push((
                    shape.shape.visual_bounding_rect(), shape.clip_rect, ui.clip_rect(),
                )));
            }
            index += 1;
        });
    }

    pub fn record(ui: &egui::Ui, node: &Node, path: &[usize], start: usize) {
        let mut index = 0;
        let mut rect = egui::Rect::NOTHING;
        ui.painter().for_each_shape(|shape| {
            if index >= start {
                let bounds = shape_bounds(&shape.shape);
                rect = rect.union(bounds);
                LAYOUT.with(|layout| layout.borrow_mut().paints.push((shape.shape.visual_bounding_rect(), shape.clip_rect)));
            }
            index += 1;
        });
        DISPLAYED.with(|nodes| nodes.borrow_mut().push(DisplayedNode {
            namespace: ui.id(), path: path.to_vec(), node: node.clone(), rect,
        }));
    }
}

fn table_semantic_id(namespace: egui::Id, path: &[usize], salt: &'static str, row: usize, column: usize) -> egui::Id {
    namespace.with(("surface-accessibility", path, salt, row, column))
}

fn table_cell(ui: &mut egui::Ui, text: &str, role: accesskit::Role, id: egui::Id) {
    egui::Frame::new()
        .inner_margin(8)
        .stroke(ui.visuals().widgets.inactive.bg_stroke)
        .show(ui, |ui| {
            let response = if role == accesskit::Role::ColumnHeader {
                ui.strong(text)
            } else {
                ui.label(text)
            };
            accessibility::hide(&response);
            accessibility::leaf(ui, id, role, Some(text), AccessibilityValue::None, Some(&response));
        });
}

fn control<R>(ui: &mut egui::Ui, draw: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope_builder(
        egui::UiBuilder::new().layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(0, 6))
                .show(ui, draw)
                .inner
        },
    )
    .inner
}

pub fn button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    control(ui, |ui| {
        let response = ui.add(egui::Button::new(label).min_size(egui::vec2(48.0, 36.0)));
        padded_response(ui, response)
    })
}

pub fn compose(ui: &mut egui::Ui, text: &mut String) -> egui::Response {
    control(ui, |ui| {
        let response = ui.add_sized([280.0, 36.0], egui::TextEdit::singleline(text));
        padded_response(ui, response)
    })
}

fn padded_response(ui: &egui::Ui, response: egui::Response) -> egui::Response {
    let visual = response.rect;
    let target = egui::Rect::from_center_size(visual.center(),
        visual.size().max(response.interact_rect.size()).max(egui::vec2(48.0, 48.0)));
    let padded = ui.interact(target, response.id, response.sense);
    let mut response = response.union(padded);
    response.rect = visual;
    response
}

fn identified_control(
    ui: &mut egui::Ui,
    id: egui::Id,
    label: &str,
    mut checked: Option<&mut bool>,
) -> egui::Response {
    control(ui, |ui| {
        let padding = ui.spacing().button_padding;
        let icon_width = ui.spacing().icon_width;
        let icon_spacing = ui.spacing().icon_spacing;
        let extra = if checked.is_some() {
            egui::vec2(icon_width + icon_spacing, 0.0)
        } else {
            2.0 * padding
        };
        let galley = egui::WidgetText::from(label).into_galley(
            ui, None, ui.available_width() - extra.x, egui::TextStyle::Button,
        );
        let desired = (galley.size() + extra).max(ui.spacing().interact_size);
        let (_, allocated) = ui.allocate_space(desired);
        let mut response = ui.interact(allocated.expand2(egui::vec2(0.0, 6.0)), id, egui::Sense::click());
        response.rect = allocated;
        response.intrinsic_size = Some(desired);
        if let Some(value) = checked.as_deref_mut() {
            if response.clicked() {
                *value = !*value;
                response.mark_changed();
            }
        }
        response.widget_info(|| match checked.as_deref() {
            Some(value) => egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label),
            None => egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label),
        });
        if ui.is_rect_visible(allocated) {
            let visuals = ui.style().interact(&response);
            let text_pos = if let Some(value) = checked.as_deref() {
                let rect = ui.layout().align_size_within_rect(desired, allocated);
                let (small, big) = ui.spacing().icon_rectangles(rect);
                ui.painter().rect(big.expand(visuals.expansion), visuals.corner_radius,
                    visuals.bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
                if *value {
                    ui.painter().add(egui::Shape::line(vec![
                        egui::pos2(small.left(), small.center().y),
                        egui::pos2(small.center().x, small.bottom()),
                        egui::pos2(small.right(), small.top()),
                    ], visuals.fg_stroke));
                }
                egui::pos2(rect.left() + icon_width + icon_spacing, rect.center().y - 0.5 * galley.size().y)
            } else {
                ui.painter().rect(allocated.expand(visuals.expansion), visuals.corner_radius,
                    visuals.weak_bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
                ui.layout().align_size_within_rect(galley.size(), allocated.shrink2(padding)).min
            };
            ui.painter().galley(text_pos, galley, visuals.text_color());
        }
        if checked.is_none() && response.hovered() {
            if let Some(cursor) = ui.visuals().interact_cursor {
                ui.ctx().set_cursor_icon(cursor);
            }
        }
        response
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::output::OutputEvent;

    fn frame(
        ctx: &egui::Context,
        node: &Node,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<SurfaceEvent>, egui::Rect) {
        let mut clicked = Vec::new();
        let mut rect = egui::Rect::NOTHING;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rect = ui.scope(|ui| draw(ui, node, &mut clicked)).response.rect;
                });
            },
        );
        (output, clicked, rect)
    }

    fn click_events(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
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
        ]
    }

    fn text_center(output: &egui::FullOutput, value: &str) -> egui::Pos2 {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == value => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing rendered text {value}"))
    }

    fn assert_single_line_text(output: &egui::FullOutput, value: &str) {
        let text = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == value => Some(text),
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing rendered text {value}"));
        assert_eq!(text.galley.rows.len(), 1, "{value} wrapped");
    }

    #[test]
    fn explicit_control_ids_preserve_vector_shapes_and_widget_semantics() {
        for dark in [false, true] {
            for toggle in [false, true] {
                let actual = egui::Context::default();
                let native = egui::Context::default();
                for ctx in [&actual, &native] {
                    ctx.set_style(crate::theme::style(dark));
                }
                let label = "Long control label";
                let node = if toggle {
                    Node::Toggle { label: label.into(), name: "enabled".into(), value: true }
                } else {
                    Node::Action { label: label.into(), event: "activate".into() }
                };
                let mut checked = true;
                let mut native_frame = |events: Vec<egui::Event>| {
                    let mut rect = egui::Rect::NOTHING;
                    let output = native.run(egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                        events,
                        ..Default::default()
                    }, |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| {
                            rect = ui.scope(|ui| {
                                if toggle {
                                    control(ui, |ui| ui.add(egui::Checkbox::new(&mut checked, label)));
                                } else {
                                    button(ui, label);
                                }
                            }).response.rect;
                        });
                    });
                    (output, rect)
                };
                let (painted, _, rect) = frame(&actual, &node, vec![]);
                let (expected, native_rect) = native_frame(vec![]);
                assert_eq!(rect, native_rect);
                assert_eq!(painted.shapes, expected.shapes);
                let key = |key, pressed| egui::Event::Key {
                    key, physical_key: None, pressed, repeat: false, modifiers: Default::default(),
                };
                for events in [
                    vec![egui::Event::PointerMoved(rect.center())],
                    vec![key(egui::Key::Tab, true)],
                    vec![key(egui::Key::Tab, false)],
                    click_events(egui::pos2(rect.right() - 1.0, rect.bottom() - 1.0)),
                    vec![key(egui::Key::Space, true)],
                    vec![key(egui::Key::Space, false)],
                    vec![key(egui::Key::Enter, true)],
                    vec![key(egui::Key::Enter, false)],
                ] {
                    let (painted, _, _) = frame(&actual, &node, events.clone());
                    let (expected, native_rect) = native_frame(events);
                    assert_eq!(rect, native_rect);
                    assert_eq!(painted.shapes, expected.shapes, "dark={dark}, toggle={toggle}");
                    assert_eq!(painted.platform_output.events, expected.platform_output.events,
                        "widget role, name, focus or selection changed: dark={dark}, toggle={toggle}");
                }
            }
        }
    }

    #[test]
    fn markup_strings_are_exact_inert_text_shapes_through_the_real_client() {
        use crate::app::observation::ObservationHarness;
        use serde_json::json;
        let literals = [
            "<b>hi</b>",
            "<script>alert('hi')</script>",
            "# **bold** _italic_ `code` [link](https://example.com) ![image](asset)",
        ];
        let node = Node::Stack {
            children: literals.iter().map(|value| Node::Text { value: (*value).into() }).collect(),
        };
        let mut harness = ObservationHarness::default();
        harness.event(&mote_eventlog::Event {
            seq: 1,
            ts: 0,
            scope_id: "main".into(),
            kind: "ui.surface".into(),
            payload: json!({"surface_id":"matrix", "surface":node}),
        });
        let (display, output) = harness.frame_output(vec![]);
        assert_eq!(harness.surface(), Some((1, node.clone())));
        assert_eq!(display.len(), 4);
        for (index, literal) in literals.iter().enumerate() {
            assert_eq!(display[index].path, [index]);
            assert_eq!(display[index].node, Node::Text { value: (*literal).into() });
            let text: Vec<_> = output.shapes.iter().filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if display[index].rect.contains(text.pos) => Some(text),
                _ => None,
            }).collect();
            assert_eq!(text.len(), 1, "extra or missing text shape for {literal}");
            assert_eq!(text[0].galley.text(), *literal);
            assert_eq!(text[0].galley.job.sections.len(), 1, "interpreted markup styling");
            assert!(output.platform_output.commands.is_empty());
            let (clicked, output) = harness.frame_output(click_events(display[index].rect.center()));
            assert_eq!(clicked, display);
            assert!(output.platform_output.commands.is_empty(), "link or clipboard command");
            assert!(harness.queued().is_empty());
            assert_eq!(harness.enter(), display);
            assert!(harness.queued().is_empty());
        }
        assert_eq!(display[3].node, node);
        assert!(display[3].path.is_empty());
        assert_eq!(harness.switches, 1);
    }

    #[test]
    fn resolved_progress_and_populated_list_draw_only_resolved_values() {
        let node = Node::Stack {
            children: vec![
                Node::Progress { value: 0.75 },
                Node::List {
                    children: ["Review", "Ship", "Observe"]
                        .into_iter()
                        .map(|value| Node::Text { value: value.into() })
                        .collect(),
                },
            ],
        };
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let (output, events, _) = frame(&ctx, &node, vec![]);
        let text: Vec<_> = output.shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        }).collect();

        assert!(events.is_empty());
        assert!(text.iter().any(|text| text == "75%"));
        for value in ["Review", "Ship", "Observe"] {
            assert!(text.iter().any(|text| text == value));
        }
        assert!(!text.iter().any(|text| text.contains("host.")));
    }

    #[test]
    fn long_action_label_stays_on_one_line_and_clicks_at_padded_edge() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let label = "Preview action";
        let node = Node::Action {
            label: label.into(),
            event: "gallery.preview".into(),
        };
        let (output, _, rect) = frame(&ctx, &node, vec![]);
        assert_single_line_text(&output, label);
        assert!(
            rect.width() > 48.0,
            "action stayed fixed at 48 points: {rect:?}"
        );
        assert!(rect.height() >= 48.0);
        let (_, clicked, _) = frame(
            &ctx,
            &node,
            click_events(egui::pos2(rect.right() - 1.0, rect.bottom() - 1.0)),
        );
        assert_eq!(
            clicked,
            vec![SurfaceEvent::Action {
                context: Default::default(),
                event: "gallery.preview".into()
            }]
        );
    }

    #[test]
    fn long_toggle_label_stays_on_one_line_and_changes_at_padded_edge() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let label = "Enable preview";
        let node = Node::Toggle {
            label: label.into(),
            name: "gallery_preview_enabled".into(),
            value: true,
        };
        let (output, _, rect) = frame(&ctx, &node, vec![]);
        assert_single_line_text(&output, label);
        assert!(
            rect.width() > 48.0,
            "toggle stayed fixed at 48 points: {rect:?}"
        );
        assert!(rect.height() >= 48.0);
        let (clicked, _, _) = frame(
            &ctx,
            &node,
            click_events(egui::pos2(rect.right() - 1.0, rect.bottom() - 1.0)),
        );
        assert!(clicked.platform_output.events.iter().any(|event| {
            matches!(event, OutputEvent::Clicked(info)
                if info.typ == egui::WidgetType::Checkbox && info.selected == Some(false))
        }));
    }

    #[test]
    fn adjacent_popup_choices_keep_independent_48_point_edge_targets() {
        for offset in [-23.0, 23.0] {
            let ctx = egui::Context::default();
            crate::theme::install(&ctx);
            let node = Node::Select {
                label: "Mode".into(),
                name: "mode".into(),
                options: vec!["Alpha".into(), "Bravo".into(), "Charlie".into()],
                value: "Alpha".into(),
            };
            let (closed, _, _) = frame(&ctx, &node, vec![]);
            frame(&ctx, &node, click_events(text_center(&closed, "Alpha")));
            let (popup, _, _) = frame(&ctx, &node, vec![]);
            let middle = text_center(&popup, "Bravo");
            assert!(text_center(&popup, "Charlie").y > middle.y);
            frame(&ctx, &node, click_events(middle + egui::vec2(0.0, offset)));
            let (selected, _, _) = frame(&ctx, &node, vec![]);
            assert!(selected.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Bravo")), "middle option was not selected at offset {offset}, middle={middle:?}, labels={:?}", selected.shapes.iter().filter_map(|shape| match &shape.shape { egui::Shape::Text(text) => Some((text.galley.text(), text.pos)), _ => None }).collect::<Vec<_>>());
            assert!(!selected.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Charlie")), "popup did not close after selection");
        }
    }

    #[test]
    fn toggle_emits_one_typed_change_only_when_its_value_changes() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let node = Node::Toggle {
            label: "Enable preview".into(),
            name: "preview.enabled".into(),
            value: true,
        };
        let (_, idle, rect) = frame(&ctx, &node, vec![]);
        assert!(idle.is_empty());

        let (_, changed, _) = frame(&ctx, &node, click_events(rect.center()));
        assert!(matches!(
            changed.as_slice(),
            [SurfaceEvent::Change { field, value, .. }]
                if field == "preview.enabled" && value == &serde_json::Value::Bool(false)
        ));

        let (_, idle_again, _) = frame(&ctx, &node, vec![]);
        assert!(idle_again.is_empty());
    }

    #[test]
    fn select_emits_one_typed_change_only_for_a_different_choice() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let node = Node::Select {
            label: "Mode".into(),
            name: "models.talk".into(),
            options: vec!["Alpha".into(), "Bravo".into()],
            value: "Alpha".into(),
        };
        let (closed, idle, _) = frame(&ctx, &node, vec![]);
        assert!(idle.is_empty());

        let (_, opened, _) = frame(&ctx, &node, click_events(text_center(&closed, "Alpha")));
        assert!(opened.is_empty());
        let (popup, _, _) = frame(&ctx, &node, vec![]);
        let (_, selected, _) = frame(&ctx, &node, click_events(text_center(&popup, "Bravo")));
        assert!(matches!(
            selected.as_slice(),
            [SurfaceEvent::Change { field, value, .. }]
                if field == "models.talk"
                    && value == &serde_json::Value::String("Bravo".into())
        ));

        let (closed, idle_again, _) = frame(&ctx, &node, vec![]);
        assert!(idle_again.is_empty());
        let (_, opened, _) = frame(&ctx, &node, click_events(text_center(&closed, "Bravo")));
        assert!(opened.is_empty());
        let (popup, _, _) = frame(&ctx, &node, vec![]);
        let (_, reselected, _) = frame(&ctx, &node, click_events(text_center(&popup, "Bravo")));
        assert!(reselected.is_empty());
    }

    #[test]
    fn rejected_select_change_discards_its_optimistic_value() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let node = Node::Select {
            label: "Mode".into(),
            name: "models.talk".into(),
            options: vec!["Alpha".into(), "Bravo".into()],
            value: "Alpha".into(),
        };
        let (closed, _, _) = frame(&ctx, &node, vec![]);
        frame(&ctx, &node, click_events(text_center(&closed, "Alpha")));
        let (popup, _, _) = frame(&ctx, &node, vec![]);
        let (_, events, _) = frame(&ctx, &node, click_events(text_center(&popup, "Bravo")));
        assert_eq!(events.len(), 1);

        discard_change(&ctx, &events[0]);
        let (reverted, _, _) = frame(&ctx, &node, vec![]);
        assert!(reverted.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Alpha"
        )));
        assert!(!reverted.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Bravo"
        )));
    }

    #[test]
    fn replacement_surface_host_value_overrides_cached_select_edit() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let original = Node::Select {
            label: "Mode".into(),
            name: "models.talk".into(),
            options: vec!["Alpha".into(), "Bravo".into(), "Charlie".into()],
            value: "Alpha".into(),
        };
        let (closed, _, _) = frame(&ctx, &original, vec![]);
        frame(
            &ctx,
            &original,
            click_events(text_center(&closed, "Alpha")),
        );
        let (popup, _, _) = frame(&ctx, &original, vec![]);
        frame(
            &ctx,
            &original,
            click_events(text_center(&popup, "Bravo")),
        );

        let replacement = Node::Select {
            label: "Mode".into(),
            name: "models.talk".into(),
            options: vec!["Alpha".into(), "Bravo".into(), "Charlie".into()],
            value: "Charlie".into(),
        };
        let (rendered, _, _) = frame(&ctx, &replacement, vec![]);
        assert!(rendered.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Charlie"
        )));
        assert!(!rendered.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "Bravo"
        )));
    }

    #[test]
    fn actions_return_the_existing_event_string() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let node = Node::Action {
            label: "Run".into(),
            event: "job.run".into(),
        };
        let (_, _, rect) = frame(&ctx, &node, vec![]);
        let (_, clicked, _) = frame(&ctx, &node, click_events(rect.center()));
        assert_eq!(
            clicked,
            vec![SurfaceEvent::Action {
                context: Default::default(),
                event: "job.run".into()
            }]
        );
    }

    #[test]
    fn controls_reserve_a_48_point_touch_target() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let node = Node::Action {
            label: "Run".into(),
            event: "job.run".into(),
        };
        let (_, _, rect) = frame(&ctx, &node, vec![]);
        assert_eq!(rect.height(), 48.0);
        let (_, clicked, _) = frame(
            &ctx,
            &node,
            click_events(egui::pos2(rect.center().x, rect.bottom() - 1.0)),
        );
        assert_eq!(
            clicked,
            vec![SurfaceEvent::Action {
                context: Default::default(),
                event: "job.run".into()
            }]
        );
    }

    #[test]
    fn keyboard_focus_is_red_and_space_activates_the_action() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let node = Node::Action {
            label: "Run".into(),
            event: "job.run".into(),
        };
        frame(&ctx, &node, vec![]);
        frame(
            &ctx,
            &node,
            vec![egui::Event::Key {
                key: egui::Key::Tab,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        let (output, _, _) = frame(&ctx, &node, vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke.color == egui::Color32::from_rgb(229, 72, 77))));
        let (_, clicked, _) = frame(
            &ctx,
            &node,
            vec![egui::Event::Key {
                key: egui::Key::Space,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(
            clicked,
            vec![SurfaceEvent::Action {
                context: Default::default(),
                event: "job.run".into()
            }]
        );
    }

    #[test]
    fn field_edits_survive_the_next_frame() {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        let node = Node::Field {
            label: "Name".into(),
            name: "name".into(),
            value: "original".into(),
        };
        let (output, _, _) = frame(&ctx, &node, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "original" => {
                    Some(text.pos + text.galley.rect.center().to_vec2())
                }
                _ => None,
            })
            .unwrap();
        frame(&ctx, &node, click_events(pos));
        frame(
            &ctx,
            &node,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
                egui::Event::Text("edited".into()),
            ],
        );
        let (output, _, _) = frame(&ctx, &node, vec![]);
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "edited")));
    }

    #[test]
    fn all_existing_fixtures_draw_without_a_gpu() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/mote-ui/fixtures");
        let mut count = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let node = mote_ui::validate(&std::fs::read(path).unwrap()).unwrap();
                let ctx = egui::Context::default();
                crate::theme::install(&ctx);
                let (output, _, rect) = frame(&ctx, &node, vec![]);
                assert!(!output.shapes.is_empty());
                assert!(rect.is_finite());
                count += 1;
            }
        }
        assert_eq!(count, 10);
    }
}
