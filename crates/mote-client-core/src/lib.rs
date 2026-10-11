//! Platform-neutral Matrix rendering and chat state shared by native host adapters.

pub mod accessibility;
pub mod contract;
pub mod gallery;
#[cfg(any(test, feature = "test-observation"))]
pub mod observation;
pub mod state;
pub mod surface;
pub mod surface_v1;
pub mod theme;
pub mod transition;

pub use contract::Surface;
pub use state::ChatState;
pub use surface::SurfaceEvent;
pub use theme::install as install_theme;
