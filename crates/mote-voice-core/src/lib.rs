//! Portable, deterministic voice decisions.

pub mod endpoint;
pub mod gate;
pub mod pcm;
pub mod words;

pub use endpoint::{Endpoint, EndpointConfig, EndpointDecision};
pub use gate::{GateConfig, GateEvent, GateFrame, SpeechGate};
pub use pcm::{PcmError, PcmFrame, FRAME_SAMPLES, SAMPLE_RATE};
pub use words::{classify_barge, normalize, Vocabulary, WordDecision};

/// Keeps the protocol dependency intentional and visible without adding runtime behavior.
pub const PROTOCOL_MAX_LINE_BYTES: usize = mote_voice_protocol::MAX_LINE_BYTES;
