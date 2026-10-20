use base64::Engine as _;
use mote_voice_core::{
    pcm, Endpoint, EndpointConfig, EndpointDecision, GateConfig, GateEvent, PcmFrame, SpeechGate,
    FRAME_SAMPLES, SAMPLE_RATE,
};
use mote_voice_macos::{
    CallMonitor, LocalWhisper, MacCallMonitor, MacMicrophone, Microphone, Recognizer, ScriptLine,
    ScriptedRecognizer,
};
use mote_voice_protocol::{decode_command, encode_line, Command, Event, InputMode, Source};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

const HELD_AUDIO_MS: u64 = 30_000;
const SEGMENT_AUDIO_MS: u64 = 25_000;
const UTTERANCE_MS: u64 = 90_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListenerInput {
    Mic,
    Fixture,
}

#[derive(Clone, Debug)]
struct ListenerConfig {
    input: ListenerInput,
    model_path: PathBuf,
    fixture_mode: bool,
    held_audio_ms: u64,
    utterance_ms: u64,
}

impl ListenerConfig {
    fn parse() -> Result<Self, String> {
        let mut input = None;
        let mut model_path = None;
        let mut arguments = std::env::args().skip(1);
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--input" => {
                    let value = arguments
                        .next()
                        .ok_or_else(|| "--input needs mic or fixture".to_string())?;
                    input = Some(match value.as_str() {
                        "mic" => ListenerInput::Mic,
                        "fixture" => ListenerInput::Fixture,
                        _ => return Err("--input must be mic or fixture".to_string()),
                    });
                }
                "--model" => {
                    model_path = Some(PathBuf::from(
                        arguments
                            .next()
                            .ok_or_else(|| "--model needs a local path".to_string())?,
                    ));
                }
                _ => return Err(format!("unknown argument: {argument}")),
            }
        }
        let input = input.ok_or_else(|| "--input is required".to_string())?;
        let model_path = model_path.ok_or_else(|| "--model is required".to_string())?;
        Ok(Self {
            input,
            model_path,
            fixture_mode: input == ListenerInput::Fixture,
            held_audio_ms: HELD_AUDIO_MS,
            utterance_ms: UTTERANCE_MS,
        })
    }
}

struct JsonLines<W: Write> {
    writer: W,
}

impl<W: Write> JsonLines<W> {
    fn send(&mut self, event: &Event) {
        if let Ok(line) = encode_line(event) {
            let _ = self.writer.write_all(line.as_bytes());
            let _ = self.writer.flush();
        }
    }
}

struct Listener<W: Write> {
    output: JsonLines<W>,
    gate: SpeechGate,
    endpoint: Endpoint,
    recognizer: Box<dyn Recognizer>,
    hint: String,
    now_sample: u64,
    frames: u64,
    suppress_until: u64,
    held_max_samples: usize,
    starts: HashMap<u64, u64>,
    segments: HashMap<u64, String>,
}

impl<W: Write> Listener<W> {
    fn new(output: W, recognizer: Box<dyn Recognizer>, utterance_ms: u64) -> Self {
        let endpoint_config = EndpointConfig {
            longest_ms: utterance_ms,
            ..EndpointConfig::default()
        };
        Self {
            output: JsonLines { writer: output },
            gate: SpeechGate::new(GateConfig::default()),
            endpoint: Endpoint::new(endpoint_config),
            recognizer,
            hint: String::new(),
            now_sample: 0,
            frames: 0,
            suppress_until: 0,
            held_max_samples: 0,
            starts: HashMap::new(),
            segments: HashMap::new(),
        }
    }

