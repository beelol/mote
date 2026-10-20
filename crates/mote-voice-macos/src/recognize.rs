//! Local speech recognition and a deterministic scripted recognizer.

use crate::{VoiceError, VoiceResult};

const MAX_HINT_CHARS: usize = 600;

pub trait Recognizer: Send {
    fn recognize(
        &mut self,
        samples: &[f32],
        start_ms: u64,
        vocabulary_hint: &str,
    ) -> VoiceResult<String>;

    fn name(&self) -> &str;
}

pub fn bounded_hint(vocabulary_hint: &str) -> String {
    vocabulary_hint
        .trim()
        .chars()
        .take(MAX_HINT_CHARS)
        .collect()
}

/// Removes Whisper's non-word annotations such as `[BLANK_AUDIO]` and `(typing)`.
pub fn clean(text: &str) -> String {
    let mut output = String::new();
    let mut depth = 0_u32;
    for character in text.chars() {
        match character {
            '[' | '(' => depth = depth.saturating_add(1),
            ']' | ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => output.push(character),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptLine {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Default)]
pub struct ScriptedRecognizer {
    lines: Vec<ScriptLine>,
}

impl ScriptedRecognizer {
    pub fn new(lines: Vec<ScriptLine>) -> Self {
        Self { lines }
    }
}

impl Recognizer for ScriptedRecognizer {
    fn recognize(
        &mut self,
        samples: &[f32],
        start_ms: u64,
        _vocabulary_hint: &str,
    ) -> VoiceResult<String> {
        let end_ms = start_ms
            .saturating_add(samples.len() as u64 * 1_000 / mote_voice_core::SAMPLE_RATE as u64);
        let mut output = Vec::new();
        for line in &self.lines {
            if line.end_ms <= start_ms || line.start_ms >= end_ms {
                continue;
            }
            let words: Vec<&str> = line.text.split_whitespace().collect();
            let spoken = if end_ms >= line.end_ms {
                words.len()
            } else {
                let covered = end_ms.saturating_sub(line.start_ms);
                (covered as f64 / line.end_ms.saturating_sub(line.start_ms).max(1) as f64
                    * words.len() as f64) as usize
            };
            output.extend(words.into_iter().take(spoken));
        }
        if output.contains(&"<recognizer-fails>") {
            return Err(VoiceError::Runtime(
                "the scripted recognizer failed".to_string(),
            ));
        }
        Ok(output.join(" "))
    }

    fn name(&self) -> &str {
        "scripted"
    }
}

#[cfg(target_os = "macos")]
pub struct LocalWhisper {
    state: whisper_rs::WhisperState,
    name: String,
    threads: i32,
}

#[cfg(not(target_os = "macos"))]
pub struct LocalWhisper;

#[cfg(target_os = "macos")]
impl LocalWhisper {
    pub fn load(path: &std::path::Path) -> VoiceResult<Self> {
        whisper_rs::install_logging_hooks();
        let context = whisper_rs::WhisperContext::new_with_params(
            path,
            whisper_rs::WhisperContextParameters::default(),
        )
        .map_err(|error| VoiceError::Runtime(format!("could not load local Whisper: {error}")))?;
        let state = context.create_state().map_err(|error| {
            VoiceError::Runtime(format!("could not create Whisper state: {error}"))
        })?;
        let name = path
            .file_name()
            .map(|name| {
                name.to_string_lossy()
                    .trim_start_matches("ggml-")
                    .trim_end_matches(".bin")
                    .to_string()
            })
            .unwrap_or_default();
        let threads = std::thread::available_parallelism()
            .map(|threads| threads.get().min(4) as i32)
            .unwrap_or(4);
        Ok(Self {
            state,
            name,
            threads,
        })
    }
}

#[cfg(not(target_os = "macos"))]
impl LocalWhisper {
    pub fn load(_path: &std::path::Path) -> VoiceResult<Self> {
        Err(VoiceError::Unavailable(
            "local Whisper recognition is available on macOS only",
        ))
    }
}

#[cfg(target_os = "macos")]
impl Recognizer for LocalWhisper {
    fn recognize(
        &mut self,
        samples: &[f32],
        _start_ms: u64,
        vocabulary_hint: &str,
    ) -> VoiceResult<String> {
        let mut padded = Vec::new();
        let samples = if samples.len() < mote_voice_core::SAMPLE_RATE as usize {
            padded.extend_from_slice(samples);
            padded.resize(mote_voice_core::SAMPLE_RATE as usize, 0.0);
            padded.as_slice()
        } else {
            samples
        };
        let mut parameters =
            whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        parameters.set_language(Some("en"));
        parameters.set_n_threads(self.threads);
        parameters.set_no_context(true);
        parameters.set_single_segment(true);
        parameters.set_suppress_blank(true);
        parameters.set_print_progress(false);
        parameters.set_print_realtime(false);
        parameters.set_print_timestamps(false);
        parameters.set_print_special(false);
        let hint = bounded_hint(vocabulary_hint);
        if !hint.is_empty() {
            parameters.set_initial_prompt(&hint);
        }
        self.state
            .full(parameters, samples)
            .map_err(|error| VoiceError::Runtime(format!("local Whisper failed: {error}")))?;
        let mut text = String::new();
        for segment in self.state.as_iter() {
            if segment.no_speech_probability() <= 0.6 {
                text.push_str(
                    &segment
                        .to_str_lossy()
                        .map(|value| value.into_owned())
                        .unwrap_or_default(),
                );
            }
        }
        Ok(clean(&text))
    }

    fn name(&self) -> &str {
        &self.name
    }
}

#[cfg(not(target_os = "macos"))]
impl Recognizer for LocalWhisper {
    fn recognize(
        &mut self,
        _samples: &[f32],
        _start_ms: u64,
        _vocabulary_hint: &str,
    ) -> VoiceResult<String> {
        Err(VoiceError::Unavailable(
            "local Whisper recognition is available on macOS only",
        ))
    }

    fn name(&self) -> &str {
        "unavailable"
    }
}
