use crate::surface::SurfaceEvent;
use egui::{Color32, Pos2, Rect, Shape, Stroke};

const DURATION: f64 = 0.120;
const MAX_PARTICLES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StructuralId(egui::Id);

impl StructuralId {
    pub fn new(namespace: egui::Id, path: &[usize], kind: &str, part: usize) -> Self {
        Self(namespace.with((path, kind, part)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructuralStroke {
    pub id: StructuralId,
    pub points: Vec<Pos2>,
    pub clip: Rect,
    pub stroke: Stroke,
}

impl StructuralStroke {
    fn same_geometry(&self, other: &Self) -> bool {
        self.id == other.id && self.points == other.points && self.clip == other.clip
    }
}

#[derive(Clone, Debug, Default)]
pub struct MaterializeState {
    current: Vec<StructuralStroke>,
    changes: Vec<Change>,
    initialized: bool,
    #[cfg(any(test, all(target_os = "ios", target_abi = "sim")))]
    observe: bool,
}

#[derive(Clone, Debug)]
struct Change {
    old: Option<StructuralStroke>,
    new: Option<StructuralStroke>,
    started: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterializeFrame {
    pub hidden: Vec<StructuralId>,
    pub paint: Vec<egui::epaint::ClippedShape>,
    pub active: bool,
    #[cfg(any(test, all(target_os = "ios", target_abi = "sim")))]
    paint_owners: Vec<StructuralId>,
}

#[cfg(any(test, all(target_os = "ios", target_abi = "sim")))]
#[derive(Clone, Debug, serde::Serialize)]
pub struct StrokeObservation {
    pub points: Vec<[f32; 2]>,
    pub previous_points: Option<Vec<[f32; 2]>>,
    pub painted: bool,
    pub particle_count: usize,
}

#[cfg(any(test, all(target_os = "ios", target_abi = "sim")))]
pub fn enable_materialization_observation(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("observe-materialization"), true));
}

#[cfg(any(test, all(target_os = "ios", target_abi = "sim")))]
pub fn materialization_observation(
    ctx: &egui::Context,
    namespace: egui::Id,
    path: &[usize],
    kind: &str,
) -> Option<StrokeObservation> {
    ctx.data_mut(|data| {
        data.get_temp::<std::collections::HashMap<StructuralId, StrokeObservation>>(
            namespace.with("materialization-observations"),
        )
    })?
    .get(&StructuralId::new(namespace, path, kind, 0))
    .cloned()
}

impl MaterializeState {
    pub fn replace(&mut self, strokes: Vec<StructuralStroke>, now: f64) {
        if self.initialized {
            self.changes
                .retain(|change| now - change.started < DURATION);
            for old in &self.current {
                if !strokes.iter().any(|new| new.same_geometry(old)) {
                    self.changes.retain(|change| {
                        change.new.as_ref().or(change.old.as_ref()).unwrap().id != old.id
                    });
                    self.changes.push(Change {
                        old: Some(old.clone()),
                        new: strokes.iter().find(|new| new.id == old.id).cloned(),
                        started: now,
                    });
                }
            }
            for new in &strokes {
                if !self.current.iter().any(|old| old.id == new.id) {
                    self.changes
                        .retain(|change| change.old.as_ref().is_none_or(|old| old.id != new.id));
                    self.changes.push(Change {
                        old: None,
                        new: Some(new.clone()),
                        started: now,
                    });
                }
            }
        }
        self.initialized = true;
        self.current = strokes;
    }

    pub fn frame(&self, now: f64, reduced_motion: bool, accent: Color32) -> MaterializeFrame {
        let mut frame = MaterializeFrame::default();
        let stroke_count = self
            .changes
            .iter()
            .filter(|change| now - change.started + 1e-9 < DURATION)
            .map(|change| usize::from(change.old.is_some()) + usize::from(change.new.is_some()))
            .sum::<usize>();
        let budget = MAX_PARTICLES / stroke_count.max(1);
        for change in &self.changes {
            let elapsed = (now - change.started).max(0.0);
            if elapsed + 1e-9 >= DURATION {
                continue;
            }
            frame.active = true;
            let t = (elapsed / DURATION) as f32;
            let eased = 1.0 - (1.0 - t).powi(3);
            if reduced_motion {
                if let Some(new) = &change.new {
                    #[cfg(any(test, all(target_os = "ios", target_abi = "sim")))]
                    if self.observe {
                        frame.paint_owners.push(new.id);
                    }
                    frame.paint.push(egui::epaint::ClippedShape {
                        clip_rect: new.clip,
                        shape: Shape::line(
                            new.points.clone(),
                            Stroke::new(2.0, accent.gamma_multiply(1.0 - t)),
                        ),
                    });
                }
                continue;
            }
            if let Some(new) = &change.new {
                frame.hidden.push(new.id);
            }
            for (stroke, incoming) in [(&change.old, false), (&change.new, true)] {
                let Some(stroke) = stroke else {
                    continue;
                };
                let points = samples(stroke, budget);
                let mut shapes = Vec::with_capacity(points.len());
                for (index, point) in points.into_iter().enumerate() {
                    let hash = stroke.id.0.with(index).value();
                    let drift = egui::vec2(
                        ((hash & 15) as f32 - 7.5) * 2.0,
                        (((hash >> 4) & 15) as f32 - 7.5) * 2.0,
                    );
                    let center = point + drift * if incoming { 1.0 - eased } else { eased };
                    let alpha = if incoming {
                        0.3 + 0.7 * eased
                    } else {
                        1.0 - eased
                    };
                    shapes.push(Shape::rect_filled(
                        Rect::from_center_size(center, egui::vec2(4.0, 4.0)),
                        0,
                        stroke.stroke.color.gamma_multiply(alpha),
                    ));
                }
                #[cfg(any(test, all(target_os = "ios", target_abi = "sim")))]
                if self.observe {
                    frame.paint_owners.push(stroke.id);
                }
                frame.paint.push(egui::epaint::ClippedShape {
                    clip_rect: stroke.clip,
                    shape: Shape::Vec(shapes),
                });
            }
        }
        frame
    }
}

fn samples(stroke: &StructuralStroke, budget: usize) -> Vec<Pos2> {
    let mut points = Vec::new();
    let clip = stroke.clip.expand(16.0);
    let mut segments = Vec::new();
    for segment in stroke.points.windows(2) {
        let delta = segment[1] - segment[0];
        let mut start = 0.0_f32;
        let mut end = 1.0_f32;
        for axis in 0..2 {
            if delta[axis] == 0.0 {
                if segment[0][axis] < clip.min[axis] || segment[0][axis] > clip.max[axis] {
                    end = -1.0;
                }
            } else {
                let a = (clip.min[axis] - segment[0][axis]) / delta[axis];
                let b = (clip.max[axis] - segment[0][axis]) / delta[axis];
                start = start.max(a.min(b));
                end = end.min(a.max(b));
            }
        }
        if end > start {
            segments.push((segment[0] + delta * start, delta * (end - start)));
        }
    }
    let length = segments
        .iter()
        .map(|(_, delta)| delta.length())
        .sum::<f32>();
    let count = ((length / 4.0).ceil() as usize).min(budget);
    let step = length / count.max(1) as f32;
    let mut offset = 0.0;
    for (start, delta) in segments {
        let length = delta.length();
        while offset < length && points.len() < count {
            points.push(start + delta * (offset / length));
            offset += step;
        }
        offset -= length;
    }
    points.sort_by_key(|point| {
        stroke
            .id
            .0
            .with((point.x.to_bits(), point.y.to_bits()))
            .value()
    });
    points
}

pub fn set_reduced_motion(ctx: &egui::Context, reduced: bool) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("vector-reduced-motion"), reduced));
}

