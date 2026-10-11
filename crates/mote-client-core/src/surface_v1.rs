use crate::accessibility::{self, Value};
#[cfg(any(test, feature = "test-observation"))]
use crate::observation::{observe, OBSERVATIONS};
use mote_ui::v1::{Alignment, ButtonIntent, Distribution, Node, Surface, Wrapping};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Action {
        event: String,
        path: Vec<usize>,
    },
    Change {
        field: String,
        value: serde_json::Value,
        path: Vec<usize>,
    },
}

impl Event {
    pub fn into_rpc(self, ui: &egui::Ui, surface: &Surface) -> crate::surface::SurfaceEvent {
        let path = match &self {
            Self::Action { path, .. } | Self::Change { path, .. } => path,
        };
        let mut node = &surface.root;
        for index in path {
            node = match node {
                Node::Screen { children, .. }
                | Node::Col { children, .. }
                | Node::Row { children, .. }
                | Node::Header { children }
                | Node::Footer { children } => &children[*index],
                _ => unreachable!(),
            };
        }
        let state_id = ui.id().with("matrix-contract-1").with((kind(node), path));
        let context = crate::surface::SurfaceContext {
            surface_seq: None,
            surface_contract: Some(1),
            node_path: path.clone(),
        };
        match self {
            Self::Action { event, .. } => crate::surface::SurfaceEvent::Action { event, context },
            Self::Change { field, value, .. } => crate::surface::SurfaceEvent::Change {
                field,
                value,
                state_id,
                context,
            },
        }
    }
}

pub fn discard_change(ctx: &egui::Context, state_id: egui::Id) {
    ctx.data_mut(|data| data.remove::<State>(state_id));
}

#[derive(Clone, Default)]
struct State {
    host: Option<serde_json::Value>,
    edited: Option<serde_json::Value>,
    confirming: bool,
    widget: Option<egui::Id>,
    confirmation_widgets: Vec<egui::Id>,
}

pub fn draw(ui: &mut egui::Ui, surface: &Surface, events: &mut Vec<Event>) {
    let namespace = ui.id().with("matrix-contract-1");
    let rect = egui::Rect::from_min_size(ui.cursor().min, ui.available_size());
    ui.allocate_rect(rect, egui::Sense::hover());
    let mut renderer = Renderer {
        namespace,
        events,
        live: HashSet::new(),
    };
    renderer.node(ui, &surface.root, rect, &mut Vec::new());
    let cache = namespace.with("live-controls");
    let previous = ui
        .data_mut(|data| data.get_temp::<HashSet<egui::Id>>(cache))
        .unwrap_or_default();
    discard_controls(ui, previous.difference(&renderer.live).copied());
    ui.data_mut(|data| data.insert_temp(cache, renderer.live));
}

pub fn discard(ui: &egui::Ui) {
    let cache = ui.id().with("matrix-contract-1").with("live-controls");
    let controls = ui
        .data_mut(|data| data.remove_temp::<HashSet<egui::Id>>(cache))
        .unwrap_or_default();
    discard_controls(ui, controls);
}

fn discard_controls(ui: &egui::Ui, controls: impl IntoIterator<Item = egui::Id>) {
    for id in controls {
        let state = ui
            .data_mut(|data| data.get_temp::<State>(id))
            .unwrap_or_default();
        let widget = state.widget.unwrap_or(id);
        ui.data_mut(|data| data.remove::<State>(id));
        ui.memory_mut(|memory| {
            memory.surrender_focus(widget);
            for widget in &state.confirmation_widgets {
                memory.surrender_focus(*widget);
            }
        });
        if egui::ComboBox::is_open(ui.ctx(), widget) {
            ui.memory_mut(|memory| memory.close_popup());
        }
    }
}

struct Renderer<'a> {
    namespace: egui::Id,
    events: &'a mut Vec<Event>,
    live: HashSet<egui::Id>,
}

