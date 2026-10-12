use serde::Serialize;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Button,
    Toggle,
    Select,
    Slider,
    TextField,
    Label,
}

#[derive(Clone, Debug, Serialize)]
pub struct Element {
    pub id: u64,
    pub role: Role,
    pub label: String,
    pub value: Option<String>,
    pub enabled: bool,
    pub rect: [f32; 4],
}

thread_local! {
    static FRAME: RefCell<Vec<Element>> = const { RefCell::new(Vec::new()) };
}

pub fn begin_frame() {
    FRAME.with(|frame| frame.borrow_mut().clear());
}

pub fn take_frame() -> Vec<Element> {
    FRAME.with(|frame| {
        let mut elements = std::mem::take(&mut *frame.borrow_mut());
        elements.sort_by(|left, right| {
            left.rect[1]
                .total_cmp(&right.rect[1])
                .then_with(|| left.rect[0].total_cmp(&right.rect[0]))
        });
        elements
    })
}

pub fn record(
    ui: &egui::Ui,
    id: egui::Id,
    role: Role,
    label: &str,
    value: Option<String>,
    response: &egui::Response,
) {
    let rect = response.rect.intersect(ui.clip_rect());
    if !rect.is_positive() || label.is_empty() {
        return;
    }
    let element = Element {
        id: id.value(),
        role,
        label: label.to_owned(),
        value,
        enabled: response.enabled(),
        rect: [rect.left(), rect.top(), rect.width(), rect.height()],
    };
    FRAME.with(|frame| frame.borrow_mut().push(element));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_replaces_stale_controls_and_retains_order() {
        begin_frame();
        let ctx = egui::Context::default();
        let mut first = None;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for label in ["One", "Two"] {
                    let response = ui.button(label);
                    record(ui, response.id, Role::Button, label, None, &response);
                }
                first = Some(take_frame());
            });
        });
        let first = first.unwrap();
        assert_eq!(
            first
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            ["One", "Two"]
        );
        begin_frame();
        assert!(take_frame().is_empty());
    }
}
