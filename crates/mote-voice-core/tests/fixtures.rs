use mote_voice_core::{
    classify_barge, Endpoint, EndpointConfig, EndpointDecision, GateConfig, GateEvent, PcmFrame,
    SpeechGate, Vocabulary, WordDecision, FRAME_SAMPLES, SAMPLE_RATE,
};
use std::f32::consts::PI;

struct Noise(u64);

impl Noise {
    fn new(seed: u64) -> Self {
        Self(
            seed.wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407),
        )
    }

    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as f32 / (1_u64 << 31) as f32) * 2.0 - 1.0
    }
}

fn sample_count(seconds: f32) -> usize {
    (seconds * SAMPLE_RATE as f32) as usize
}

fn room(seconds: f32, amplitude: f32, seed: u64) -> Vec<f32> {
    let mut noise = Noise::new(seed);
    (0..sample_count(seconds))
        .map(|_| amplitude * noise.next())
        .collect()
}

fn speechlike(seconds: f32, amplitude: f32) -> Vec<f32> {
    let mut phase = 0.0;
    (0..sample_count(seconds))
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE as f32;
            let pitch =
                140.0 + 30.0 * (2.0 * PI * 1.3 * time).sin() + 10.0 * (2.0 * PI * 3.1 * time).sin();
            phase += 2.0 * PI * pitch / SAMPLE_RATE as f32;
            let syllable = (0.55 + 0.45 * (2.0 * PI * 4.0 * time).sin()).max(0.1);
            let value: f32 = (1..=12)
                .map(|harmonic| {
                    (harmonic as f32 * phase).sin()
                        * (1.0 / harmonic as f32)
                        * if (500.0..1500.0).contains(&(harmonic as f32 * pitch)) {
                            2.0
                        } else {
                            1.0
                        }
                })
                .sum();
            amplitude * syllable * value / 3.0
        })
        .collect()
}

fn clicks(seconds: f32, per_second: f32, amplitude: f32, seed: u64) -> Vec<f32> {
    let mut noise = Noise::new(seed);
    let mut output = room(seconds, 0.0005, seed + 1);
    let every = (SAMPLE_RATE as f32 / per_second) as usize;
    let mut at = every / 3;
    while at + 400 < output.len() {
        for index in 0..240 {
            output[at + index] += amplitude * noise.next() * (-(index as f32) / 40.0).exp();
        }
        at += every + (noise.next().abs() * every as f32 * 0.5) as usize;
    }
    output
}

fn drive_gate(samples: &[f32]) -> (usize, Option<usize>, f32, bool) {
    let mut gate = SpeechGate::new(GateConfig::default());
    let mut opens = 0;
    let mut first_open = None;
    let mut maximum_level = 0.0_f32;
    for (index, samples) in samples.chunks_exact(FRAME_SAMPLES).enumerate() {
        let frame = PcmFrame::try_from_slice(samples).expect("fixture frame");
        let (_, event) = gate.push(&frame);
        if matches!(event, Some(GateEvent::Open { .. })) {
            opens += 1;
            first_open.get_or_insert(index);
        }
        maximum_level = maximum_level.max(gate.level());
    }
    (opens, first_open, maximum_level, gate.is_open())
}

#[test]
fn pinned_silence_and_noise_fixtures_never_open() {
    for amplitude in [0.0, 0.001, 0.01, 0.05] {
        let decision = drive_gate(&room(5.0, amplitude, 7));
        assert_eq!(decision.0, 0, "room tone at {amplitude}");
        assert_eq!(decision.2, 0.0);
    }
    for (per_second, amplitude) in [(8.0, 0.3), (2.0, 0.8), (15.0, 0.1)] {
        let decision = drive_gate(&clicks(6.0, per_second, amplitude, 11));
        assert_eq!(decision.0, 0, "{per_second} clicks at {amplitude}");
        assert_eq!(decision.2, 0.0);
    }
}

#[test]
fn pinned_speech_fixture_opens_quickly_and_closes() {
    let mut samples = room(1.0, 0.002, 3);
    samples.extend(speechlike(2.0, 0.3));
    samples.extend(room(1.0, 0.002, 4));
    let (opens, first, level, open) = drive_gate(&samples);
    assert_eq!(opens, 1);
    let open_ms = first.expect("gate opens") as f32 * 20.0 - 1000.0;
    assert!(open_ms <= 300.0, "opened after {open_ms} ms");
    assert!(level > 0.3, "peak level {level}");
    assert!(!open, "gate closes after speech");
}

