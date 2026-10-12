use accesskit::{Action, Role, Toggled};

#[derive(Clone, Copy)]
pub enum Value<'a> {
    None,
    Text(&'a str),
    Toggle(bool),
    Progress(f64),
    Range {
        value: f64,
        min: f64,
        max: f64,
        step: f64,
    },
}

pub fn id(namespace: egui::Id, path: &[usize], salt: &'static str) -> egui::Id {
    namespace.with(("surface-accessibility", path, salt))
}

pub fn parent<R>(
    ui: &mut egui::Ui,
    id: egui::Id,
    role: Role,
    draw: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.ctx()
        .accesskit_node_builder(id, |node| node.set_role(role));
    let ctx = ui.ctx().clone();
    ctx.with_accessibility_parent(id, || draw(ui))
}

pub fn leaf(
    ui: &egui::Ui,
    id: egui::Id,
    role: Role,
    name: Option<&str>,
    value: Value<'_>,
    response: Option<&egui::Response>,
) {
    #[cfg(target_os = "ios")]
    if let Some(response) = response {
        use crate::native_accessibility::{self, Role as NativeRole};
        let native_role = match role {
            Role::Button => Some(NativeRole::Button),
            Role::Switch => Some(NativeRole::Toggle),
            Role::ComboBox => Some(NativeRole::Select),
            Role::Slider => Some(NativeRole::Slider),
            Role::Label => Some(NativeRole::Label),
            Role::TextInput => Some(NativeRole::TextField),
            _ => None,
        };
        if let (Some(native_role), Some(name)) = (native_role, name) {
            let native_value = match value {
                Value::None => None,
                Value::Text(text) => Some(text.to_owned()),
                Value::Toggle(on) => Some(if on { "On" } else { "Off" }.to_owned()),
                Value::Progress(progress) => Some(progress.to_string()),
                Value::Range { value, .. } => Some(value.to_string()),
            };
            native_accessibility::record(ui, id, native_role, name, native_value, response);
        }
    }
    ui.ctx().accesskit_node_builder(id, |node| {
        node.set_role(role);
        if let Some(name) = name {
            node.set_label(name);
        }
        match value {
            Value::None => {}
            Value::Text(value) => node.set_value(value),
            Value::Toggle(value) => {
                node.set_toggled(if value { Toggled::True } else { Toggled::False })
            }
            Value::Progress(value) => node.set_numeric_value(value),
            Value::Range {
                value,
                min,
                max,
                step,
            } => {
                node.set_numeric_value(value);
                node.set_min_numeric_value(min);
                node.set_max_numeric_value(max);
                node.set_numeric_value_step(step);
                node.add_action(Action::SetValue);
                node.add_action(Action::Increment);
                node.add_action(Action::Decrement);
            }
        }
        if let Some(response) = response {
            node.set_bounds(accesskit::Rect {
                x0: response.rect.left().into(),
                y0: response.rect.top().into(),
                x1: response.rect.right().into(),
                y1: response.rect.bottom().into(),
            });
            if response.sense.is_focusable() {
                node.add_action(Action::Focus);
            }
            if response.sense.senses_click() {
                node.add_action(Action::Click);
            }
        }
    });
}

pub fn hide(response: &egui::Response) {
    hide_id(&response.ctx, response.id);
}

pub fn hide_id(ctx: &egui::Context, id: egui::Id) {
    ctx.accesskit_node_builder(id, |node| node.set_hidden());
}
