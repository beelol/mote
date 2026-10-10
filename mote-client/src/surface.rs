use eframe::egui;
use mote_ui::Node;

pub fn draw(ui: &mut egui::Ui, node: &Node, clicked: &mut Vec<String>) {
    match node {
        Node::Text { value } => {
            ui.label(value);
        }
        Node::Stack { children } | Node::List { children } | Node::Chat { children } => {
            egui::Frame::new().inner_margin(16).show(ui, |ui| {
                for (index, child) in children.iter().enumerate() {
                    if index > 0 && !matches!(node, Node::Stack { .. }) {
                        ui.separator();
                    }
                    ui.push_id(index, |ui| draw(ui, child, clicked));
                }
            });
        }
        Node::Row { children } => {
            egui::Frame::new().inner_margin(16).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (index, child) in children.iter().enumerate() {
                        ui.push_id(index, |ui| draw(ui, child, clicked));
                    }
                });
            });
        }
        Node::Table { columns, rows } => {
            egui::Grid::new(ui.next_auto_id())
                .striped(false)
                .show(ui, |ui| {
                    for column in columns {
                        table_cell(ui, column, true);
                    }
                    ui.end_row();
                    for row in rows {
                        for cell in row {
                            table_cell(ui, cell, false);
                        }
                        ui.end_row();
                    }
                });
        }
        Node::Field { label, name, value } => {
            let id = ui.make_persistent_id(("field", name));
            let mut edited = ui
                .data_mut(|data| data.get_temp::<String>(id))
                .unwrap_or_else(|| value.clone());
            control(ui, |ui| {
                ui.label(label);
                ui.add_sized(
                    [ui.available_width().min(320.0), 36.0],
                    egui::TextEdit::singleline(&mut edited).id(id),
                );
            });
            ui.data_mut(|data| data.insert_temp(id, edited));
        }
        Node::Toggle { label, name, value } => {
            let id = ui.make_persistent_id(("toggle", name));
            let mut edited = ui
                .data_mut(|data| data.get_temp::<bool>(id))
                .unwrap_or(*value);
            control(ui, |ui| {
                ui.add_sized([48.0, 36.0], egui::Checkbox::new(&mut edited, label));
            });
            ui.data_mut(|data| data.insert_temp(id, edited));
        }
        Node::Select {
            label,
            name,
            options,
            value,
        } => {
            let id = ui.make_persistent_id(("select", name));
            let mut selected = ui
                .data_mut(|data| data.get_temp::<String>(id))
                .unwrap_or_else(|| value.clone());
            control(ui, |ui| {
                ui.label(label);
                egui::ComboBox::from_id_salt(id)
                    .selected_text(&selected)
                    .show_ui(ui, |ui| {
                        for option in options {
                            ui.selectable_value(&mut selected, option.clone(), option);
                        }
                    });
            });
            ui.data_mut(|data| data.insert_temp(id, selected));
        }
        Node::Action { label, event } => {
            if button(ui, label).clicked() {
                clicked.push(event.clone());
            }
        }
        Node::Progress { value } => {
            let value = value.clamp(0.0, 1.0);
            let (rect, _) = ui.allocate_exact_size(
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
            crate::theme::caption(ui, &format!("{:.0}%", value * 100.0));
        }
    }
}

fn table_cell(ui: &mut egui::Ui, text: &str, heading: bool) {
    egui::Frame::new()
        .inner_margin(8)
        .stroke(ui.visuals().widgets.inactive.bg_stroke)
        .show(ui, |ui| {
            if heading {
                ui.strong(text);
            } else {
                ui.label(text);
            }
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
        ui.add_sized([48.0, 36.0], egui::Button::new(label))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        node: &Node,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<String>, egui::Rect) {
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
        assert_eq!(clicked, vec!["job.run"]);
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
        assert_eq!(clicked, vec!["job.run"]);
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
        assert_eq!(clicked, vec!["job.run"]);
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
