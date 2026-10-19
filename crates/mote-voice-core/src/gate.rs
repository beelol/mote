//! Sound-only speech gate derived from the pinned listener implementation.

use crate::pcm::{PcmFrame, FRAME_SAMPLES, SAMPLE_RATE};
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct GateConfig {
    pub above_floor_db: f32,
    pub min_level_db: f32,
    pub voicing: f32,
    pub band_share: f32,
    pub window: usize,
    pub voiced_needed: usize,
    pub same_shape: f32,
    pub steady_share: f32,
    pub fast_needed: usize,
    pub fast_window: usize,
    pub fast_above_db: f32,
    pub hangover: usize,
    pub floor_rise_db_per_s: f32,
}

impl Default for GateConfig {
    fn default() -> Self {
        Self {
            above_floor_db: 12.0,
            min_level_db: -60.0,
            voicing: 0.45,
            band_share: 0.3,
            window: 16,
            voiced_needed: 9,
            same_shape: 0.97,
            steady_share: 0.6,
            fast_needed: 7,
            fast_window: 8,
            fast_above_db: 18.0,
            hangover: 15,
            floor_rise_db_per_s: 1.5,
        }
    }
}

#[derive(Clone, Debug)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    fn new(high_pass: bool, hz: f32) -> Self {
        let angular = 2.0 * std::f32::consts::PI * hz / SAMPLE_RATE as f32;
        let (sine, cosine) = angular.sin_cos();
        let alpha = sine / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let (b, a0, a) = if high_pass {
            (
                [(1.0 + cosine) / 2.0, -(1.0 + cosine), (1.0 + cosine) / 2.0],
                1.0 + alpha,
                [-2.0 * cosine, 1.0 - alpha],
            )
        } else {
            (
                [(1.0 - cosine) / 2.0, 1.0 - cosine, (1.0 - cosine) / 2.0],
                1.0 + alpha,
                [-2.0 * cosine, 1.0 - alpha],
            )
        };
        Self {
            b: [b[0] / a0, b[1] / a0, b[2] / a0],
            a: [a[0] / a0, a[1] / a0],
            z: [0.0; 2],
        }
    }

    fn run(&mut self, input: f32) -> f32 {
        let output = self.b[0] * input + self.z[0];
        self.z[0] = self.b[1] * input - self.a[0] * output + self.z[1];
        self.z[1] = self.b[2] * input - self.a[1] * output;
        output
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GateFrame {
    pub level_db: f32,
    pub floor_db: f32,
    pub voicing: f32,
    pub period: Option<usize>,
    pub voiced: bool,
    pub steady: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateEvent {
    Open { lead_samples: usize },
    Close,
}

pub struct SpeechGate {
    config: GateConfig,
    high_pass: Biquad,
    low_pass: Biquad,
    dc_filter: Biquad,
    band: Vec<f32>,
    floor_db: f32,
    frames_seen: u64,
    recent: VecDeque<GateFrame>,
    last_shape: Option<Vec<f32>>,
    open: bool,
    quiet: usize,
    level: f32,
    peak_db: f32,
}

impl SpeechGate {
    pub fn new(config: GateConfig) -> Self {
        Self {
            config,
            high_pass: Biquad::new(true, 200.0),
            low_pass: Biquad::new(false, 3800.0),
            dc_filter: Biquad::new(true, 20.0),
            band: vec![0.0; 2 * FRAME_SAMPLES],
            floor_db: -70.0,
            frames_seen: 0,
            recent: VecDeque::new(),
            last_shape: None,
            open: false,
            quiet: 0,
            level: 0.0,
            peak_db: -100.0,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn level(&self) -> f32 {
        if self.open {
            self.level
        } else {
            0.0
        }
    }

    pub fn floor_db(&self) -> f32 {
        self.floor_db
    }

    pub fn push(&mut self, frame: &PcmFrame) -> (GateFrame, Option<GateEvent>) {
        self.band.drain(..FRAME_SAMPLES);
        let mut whole = 0.0f32;
        for &sample in frame.samples() {
            let filtered = self.low_pass.run(self.high_pass.run(sample));
            self.band.push(filtered);
            let centered = self.dc_filter.run(sample);
            whole += centered * centered;
        }
        let current = &self.band[FRAME_SAMPLES..];
        let energy = current.iter().map(|value| value * value).sum::<f32>() / FRAME_SAMPLES as f32;
        let band_share = energy * FRAME_SAMPLES as f32 / whole.max(1e-12);
        let level_db = 10.0 * (energy + 1e-12).log10() + 3.01;
        self.frames_seen += 1;
        if self.frames_seen <= 5 || level_db < self.floor_db {
            self.floor_db = if self.frames_seen == 1 {
                level_db
            } else {
                self.floor_db.min(level_db).max(-100.0)
            };
        } else {
            self.floor_db += self.config.floor_rise_db_per_s / 50.0;
            self.floor_db = self.floor_db.min(level_db);
        }
        let loud = level_db >= self.config.min_level_db
            && level_db - self.floor_db >= self.config.above_floor_db
            && band_share >= self.config.band_share;
        let (voicing, period, shape) = if loud {
            periodicity(&self.band)
        } else {
            (0.0, None, Vec::new())
        };
        let voiced = loud && voicing >= self.config.voicing;
        let steady = voiced
            && self
                .last_shape
                .as_ref()
                .is_some_and(|previous| similarity(previous, &shape) >= self.config.same_shape);
        self.last_shape = voiced.then_some(shape);
        let analysis = GateFrame {
            level_db,
            floor_db: self.floor_db,
            voicing,
            period: if voiced { period } else { None },
            voiced,
            steady,
        };
        self.recent.push_back(analysis);
        if self.recent.len() > self.config.window {
            self.recent.pop_front();
        }
        if voiced {
            self.peak_db = self.peak_db.max(level_db);
        } else {
            self.peak_db =
                (self.peak_db - 6.0 / 50.0).max(self.floor_db + self.config.above_floor_db);
        }
        let target = ((level_db - (self.peak_db - 30.0)) / 30.0)
            .clamp(0.0, 1.0)
            .powf(2.2);
        self.level = if voiced {
            target
        } else {
            self.level.min(target)
        };

        let mut event = None;
        if !self.open {
            let voiced_count = self.recent.iter().filter(|item| item.voiced).count();
            let fast_count = self
                .recent
                .iter()
                .rev()
                .take(self.config.fast_window)
                .filter(|item| {
                    item.voiced && item.level_db - item.floor_db >= self.config.fast_above_db
                })
                .count();
            let enough = (self.recent.len() == self.config.window
                && voiced_count >= self.config.voiced_needed)
                || fast_count >= self.config.fast_needed;
            if enough && !self.held_note() {
                self.open = true;
                self.quiet = 0;
                let first = self.recent.iter().position(|item| item.voiced).unwrap_or(0);
                event = Some(GateEvent::Open {
                    lead_samples: (self.config.window - first) * FRAME_SAMPLES,
                });
            }
        } else if voiced {
            self.quiet = 0;
        } else {
            self.quiet += 1;
            if self.quiet >= self.config.hangover {
                self.open = false;
                self.level = 0.0;
                self.peak_db = -100.0;
                event = Some(GateEvent::Close);
            }
        }
        (analysis, event)
    }

    fn held_note(&self) -> bool {
        let voiced = self.recent.iter().filter(|item| item.voiced).count();
        let steady = self.recent.iter().filter(|item| item.steady).count();
        voiced >= 3 && steady as f32 / (voiced - 1) as f32 > self.config.steady_share
    }
}

fn periodicity(samples: &[f32]) -> (f32, Option<usize>, Vec<f32>) {
    let downsampled: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let length = downsampled.len();
    let half_rate = SAMPLE_RATE / 2;
    let (minimum_lag, maximum_lag) = (
        (half_rate / 400) as usize,
        ((half_rate / 60) as usize).min(length / 2),
    );
    let mut best = (0.0f32, None);
    let mut shape = Vec::with_capacity(maximum_lag + 1);
    for lag in 1..=maximum_lag {
        let (mut correlation, mut first_energy, mut second_energy) = (0.0, 0.0, 0.0);
        for index in 0..length - lag {
            correlation += downsampled[index] * downsampled[index + lag];
            first_energy += downsampled[index] * downsampled[index];
            second_energy += downsampled[index + lag] * downsampled[index + lag];
        }
        let normalized = correlation / (first_energy * second_energy).sqrt().max(1e-12);
        shape.push(normalized);
        if lag >= minimum_lag && normalized > best.0 {
            best = (normalized, Some(lag * 2));
        }
    }
    (best.0, best.1, shape)
}

fn similarity(first: &[f32], second: &[f32]) -> f32 {
    let (mut product, mut first_energy, mut second_energy) = (0.0, 0.0, 0.0);
    for (left, right) in first.iter().zip(second) {
        product += left * right;
        first_energy += left * left;
        second_energy += right * right;
    }
    product / (first_energy * second_energy).sqrt().max(1e-12)
}