    fn ready(&mut self, input: &str, model_path: &Path) {
        let model = model_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.recognizer.name().to_string());
        self.output.send(&Event::Ready {
            input: input.to_string(),
            model,
            t_ms: self.now_ms(),
        });
    }

    fn now_ms(&self) -> u64 {
        self.now_sample * 1_000 / SAMPLE_RATE as u64
    }

    fn error(&mut self, message: impl Into<String>) {
        self.output.send(&Event::Error {
            message: message.into(),
            t_ms: self.now_ms(),
        });
    }

    fn push_samples(&mut self, samples: &[f32]) {
        for chunk in samples.chunks(FRAME_SAMPLES) {
            let mut padded = [0.0; FRAME_SAMPLES];
            padded[..chunk.len()].copy_from_slice(chunk);
            self.push_frame(PcmFrame::new(padded));
        }
    }

    fn push_frame(&mut self, frame: PcmFrame) {
        self.now_sample = self.now_sample.saturating_add(FRAME_SAMPLES as u64);
        self.frames = self.frames.saturating_add(1);
        let suppressed = self.now_sample <= self.suppress_until;
        let (gate_frame, gate_event) = if suppressed {
            (Default::default(), None)
        } else {
            self.gate.push(&frame)
        };
        let open = !suppressed && self.gate.is_open();
        if let Some(event) = gate_event {
            self.output.send(&Event::Gate {
                open: matches!(event, GateEvent::Open { .. }),
                t_ms: self.now_ms(),
            });
        }
        if open && self.frames.is_multiple_of(2) {
            self.output.send(&Event::Level {
                source: Source::Owner,
                value: self.gate.level().max((gate_frame.level_db + 100.0) / 100.0),
                t_ms: self.now_ms(),
            });
        }
        let decisions = self
            .endpoint
            .push(&frame, gate_event, open, self.now_sample);
        for decision in decisions {
            self.apply_decision(decision);
        }
        self.held_max_samples = self.held_max_samples.max(self.endpoint.audio().len());
        let held_limit = HELD_AUDIO_MS as usize * SAMPLE_RATE as usize / 1_000;
        if self.endpoint.audio().len() >= held_limit {
            let segment_samples = SEGMENT_AUDIO_MS as usize * SAMPLE_RATE as usize / 1_000;
            if let Some((id, start_ms, audio)) = self.endpoint.take_front(segment_samples) {
                if let Some(text) = self.recognize(&audio, start_ms) {
                    append_words(self.segments.entry(id).or_default(), &text);
                }
            }
        }
    }

    fn apply_decision(&mut self, decision: EndpointDecision) {
        match decision {
            EndpointDecision::Started { id, start_ms } => {
                self.starts.insert(id, start_ms);
            }
            EndpointDecision::NeedWords { id } => {
                let start_ms = self.starts.get(&id).copied().unwrap_or_default();
                let audio = self.endpoint.audio().to_vec();
                if let Some(text) = self.recognize(&audio, start_ms) {
                    self.endpoint.set_words(id, &text);
                    if !text.is_empty() {
                        self.output.send(&Event::Words {
                            id,
                            text,
                            t_ms: self.now_ms(),
                        });
                    }
                }
            }
            EndpointDecision::Ended {
                id,
                start_ms,
                end_ms,
                complete,
                audio,
                audio_start_ms,
            } => {
                let mut text = self.segments.remove(&id).unwrap_or_default();
                if let Some(final_text) = self.recognize(&audio, audio_start_ms) {
                    append_words(&mut text, &final_text);
                }
                self.starts.remove(&id);
                if text.is_empty() {
                    self.output.send(&Event::Dropped {
                        id,
                        text,
                        reason: "empty".to_string(),
                        t_ms: self.now_ms(),
                    });
                } else {
                    self.output.send(&Event::Utterance {
                        id,
                        text,
                        complete,
                        start_ms,
                        end_ms,
                        t_ms: self.now_ms(),
                    });
                }
            }
        }
    }

    fn recognize(&mut self, audio: &[f32], start_ms: u64) -> Option<String> {
        match self.recognizer.recognize(audio, start_ms, &self.hint) {
            Ok(text) => Some(text),
            Err(error) => {
                self.error(format!("local recognition failed: {error}"));
                None
            }
        }
    }

    fn finish_input(&mut self) {
        self.push_samples(&vec![0.0; SAMPLE_RATE as usize * 3]);
        self.output.send(&Event::End {
            t_ms: self.now_ms(),
            held_max_ms: self.held_max_samples as u64 * 1_000 / SAMPLE_RATE as u64,
        });
    }
}

fn append_words(output: &mut String, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    if !output.is_empty() {
        output.push(' ');
    }
    output.push_str(text);
}

fn main() {
    let config = match ListenerConfig::parse() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let result = match config.input {
        ListenerInput::Fixture => run_fixture(config),
        ListenerInput::Mic => run_microphone(config),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run_fixture(config: ListenerConfig) -> Result<(), String> {
    debug_assert!(config.fixture_mode);
    debug_assert_eq!(config.held_audio_ms, HELD_AUDIO_MS);
    let stdout = std::io::stdout();
    let mut listener = Listener::new(
        stdout.lock(),
        Box::new(ScriptedRecognizer::default()),
        config.utterance_ms,
    );
    listener.ready("fixture", &config.model_path);
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("could not read command: {error}"))?;
        let command = match decode_command(&line, InputMode::Fixture) {
            Ok(command) => command,
            Err(error) => {
                listener.error(format!("invalid command: {error}"));
                continue;
            }
        };
        match command {
            Command::Feed { pcm: encoded } => {
                match base64::engine::general_purpose::STANDARD.decode(encoded) {
                    Ok(bytes) => listener.push_samples(&pcm::from_s16le(&bytes)),
                    Err(error) => listener.error(format!("invalid fixture audio: {error}")),
                }
            }
            Command::FeedEnd => listener.finish_input(),
            Command::Script { lines } => {
                listener.recognizer = Box::new(ScriptedRecognizer::new(
                    lines
                        .into_iter()
                        .map(|line| ScriptLine {
                            start_ms: line.start_ms,
                            end_ms: line.end_ms,
                            text: line.text,
                        })
                        .collect(),
                ));
            }
            Command::Simulate {
                speechlike: Some(seconds),
                gain,
                ..
            } if seconds.is_finite() && (0.0..=120.0).contains(&seconds) => {
                simulate_speechlike(&mut listener, seconds, gain.unwrap_or(0.3));
            }
            Command::Hint { text } => listener.hint = text,
            Command::Suppress { ms } => {
                listener.suppress_until = listener
                    .suppress_until
                    .max(listener.now_sample + ms * SAMPLE_RATE as u64 / 1_000);
            }
            Command::Quit => break,
            Command::Restore | Command::Stop { .. } | Command::Speak { .. } => {
                listener.error("speech output is not connected yet");
            }
            Command::Simulate { .. } => listener.error("unsupported fixture simulation"),
        }
    }
    Ok(())
}

