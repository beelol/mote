#![cfg(all(feature = "test-fixture", not(test)))]

// Compile the same dispatcher without exposing its internals or adding a production test mode.
#[path = "main.rs"]
#[allow(dead_code)]
mod daemon;

pub use daemon::test_fixture;
use daemon::{
    config, host_snapshot, inferred_ui, lanes, matrix, model_call, now_ms, pipeline, Daemon,
};