#[derive(Default)]
pub(crate) struct StrokeRegistry {
    strokes: Vec<StructuralStroke>,
    slots: Vec<(egui::LayerId, egui::layers::ShapeIdx)>,
}

pub(crate) fn paint_start(ui: &egui::Ui) -> usize {
    ui.ctx().graphics(|graphics| {
        graphics
            .get(ui.layer_id())
            .map_or(0, |list| list.next_idx().0)
    })
}

impl StrokeRegistry {
    pub fn capture(
        &mut self,
        ui: &egui::Ui,
        namespace: egui::Id,
        path: &[usize],
        kind: &str,
        start: usize,
        lines_only: bool,
    ) {
        let shapes = ui.ctx().graphics(|graphics| {
            graphics
                .get(ui.layer_id())
                .map(|list| list.all_entries().skip(start).cloned().collect::<Vec<_>>())
                .unwrap_or_default()
        });
        let mut part = 0;
        for (offset, clipped) in shapes.into_iter().enumerate() {
            let (points, stroke) = match clipped.shape {
                Shape::Rect(rect) if !lines_only && rect.stroke.width > 0.0 => (
                    vec![
                        rect.rect.left_top(),
                        rect.rect.right_top(),
                        rect.rect.right_bottom(),
                        rect.rect.left_bottom(),
                        rect.rect.left_top(),
                    ],
                    rect.stroke,
                ),
                Shape::LineSegment { points, stroke } if lines_only => (points.to_vec(), stroke),
                _ => continue,
            };
            self.strokes.push(StructuralStroke {
                id: StructuralId::new(namespace, path, kind, part),
                points,
                clip: clipped.clip_rect,
                stroke,
            });
            self.slots
                .push((ui.layer_id(), egui::layers::ShapeIdx(start + offset)));
            part += 1;
        }
    }

