use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

pub const DISPLAY_BOUNDARY: &str = "complete decoded IPC frame through first egui render-list construction; excludes GPU and compositor presentation";

pub trait Clock: Send + Sync {
    fn now_ns(&self) -> u64;
}

pub type SharedClock = Arc<dyn Clock>;

pub struct InstantClock {
    origin: Instant,
}

impl InstantClock {
    pub fn shared() -> SharedClock {
        Arc::new(Self {
            origin: Instant::now(),
        })
    }
}

impl Clock for InstantClock {
    fn now_ns(&self) -> u64 {
        self.origin.elapsed().as_nanos().min(u64::MAX as u128) as u64
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Sample {
    pub sequence: i64,
    pub arrival_offset_ns: u64,
    pub render_offset_ns: u64,
    pub duration_ns: u64,
}

#[derive(Debug, Serialize)]
pub struct Report<'a> {
    pub boundary: &'static str,
    pub sample_count: usize,
    pub p50_ns: Option<u64>,
    pub p95_ns: Option<u64>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub samples: &'a [Sample],
}

impl<'a> Report<'a> {
    pub fn from_samples(samples: &'a [Sample]) -> Self {
        let mut durations: Vec<_> = samples.iter().map(|sample| sample.duration_ns).collect();
        durations.sort_unstable();
        let p50_ns = nearest_rank(&durations, 50);
        let p95_ns = nearest_rank(&durations, 95);
        Self {
            boundary: DISPLAY_BOUNDARY,
            sample_count: samples.len(),
            p50_ns,
            p95_ns,
            p50_ms: p50_ns.map(|value| value as f64 / 1_000_000.0),
            p95_ms: p95_ns.map(|value| value as f64 / 1_000_000.0),
            samples,
        }
    }
}

fn nearest_rank(sorted: &[u64], percentile: usize) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (percentile * sorted.len()).div_ceil(100);
    sorted.get(rank.saturating_sub(1)).copied()
}

#[derive(Default)]
pub struct DisplayLatency {
    pending: HashMap<u64, (i64, u64)>,
    samples: Vec<Sample>,
}

impl DisplayLatency {
    pub fn accept(
        &mut self,
        revision: u64,
        sequence: i64,
        superseded_revision: Option<u64>,
        arrival_ns: u64,
    ) {
        if let Some(superseded) = superseded_revision {
            self.pending.remove(&superseded);
        }
        self.pending.insert(revision, (sequence, arrival_ns));
    }

    pub fn drawn(&mut self, revisions: &[u64], render_ns: u64) {
        for revision in revisions {
            let Some((sequence, arrival_ns)) = self.pending.remove(revision) else {
                continue;
            };
            self.samples.push(Sample {
                sequence,
                arrival_offset_ns: arrival_ns,
                render_offset_ns: render_ns,
                duration_ns: render_ns.saturating_sub(arrival_ns),
            });
        }
    }

    #[cfg(test)]
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    pub fn write_report(&self, path: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec_pretty(&Report::from_samples(&self.samples))
            .map_err(std::io::Error::other)?;
        std::fs::write(path, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_first_draw_once_and_drops_superseded_arrivals() {
        let mut latency = DisplayLatency::default();
        latency.accept(1, 7, None, 100_000_000);
        latency.accept(2, 8, Some(1), 105_000_000);

        latency.drawn(&[1, 2], 111_000_000);
        latency.drawn(&[2], 120_000_000);

        assert_eq!(
            latency.samples(),
            &[Sample {
                sequence: 8,
                arrival_offset_ns: 105_000_000,
                render_offset_ns: 111_000_000,
                duration_ns: 6_000_000,
            }]
        );
    }

    #[test]
    fn nearest_rank_summary_uses_explicit_twenty_sample_boundaries() {
        let durations = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 40,
        ];
        let samples: Vec<_> = durations
            .into_iter()
            .enumerate()
            .map(|(index, duration_ms)| Sample {
                sequence: index as i64 + 1,
                arrival_offset_ns: index as u64,
                render_offset_ns: index as u64 + duration_ms * 1_000_000,
                duration_ns: duration_ms * 1_000_000,
            })
            .collect();

        let report = Report::from_samples(&samples);

        assert_eq!(report.sample_count, 20);
        assert_eq!(report.p50_ns, Some(10_000_000));
        assert_eq!(report.p95_ns, Some(19_000_000));
        assert_eq!(report.boundary, DISPLAY_BOUNDARY);
    }

    #[test]
    fn report_contains_only_timing_and_sequence_metadata() {
        let samples = [Sample {
            sequence: 41,
            arrival_offset_ns: 2_000_000,
            render_offset_ns: 5_000_000,
            duration_ns: 3_000_000,
        }];

        let value = serde_json::to_value(Report::from_samples(&samples)).unwrap();

        assert_eq!(
            value,
            serde_json::json!({
                "boundary": DISPLAY_BOUNDARY,
                "sample_count": 1,
                "p50_ns": 3_000_000,
                "p95_ns": 3_000_000,
                "p50_ms": 3.0,
                "p95_ms": 3.0,
                "samples": [{
                    "sequence": 41,
                    "arrival_offset_ns": 2_000_000,
                    "render_offset_ns": 5_000_000,
                    "duration_ns": 3_000_000
                }]
            })
        );
        let object = value.as_object().unwrap();
        assert!(!object.contains_key("payload"));
        assert!(!object.contains_key("source"));
    }
}
