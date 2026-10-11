#![cfg(feature = "test-observation")]
#![allow(dead_code)]

mod app;
mod accessibility;
mod contract;
mod cursor;
mod display_latency;
mod gallery;
mod matrix_submission;
mod pipeline;
mod state;
mod stream;
mod surface;
mod surface_v1;
mod theme;

pub use app::observation::*;
#[cfg(test)]
use matrix_submission::submit_matrix;
pub use egui::Shape as PaintedShape;
pub use egui::Key as InputKey;
