use mote_ui::Node;
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
    LAYOUT.with(|layout| {
        layout.borrow_mut().controls.push(Control {
            kind,
            path: path.to_vec(),
            id: response.id,
            layer: response.layer_id,
            visual: response.rect,
            hit: response.interact_rect,
            clip: ui.clip_rect(),
        })
    });
}

pub fn scroll<R>(
    name: &'static str,
    clip: egui::Rect,
    output: &egui::scroll_area::ScrollAreaOutput<R>,
) {
    LAYOUT.with(|layout| {
        layout.borrow_mut().scrolls.push(Scroll {
            name,
            viewport: output.inner_rect,
            clip,
            content: output.content_size,
            offset: output.state.offset,
        })
    });
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
            LAYOUT.with(|layout| {
                layout.borrow_mut().popup_paints.push((
                    shape.shape.visual_bounding_rect(),
                    shape.clip_rect,
                    ui.clip_rect(),
                ))
            });
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
            LAYOUT.with(|layout| {
                layout
                    .borrow_mut()
                    .paints
                    .push((shape.shape.visual_bounding_rect(), shape.clip_rect))
            });
        }
        index += 1;
    });
    DISPLAYED.with(|nodes| {
        nodes.borrow_mut().push(DisplayedNode {
            namespace: ui.id(),
            path: path.to_vec(),
            node: node.clone(),
            rect,
        })
    });
}

pub(crate) fn observe(
    ui: &egui::Ui,
    kind: &'static str,
    path: &[usize],
    rect: egui::Rect,
    id: egui::Id,
) {
    OBSERVATIONS.with(|obs| {
        obs.borrow_mut().push(MatrixObservation {
            kind,
            path: path.to_vec(),
            rect,
            clip: ui.clip_rect(),
            id,
        })
    });
}

#[derive(Clone, Debug)]
pub struct MatrixObservation {
    pub kind: &'static str,
    pub path: Vec<usize>,
    pub rect: egui::Rect,
    pub clip: egui::Rect,
    pub id: egui::Id,
}

thread_local! {
    pub(crate) static OBSERVATIONS: std::cell::RefCell<Vec<MatrixObservation>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub struct MatrixHarness {
    pub ctx: egui::Context,
    pub size: egui::Vec2,
    pub surface: mote_ui::v1::Surface,
}
impl MatrixHarness {
    pub fn new(size: egui::Vec2, surface: mote_ui::v1::Surface) -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        ctx.set_style(crate::theme::style(true));
        Self { ctx, size, surface }
    }
    pub fn frame(
        &self,
        events: Vec<egui::Event>,
    ) -> (
        Vec<MatrixObservation>,
        Vec<crate::surface_v1::Event>,
        egui::FullOutput,
    ) {
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
                    .show(ctx, |ui| {
                        crate::surface_v1::draw(ui, &self.surface, &mut emitted)
                    });
            },
        );
        (
            OBSERVATIONS.with(|obs| obs.borrow().clone()),
            emitted,
            output,
        )
    }
    pub fn click(&self, pos: egui::Pos2) -> Vec<crate::surface_v1::Event> {
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
    pub fn key(&self, key: egui::Key) -> Vec<crate::surface_v1::Event> {
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
