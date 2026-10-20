use mote_voice_core::PcmFrame;
use mote_voice_macos::recognize::{clean, Recognizer};
use mote_voice_macos::synth::FixtureSynthesizer;
use mote_voice_macos::{
    AudioCallback, BufferedSpeechOutput, CallMonitor, MacVoiceRuntime, Microphone, RuntimeEvent,
    ScriptLine, ScriptedRecognizer, SpeakEvent, SpeechOutput, SpeechRequest, Synthesizer,
    VoiceError, VoiceResult,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Default, Debug)]
struct DeviceState {
    starts: usize,
    stops: usize,
    pauses: usize,
    resumes: usize,
}

struct FakeMicrophone {
    state: Arc<Mutex<DeviceState>>,
}

impl Microphone for FakeMicrophone {
    fn start(&mut self, mut on_audio: AudioCallback) -> VoiceResult<()> {
        self.state.lock().unwrap().starts += 1;
        on_audio(PcmFrame::silence());
        Ok(())
    }

    fn stop(&mut self) -> VoiceResult<()> {
        self.state.lock().unwrap().stops += 1;
        Ok(())
    }

    fn pause(&mut self) -> VoiceResult<()> {
        self.state.lock().unwrap().pauses += 1;
        Ok(())
    }

    fn resume(&mut self) -> VoiceResult<()> {
        self.state.lock().unwrap().resumes += 1;
        Ok(())
    }
}

struct FakeCalls {
    polls: VecDeque<Vec<String>>,
}

impl CallMonitor for FakeCalls {
    fn active_calls(&mut self) -> VoiceResult<Vec<String>> {
        Ok(self.polls.pop_front().unwrap_or_default())
    }
}

#[derive(Default, Debug)]
struct SpeechState {
    pauses: usize,
    resumes: usize,
    stops: usize,
}

struct FakeSpeech {
    state: Arc<Mutex<SpeechState>>,
}

impl SpeechOutput for FakeSpeech {
    fn pause(&mut self) -> VoiceResult<()> {
        self.state.lock().unwrap().pauses += 1;
        Ok(())
    }

    fn resume(&mut self) -> VoiceResult<()> {
        self.state.lock().unwrap().resumes += 1;
        Ok(())
    }

    fn stop(&mut self) -> VoiceResult<()> {
        self.state.lock().unwrap().stops += 1;
        Ok(())
    }
}

#[test]
fn runtime_pauses_capture_and_speech_for_another_call_then_resumes_both() {
    let device = Arc::new(Mutex::new(DeviceState::default()));
    let speech = Arc::new(Mutex::new(SpeechState::default()));
    let mut runtime = MacVoiceRuntime::new(
        FakeMicrophone {
            state: Arc::clone(&device),
        },
        FakeCalls {
            polls: VecDeque::from([
                vec!["com.example.call".to_string()],
                vec!["com.example.call".to_string()],
                Vec::new(),
            ]),
        },
        ScriptedRecognizer::default(),
        FakeSpeech {
            state: Arc::clone(&speech),
        },
    );
    let received = Arc::new(Mutex::new(0_usize));
    let count = Arc::clone(&received);
    runtime
        .start(Box::new(move |_| *count.lock().unwrap() += 1))
        .unwrap();
    assert_eq!(*received.lock().unwrap(), 1);
    assert_eq!(
        runtime.poll_calls().unwrap(),
        Some(RuntimeEvent::PausedForCall {
            applications: vec!["com.example.call".to_string()]
        })
    );
    assert!(runtime.is_paused_for_call());
    assert_eq!(runtime.poll_calls().unwrap(), None);
    assert_eq!(
        runtime.poll_calls().unwrap(),
        Some(RuntimeEvent::ResumedAfterCall)
    );
    assert!(!runtime.is_paused_for_call());
    runtime.stop().unwrap();

    let device = device.lock().unwrap();
    assert_eq!(
        (device.starts, device.pauses, device.resumes, device.stops),
        (1, 1, 1, 1)
    );
    let speech = speech.lock().unwrap();
    assert_eq!((speech.pauses, speech.resumes, speech.stops), (1, 1, 1));
}

#[test]
fn scripted_recognition_tracks_the_audio_window_and_accepts_caller_vocabulary() {
    let mut recognizer = ScriptedRecognizer::new(vec![ScriptLine {
        start_ms: 1_000,
        end_ms: 3_000,
        text: "tell the kitchen display to show the timer".to_string(),
    }]);
    assert_eq!(
        recognizer
            .recognize(&vec![0.0; 16_000], 0, "kitchen")
            .unwrap(),
        ""
    );
    assert_eq!(
        recognizer
            .recognize(&vec![0.0; 32_000], 0, "kitchen")
            .unwrap(),
        "tell the kitchen display"
    );
    assert_eq!(
        recognizer
            .recognize(&vec![0.0; 64_000], 0, "kitchen")
            .unwrap(),
        "tell the kitchen display to show the timer"
    );
    assert_eq!(
        clean("(typing) show [BLANK_AUDIO] the timer"),
        "show the timer"
    );
}

