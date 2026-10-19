//! The portable voice core's single audio format: 16 kHz mono `f32` frames.

use std::fmt;

pub const SAMPLE_RATE: u32 = 16_000;
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE / 50) as usize;

#[derive(Clone, Debug, PartialEq)]
pub struct PcmFrame {
    samples: [f32; FRAME_SAMPLES],
}

impl PcmFrame {
    pub fn new(samples: [f32; FRAME_SAMPLES]) -> Self {
        Self { samples }
    }

    pub fn silence() -> Self {
        Self::new([0.0; FRAME_SAMPLES])
    }

    pub fn try_from_slice(samples: &[f32]) -> Result<Self, PcmError> {
        let samples: [f32; FRAME_SAMPLES] =
            samples.try_into().map_err(|_| PcmError::WrongFrameLength {
                expected: FRAME_SAMPLES,
                actual: samples.len(),
            })?;
        Ok(Self::new(samples))
    }

    pub fn samples(&self) -> &[f32; FRAME_SAMPLES] {
        &self.samples
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PcmError {
    WrongFrameLength { expected: usize, actual: usize },
}

impl fmt::Display for PcmError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongFrameLength { expected, actual } => {
                write!(formatter, "expected {expected} PCM samples, got {actual}")
            }
        }
    }
}

impl std::error::Error for PcmError {}

pub fn from_s16le(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32768.0)
        .collect()
}

pub fn to_s16le(samples: &[f32]) -> Vec<u8> {
    let mut output = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        output.extend_from_slice(&value.to_le_bytes());
    }
    output
}

pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let filtered = if from > to {
        let width = (from as f32 / to as f32).round().max(1.0) as usize;
        let mut accumulator = 0.0;
        let mut output = Vec::with_capacity(input.len());
        for (index, sample) in input.iter().enumerate() {
            accumulator += sample;
            if index >= width {
                accumulator -= input[index - width];
            }
            output.push(accumulator / width.min(index + 1) as f32);
        }
        output
    } else {
        input.to_vec()
    };
    let step = from as f64 / to as f64;
    let count = (input.len() as f64 / step).floor() as usize;
    (0..count)
        .map(|index| {
            let position = index as f64 * step;
            let first = position.floor() as usize;
            let second = (first + 1).min(filtered.len() - 1);
            let fraction = (position - first as f64) as f32;
            filtered[first] * (1.0 - fraction) + filtered[second] * fraction
        })
        .collect()
}
