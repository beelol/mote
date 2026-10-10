use eframe::egui;

pub fn style(dark: bool) -> egui::Style {
    let (surface, text, line, accent) = if dark {
        (
            rgb(11, 11, 12),
            rgb(232, 227, 218),
            rgb(46, 46, 48),
            rgb(229, 72, 77),
        )
    } else {
        (
            rgb(244, 241, 234),
            rgb(26, 26, 26),
            rgb(216, 211, 200),
            rgb(200, 50, 46),
        )
    };
    let mut style = egui::Style {
        visuals: if dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        },
        ..Default::default()
    };
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.window_margin = egui::Margin::same(16);
    style.spacing.menu_margin = egui::Margin::same(16);
    style.spacing.interact_size = egui::vec2(48.0, 36.0);
    style.interaction.interact_radius = 6.0;
    style.animation_time = 0.18;
    style.text_styles = [
        (egui::TextStyle::Heading, egui::FontId::monospace(20.0)),
        (egui::TextStyle::Body, egui::FontId::monospace(14.0)),
        (egui::TextStyle::Button, egui::FontId::monospace(14.0)),
        (egui::TextStyle::Small, egui::FontId::monospace(12.0)),
        (egui::TextStyle::Monospace, egui::FontId::monospace(13.0)),
    ]
    .into();
    let v = &mut style.visuals;
    v.override_text_color = Some(text);
    v.panel_fill = surface;
    v.window_fill = surface;
    v.extreme_bg_color = surface;
    v.faint_bg_color = surface;
    v.code_bg_color = surface;
    v.window_stroke = egui::Stroke::new(1.0, line);
    v.window_corner_radius = egui::CornerRadius::ZERO;
    v.menu_corner_radius = egui::CornerRadius::ZERO;
    v.window_shadow = egui::epaint::Shadow::NONE;
    v.popup_shadow = egui::epaint::Shadow::NONE;
    v.selection.bg_fill = surface;
    v.selection.stroke = egui::Stroke::new(1.0, accent);
    v.text_cursor.stroke = egui::Stroke::new(1.0, accent);
    v.hyperlink_color = accent;
    v.warn_fg_color = rgb(217, 164, 65);
    v.error_fg_color = accent;
    for widget in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.bg_fill = surface;
        widget.weak_bg_fill = surface;
        widget.bg_stroke = egui::Stroke::new(1.0, line);
        widget.fg_stroke = egui::Stroke::new(1.0, text);
        widget.corner_radius = egui::CornerRadius::ZERO;
        widget.expansion = 0.0;
    }
    v.widgets.hovered.bg_stroke.color = accent;
    v.widgets.active.bg_stroke.color = accent;
    v.widgets.open.bg_stroke.color = accent;
    style
}

fn rgb(r: u8, g: u8, b: u8) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}

pub fn muted(dark: bool) -> egui::Color32 {
    if dark {
        rgb(138, 133, 120)
    } else {
        rgb(107, 102, 89)
    }
}

pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .small()
            .color(muted(ui.visuals().dark_mode)),
    );
}

pub fn selectable_label(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    let response = ui.selectable_label(selected, text);
    if selected && ui.is_rect_visible(response.rect) {
        ui.painter().rect_stroke(
            response.rect,
            0,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
    response
}

pub fn install(ctx: &egui::Context) {
    ctx.set_style_of(egui::Theme::Dark, style(true));
    ctx.set_style_of(egui::Theme::Light, style(false));
    ctx.set_theme(egui::ThemePreference::System);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_controls_keep_the_surface_fill_in_both_themes() {
        for (dark, surface) in [
            (true, egui::Color32::from_rgb(11, 11, 12)),
            (false, egui::Color32::from_rgb(244, 241, 234)),
        ] {
            let ctx = egui::Context::default();
            ctx.set_style(style(dark));
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let _ = selectable_label(ui, true, "Selected");
                });
            });
            assert!(output
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape, egui::Shape::Rect(_))));
            let accent = if dark {
                egui::Color32::from_rgb(229, 72, 77)
            } else {
                egui::Color32::from_rgb(200, 50, 46)
            };
            assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.stroke.color == accent)), "selected control has no red outline");
            for shape in output.shapes {
                if let egui::Shape::Rect(rect) = shape.shape {
                    assert!(
                        rect.fill == surface || rect.fill == egui::Color32::TRANSPARENT,
                        "selected control introduced fill {:?}",
                        rect.fill
                    );
                }
            }
        }
    }

    #[test]
    fn runtime_selects_vector_for_system_light_and_dark() {
        let ctx = egui::Context::default();
        install(&ctx);
        for (theme, color) in [
            (egui::Theme::Light, egui::Color32::from_rgb(244, 241, 234)),
            (egui::Theme::Dark, egui::Color32::from_rgb(11, 11, 12)),
        ] {
            let _ = ctx.run(
                egui::RawInput {
                    system_theme: Some(theme),
                    ..Default::default()
                },
                |_| {},
            );
            assert_eq!(ctx.style().visuals.panel_fill, color);
        }
    }

    #[test]
    fn vector_uses_exact_dark_and_light_colors() {
        for (dark, surface, text, muted, line, accent) in [
            (
                true,
                [11, 11, 12],
                [232, 227, 218],
                [138, 133, 120],
                [46, 46, 48],
                [229, 72, 77],
            ),
            (
                false,
                [244, 241, 234],
                [26, 26, 26],
                [107, 102, 89],
                [216, 211, 200],
                [200, 50, 46],
            ),
        ] {
            let s = style(dark);
            assert_eq!(s.visuals.dark_mode, dark);
            assert_eq!(
                s.visuals.panel_fill,
                egui::Color32::from_rgb(surface[0], surface[1], surface[2])
            );
            assert_eq!(
                s.visuals.text_color(),
                egui::Color32::from_rgb(text[0], text[1], text[2])
            );
            assert_eq!(
                super::muted(dark),
                egui::Color32::from_rgb(muted[0], muted[1], muted[2])
            );
            assert_eq!(
                s.visuals.widgets.inactive.bg_stroke.color,
                egui::Color32::from_rgb(line[0], line[1], line[2])
            );
            assert_eq!(
                s.visuals.selection.stroke.color,
                egui::Color32::from_rgb(accent[0], accent[1], accent[2])
            );
        }
    }

    #[test]
    fn vector_applies_fixed_spacing_monospace_and_sharp_strokes() {
        let s = style(true);
        assert_eq!(s.spacing.item_spacing, egui::vec2(8.0, 8.0));
        assert_eq!(s.spacing.window_margin, egui::Margin::same(16));
        assert_eq!(s.spacing.interact_size.y, 36.0);
        assert_eq!(
            s.text_styles[&egui::TextStyle::Body],
            egui::FontId::monospace(14.0)
        );
        assert_eq!(
            s.text_styles[&egui::TextStyle::Heading],
            egui::FontId::monospace(20.0)
        );
        assert_eq!(
            s.text_styles[&egui::TextStyle::Small],
            egui::FontId::monospace(12.0)
        );
        assert_eq!(
            s.visuals.widgets.inactive.corner_radius,
            egui::CornerRadius::ZERO
        );
        assert_eq!(s.visuals.widgets.inactive.bg_stroke.width, 1.0);
        assert_eq!(s.visuals.window_shadow, egui::epaint::Shadow::NONE);
    }
}
