//! Optional, product-neutral macOS voice I/O.
//!
//! The deterministic interfaces and fakes compile on every target. Real capture, local Whisper
//! recognition, call detection and system speech report [`VoiceError::Unavailable`] away from
//! macOS.

pub mod calls;
pub mod memspeech;
pub mod mic;
pub mod recognize;
pub mod speak;
pub mod synth;

use std::fmt;

pub use calls::{CallMonitor, MacCallMonitor};
pub use mic::{AudioCallback, MacMicrophone, Microphone, SharedOutput};
pub use recognize::{LocalWhisper, Recognizer, ScriptLine, ScriptedRecognizer};
pub use speak::{BufferedSpeechOutput, SpeakEvent, SpeechOutput, LOWERED_GAIN};
pub use synth::{MacSynthesizer, SpeechRequest, Synthesizer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceError {
    Unavailable(&'static str),
    InvalidInput(String),
    Runtime(String),
}

impl fmt::Display for VoiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) => formatter.write_str(message),
            Self::InvalidInput(message) | Self::Runtime(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for VoiceError {}

pub type VoiceResult<T> = Result<T, VoiceError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeEvent {
    PausedForCall { applications: Vec<String> },
    ResumedAfterCall,
}

/// Coordinates the target-specific pieces without knowing a daemon, task, UI or product.
pub struct MacVoiceRuntime<M, C, R, S> {
    microphone: M,
    calls: C,
    recognizer: R,
    speech: S,
    started: bool,
    paused_for_call: bool,
}

impl<M, C, R, S> MacVoiceRuntime<M, C, R, S>
where
    M: Microphone,
    C: CallMonitor,
    R: Recognizer,
    S: SpeechOutput,
{
    pub fn new(microphone: M, calls: C, recognizer: R, speech: S) -> Self {
        Self {
            microphone,
            calls,
            recognizer,
            speech,
            started: false,
            paused_for_call: false,
        }
    }

    pub fn start(&mut self, on_audio: AudioCallback) -> VoiceResult<()> {
        if self.started {
            return Ok(());
        }
        self.microphone.start(on_audio)?;
        self.started = true;
        Ok(())
    }

    pub fn stop(&mut self) -> VoiceResult<()> {
        if !self.started {
            return Ok(());
        }
        let speech_result = self.speech.stop();
        let microphone_result = self.microphone.stop();
        self.started = false;
        self.paused_for_call = false;
        speech_result.and(microphone_result)
    }

    /// Polls for another application using both microphone and playback. Both capture and speech
    /// are paused before this returns a pause event.
    pub fn poll_calls(&mut self) -> VoiceResult<Option<RuntimeEvent>> {
        if !self.started {
            return Ok(None);
        }
        let applications = self.calls.active_calls()?;
        if !applications.is_empty() && !self.paused_for_call {
            self.speech.pause()?;
            if let Err(error) = self.microphone.pause() {
                let _ = self.speech.resume();
                return Err(error);
            }
            self.paused_for_call = true;
            return Ok(Some(RuntimeEvent::PausedForCall { applications }));
        }
        if applications.is_empty() && self.paused_for_call {
            self.speech.resume()?;
            if let Err(error) = self.microphone.resume() {
                let _ = self.speech.pause();
                return Err(error);
            }
            self.paused_for_call = false;
            return Ok(Some(RuntimeEvent::ResumedAfterCall));
        }
        Ok(None)
    }

    pub fn recognize(
        &mut self,
        samples: &[f32],
        start_ms: u64,
        vocabulary_hint: &str,
    ) -> VoiceResult<String> {
        self.recognizer
            .recognize(samples, start_ms, vocabulary_hint)
    }

    pub fn speech_mut(&mut self) -> &mut S {
        &mut self.speech
    }

    pub fn is_paused_for_call(&self) -> bool {
        self.paused_for_call
    }

    pub fn into_parts(self) -> (M, C, R, S) {
        (self.microphone, self.calls, self.recognizer, self.speech)
    }
}
