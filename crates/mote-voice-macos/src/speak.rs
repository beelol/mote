//! Deterministic, in-memory speech queue and playback buffer.

use crate::{SharedOutput, VoiceError, VoiceResult};
use std::collections::VecDeque;

pub const LOWERED_GAIN: f32 = 0.3;

pub trait SpeechOutput: Send {
    fn pause(&mut self) -> VoiceResult<()>;
    fn resume(&mut self) -> VoiceResult<()>;
    fn stop(&mut self) -> VoiceResult<()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpeakEvent {
    Started { line: u64 },
    Lowered { line: u64, phrase: usize },
    Restored { line: u64, phrase: usize },
    Stopped { line: u64, phrase: usize },
    PhraseFinished { line: u64, phrase: usize },
    Finished { line: u64 },
}

struct Line {
    id: u64,
    phrases: Vec<String>,
    audio: Vec<Option<Vec<f32>>>,
}

struct Playing {
    line: Line,
    phrase: usize,
    position: usize,
    started: bool,
}

/// Splits speech at clause boundaries so interruption can stop at a natural boundary.
pub fn phrases(text: &str) -> Vec<String> {
    let mut output: Vec<String> = Vec::new();
    let mut current = String::new();
    let characters: Vec<char> = text.chars().collect();
    for (index, character) in characters.iter().copied().enumerate() {
        current.push(character);
        let boundary = matches!(character, '.' | '?' | '!' | ';' | ':' | ',' | '—')
            && characters
                .get(index + 1)
                .is_none_or(|next| next.is_whitespace());
        if boundary {
            push_phrase(&mut output, current.trim());
            current.clear();
        }
    }
    push_phrase(&mut output, current.trim());
    output
}

fn push_phrase(output: &mut Vec<String>, phrase: &str) {
    if phrase.is_empty() {
        return;
    }
    if phrase.split_whitespace().count() <= 2
        && !output.is_empty()
        && !phrase.ends_with(['.', '?', '!'])
    {
        let previous = output.last_mut().expect("checked non-empty");
        previous.push(' ');
        previous.push_str(phrase);
    } else {
        output.push(phrase.to_string());
    }
}

/// Holds synthesized audio in memory and feeds an optional audio-unit output queue.
pub struct BufferedSpeechOutput {
    output: Option<SharedOutput>,
    queue: VecDeque<Line>,
    playing: Option<Playing>,
    paused: bool,
    lowered: bool,
    stop_after_phrase: bool,
    recent: Vec<String>,
    samples_since_done: usize,
    peak_db: f32,
}

impl Default for BufferedSpeechOutput {
    fn default() -> Self {
        Self::new(None)
    }
}

impl BufferedSpeechOutput {
    pub fn new(output: Option<SharedOutput>) -> Self {
        Self {
            output,
            queue: VecDeque::new(),
            playing: None,
            paused: false,
            lowered: false,
            stop_after_phrase: false,
            recent: Vec::new(),
            samples_since_done: usize::MAX / 2,
            peak_db: -100.0,
        }
    }

    /// Queues a line and returns one synthesis request per phrase.
    pub fn queue(&mut self, id: u64, text: &str) -> Vec<(u64, usize, String)> {
        let phrases = phrases(text);
        let jobs = phrases
            .iter()
            .enumerate()
            .map(|(index, phrase)| (id, index, phrase.clone()))
            .collect();
        self.queue.push_back(Line {
            id,
            audio: vec![None; phrases.len()],
            phrases,
        });
        jobs
    }

    pub fn synthesized(&mut self, line: u64, phrase: usize, audio: Vec<f32>) {
        let line = self
            .playing
            .as_mut()
            .filter(|playing| playing.line.id == line)
            .map(|playing| &mut playing.line)
            .or_else(|| self.queue.iter_mut().find(|queued| queued.id == line));
        if let Some(slot) = line.and_then(|line| line.audio.get_mut(phrase)) {
            *slot = Some(audio);
        }
    }

    pub fn is_speaking(&self) -> bool {
        !self.paused && self.playing.as_ref().is_some_and(|playing| playing.started)
    }

    pub fn current(&self) -> Option<(u64, usize)> {
        self.playing
            .as_ref()
            .filter(|playing| playing.started)
            .map(|playing| (playing.line.id, playing.phrase))
    }

    pub fn recent_words(&self) -> Vec<String> {
        let mut words = if self.samples_since_done < 24_000 {
            self.recent.clone()
        } else {
            Vec::new()
        };
        if let Some(playing) = &self.playing {
            for phrase in &playing.line.phrases {
                words.extend(mote_voice_core::normalize(phrase));
            }
        }
        words
    }

    pub fn lower(&mut self) -> Option<SpeakEvent> {
        let (line, phrase) = self.current()?;
        if self.lowered {
            return None;
        }
        self.lowered = true;
        Some(SpeakEvent::Lowered { line, phrase })
    }

    pub fn restore(&mut self) -> Option<SpeakEvent> {
        if !self.lowered {
            return None;
        }
        self.lowered = false;
        self.stop_after_phrase = false;
        self.current()
            .map(|(line, phrase)| SpeakEvent::Restored { line, phrase })
    }

