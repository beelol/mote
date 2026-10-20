//! In-memory system speech synthesis and deterministic test synthesis.

use crate::{memspeech, VoiceResult};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpeechRequest<'a> {
    pub text: &'a str,
    pub voice: Option<&'a str>,
    pub words_per_minute: Option<u32>,
}

pub trait Synthesizer: Send {
    fn synthesize(&mut self, request: SpeechRequest<'_>) -> VoiceResult<Vec<f32>>;
    fn name(&self) -> &str;
}

#[derive(Default)]
pub struct MacSynthesizer;

impl Synthesizer for MacSynthesizer {
    fn synthesize(&mut self, request: SpeechRequest<'_>) -> VoiceResult<Vec<f32>> {
        memspeech::speak(request.text, request.voice, request.words_per_minute)
    }

    fn name(&self) -> &str {
        "macos-system-speech"
    }
}

/// Deterministic speech-shaped samples for orchestration tests. This never opens a device.
#[derive(Default)]
pub struct FixtureSynthesizer;

impl Synthesizer for FixtureSynthesizer {
    fn synthesize(&mut self, request: SpeechRequest<'_>) -> VoiceResult<Vec<f32>> {
        let samples = (request.text.split_whitespace().count().max(1)
            * mote_voice_core::SAMPLE_RATE as usize)
            / 4;
        Ok((0..samples)
            .map(|index| (index as f32 * 0.075).sin() * 0.2)
            .collect())
    }

    fn name(&self) -> &str {
        "fixture"
    }
}
