#![cfg(feature = "test-observation")]
#![allow(dead_code)]

mod app;
mod accessibility;
mod contract;
mod cursor;
mod display_latency;
mod pipeline;
mod state;
mod stream;
mod surface;
mod theme;

pub use app::observation::*;
pub use egui::Shape as PaintedShape;
pub use egui::Key as InputKey;
