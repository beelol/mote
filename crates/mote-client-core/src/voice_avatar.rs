use accesskit::Role;
use egui::{Color32, Pos2, Sense, Stroke, Vec2};
use serde::{Deserialize, Serialize};

const AVATAR_SIZE: f32 = 184.0;
const COMPACT_AVATAR_SIZE: f32 = 56.0;
const CONTROL_SIZE: Vec2 = Vec2::new(168.0, 48.0);
const TAU: f32 = std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceState {
    #[default]
    Off,
    Starting,
    Listening,
    Hearing,
    Thinking,
    Speaking,
    Paused,
    Unavailable,
    Error,
}

impl VoiceState {
    pub const ALL: [Self; 9] = [
        Self::Off,
        Self::Starting,
        Self::Listening,
        Self::Hearing,
        Self::Thinking,
        Self::Speaking,
        Self::Paused,
        Self::Unavailable,
        Self::Error,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Voice off",
            Self::Starting => "Voice starting",
            Self::Listening => "Listening",
            Self::Hearing => "Hearing you",
            Self::Thinking => "Thinking",
            Self::Speaking => "Speaking",
            Self::Paused => "Voice paused",
            Self::Unavailable => "Voice unavailable",
            Self::Error => "Voice error",
        }
    }

    const fn can_start(self) -> bool {
        matches!(self, Self::Off | Self::Unavailable | Self::Error)
    }

    const fn control_label(self) -> &'static str {
        if self.can_start() {
            "Start voice"
        } else {
            "Stop voice"
        }
    }

    pub const fn is_animated(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Listening | Self::Hearing | Self::Thinking | Self::Speaking
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceAction {
    Start,
    Stop,
}

#[derive(Clone, Copy, Debug)]
pub struct VoiceAvatar {
    pub state: VoiceState,
    pub elapsed_seconds: f64,
    pub reduced_motion: bool,
}

impl VoiceAvatar {
    pub fn draw(self, ui: &mut egui::Ui) -> Option<VoiceAction> {
        self.draw_with_size(ui, AVATAR_SIZE, CONTROL_SIZE, false)
    }

    pub fn draw_compact(self, ui: &mut egui::Ui) -> Option<VoiceAction> {
        self.draw_with_size(ui, COMPACT_AVATAR_SIZE, Vec2::new(128.0, 48.0), true)
    }

    fn draw_with_size(
        self,
        ui: &mut egui::Ui,
        avatar_size: f32,
        control_size: Vec2,
        horizontal: bool,
    ) -> Option<VoiceAction> {
        let visual = Visual::new(self.state, self.elapsed_seconds, self.reduced_motion);
        let mut action = None;
        let draw = |ui: &mut egui::Ui, action: &mut Option<VoiceAction>| {
            let (rect, avatar_response) =
                ui.allocate_exact_size(Vec2::splat(avatar_size), Sense::hover());
            paint_avatar(ui, rect.center(), visual, avatar_size / AVATAR_SIZE);
            avatar_response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Label, true, self.state.label())
            });
            crate::accessibility::leaf(
                ui,
                avatar_response.id,
                Role::Label,
                Some("Voice status"),
                crate::accessibility::Value::Text(self.state.label()),
                Some(&avatar_response),
            );

            if !horizontal {
                ui.add_space(8.0);
            }
            let label = self.state.control_label();
            let response = ui.add_sized(control_size, egui::Button::new(label));
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
            crate::accessibility::leaf(
                ui,
                response.id,
                Role::Button,
                Some(label),
                crate::accessibility::Value::None,
                Some(&response),
            );
            if response.clicked() {
                *action = Some(if self.state.can_start() {
                    VoiceAction::Start
                } else {
                    VoiceAction::Stop
                });
            }
        };
        if horizontal {
            ui.horizontal(|ui| draw(ui, &mut action));
        } else {
            ui.vertical(|ui| draw(ui, &mut action));
        }
        action
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Visual {
    rotation: f32,
    pulse: f32,
    segments: usize,
    intensity: f32,
}

impl Visual {
    fn new(state: VoiceState, elapsed_seconds: f64, reduced_motion: bool) -> Self {
        let (speed, pulse_speed, segments, intensity) = match state {
            VoiceState::Off => (0.0, 0.0, 2, 0.35),
            VoiceState::Starting => (1.7, 2.8, 3, 0.72),
            VoiceState::Listening => (0.55, 1.2, 3, 0.85),
            VoiceState::Hearing => (1.05, 3.0, 4, 1.0),
            VoiceState::Thinking => (1.9, 1.0, 3, 0.9),
            VoiceState::Speaking => (0.8, 4.2, 5, 1.0),
            VoiceState::Paused => (0.0, 0.0, 2, 0.5),
            VoiceState::Unavailable => (0.0, 0.0, 1, 0.25),
            VoiceState::Error => (0.0, 0.0, 4, 1.0),
        };
        if reduced_motion {
            return Self {
                rotation: 0.0,
                pulse: 0.5 + intensity * 0.08,
                segments,
                intensity,
            };
        }
        let elapsed = elapsed_seconds as f32;
        Self {
            rotation: (elapsed * speed).rem_euclid(TAU),
            pulse: 0.5 + (elapsed * pulse_speed).sin() * 0.08,
            segments,
            intensity,
        }
    }
}