fn kind(node: &Node) -> &'static str {
    match node {
        Node::Screen { .. } => "screen",
        Node::Col { .. } => "col",
        Node::Row { .. } => "row",
        Node::Header { .. } => "header",
        Node::Footer { .. } => "footer",
        Node::Text { .. } => "text",
        Node::Toggle { .. } => "toggle",
        Node::Select { .. } => "select",
        Node::Slider { .. } => "slider",
        Node::Button { .. } => "button",
    }
}
fn filled(node: &Node) -> bool {
    match node {
        Node::Col { fill, .. }
        | Node::Row { fill, .. }
        | Node::Text { fill, .. }
        | Node::Toggle { fill, .. }
        | Node::Select { fill, .. }
        | Node::Slider { fill, .. }
        | Node::Button { fill, .. } => *fill,
        _ => false,
    }
}
fn label_size(ui: &egui::Ui, text: &str, width: f32) -> egui::Vec2 {
    ui.painter()
        .layout(
            text.into(),
            egui::FontId::monospace(14.0),
            ui.visuals().text_color(),
            width.max(1.0),
        )
        .size()
}
fn measure(ui: &egui::Ui, node: &Node, width: f32) -> egui::Vec2 {
    match node {
        Node::Text { value, .. } => label_size(ui, value, width),
        Node::Button { label, .. } | Node::Toggle { label, .. } => egui::vec2(
            (label_size(ui, label, f32::INFINITY).x + 32.0).max(48.0),
            48.0,
        ),
        Node::Slider { .. } => egui::vec2(width.min(320.0), 48.0),
        Node::Select { label, .. } => egui::vec2(
            (label_size(ui, label, f32::INFINITY).x + 120.0).max(160.0),
            48.0,
        ),
        Node::Screen { children, .. }
        | Node::Col { children, .. }
        | Node::Header { children }
        | Node::Footer { children } => {
            let sizes: Vec<_> = children
                .iter()
                .map(|child| measure(ui, child, (width - 32.0).max(1.0)))
                .collect();
            egui::vec2(
                sizes.iter().map(|s| s.x).fold(0.0, f32::max) + 32.0,
                sizes.iter().map(|s| s.y).sum::<f32>()
                    + 8.0 * sizes.len().saturating_sub(1) as f32
                    + 32.0,
            )
        }
        Node::Row {
            children, wrapping, ..
        } => {
            let available = (width - 32.0).max(1.0);
            let mut x = 0.0_f32;
            let mut y = 0.0_f32;
            let mut height = 0.0_f32;
            let mut widest = 0.0_f32;
            for child in children {
                let size = measure(ui, child, available);
                if *wrapping != Some(Wrapping::Nowrap) && x > 0.0 && x + size.x > available {
                    widest = widest.max(x - 8.0);
                    y += height + 8.0;
                    x = 0.0;
                    height = 0.0;
                }
                x += size.x + 8.0;
                height = height.max(size.y);
            }
            egui::vec2(widest.max((x - 8.0).max(0.0)) + 32.0, y + height + 32.0)
        }
    }
}