fn drive_endpoint(
    endpoint: &mut Endpoint,
    plan: &[(bool, usize)],
    words: Option<&str>,
) -> Vec<(u64, EndpointDecision)> {
    let mut now_sample = 0_u64;
    let mut was_open = false;
    let mut output = Vec::new();
    let frame = PcmFrame::silence();
    for &(open, frame_count) in plan {
        for _ in 0..frame_count {
            now_sample += FRAME_SAMPLES as u64;
            let event = match (was_open, open) {
                (false, true) => Some(GateEvent::Open { lead_samples: 0 }),
                (true, false) => Some(GateEvent::Close),
                _ => None,
            };
            was_open = open;
            for decision in endpoint.push(&frame, event, open, now_sample) {
                if let (EndpointDecision::NeedWords { id }, Some(words)) = (&decision, words) {
                    endpoint.set_words(*id, words);
                }
                output.push((now_sample * 1000 / SAMPLE_RATE as u64, decision));
            }
        }
    }
    output
}

fn ending_times(decisions: &[(u64, EndpointDecision)]) -> Vec<(u64, bool)> {
    decisions
        .iter()
        .filter_map(|(time, decision)| match decision {
            EndpointDecision::Ended { complete, .. } => Some((*time, *complete)),
            _ => None,
        })
        .collect()
}

#[test]
fn pinned_endpoint_complete_and_timeout_fixtures_match() {
    let mut complete = Endpoint::new(EndpointConfig::default());
    let decisions = drive_endpoint(
        &mut complete,
        &[(false, 25), (true, 100), (false, 75)],
        Some("Finish this request."),
    );
    assert_eq!(ending_times(&decisions), vec![(3200, true)]);

    let mut unfinished = Endpoint::new(EndpointConfig::default());
    let decisions = drive_endpoint(
        &mut unfinished,
        &[(false, 25), (true, 100), (false, 150)],
        Some("Finish this and"),
    );
    assert_eq!(ending_times(&decisions), vec![(5000, false)]);
}

#[test]
fn pinned_longest_utterance_fixture_splits_at_ninety_seconds() {
    let mut endpoint = Endpoint::new(EndpointConfig::default());
    let decisions = drive_endpoint(&mut endpoint, &[(true, 4_750)], None);
    assert_eq!(ending_times(&decisions).len(), 1);
    assert_eq!(
        decisions
            .iter()
            .filter(|(_, decision)| matches!(decision, EndpointDecision::Started { .. }))
            .count(),
        2
    );
    assert_eq!(ending_times(&decisions)[0].0, 90_000);
}

#[test]
fn barge_rules_are_caller_supplied_and_match_pinned_decisions() {
    let vocabulary = Vocabulary::new(
        ["mm", "mhm", "yeah", "okay", "got it"],
        ["stop", "wait", "hold on"],
        ["sending the summary to the desktop agent"],
    );
    assert_eq!(classify_barge("", &vocabulary), WordDecision::Ignore);
    assert_eq!(
        classify_barge("Yeah, okay.", &vocabulary),
        WordDecision::Backchannel
    );
    assert_eq!(
        classify_barge("Stop right there", &vocabulary),
        WordDecision::Stop
    );
    assert_eq!(
        classify_barge("sending the summary", &vocabulary),
        WordDecision::Echo
    );
    assert_eq!(
        classify_barge("change the destination", &vocabulary),
        WordDecision::Barge
    );
    assert_eq!(classify_barge("change", &vocabulary), WordDecision::Ignore);
}

#[test]
fn pcm_frame_and_conversion_boundaries_are_deterministic() {
    assert!(PcmFrame::try_from_slice(&[0.0; FRAME_SAMPLES - 1]).is_err());
    let samples = [0.0, 0.5, -0.5, 0.999, -1.0];
    let bytes = mote_voice_core::pcm::to_s16le(&samples);
    let round_trip = mote_voice_core::pcm::from_s16le(&bytes);
    assert!(samples
        .iter()
        .zip(round_trip)
        .all(|(left, right)| (left - right).abs() < 0.001));
}