fn tone(samples: usize) -> Vec<f32> {
    (0..samples)
        .map(|index| (index as f32 * 0.1).sin() * 0.5)
        .collect()
}

#[test]
fn buffered_speech_never_needs_a_temporary_file_and_pauses_in_place() {
    let mut output = BufferedSpeechOutput::default();
    let jobs = output.queue(7, "First part, second part here. Third part now.");
    assert_eq!(jobs.len(), 3);
    for (line, phrase, _) in jobs {
        output.synthesized(line, phrase, tone(960));
    }
    let (first, events, level) = output.next(320);
    assert_eq!(events, vec![SpeakEvent::Started { line: 7 }]);
    assert!(level.is_some_and(|level| level > 0.0));
    output.pause().unwrap();
    let (paused, events, level) = output.next(320);
    assert!(paused.iter().all(|sample| *sample == 0.0));
    assert!(events.is_empty());
    assert_eq!(level, None);
    output.resume().unwrap();
    let (resumed, _, _) = output.next(320);
    assert_ne!(first, resumed);
    assert!(resumed.iter().any(|sample| sample.abs() > 0.01));
}

#[test]
fn fixture_synthesis_is_deterministic_and_memory_only() {
    let request = SpeechRequest {
        text: "A short local line.",
        voice: None,
        words_per_minute: None,
    };
    let mut first = FixtureSynthesizer;
    let mut second = FixtureSynthesizer;
    let first = first.synthesize(request.clone()).unwrap();
    let second = second.synthesize(request).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.len(), 16_000);
    assert!(first.iter().any(|sample| sample.abs() > 0.1));
}

struct PauseFailingMicrophone {
    stops: Arc<Mutex<usize>>,
}

impl Microphone for PauseFailingMicrophone {
    fn start(&mut self, _on_audio: AudioCallback) -> VoiceResult<()> {
        Ok(())
    }

    fn stop(&mut self) -> VoiceResult<()> {
        *self.stops.lock().unwrap() += 1;
        Ok(())
    }

    fn pause(&mut self) -> VoiceResult<()> {
        Err(VoiceError::Runtime("pause failed".to_string()))
    }

    fn resume(&mut self) -> VoiceResult<()> {
        Ok(())
    }
}

struct StopFailingSpeech {
    pauses: Arc<Mutex<usize>>,
    resumes: Arc<Mutex<usize>>,
}

impl SpeechOutput for StopFailingSpeech {
    fn pause(&mut self) -> VoiceResult<()> {
        *self.pauses.lock().unwrap() += 1;
        Ok(())
    }

    fn resume(&mut self) -> VoiceResult<()> {
        *self.resumes.lock().unwrap() += 1;
        Ok(())
    }

    fn stop(&mut self) -> VoiceResult<()> {
        Err(VoiceError::Runtime("speech stop failed".to_string()))
    }
}

#[test]
fn lifecycle_failures_roll_back_pause_and_still_close_capture() {
    let stops = Arc::new(Mutex::new(0));
    let pauses = Arc::new(Mutex::new(0));
    let resumes = Arc::new(Mutex::new(0));
    let mut runtime = MacVoiceRuntime::new(
        PauseFailingMicrophone {
            stops: Arc::clone(&stops),
        },
        FakeCalls {
            polls: VecDeque::from([vec!["com.example.call".to_string()]]),
        },
        ScriptedRecognizer::default(),
        StopFailingSpeech {
            pauses: Arc::clone(&pauses),
            resumes: Arc::clone(&resumes),
        },
    );
    runtime.start(Box::new(|_| {})).unwrap();
    assert!(runtime.poll_calls().is_err());
    assert!(!runtime.is_paused_for_call());
    assert_eq!(*pauses.lock().unwrap(), 1);
    assert_eq!(*resumes.lock().unwrap(), 1);
    assert!(runtime.stop().is_err());
    assert_eq!(*stops.lock().unwrap(), 1);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn real_platform_types_are_explicitly_unavailable_off_macos() {
    use mote_voice_macos::{LocalWhisper, MacMicrophone};

    let error = LocalWhisper::load(std::path::Path::new("model.bin"))
        .err()
        .expect("non-macOS must reject local Whisper");
    assert!(error.to_string().contains("macOS only"));
    let mut microphone = MacMicrophone::new(true);
    let error = microphone
        .start(Box::new(|_| {}))
        .expect_err("non-macOS must reject microphone capture");
    assert!(error.to_string().contains("macOS only"));
}