fn paint_avatar(ui: &egui::Ui, center: Pos2, visual: Visual, scale: f32) {
    let accent = ui.visuals().selection.stroke.color;
    let line = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let painter = ui.painter();
    let core_radius = (29.0 + visual.pulse * 8.0) * scale;
    painter.circle_filled(
        center,
        core_radius,
        tint(accent, 0.12 + visual.intensity * 0.08),
    );
    painter.circle_stroke(center, core_radius, Stroke::new(1.0, accent));

    for ring in 0..3 {
        let radius = (51.0 + ring as f32 * 13.0) * scale;
        painter.circle_stroke(
            center,
            radius,
            Stroke::new(scale.max(0.75), tint(line, 0.55)),
        );
        for segment in 0..visual.segments {
            let offset = segment as f32 * TAU / visual.segments as f32;
            let start = visual.rotation * if ring % 2 == 0 { 1.0 } else { -0.7 }
                + offset
                + ring as f32 * 0.7;
            let arc = 0.24 + visual.intensity * 0.25;
            paint_arc(
                painter,
                center,
                radius,
                start,
                start + arc,
                Stroke::new(
                    (1.0 + visual.intensity) * scale.max(0.65),
                    tint(accent, 0.55 + 0.3 * visual.intensity),
                ),
            );
        }
    }

    let spoke = Vec2::angled(visual.rotation) * 77.0 * scale;
    painter.line_segment(
        [center + spoke * 0.84, center + spoke],
        Stroke::new(2.0 * scale.max(0.65), accent),
    );
}

fn paint_arc(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    start: f32,
    end: f32,
    stroke: Stroke,
) {
    let points = (0..=10)
        .map(|step| {
            let angle = egui::lerp(start..=end, step as f32 / 10.0);
            center + Vec2::angled(angle) * radius
        })
        .collect();
    painter.add(egui::Shape::line(points, stroke));
}

fn tint(color: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        color.r(),
        color.g(),
        color.b(),
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_has_a_distinct_accessible_label() {
        let labels = VoiceState::ALL.map(VoiceState::label);
        assert!(labels.iter().all(|label| !label.is_empty()));
        let unique = labels
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), VoiceState::ALL.len());
    }

    #[test]
    fn animation_is_deterministic_and_reduced_motion_is_static() {
        for state in VoiceState::ALL {
            assert_eq!(
                Visual::new(state, 1.25, false),
                Visual::new(state, 1.25, false)
            );
            assert_eq!(
                Visual::new(state, 0.0, true),
                Visual::new(state, 99.0, true)
            );
        }
        assert_ne!(
            Visual::new(VoiceState::Speaking, 0.0, false),
            Visual::new(VoiceState::Speaking, 0.5, false)
        );
        assert!(VoiceState::Starting.is_animated());
        assert!(VoiceState::Listening.is_animated());
        assert!(!VoiceState::Off.is_animated());
        assert!(!VoiceState::Unavailable.is_animated());
    }

    #[test]
    fn status_and_touch_sized_control_are_accessible() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(400.0, 400.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    VoiceAvatar {
                        state: VoiceState::Listening,
                        elapsed_seconds: 1.0,
                        reduced_motion: false,
                    }
                    .draw(ui);
                });
            },
        );
        let update = output
            .platform_output
            .accesskit_update
            .expect("accessibility update");
        let status = update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Voice status"))
            .expect("voice status node");
        assert_eq!(status.1.value(), Some("Listening"));
        let control = update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Stop voice"))
            .expect("voice control node");
        let bounds = control.1.bounds().expect("voice control bounds");
        assert!(bounds.width() >= 48.0 && bounds.height() >= 48.0);
    }

    #[test]
    fn compact_control_keeps_the_same_accessible_state_and_touch_target() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(400.0, 120.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    VoiceAvatar {
                        state: VoiceState::Off,
                        elapsed_seconds: 1.0,
                        reduced_motion: true,
                    }
                    .draw_compact(ui);
                });
            },
        );
        let update = output
            .platform_output
            .accesskit_update
            .expect("accessibility update");
        let status = update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Voice status"))
            .expect("compact voice status");
        assert_eq!(status.1.value(), Some("Voice off"));
        let control = update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some("Start voice"))
            .expect("compact voice control");
        let bounds = control.1.bounds().expect("compact voice bounds");
        assert!(bounds.width() >= 48.0 && bounds.height() >= 48.0);
    }
}