    pub fn finish(self, ui: &egui::Ui, namespace: egui::Id) {
        let key = namespace.with("vector-materialize");
        let mut state = ui
            .data_mut(|data| data.get_temp::<MaterializeState>(key))
            .unwrap_or_default();
        let now = ui.input(|input| input.time);
        state.replace(self.strokes.clone(), now);
        let reduced = ui
            .data_mut(|data| data.get_temp::<bool>(egui::Id::new("vector-reduced-motion")))
            .unwrap_or(false);
        #[cfg(any(test, all(target_os = "ios", target_abi = "sim")))]
        {
            state.observe = ui
                .data_mut(|data| data.get_temp::<bool>(egui::Id::new("observe-materialization")))
                .unwrap_or(false);
        }
        let frame = state.frame(now, reduced, ui.visuals().selection.stroke.color);
        #[cfg(any(test, all(target_os = "ios", target_abi = "sim")))]
        if state.observe {
            let observations: std::collections::HashMap<_, _> = self
                .strokes
                .iter()
                .map(|stroke| {
                    let previous_points = state
                        .changes
                        .iter()
                        .find_map(|change| change.old.as_ref().filter(|old| old.id == stroke.id))
                        .map(|old| old.points.iter().map(|point| [point.x, point.y]).collect());
                    let particle_count = frame
                        .paint_owners
                        .iter()
                        .zip(&frame.paint)
                        .filter(|(owner, _)| **owner == stroke.id)
                        .map(|(_, paint)| match &paint.shape {
                            Shape::Vec(shapes) => shapes.len(),
                            _ => 0,
                        })
                        .sum();
                    (
                        stroke.id,
                        StrokeObservation {
                            points: stroke
                                .points
                                .iter()
                                .map(|point| [point.x, point.y])
                                .collect(),
                            previous_points,
                            painted: !frame.hidden.contains(&stroke.id),
                            particle_count,
                        },
                    )
                })
                .collect();
            ui.data_mut(|data| {
                data.insert_temp(namespace.with("materialization-observations"), observations)
            });
        }
        ui.ctx().graphics_mut(|graphics| {
            for (stroke, (layer, slot)) in self.strokes.iter().zip(self.slots) {
                if frame.hidden.contains(&stroke.id) {
                    graphics
                        .entry(layer)
                        .mutate_shape(slot, |shape| match &mut shape.shape {
                            Shape::Rect(rect) => rect.stroke = Stroke::NONE,
                            _ => shape.shape = Shape::Noop,
                        });
                }
            }
            for paint in frame.paint {
                graphics
                    .entry(ui.layer_id())
                    .add(paint.clip_rect, paint.shape);
            }
        });
        if frame.active {
            ui.ctx().request_repaint();
        }
        ui.data_mut(|data| data.insert_temp(key, state));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gallery::{Gallery, GalleryPage},
        surface_v1,
    };

    #[test]
    fn ordinary_materialization_records_no_paint_owners() {
        let mut state = MaterializeState::default();
        state.replace(vec![line(0, 20.0)], 0.0);
        state.replace(vec![line(0, 40.0)], 1.0);
        for reduced in [false, true] {
            let frame = state.frame(1.0, reduced, Color32::RED);
            assert!(!frame.paint.is_empty());
            assert!(
                frame.paint_owners.is_empty(),
                "disabled observation must not collect owners"
            );
            assert_eq!(
                frame.paint_owners.capacity(),
                0,
                "disabled observation must not allocate owner storage"
            );
            state.observe = true;
            let observed = state.frame(1.0, reduced, Color32::RED);
            assert!(!observed.paint_owners.is_empty());
            assert_eq!(observed.paint, frame.paint);
            assert_eq!(observed.hidden, frame.hidden);
            state.observe = false;
        }
    }