impl Renderer<'_> {
    fn node(
        &mut self,
        parent: &mut egui::Ui,
        node: &Node,
        rect: egui::Rect,
        path: &mut Vec<usize>,
    ) {
        let id = self.namespace.with((kind(node), &*path));
        let mut ui = parent.new_child(
            egui::UiBuilder::new()
                .id_salt(id)
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        ui.set_clip_rect(parent.clip_rect());
        #[cfg(any(test, feature = "test-observation"))]
        observe(&ui, kind(node), path, rect, id);
        let role = match node {
            Node::Screen { .. } => accesskit::Role::Main,
            Node::Header { .. } => accesskit::Role::Header,
            Node::Footer { .. } => accesskit::Role::Footer,
            _ => accesskit::Role::GenericContainer,
        };
        match node {
            Node::Screen { title, children } => {
                accessibility::parent(&mut ui, id, role, |ui| {
                    if let Some(title) = title {
                        ui.ctx()
                            .accesskit_node_builder(id, |node| node.set_label(title.as_str()));
                    }
                    let header = children
                        .iter()
                        .position(|n| matches!(n, Node::Header { .. }));
                    let footer = children
                        .iter()
                        .position(|n| matches!(n, Node::Footer { .. }));
                    let header_height = header
                        .map(|i| measure(ui, &children[i], rect.width()).y)
                        .unwrap_or(0.0)
                        .min(rect.height());
                    let footer_height = footer
                        .map(|i| measure(ui, &children[i], rect.width()).y)
                        .unwrap_or(0.0)
                        .min((rect.height() - header_height).max(0.0));
                    if let Some(i) = header {
                        let r = egui::Rect::from_min_size(
                            rect.min,
                            egui::vec2(rect.width(), header_height),
                        );
                        path.push(i);
                        self.node(ui, &children[i], r, path);
                        path.pop();
                        ui.painter().hline(
                            r.x_range(),
                            r.bottom(),
                            ui.visuals().widgets.noninteractive.bg_stroke,
                        );
                    }
                    let body = egui::Rect::from_min_max(
                        egui::pos2(rect.left(), rect.top() + header_height),
                        egui::pos2(rect.right(), rect.bottom() - footer_height),
                    );
                    #[cfg(any(test, feature = "test-observation"))]
                    observe(ui, "body", path, body, id.with("body"));
                    let mut body_ui =
                        ui.new_child(egui::UiBuilder::new().id_salt("body").max_rect(body));
                    body_ui.set_clip_rect(ui.clip_rect().intersect(body));
                    let body_children: Vec<_> = children
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| Some(*i) != header && Some(*i) != footer)
                        .collect();
                    let natural = body_children
                        .iter()
                        .map(|(_, n)| measure(ui, n, body.width()).y)
                        .sum::<f32>()
                        + 8.0 * body_children.len().saturating_sub(1) as f32;
                    let natural_width = body_children
                        .iter()
                        .map(|(_, n)| measure(ui, n, body.width()).x)
                        .fold(body.width(), f32::max);
                    let scroll = egui::ScrollArea::both()
                        .id_salt(id.with("body-scroll"))
                        .auto_shrink([false, false])
                        .max_height(body.height())
                        .show(&mut body_ui, |ui| {
                            let x = ui.cursor().min.x;
                            let mut y = ui.cursor().min.y;
                            for (i, child) in &body_children {
                                let mut size = measure(ui, child, body.width());
                                if body_children.len() == 1 && matches!(child, Node::Col { .. }) {
                                    size.y = size.y.max(body.height());
                                }
                                let r = egui::Rect::from_min_size(
                                    egui::pos2(x, y),
                                    egui::vec2(body.width(), size.y),
                                );
                                path.push(*i);
                                self.node(ui, child, r, path);
                                path.pop();
                                y += size.y + 8.0;
                            }
                            ui.allocate_rect(
                                egui::Rect::from_min_size(
                                    ui.cursor().min,
                                    egui::vec2(natural_width, natural.max(body.height())),
                                ),
                                egui::Sense::hover(),
                            );
                        });
                    for salt in [0, 1] {
                        accessibility::hide_id(ui.ctx(), scroll.id.with(salt));
                    }
                    accessibility::hide_id(ui.ctx(), scroll.id.with("area"));
                    if let Some(i) = footer {
                        let r = egui::Rect::from_min_size(
                            egui::pos2(rect.left(), rect.bottom() - footer_height),
                            egui::vec2(rect.width(), footer_height),
                        );
                        path.push(i);
                        self.node(ui, &children[i], r, path);
                        path.pop();
                        ui.painter().hline(
                            r.x_range(),
                            r.top(),
                            ui.visuals().widgets.noninteractive.bg_stroke,
                        );
                    }
                });
            }
            Node::Col {
                children,
                alignment,
                distribution,
                ..
            } => accessibility::parent(&mut ui, id, role, |ui| {
                self.layout(
                    ui,
                    children,
                    rect,
                    path,
                    false,
                    false,
                    *alignment,
                    *distribution,
                )
            }),
            Node::Row {
                children,
                wrapping,
                alignment,
                distribution,
                ..
            } => accessibility::parent(&mut ui, id, role, |ui| {
                self.layout(
                    ui,
                    children,
                    rect,
                    path,
                    true,
                    *wrapping != Some(Wrapping::Nowrap),
                    *alignment,
                    *distribution,
                )
            }),
            Node::Header { children } | Node::Footer { children } => {
                accessibility::parent(&mut ui, id, role, |ui| {
                    self.layout(ui, children, rect, path, false, false, None, None)
                })
            }
            Node::Text { value, .. } => {
                let response = ui.label(value);
                accessibility::hide(&response);
                accessibility::leaf(
                    &ui,
                    id,
                    accesskit::Role::Label,
                    Some(value),
                    Value::None,
                    Some(&response),
                );
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
                let mut state =
                    self.state(&ui, id, serde_json::json!([field, value, min, max, step]));
                let mut edited = state
                    .edited
                    .as_ref()
                    .and_then(|n| n.as_f64())
                    .unwrap_or(*value);
                let previous = edited;
                let hit =
                    egui::Rect::from_min_size(rect.min, egui::vec2(rect.width().max(48.0), 48.0));
                let mut response = ui.interact(hit, id, egui::Sense::click_and_drag());
                let quantum = step.unwrap_or_else(|| {
                    let increment = max / 100.0 - min / 100.0;
                    if increment > 0.0 {
                        increment
                    } else {
                        1.0
                    }
                });
                let snap = |n: f64| {
                    if n.is_nan() {
                        return previous;
                    }
                    let bounded = n.clamp(*min, *max);
                    if step.is_none() {
                        return bounded;
                    }
                    let index = |value: f64| {
                        let difference = value - min;
                        if difference.is_finite() {
                            difference / quantum
                        } else {
                            value / quantum - min / quantum
                        }
                    };
                    let upper = index(*max);
                    let nearest = upper.round();
                    let reconstruction = min + nearest * quantum;
                    let quotient_tolerance = (8.0 * f64::EPSILON * upper.abs().max(1.0)).min(0.25);
                    let value_tolerance =
                        (8.0 * f64::EPSILON * max.abs().max(reconstruction.abs()).max(1.0))
                            .min(quantum.abs() * 0.25);
                    let highest = if upper.is_finite() {
                        if (upper - nearest).abs() <= quotient_tolerance
                            && reconstruction.is_finite()
                            && (max - reconstruction).abs() <= value_tolerance
                        {
                            nearest
                        } else {
                            upper.floor()
                        }
                    } else {
                        f64::INFINITY
                    };
                    let index = index(bounded).round();
                    if !index.is_finite() {
                        return previous;
                    }
                    let index = index.clamp(0.0, highest);
                    let reconstructed = min + index * quantum;
                    let reconstructed = if reconstructed.is_finite() {
                        reconstructed
                    } else {
                        (min / quantum + index) * quantum
                    };
                    if reconstructed.is_finite() {
                        reconstructed.clamp(*min, *max)
                    } else {
                        previous
                    }
                };
                if (response.clicked() || response.dragged())
                    && response.interact_pointer_pos().is_some()
                {
                    let pos = response.interact_pointer_pos().unwrap();
                    let fraction =
                        ((pos.x - hit.left() - 8.0) / (hit.width() - 16.0)).clamp(0.0, 1.0) as f64;
                    edited = snap(min * (1.0 - fraction) + max * fraction);
                    response.request_focus();
                }
                if response.has_focus() {
                    ui.input(|input| {
                        if input.key_pressed(egui::Key::ArrowRight)
                            || input.key_pressed(egui::Key::ArrowUp)
                        {
                            edited = snap(edited + quantum);
                        }
                        if input.key_pressed(egui::Key::ArrowLeft)
                            || input.key_pressed(egui::Key::ArrowDown)
                        {
                            edited = snap(edited - quantum);
                        }
                        if input.key_pressed(egui::Key::Home) {
                            edited = *min;
                        }
                        if input.key_pressed(egui::Key::End) {
                            edited = snap(*max);
                        }
                    });
                }
                ui.input(|input| {
                    for request in [
                        accesskit::Action::Increment,
                        accesskit::Action::Decrement,
                        accesskit::Action::SetValue,
                    ]
                    .into_iter()
                    .flat_map(|action| input.accesskit_action_requests(id, action))
                    {
                        match request.action {
                            accesskit::Action::Increment => edited = snap(edited + quantum),
                            accesskit::Action::Decrement => edited = snap(edited - quantum),
                            accesskit::Action::SetValue => {
                                if let Some(accesskit::ActionData::NumericValue(n)) = request.data {
                                    if n.is_finite() {
                                        edited = snap(n);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                });
                if edited != previous {
                    response.mark_changed();
                    self.events.push(Event::Change {
                        field: field.clone(),
                        value: serde_json::json!(edited),
                        path: path.clone(),
                    });
                }
                let visual = egui::Rect::from_min_size(
                    hit.min + egui::vec2(0.0, 6.0),
                    egui::vec2(hit.width(), 36.0),
                );
                let text = label_size(&ui, label, visual.width());
                ui.painter().text(
                    visual.left_top(),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::monospace(14.0),
                    ui.visuals().text_color(),
                );
                let y = visual.bottom() - 8.0;
                ui.painter().hline(
                    (visual.left() + 8.0)..=(visual.right() - 8.0),
                    y,
                    ui.visuals().widgets.inactive.bg_stroke,
                );
                let fraction = if max > min {
                    let span = max - min;
                    if span.is_finite() {
                        ((edited - min) / span) as f32
                    } else {
                        ((edited / 2.0 - min / 2.0) / (max / 2.0 - min / 2.0)) as f32
                    }
                } else {
                    0.0
                };
                let x = visual.left() + 8.0 + fraction * (visual.width() - 16.0);
                ui.painter().rect(
                    egui::Rect::from_center_size(
                        egui::pos2(x, y),
                        egui::vec2(8.0, (36.0 - text.y).max(8.0)),
                    ),
                    0,
                    ui.visuals().panel_fill,
                    ui.visuals().selection.stroke,
                    egui::StrokeKind::Inside,
                );
                if response.has_focus() {
                    ui.painter().rect_stroke(
                        visual,
                        0,
                        ui.visuals().selection.stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                accessibility::leaf(
                    &ui,
                    id,
                    accesskit::Role::Slider,
                    Some(label),
                    Value::Range {
                        value: edited,
                        min: *min,
                        max: *max,
                        step: quantum,
                    },
                    Some(&response),
                );
                response.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), edited, label));
                state.edited = Some(serde_json::json!(edited));
                state.widget = Some(response.id);
                ui.data_mut(|data| data.insert_temp(id, state));
            }
            Node::Toggle {
                label,
                field,
                value,
                ..
            } => {
                let mut state = self.state(&ui, id, serde_json::json!([field, value]));
                let mut edited = state
                    .edited
                    .as_ref()
                    .and_then(|n| n.as_bool())
                    .unwrap_or(*value);
                let response =
                    Self::control(&mut ui, id, rect, label, Some(&mut edited), false, None);
                accessibility::leaf(
                    &ui,
                    id,
                    accesskit::Role::Switch,
                    Some(label),
                    Value::Toggle(edited),
                    Some(&response),
                );
                if response.changed() {
                    self.events.push(Event::Change {
                        field: field.clone(),
                        value: serde_json::json!(edited),
                        path: path.clone(),
                    });
                }
                state.edited = Some(serde_json::json!(edited));
                ui.data_mut(|data| data.insert_temp(id, state));
            }
            Node::Select {
                label,
                field,
                value,
                options,
                ..
            } => {
                let mut state = self.state(&ui, id, serde_json::json!([field, value, options]));
                let mut edited = state
                    .edited
                    .as_ref()
                    .and_then(|n| n.as_str())
                    .unwrap_or(value)
                    .to_owned();
                let previous = edited.clone();
                let response = ui
                    .push_id(id, |ui| {
                        ui.spacing_mut().interact_size.y = 48.0;
                        egui::ComboBox::from_id_salt(id)
                            .width(rect.width())
                            .selected_text(format!("{label}: {edited}"))
                            .show_ui(ui, |ui| {
                                for option in options {
                                    if ui
                                        .add_sized(
                                            [ui.available_width(), 48.0],
                                            egui::Button::new(option),
                                        )
                                        .clicked()
                                    {
                                        edited = option.clone();
                                        ui.memory_mut(|memory| memory.close_popup());
                                    }
                                }
                            })
                            .response
                    })
                    .inner;
                accessibility::leaf(
                    &ui,
                    response.id,
                    accesskit::Role::ComboBox,
                    Some(label),
                    Value::Text(&edited),
                    Some(&response),
                );
                #[cfg(any(test, feature = "test-observation"))]
                OBSERVATIONS.with(|observations| {
                    if let Some(control) = observations.borrow_mut().last_mut() {
                        control.id = response.id;
                        control.rect = response.rect;
                    }
                });
                if previous != edited {
                    self.events.push(Event::Change {
                        field: field.clone(),
                        value: serde_json::json!(edited),
                        path: path.clone(),
                    });
                }
                state.edited = Some(serde_json::json!(edited));
                state.widget = Some(response.id);
                ui.data_mut(|data| data.insert_temp(id, state));
            }
            Node::Button {
                label,
                event,
                intent,
                confirm,
                disabled,
                ..
            } => {
                let mut state = self.state(
                    &ui,
                    id,
                    serde_json::json!([event, label, confirm, disabled]),
                );
                let response = Self::control(&mut ui, id, rect, label, None, *disabled, *intent);
                accessibility::leaf(
                    &ui,
                    id,
                    accesskit::Role::Button,
                    Some(label),
                    Value::None,
                    Some(&response),
                );
                if *disabled {
                    ui.ctx().accesskit_node_builder(id, |node| {
                        node.set_disabled();
                        node.clear_actions();
                    });
                }
                if !disabled && response.clicked() {
                    if *confirm {
                        state.confirming = true;
                    } else {
                        self.events.push(Event::Action {
                            event: event.clone(),
                            path: path.clone(),
                        });
                    }
                }
                if state.confirming && !disabled {
                    egui::Window::new("Confirm action")
                        .id(id.with("confirmation"))
                        .collapsible(false)
                        .resizable(false)
                        .show(ui.ctx(), |ui| {
                            ui.label(format!("Continue with \u{201c}{label}\u{201d}?"));
                            let yes = crate::surface::button(ui, "Confirm");
                            #[cfg(any(test, feature = "test-observation"))]
                            observe(ui, "confirm", path, yes.rect, yes.id);
                            if yes.clicked() {
                                self.events.push(Event::Action {
                                    event: event.clone(),
                                    path: path.clone(),
                                });
                                state.confirming = false;
                            }
                            let cancel = crate::surface::button(ui, "Cancel");
                            state.confirmation_widgets = vec![yes.id, cancel.id];
                            if cancel.clicked()
                                || ui.input(|input| input.key_pressed(egui::Key::Escape))
                            {
                                state.confirming = false;
                            }
                        });
                }
                ui.data_mut(|data| data.insert_temp(id, state));
            }
        }
    }

    fn state(&mut self, ui: &egui::Ui, id: egui::Id, host: serde_json::Value) -> State {
        self.live.insert(id);
        ui.data_mut(|data| data.get_temp::<State>(id))
            .filter(|state| state.host.as_ref() == Some(&host))
            .unwrap_or(State {
                host: Some(host.clone()),
                edited: Some(host),
                confirming: false,
                widget: None,
                confirmation_widgets: Vec::new(),
            })
    }
    fn control(
        ui: &mut egui::Ui,
        id: egui::Id,
        rect: egui::Rect,
        label: &str,
        mut toggle: Option<&mut bool>,
        disabled: bool,
        intent: Option<ButtonIntent>,
    ) -> egui::Response {
        let hit = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width().max(48.0), 48.0));
        let mut response = ui
            .add_enabled_ui(!disabled, |ui| ui.interact(hit, id, egui::Sense::click()))
            .inner;
        if let Some(value) = toggle.as_mut() {
            if response.clicked() {
                **value = !**value;
                response.mark_changed();
            }
        }
        let visual = hit.shrink2(egui::vec2(0.0, 6.0));
        let color = if disabled {
            crate::theme::muted(ui.visuals().dark_mode)
        } else if intent.is_some() || response.has_focus() || response.hovered() {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        };
        ui.painter().rect_stroke(
            visual,
            0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
        let display = if let Some(value) = toggle {
            format!("{} {label}", if *value { "[x]" } else { "[ ]" })
        } else {
            label.into()
        };
        ui.painter().text(
            visual.center(),
            egui::Align2::CENTER_CENTER,
            display,
            egui::FontId::monospace(14.0),
            color,
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, !disabled, label));
        response
    }

    #[allow(clippy::too_many_arguments)]
    fn layout(
        &mut self,
        ui: &mut egui::Ui,
        children: &[Node],
        rect: egui::Rect,
        path: &mut Vec<usize>,
        horizontal: bool,
        wrap: bool,
        alignment: Option<Alignment>,
        distribution: Option<Distribution>,
    ) {
        let inner = rect.shrink(16.0);
        let sizes: Vec<_> = children
            .iter()
            .map(|n| measure(ui, n, inner.width()))
            .collect();
        let axis = if horizontal { 0 } else { 1 };
        let cross = 1 - axis;
        let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
        let mut used = 0.0;
        for (i, size) in sizes.iter().enumerate() {
            let needed = size[axis]
                + if lines.last().unwrap().is_empty() {
                    0.0
                } else {
                    8.0
                };
            if horizontal && wrap && used > 0.0 && used + needed > inner.width() {
                lines.push(Vec::new());
                used = 0.0;
            }
            used += size[axis]
                + if lines.last().unwrap().is_empty() {
                    0.0
                } else {
                    8.0
                };
            lines.last_mut().unwrap().push(i);
        }
        let mut cross_start = inner.min[cross];
        for line in lines {
            let fill_count = line.iter().filter(|&&i| filled(&children[i])).count();
            let intrinsic = line
                .iter()
                .filter(|&&i| !filled(&children[i]))
                .map(|&i| sizes[i][axis])
                .sum::<f32>();
            let gaps = 8.0 * line.len().saturating_sub(1) as f32;
            let remaining = (inner.size()[axis] - intrinsic - gaps).max(0.0);
            let mut extents: Vec<_> = line.iter().map(|&i| sizes[i][axis]).collect();
            let mut unassigned: Vec<_> = line
                .iter()
                .enumerate()
                .filter_map(|(slot, &i)| filled(&children[i]).then_some(slot))
                .collect();
            let mut budget = remaining;
            while !unassigned.is_empty() {
                let share = budget.max(0.0) / unassigned.len() as f32;
                let pinned: Vec<_> = unassigned
                    .iter()
                    .copied()
                    .filter(|&slot| {
                        let i = line[slot];
                        let minimum = match children[i] {
                            Node::Slider { .. } if horizontal => 48.0,
                            _ => sizes[i][axis],
                        };
                        if minimum > share {
                            extents[slot] = minimum;
                            true
                        } else {
                            false
                        }
                    })
                    .collect();
                if pinned.is_empty() {
                    for slot in unassigned {
                        extents[slot] = share;
                    }
                    break;
                }
                for slot in &pinned {
                    budget -= extents[*slot];
                }
                unassigned.retain(|slot| !pinned.contains(slot));
            }
            if !horizontal && fill_count == 0 {
                extents = line.iter().map(|&i| sizes[i].y).collect();
            }
            let spare = (inner.size()[axis] - extents.iter().sum::<f32>() - gaps).max(0.0);
            let (offset, gap) = match distribution {
                Some(Distribution::Between) if line.len() > 1 => {
                    (0.0, 8.0 + spare / (line.len() - 1) as f32)
                }
                Some(Distribution::Around) if !line.is_empty() => {
                    let extra = spare / line.len() as f32;
                    (extra / 2.0, 8.0 + extra)
                }
                _ => (0.0, 8.0),
            };
            let line_cross = if horizontal {
                line.iter().map(|&i| sizes[i][cross]).fold(0.0, f32::max)
            } else {
                inner.width()
            };
            let mut start = inner.min[axis] + offset;
            for (&i, extent) in line.iter().zip(extents) {
                let cross_extent = sizes[i][cross].min(line_cross);
                let offset = match alignment {
                    Some(Alignment::Center) => (line_cross - cross_extent) / 2.0,
                    Some(Alignment::End) => line_cross - cross_extent,
                    _ => 0.0,
                };
                let mut min = inner.min;
                min[axis] = start;
                min[cross] = cross_start + offset;
                let mut size = sizes[i];
                size[axis] = extent;
                size[cross] = cross_extent;
                let r = egui::Rect::from_min_size(min, size);
                path.push(i);
                self.node(ui, &children[i], r, path);
                path.pop();
                start += extent + gap;
            }
            cross_start += line_cross + 8.0;
        }
    }
}

#[cfg(test)]
#[path = "surface_v1_tests.rs"]
mod tests;