    pub fn stop_at_phrase(&mut self) {
        if self.playing.is_some() {
            self.stop_after_phrase = true;
        }
    }

    pub fn stop_now(&mut self, line: Option<u64>) -> Vec<SpeakEvent> {
        let mut events = Vec::new();
        if let Some(playing) = self.playing.take() {
            if line.is_none_or(|id| id == playing.line.id) {
                if playing.started {
                    self.remember(&playing.line);
                    events.push(SpeakEvent::Stopped {
                        line: playing.line.id,
                        phrase: playing.phrase,
                    });
                }
                self.queue.clear();
            } else {
                self.playing = Some(playing);
                self.queue.retain(|queued| Some(queued.id) != line);
            }
        } else {
            self.queue
                .retain(|queued| line.is_some_and(|id| id != queued.id));
        }
        self.lowered = false;
        self.stop_after_phrase = false;
        self.clear_device_output();
        events
    }

    /// Produces exactly `sample_count` samples. Nothing advances while paused.
    pub fn next(&mut self, sample_count: usize) -> (Vec<f32>, Vec<SpeakEvent>, Option<f32>) {
        if self.paused {
            return (vec![0.0; sample_count], Vec::new(), None);
        }
        let mut output = vec![0.0_f32; sample_count];
        let mut events = Vec::new();
        let mut filled = 0;
        self.samples_since_done = self.samples_since_done.saturating_add(sample_count);
        while filled < sample_count {
            if self.playing.is_none() {
                let Some(line) = self.queue.pop_front() else {
                    break;
                };
                self.playing = Some(Playing {
                    line,
                    phrase: 0,
                    position: 0,
                    started: false,
                });
            }
            let playing = self.playing.as_mut().expect("playing line exists");
            let Some(audio) = playing
                .line
                .audio
                .get(playing.phrase)
                .and_then(Option::as_ref)
            else {
                break;
            };
            if !playing.started {
                playing.started = true;
                events.push(SpeakEvent::Started {
                    line: playing.line.id,
                });
            }
            let count = (audio.len() - playing.position).min(sample_count - filled);
            let gain = if self.lowered { LOWERED_GAIN } else { 1.0 };
            for index in 0..count {
                output[filled + index] = audio[playing.position + index] * gain;
            }
            filled += count;
            playing.position += count;
            if playing.position >= audio.len() {
                let last = playing.phrase + 1 >= playing.line.phrases.len();
                if last || self.stop_after_phrase {
                    let playing = self.playing.take().expect("playing line exists");
                    self.remember(&playing.line);
                    if last {
                        events.push(SpeakEvent::Finished {
                            line: playing.line.id,
                        });
                    } else {
                        events.push(SpeakEvent::Stopped {
                            line: playing.line.id,
                            phrase: playing.phrase,
                        });
                        self.queue.clear();
                    }
                    self.lowered = false;
                    self.stop_after_phrase = false;
                } else {
                    events.push(SpeakEvent::PhraseFinished {
                        line: playing.line.id,
                        phrase: playing.phrase,
                    });
                    playing.phrase += 1;
                    playing.position = 0;
                }
            }
        }
        let level = if filled == 0 {
            None
        } else {
            let rms = (output.iter().map(|sample| sample * sample).sum::<f32>()
                / sample_count.max(1) as f32)
                .sqrt();
            let db = 20.0 * (rms + 1e-9).log10();
            self.peak_db = self.peak_db.max(db)
                - 6.0 / 50.0 * (sample_count as f32 / mote_voice_core::FRAME_SAMPLES as f32);
            self.peak_db = self.peak_db.max(db);
            Some(
                ((db - (self.peak_db - 30.0)) / 30.0)
                    .clamp(0.0, 1.0)
                    .powf(2.2),
            )
        };
        if let Some(device) = &self.output {
            if let Ok(mut queue) = device.lock() {
                queue.extend(output.iter().copied());
            }
        }
        (output, events, level)
    }

    fn remember(&mut self, line: &Line) {
        self.recent = line
            .phrases
            .iter()
            .flat_map(|phrase| mote_voice_core::normalize(phrase))
            .collect();
        self.samples_since_done = 0;
    }

    fn clear_device_output(&self) {
        if let Some(output) = &self.output {
            if let Ok(mut output) = output.lock() {
                output.clear();
            }
        }
    }
}

impl SpeechOutput for BufferedSpeechOutput {
    fn pause(&mut self) -> VoiceResult<()> {
        self.paused = true;
        self.clear_device_output();
        Ok(())
    }

    fn resume(&mut self) -> VoiceResult<()> {
        self.paused = false;
        Ok(())
    }

    fn stop(&mut self) -> VoiceResult<()> {
        self.stop_now(None);
        self.clear_device_output();
        Ok(())
    }
}

pub fn enqueue_device_samples(output: &SharedOutput, samples: &[f32]) -> VoiceResult<()> {
    output
        .lock()
        .map_err(|_| VoiceError::Runtime("speaker output lock was poisoned".to_string()))?
        .extend(samples.iter().copied());
    Ok(())
}
