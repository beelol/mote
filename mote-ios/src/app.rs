use mote_client_core::{gallery, surface, surface_v1, Surface};

pub struct MoteApp {
    gallery: Surface,
}

impl Default for MoteApp {
    fn default() -> Self {
        Self {
            gallery: gallery::preview(),
        }
    }
}

impl eframe::App for MoteApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.draw(ctx, crate::safe_area());
    }
}

impl MoteApp {
    fn draw(&mut self, ctx: &egui::Context, safe_area: egui::Margin) {
        let mut status = egui::Frame::side_top_panel(&ctx.style());
        status.inner_margin.top = status.inner_margin.top.saturating_add(safe_area.top);
        status.inner_margin.left = status.inner_margin.left.saturating_add(safe_area.left);
        status.inner_margin.right = status.inner_margin.right.saturating_add(safe_area.right);
        egui::TopBottomPanel::top("status")
            .frame(status)
            .show(ctx, |ui| {
                ui.heading("Mote");
                ui.label("Offline gallery");
            });
        let mut body = egui::Frame::central_panel(&ctx.style());
        body.inner_margin.bottom = body.inner_margin.bottom.saturating_add(safe_area.bottom);
        body.inner_margin.left = body.inner_margin.left.saturating_add(safe_area.left);
        body.inner_margin.right = body.inner_margin.right.saturating_add(safe_area.right);
        egui::CentralPanel::default().frame(body).show(ctx, |ui| {
            let screen_height = ui.available_height();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    surface::draw(ui, self.gallery.display_v0(), &mut Vec::new());
                    if let Surface::Gallery { matrix, .. } = &self.gallery {
                        let rect = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(ui.available_width(), screen_height),
                        );
                        ui.allocate_rect(rect, egui::Sense::hover());
                        let mut screen = ui.new_child(
                            egui::UiBuilder::new()
                                .id_salt("matrix-gallery")
                                .max_rect(rect),
                        );
                        surface_v1::draw(&mut screen, matrix, &mut Vec::new());
                    }
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_heading_clears_native_top_safe_area() {
        let ctx = egui::Context::default();
        let mut app = MoteApp::default();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(834.0, 1210.0),
                )),
                ..Default::default()
            },
            |ctx| {
                app.draw(
                    ctx,
                    egui::Margin {
                        top: 32,
                        ..Default::default()
                    },
                )
            },
        );
        let heading = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Mote" => Some(text),
                _ => None,
            })
            .expect("gallery heading is painted");
        assert!(
            heading.pos.y >= 32.0,
            "heading starts under the system status bar at {}",
            heading.pos.y
        );
    }
}
