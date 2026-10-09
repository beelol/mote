//! The deterministic test renderer: lays a surface tree out as colored blocks and encodes a PNG.
//! It pairs each fixture with a golden image (V0-42). The on-screen egui renderer in mote-client
//! is separate; this renderer exists so golden images are reproducible without a GPU or display.

use crate::Node;

const CANVAS_W: usize = 200;
const ROW_H: usize = 20;
const BAR_H: usize = 16;

/// A simple RGBA image.
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    color: [u8; 3],
}

fn color_of(node: &Node) -> [u8; 3] {
    match node {
        Node::Text { .. } => [120, 120, 120],
        Node::Action { .. } => [60, 120, 220],
        Node::Toggle { .. } => [80, 180, 80],
        Node::Field { .. } => [200, 180, 80],
        Node::Select { .. } => [180, 120, 200],
        Node::Progress { .. } => [220, 140, 60],
        Node::Table { .. } => [150, 150, 150],
        Node::Stack { .. } | Node::List { .. } | Node::Row { .. } | Node::Chat { .. } => [40, 40, 40],
    }
}

fn label_len(node: &Node) -> usize {
    match node {
        Node::Text { value } => value.chars().count(),
        Node::Action { label, .. }
        | Node::Field { label, .. }
        | Node::Toggle { label, .. }
        | Node::Select { label, .. } => label.chars().count(),
        Node::Progress { value } => ((value.clamp(0.0, 1.0) * 20.0) as usize).max(1),
        _ => 10,
    }
}

/// Lay out a node, pushing rectangles, returning the height consumed.
fn layout(node: &Node, x: usize, y: usize, width: usize, rects: &mut Vec<Rect>) -> usize {
    match node {
        Node::Stack { children } | Node::List { children } | Node::Chat { children } => {
            let mut cy = y;
            for child in children {
                cy += layout(child, x, cy, width, rects);
            }
            (cy - y).max(ROW_H)
        }
        Node::Row { children } => {
            if children.is_empty() {
                return ROW_H;
            }
            let cw = width / children.len();
            let mut max_h = ROW_H;
            for (i, child) in children.iter().enumerate() {
                let h = layout(child, x + i * cw, y, cw, rects);
                max_h = max_h.max(h);
            }
            max_h
        }
        Node::Table { columns, rows } => {
            let n = rows.len() + 1; // header plus rows
            let _ = columns;
            rects.push(Rect { x, y, w: width.min(CANVAS_W), h: n * ROW_H - 4, color: color_of(node) });
            n * ROW_H
        }
        leaf => {
            let bar = (label_len(leaf) * 6).clamp(6, width);
            rects.push(Rect { x, y: y + 2, w: bar, h: BAR_H, color: color_of(leaf) });
            ROW_H
        }
    }
}

/// Render a surface tree to an RGBA image.
pub fn render(node: &Node) -> Image {
    let mut rects = Vec::new();
    let h = layout(node, 0, 0, CANVAS_W, &mut rects).max(ROW_H);
    let w = CANVAS_W;
    let mut rgba = vec![255u8; w * h * 4]; // white
    for r in &rects {
        for yy in r.y..(r.y + r.h).min(h) {
            for xx in r.x..(r.x + r.w).min(w) {
                let idx = (yy * w + xx) * 4;
                rgba[idx] = r.color[0];
                rgba[idx + 1] = r.color[1];
                rgba[idx + 2] = r.color[2];
                rgba[idx + 3] = 255;
            }
        }
    }
    Image { w, h, rgba }
}

/// Encode an image as PNG bytes.
pub fn encode_png(img: &Image) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, img.w as u32, img.h as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(&img.rgba).expect("png data");
    }
    out
}

/// Render a tree straight to PNG bytes.
pub fn render_png(node: &Node) -> Vec<u8> {
    encode_png(&render(node))
}
