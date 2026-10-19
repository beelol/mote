//! Portable utterance endpoint decisions.

use crate::gate::GateEvent;
use crate::pcm::{PcmFrame, SAMPLE_RATE};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct EndpointConfig {
    pub complete_ms: u64,
    pub unfinished_ms: u64,
    pub longest_ms: u64,
    pub check_ms: u64,
    pub preroll_ms: u64,
    pub open_endings: Vec<String>,
}

impl Default for EndpointConfig {
    fn default() -> Self {
        Self {
            complete_ms: 700,
            unfinished_ms: 2_500,
            longest_ms: 90_000,
            check_ms: 450,
            preroll_ms: 300,
            open_endings: [
                "and", "or", "but", "so", "because", "the", "a", "an", "to", "of", "for", "with",
                "that", "if", "then", "um", "uh", "er", "like", "also", "when", "which", "who",
                "is", "are", "my", "your", "their", "our",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

impl EndpointConfig {
    pub fn reads_unfinished(&self, text: &str) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        if text.ends_with(',')
            || text.ends_with("...")
            || text.ends_with('…')
            || text.ends_with('-')
            || text.ends_with('—')
        {
            return true;
        }
        let last = text
            .split_whitespace()
            .last()
            .unwrap_or("")
            .trim_matches(|character: char| !character.is_alphanumeric())
            .to_lowercase();
        self.open_endings.contains(&last)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum EndpointDecision {
    Started {
        id: u64,
        start_ms: u64,
    },
    NeedWords {
        id: u64,
    },
    Ended {
        id: u64,
        start_ms: u64,
        end_ms: u64,
        complete: bool,
        audio: Vec<f32>,
        audio_start_ms: u64,
    },
}

struct Utterance {
    id: u64,
    start_sample: u64,
    audio: Vec<f32>,
    silence_samples: u64,
    asked_for_words: bool,
    words: Option<String>,
    taken_samples: u64,
}

pub struct Endpoint {
    config: EndpointConfig,
    preroll: VecDeque<f32>,
    current: Option<Utterance>,
    next_id: u64,
}

impl Endpoint {
    pub fn new(config: EndpointConfig) -> Self {
        Self {
            config,
            preroll: VecDeque::new(),
            current: None,
            next_id: 1,
        }
    }

    pub fn current(&self) -> Option<u64> {
        self.current.as_ref().map(|utterance| utterance.id)
    }

    pub fn audio(&self) -> &[f32] {
        self.current
            .as_ref()
            .map(|utterance| utterance.audio.as_slice())
            .unwrap_or(&[])
    }

    pub fn take_front(&mut self, count: usize) -> Option<(u64, u64, Vec<f32>)> {
        let utterance = self.current.as_mut()?;
        let count = count.min(utterance.audio.len());
        let start_ms = samples_to_ms(utterance.start_sample + utterance.taken_samples);
        let audio = utterance.audio.drain(..count).collect();
        utterance.taken_samples += count as u64;
        Some((utterance.id, start_ms, audio))
    }

    pub fn set_words(&mut self, id: u64, text: &str) {
        if let Some(utterance) = self.current.as_mut().filter(|item| item.id == id) {
            utterance.words = Some(text.to_owned());
        }
    }

    pub fn push(
        &mut self,
        frame: &PcmFrame,
        gate: Option<GateEvent>,
        open: bool,
        now_sample: u64,
    ) -> Vec<EndpointDecision> {
        let mut decisions = Vec::new();
        match (&mut self.current, gate) {
            (None, Some(GateEvent::Open { lead_samples })) => {
                let keep = (lead_samples
                    + self.config.preroll_ms as usize * SAMPLE_RATE as usize / 1000)
                    .min(self.preroll.len());
                let mut audio: Vec<f32> = self
                    .preroll
                    .iter()
                    .skip(self.preroll.len() - keep)
                    .copied()
                    .collect();
                audio.extend_from_slice(frame.samples());
                let start_sample = now_sample.saturating_sub(audio.len() as u64);
                let id = self.next_id;
                self.next_id += 1;
                self.current = Some(Utterance {
                    id,
                    start_sample,
                    audio,
                    silence_samples: 0,
                    asked_for_words: false,
                    words: None,
                    taken_samples: 0,
                });
                decisions.push(EndpointDecision::Started {
                    id,
                    start_ms: samples_to_ms(start_sample),
                });
            }
            (Some(utterance), _) => {
                utterance.audio.extend_from_slice(frame.samples());
                if open {
                    utterance.silence_samples = 0;
                    utterance.asked_for_words = false;
                    utterance.words = None;
                } else {
                    utterance.silence_samples += frame.samples().len() as u64;
                }
                let silence_ms = samples_to_ms(utterance.silence_samples);
                let length_ms =
                    samples_to_ms(utterance.audio.len() as u64 + utterance.taken_samples);
                if silence_ms >= self.config.check_ms && !utterance.asked_for_words {
                    utterance.asked_for_words = true;
                    decisions.push(EndpointDecision::NeedWords { id: utterance.id });
                }
                let complete = utterance
                    .words
                    .as_deref()
                    .map(|words| !self.config.reads_unfinished(words));
                let ended = (utterance.silence_samples > 0
                    && silence_ms >= self.config.complete_ms
                    && complete == Some(true))
                    || (utterance.silence_samples > 0 && silence_ms >= self.config.unfinished_ms)
                    || length_ms >= self.config.longest_ms;
                if ended {
                    let utterance = self.current.take().expect("current utterance exists");
                    let extra = utterance
                        .silence_samples
                        .saturating_sub(SAMPLE_RATE as u64 * 3 / 10);
                    let mut audio = utterance.audio;
                    audio.truncate(audio.len().saturating_sub(extra as usize));
                    decisions.push(EndpointDecision::Ended {
                        id: utterance.id,
                        start_ms: samples_to_ms(utterance.start_sample),
                        end_ms: samples_to_ms(now_sample - extra),
                        complete: complete.unwrap_or(true),
                        audio,
                        audio_start_ms: samples_to_ms(
                            utterance.start_sample + utterance.taken_samples,
                        ),
                    });
                    if open && length_ms >= self.config.longest_ms {
                        let id = self.next_id;
                        self.next_id += 1;
                        self.current = Some(Utterance {
                            id,
                            start_sample: now_sample,
                            audio: Vec::new(),
                            silence_samples: 0,
                            asked_for_words: false,
                            words: None,
                            taken_samples: 0,
                        });
                        decisions.push(EndpointDecision::Started {
                            id,
                            start_ms: samples_to_ms(now_sample),
                        });
                    }
                }
            }
            _ => {}
        }
        let keep = self.config.preroll_ms as usize * SAMPLE_RATE as usize / 1000
            + SAMPLE_RATE as usize / 2;
        self.preroll.extend(frame.samples());
        while self.preroll.len() > keep {
            self.preroll.pop_front();
        }
        decisions
    }
}

fn samples_to_ms(samples: u64) -> u64 {
    samples * 1000 / SAMPLE_RATE as u64
}