    fn frame(
        ctx: &egui::Context,
        gallery: &Gallery,
        time: f64,
        events: Vec<egui::Event>,
    ) -> (egui::FullOutput, Vec<surface_v1::Event>) {
        let mut emitted = Vec::new();
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(834.0, 1194.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    surface_v1::draw(ui, gallery.surface().matrix_v1().unwrap(), &mut emitted)
                });
            },
        );
        (output, emitted)
    }

    fn context() -> egui::Context {
        let ctx = egui::Context::default();
        crate::theme::install(&ctx);
        ctx
    }

    fn switch_frames() -> Vec<Vec<egui::epaint::ClippedShape>> {
        let ctx = context();
        let mut gallery = Gallery::default();
        frame(&ctx, &gallery, 0.0, vec![]);
        gallery.select(GalleryPage::Toggle);
        [1.0, 1.06, 1.12, 1.3]
            .into_iter()
            .map(|time| frame(&ctx, &gallery, time, vec![]).0.shapes)
            .collect()
    }

    fn particles(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::Rect> {
        shapes
            .iter()
            .flat_map(|shape| match &shape.shape {
                egui::Shape::Vec(shapes) => shapes.as_slice(),
                shape => std::slice::from_ref(shape),
            })
            .filter_map(|shape| match shape {
                egui::Shape::Rect(rect)
                    if rect.rect.size() == egui::vec2(4.0, 4.0)
                        && rect.fill != egui::Color32::TRANSPARENT =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn changed_strokes_materialize_at_0_60_120_ms() {
        let frames = switch_frames();
        assert!(
            !particles(&frames[0]).is_empty(),
            "replacement must paint square particles"
        );
        assert!(!particles(&frames[1]).is_empty());
        assert_ne!(frames[0], frames[1]);
        assert!(particles(&frames[2]).is_empty());
        assert_eq!(frames[2], frames[3]);
    }

    #[test]
    fn particle_frames_are_deterministic() {
        let first = switch_frames();
        assert!(!particles(&first[1]).is_empty());
        assert_eq!(first, switch_frames());
    }

    fn line(part: usize, y: f32) -> StructuralStroke {
        StructuralStroke {
            id: StructuralId::new(egui::Id::new("surface"), &[part], "divider", 0),
            points: vec![egui::pos2(20.0, y), egui::pos2(60.0, y)],
            clip: Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(100.0, 100.0)),
            stroke: Stroke::new(1.0, Color32::from_rgb(104, 29, 36)),
        }
    }

    #[test]
    fn unchanged_strokes_do_not_restart() {
        let mut state = MaterializeState::default();
        state.replace(vec![line(0, 20.0), line(1, 40.0)], 0.0);
        state.replace(vec![line(0, 20.0), line(1, 48.0)], 1.0);
        assert_eq!(
            state.frame(1.0, false, Color32::RED).hidden,
            vec![line(1, 48.0).id]
        );
        state.replace(vec![line(0, 20.0), line(1, 48.0)], 1.06);
        assert!(!state.frame(1.12, false, Color32::RED).active);
        state.replace(vec![line(0, 20.0), line(1, 48.0), line(2, 64.0)], 1.13);
        assert_eq!(
            state.frame(1.13, false, Color32::RED).hidden,
            vec![line(2, 64.0).id]
        );
    }

    #[test]
    fn reduced_motion_swaps_and_pulses() {
        let ctx = context();
        set_reduced_motion(&ctx, true);
        let mut gallery = Gallery::default();
        frame(&ctx, &gallery, 0.0, vec![]);
        gallery.select(GalleryPage::Toggle);
        let first = frame(&ctx, &gallery, 1.0, vec![]).0;
        let middle = frame(&ctx, &gallery, 1.06, vec![]).0;
        let final_frame = frame(&ctx, &gallery, 1.12, vec![]).0;
        assert!(particles(&first.shapes).is_empty());
        assert!(particles(&middle.shapes).is_empty());
        let outlines = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Rect(rect) if rect.stroke.width > 0.0 => Some(rect.rect),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(outlines(&first), outlines(&final_frame));
        assert!(first
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape, Shape::Path(path) if path.stroke.width == 2.0)));
        assert_ne!(first.shapes, middle.shapes);
        assert!(!final_frame
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape, Shape::Path(path) if path.stroke.width == 2.0)));
    }

    #[test]
    fn particle_travel_stays_within_sixteen_points_of_changed_borders() {
        let mut state = MaterializeState::default();
        state.replace(vec![line(0, 20.0)], 0.0);
        state.replace(vec![line(0, 40.0)], 1.0);
        for time in [1.0, 1.06] {
            let frame = state.frame(time, false, Color32::RED);
            let bounds = Rect::from_min_max(egui::pos2(3.0, 3.0), egui::pos2(77.0, 57.0));
            let squares = particles(&frame.paint);
            assert!(!squares.is_empty());
            assert!(squares.iter().all(|rect| bounds.contains_rect(*rect)));
        }
    }

    #[test]
    fn particle_work_has_a_fixed_frame_budget() {
        let mut state = MaterializeState::default();
        state.replace(vec![], 0.0);
        let strokes = (0..8)
            .map(|index| {
                let mut stroke = line(index, 40.0 + index as f32 * 8.0);
                stroke.points[1].x = 10_000.0;
                stroke.clip.max.x = 20_000.0;
                stroke
            })
            .collect();
        state.replace(strokes, 1.0);
        assert!(particles(&state.frame(1.0, false, Color32::RED).paint).len() <= 8192);
    }

    #[test]
    fn page_switch_installs_new_hit_targets_on_first_frame() {
        let ctx = context();
        let mut gallery = Gallery::default();
        frame(&ctx, &gallery, 0.0, vec![]);
        gallery.select(GalleryPage::Toggle);
        let pos = egui::pos2(50.0, 40.0);
        let (output, events) = frame(
            &ctx,
            &gallery,
            1.0,
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
            ],
        );
        assert_eq!(
            events,
            vec![surface_v1::Event::Change {
                field: "gallery_enabled".into(),
                value: serde_json::json!(false),
                path: vec![0]
            }]
        );
        assert!(!particles(&output.shapes).is_empty());
        assert!(frame(&ctx, &gallery, 1.016, vec![]).1.is_empty());
        assert!(frame(&ctx, &gallery, 1.12, vec![]).1.is_empty());
    }

    fn click(pos: egui::Pos2) -> Vec<egui::Event> {
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
    fn replacement_controls_accept_one_click_and_keep_keyboard_and_accessibility_events() {
        for page in [
            GalleryPage::Toggle,
            GalleryPage::Slider,
            GalleryPage::Select,
        ] {
            let ctx = context();
            let mut gallery = Gallery::default();
            frame(&ctx, &gallery, 0.0, vec![]);
            gallery.select(page);
            let (_, events) = frame(&ctx, &gallery, 1.0, click(egui::pos2(700.0, 40.0)));
            if page == GalleryPage::Select {
                assert!(events.is_empty());
                assert!(ctx.memory(|memory| memory.any_popup_open()));
            } else {
                assert_eq!(events.len(), 1, "page {page:?}");
                assert_eq!(
                    events[0],
                    surface_v1::Event::Change {
                        field: if page == GalleryPage::Toggle {
                            "gallery_enabled"
                        } else {
                            "gallery_volume"
                        }
                        .into(),
                        value: if page == GalleryPage::Toggle {
                            serde_json::json!(false)
                        } else {
                            serde_json::json!(8.0)
                        },
                        path: vec![0]
                    }
                );
            }
            assert!(frame(&ctx, &gallery, 1.016, vec![]).1.is_empty());
            assert!(frame(&ctx, &gallery, 1.12, vec![]).1.is_empty());
            if page == GalleryPage::Select {
                continue;
            }
            let id = ctx
                .memory(|memory| memory.focused())
                .expect("new control receives focus");
            let key = if page == GalleryPage::Toggle {
                egui::Key::Space
            } else {
                egui::Key::ArrowRight
            };
            let (_, keyboard) = frame(
                &ctx,
                &gallery,
                1.2,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
            assert_eq!(keyboard.len(), 1);
            assert!(frame(
                &ctx,
                &gallery,
                1.22,
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: Default::default()
                }]
            )
            .1
            .is_empty());
            let action = if page == GalleryPage::Toggle {
                accesskit::Action::Click
            } else {
                accesskit::Action::Decrement
            };
            let (_, accessible) = frame(
                &ctx,
                &gallery,
                1.3,
                vec![egui::Event::AccessKitActionRequest(
                    accesskit::ActionRequest {
                        action,
                        target: accesskit::NodeId(id.value()),
                        data: None,
                    },
                )],
            );
            assert_eq!(accessible.len(), 1);
            assert!(frame(&ctx, &gallery, 1.32, vec![]).1.is_empty());
        }
    }

    #[test]
    fn first_replacement_press_and_next_frame_release_emit_once() {
        let ctx = context();
        let mut gallery = Gallery::default();
        frame(&ctx, &gallery, 0.0, vec![]);
        gallery.select(GalleryPage::Toggle);
        let events = click(egui::pos2(50.0, 40.0));
        assert!(frame(&ctx, &gallery, 1.0, events[..2].to_vec())
            .1
            .is_empty());
        assert_eq!(
            frame(&ctx, &gallery, 1.016, events[2..].to_vec()).1.len(),
            1
        );
        assert!(frame(&ctx, &gallery, 1.032, vec![]).1.is_empty());
        assert!(frame(&ctx, &gallery, 1.12, vec![]).1.is_empty());
    }

    #[test]
    fn replacement_slider_captures_press_move_release_without_duplicate_changes() {
        let ctx = context();
        let mut gallery = Gallery::default();
        frame(&ctx, &gallery, 0.0, vec![]);
        gallery.select(GalleryPage::Slider);
        let press = click(egui::pos2(300.0, 40.0));
        assert!(frame(&ctx, &gallery, 1.0, press[..2].to_vec()).1.is_empty());
        let moved = frame(
            &ctx,
            &gallery,
            1.016,
            vec![egui::Event::PointerMoved(egui::pos2(700.0, 40.0))],
        )
        .1;
        assert_eq!(
            moved,
            vec![surface_v1::Event::Change {
                field: "gallery_volume".into(),
                value: serde_json::json!(8.0),
                path: vec![0]
            }]
        );
        assert!(frame(&ctx, &gallery, 1.032, vec![]).1.is_empty());
        let outside = egui::pos2(900.0, 40.0);
        assert_eq!(
            frame(
                &ctx,
                &gallery,
                1.048,
                vec![egui::Event::PointerMoved(outside)]
            )
            .1,
            vec![surface_v1::Event::Change {
                field: "gallery_volume".into(),
                value: serde_json::json!(10.0),
                path: vec![0]
            }]
        );
        assert!(frame(
            &ctx,
            &gallery,
            1.064,
            vec![egui::Event::PointerButton {
                pos: outside,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default()
            }]
        )
        .1
        .is_empty());
        assert!(frame(&ctx, &gallery, 1.12, vec![]).1.is_empty());
        assert!(ctx.dragged_id().is_none());
        let focused = ctx.memory(|memory| memory.focused()).unwrap();
        assert_eq!(
            frame(
                &ctx,
                &gallery,
                1.2,
                vec![egui::Event::Key {
                    key: egui::Key::ArrowLeft,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default()
                }]
            )
            .1,
            vec![surface_v1::Event::Change {
                field: "gallery_volume".into(),
                value: serde_json::json!(8.0),
                path: vec![0]
            }]
        );
        assert!(frame(
            &ctx,
            &gallery,
            1.216,
            vec![egui::Event::Key {
                key: egui::Key::ArrowLeft,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Default::default()
            }]
        )
        .1
        .is_empty());
        assert_eq!(
            frame(
                &ctx,
                &gallery,
                1.3,
                vec![egui::Event::AccessKitActionRequest(
                    accesskit::ActionRequest {
                        action: accesskit::Action::Decrement,
                        target: accesskit::NodeId(focused.value()),
                        data: None
                    }
                )]
            )
            .1,
            vec![surface_v1::Event::Change {
                field: "gallery_volume".into(),
                value: serde_json::json!(6.0),
                path: vec![0]
            }]
        );
        assert!(frame(&ctx, &gallery, 1.316, vec![]).1.is_empty());
    }

    #[test]
    fn escape_with_release_cancels_replacement_slider_click() {
        for escape_first in [true, false] {
            let ctx = context();
            let mut gallery = Gallery::default();
            frame(&ctx, &gallery, 0.0, vec![]);
            gallery.select(GalleryPage::Slider);
            let pointer = click(egui::pos2(700.0, 40.0));
            assert!(frame(&ctx, &gallery, 1.0, pointer[..2].to_vec())
                .1
                .is_empty());
            let escape = egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            };
            let release = pointer[2].clone();
            let events = if escape_first {
                vec![escape, release]
            } else {
                vec![release, escape]
            };
            assert_eq!(
                frame(&ctx, &gallery, 1.016, events).1,
                vec![],
                "Escape and release in one frame must cancel the click"
            );
            assert!(ctx.dragged_id().is_none());
            assert!(frame(&ctx, &gallery, 1.032, vec![]).1.is_empty());
        }
    }

    #[test]
    fn replacement_slider_release_click_emits_once() {
        let ctx = context();
        let mut gallery = Gallery::default();
        frame(&ctx, &gallery, 0.0, vec![]);
        gallery.select(GalleryPage::Slider);
        let pointer = click(egui::pos2(700.0, 40.0));
        assert!(frame(&ctx, &gallery, 1.0, pointer[..2].to_vec())
            .1
            .is_empty());
        assert_eq!(
            frame(&ctx, &gallery, 1.016, pointer[2..].to_vec()).1,
            vec![surface_v1::Event::Change {
                field: "gallery_volume".into(),
                value: serde_json::json!(8.0),
                path: vec![0]
            }]
        );
        assert!(ctx.dragged_id().is_none());
        assert!(frame(&ctx, &gallery, 1.032, vec![]).1.is_empty());
    }

    #[test]
    fn escape_cancels_replacement_slider_capture_until_a_new_press() {
        let ctx = context();
        let mut gallery = Gallery::default();
        frame(&ctx, &gallery, 0.0, vec![]);
        gallery.select(GalleryPage::Slider);
        let press = click(egui::pos2(300.0, 40.0));
        assert!(frame(&ctx, &gallery, 1.0, press[..2].to_vec()).1.is_empty());
        assert_eq!(
            frame(
                &ctx,
                &gallery,
                1.016,
                vec![egui::Event::PointerMoved(egui::pos2(700.0, 40.0))]
            )
            .1,
            vec![surface_v1::Event::Change {
                field: "gallery_volume".into(),
                value: serde_json::json!(8.0),
                path: vec![0]
            }]
        );
        assert!(frame(
            &ctx,
            &gallery,
            1.032,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default()
            }]
        )
        .1
        .is_empty());
        assert!(
            ctx.dragged_id().is_none(),
            "Escape must leave no drag owner"
        );
        let end = egui::pos2(20.0, 40.0);
        assert!(
            frame(&ctx, &gallery, 1.048, vec![egui::Event::PointerMoved(end)])
                .1
                .is_empty()
        );
        assert!(
            ctx.dragged_id().is_none(),
            "held movement must not reacquire drag ownership"
        );
        gallery.select(GalleryPage::Layout);
        assert!(frame(&ctx, &gallery, 1.05, vec![]).1.is_empty());
        gallery.select(GalleryPage::Slider);
        assert!(
            frame(&ctx, &gallery, 1.056, vec![]).1.is_empty(),
            "replacement must not adopt a cancelled held gesture"
        );
        assert!(ctx.dragged_id().is_none());
        assert!(frame(
            &ctx,
            &gallery,
            1.064,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default()
            }]
        )
        .1
        .is_empty());
        assert!(frame(&ctx, &gallery, 1.12, vec![]).1.is_empty());
        assert_eq!(
            frame(&ctx, &gallery, 1.2, click(egui::pos2(700.0, 40.0))).1,
            vec![surface_v1::Event::Change {
                field: "gallery_volume".into(),
                value: serde_json::json!(8.0),
                path: vec![0]
            }]
        );
        assert!(frame(&ctx, &gallery, 1.216, vec![]).1.is_empty());
    }

    #[test]
    fn reinserted_control_accepts_a_click_at_its_previous_geometry() {
        let ctx = context();
        let mut gallery = Gallery::default();
        gallery.select(GalleryPage::Toggle);
        frame(&ctx, &gallery, 0.0, vec![]);
        gallery.select(GalleryPage::Layout);
        frame(&ctx, &gallery, 1.0, vec![]);
        gallery.select(GalleryPage::Toggle);
        assert_eq!(
            frame(&ctx, &gallery, 2.0, click(egui::pos2(50.0, 40.0)))
                .1
                .len(),
            1
        );
    }

    #[test]
    fn unchanged_production_border_survives_a_sibling_replacement() {
        let ctx = context();
        let render = |time: f64, changed: bool| {
            let surface = mote_ui::v1::validate(&serde_json::to_vec(&serde_json::json!({"version":1,"root":{"type":"screen","children":[
                {"type":"button","label":"Stable","event":"stable"},
                if changed { serde_json::json!({"type":"slider","label":"Other","field":"other","value":4,"min":0,"max":10}) }
                else { serde_json::json!({"type":"toggle","label":"Other","field":"other","value":false}) }
            ]}})).unwrap()).unwrap();
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(834.0, 1194.0))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| surface_v1::draw(ui, &surface, &mut Vec::new()));
                },
            )
        };
        let before = render(0.0, false);
        let first = render(1.0, true);
        let stable = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Rect(rect) if rect.rect.top() == 14.0 && rect.stroke.width == 1.0 => {
                        Some(rect.clone())
                    }
                    _ => None,
                })
                .expect("first control border remains painted")
        };
        assert_eq!(stable(&before), stable(&first));
        assert!(!particles(&first.shapes).is_empty());
        assert_eq!(stable(&before), stable(&render(1.06, true)));
        assert_eq!(stable(&before), stable(&render(1.12, true)));
    }

    #[test]
    fn live_surface_revision_materializes_rules_with_new_control_identity() {
        let ctx = context();
        let render = |time: f64, revision: u64, tall: bool| {
            let surface = mote_ui::v1::validate(&serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "root": {"type": "screen", "children": [
                    {"type": "header", "children": [{"type": "text", "value":
                        if tall { "First\nSecond\nThird" } else { "First" }}]},
                    {"type": "slider", "label": "Level", "field": "level",
                        "value": 4, "min": 0, "max": 10}
                ]}
            })).unwrap()).unwrap();
            ctx.run(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(834.0, 1194.0))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.push_id(revision, |ui| {
                            surface_v1::draw_with_transition_namespace(
                                ui,
                                &surface,
                                &mut Vec::new(),
                                egui::Id::new("live-surface-transition"),
                            );
                        });
                    });
                },
            )
        };
        render(0.0, 1, false);
        let changed = render(1.0, 2, true);
        assert!(!particles(&changed.shapes).is_empty());
        assert!(!particles(&render(1.06, 2, true).shapes).is_empty());
        assert!(particles(&render(1.12, 2, true).shapes).is_empty());
    }

    #[test]
    fn moved_button_uses_new_bounds_and_never_fires_at_old_bounds() {
        let surface = |tall: bool| {
            mote_ui::v1::validate(&serde_json::to_vec(&serde_json::json!({"version":1,"root":{"type":"screen","children":[
            {"type":"header","children":[{"type":"text","value":if tall { "one\ntwo\nthree\nfour\nfive\nsix" } else { "one" }}]},
            {"type":"button","label":"Run","event":"run"}
        ]}})).unwrap()).unwrap()
        };
        let render = |ctx: &egui::Context, tall, events| {
            let mut emitted = Vec::new();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(834.0, 1194.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| surface_v1::draw(ui, &surface(tall), &mut emitted));
                },
            );
            (output, emitted)
        };
        let border = |output: egui::FullOutput| {
            output
                .shapes
                .into_iter()
                .find_map(|shape| match shape.shape {
                    Shape::Rect(rect) if rect.rect.height() == 36.0 => Some(rect.rect),
                    _ => None,
                })
                .unwrap()
        };
        let old = border(render(&context(), false, vec![]).0);
        let new = border(render(&context(), true, vec![]).0);
        assert!(!old.intersects(new));
        for (pos, expected) in [
            (old.center(), vec![]),
            (
                new.center(),
                vec![surface_v1::Event::Action {
                    event: "run".into(),
                    path: vec![1],
                }],
            ),
        ] {
            let ctx = context();
            render(&ctx, false, vec![]);
            assert_eq!(render(&ctx, true, click(pos)).1, expected);
            assert!(render(&ctx, true, vec![]).1.is_empty());
        }
    }

    #[test]
    fn legacy_caret_text_and_progress_updates_do_not_materialize() {
        let ctx = context();
        let render = |value: f32, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(834.0, 1194.0))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        crate::surface::draw(
                            ui,
                            &mote_ui::Node::Stack {
                                children: vec![
                                    mote_ui::Node::Field {
                                        label: "Name".into(),
                                        name: "name".into(),
                                        value: "Original".into(),
                                    },
                                    mote_ui::Node::Progress { value },
                                ],
                            },
                            &mut Vec::new(),
                        )
                    });
                },
            )
        };
        render(0.2, vec![]);
        let focus = render(0.2, click(egui::pos2(60.0, 70.0)));
        assert!(particles(&focus.shapes).is_empty());
        let edit = render(0.8, vec![egui::Event::Text("Changed".into())]);
        assert!(particles(&edit.shapes).is_empty());
    }
}