fn simulate_speechlike<W: Write>(listener: &mut Listener<W>, seconds: f32, gain: f32) {
    let frames = (seconds * 50.0).round() as usize;
    let mut phase = 0.0_f32;
    let mut sample_index = 0_u64;
    for _ in 0..frames {
        let mut samples = [0.0; FRAME_SAMPLES];
        for sample in &mut samples {
            let time = sample_index as f32 / SAMPLE_RATE as f32;
            let pitch = 140.0
                + 30.0 * (std::f32::consts::TAU * 1.3 * time).sin()
                + 10.0 * (std::f32::consts::TAU * 3.1 * time).sin();
            phase += std::f32::consts::TAU * pitch / SAMPLE_RATE as f32;
            let syllable = (0.55 + 0.45 * (std::f32::consts::TAU * 4.0 * time).sin()).max(0.1);
            let voiced: f32 = (1..=12)
                .map(|harmonic| {
                    let hz = harmonic as f32 * pitch;
                    (harmonic as f32 * phase).sin() / harmonic as f32
                        * if (500.0..1500.0).contains(&hz) {
                            2.0
                        } else {
                            1.0
                        }
                })
                .sum();
            *sample = gain * syllable * voiced / 3.0;
            sample_index += 1;
        }
        listener.push_frame(PcmFrame::new(samples));
    }
}

enum MicInput {
    Frame(Box<PcmFrame>),
    Command(String),
    Closed,
}

fn run_microphone(config: ListenerConfig) -> Result<(), String> {
    debug_assert!(!config.fixture_mode);
    let recognizer = LocalWhisper::load(&config.model_path).map_err(|error| error.to_string())?;
    let stdout = std::io::stdout();
    let mut listener = Listener::new(stdout.lock(), Box::new(recognizer), config.utterance_ms);
    listener.ready("mic", &config.model_path);
    let (sender, receiver) = mpsc::channel();
    let audio_sender = sender.clone();
    let mut microphone = MacMicrophone::default();
    microphone
        .start(Box::new(move |samples| {
            let _ = audio_sender.send(MicInput::Frame(Box::new(samples)));
        }))
        .map_err(|error| error.to_string())?;
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) => {
                    if sender.send(MicInput::Command(line)).is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
        let _ = sender.send(MicInput::Closed);
    });
    let mut calls = MacCallMonitor;
    let mut paused = false;
    let mut running = true;
    while running {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(MicInput::Frame(frame)) if !paused => listener.push_frame(*frame),
            Ok(MicInput::Frame(_)) => {}
            Ok(MicInput::Command(line)) => {
                let command = match decode_command(&line, InputMode::Production) {
                    Ok(command) => command,
                    Err(error) => {
                        listener.error(format!("invalid command: {error}"));
                        continue;
                    }
                };
                match command {
                    Command::Hint { text } => listener.hint = text,
                    Command::Suppress { ms } => {
                        listener.suppress_until = listener
                            .suppress_until
                            .max(listener.now_sample + ms * SAMPLE_RATE as u64 / 1_000);
                    }
                    Command::Quit => running = false,
                    Command::Speak { .. } | Command::Restore | Command::Stop { .. } => {
                        listener.error("speech output is not connected yet");
                    }
                    Command::Feed { .. }
                    | Command::FeedEnd
                    | Command::Simulate { .. }
                    | Command::Script { .. } => {
                        unreachable!("production decoder rejects fixture commands")
                    }
                }
            }
            Ok(MicInput::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => running = false,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        let active = calls.active_calls().unwrap_or_default();
        if !active.is_empty() && !paused {
            microphone.pause().map_err(|error| error.to_string())?;
            paused = true;
            listener.output.send(&Event::Paused {
                apps: active,
                t_ms: listener.now_ms(),
            });
        } else if active.is_empty() && paused {
            microphone.resume().map_err(|error| error.to_string())?;
            paused = false;
            listener.output.send(&Event::Resumed {
                t_ms: listener.now_ms(),
            });
        }
    }
    microphone.stop().map_err(|error| error.to_string())?;
    listener.finish_input();
    Ok(())
}
