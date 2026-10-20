use mote_eventlog::EventLog;
use mote_hrns::{SpokenLine, TaskId};
use mote_voice_protocol::{decode_line, encode_line, Command, Event, Frame};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command as ProcessCommand, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const MAX_RAPID_FAILURES: usize = 3;
const RAPID_FAILURE_WINDOW: Duration = Duration::from_secs(10);
const STOP_TIMEOUT: Duration = Duration::from_millis(500);
const OWNER_TURN_CAPACITY: usize = 32;
const ACCOUNTING_CAPACITY: usize = 64;
const VISIBLE_STATUS_TEXT_LIMIT: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceOwnerTurn {
    pub session_id: i64,
    pub utterance_id: u64,
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub played_context: Option<SpokenLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceAccountingEvent {
    Recognized {
        session_id: i64,
        utterance_id: u64,
        output_tokens: i64,
        audio_ms: i64,
    },
    SpeechStarted {
        line_id: u64,
        input_tokens: i64,
    },
    SpeechFinished {
        line_id: u64,
        latency_ms: i64,
        status: VoiceOperationStatus,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceOperationStatus {
    Ok,
    Stopped,
    Failed,
    Cancelled,
}

impl VoiceOperationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceState {
    #[default]
    Off,
    Starting,
    Listening,
    Hearing,
    Thinking,
    Speaking,
    Paused,
    Unavailable,
    Error,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct VoiceStatus {
    pub state: VoiceState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_words: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaking_line: Option<String>,
}

#[derive(Clone, Debug)]
pub struct VoiceConfig {
    pub listener_path: PathBuf,
    pub input: String,
    pub model_path: PathBuf,
}

pub trait VoicePermission: Send + Sync {
    fn check(&self) -> Result<(), String>;
}

pub struct PlatformVoicePermission;

impl VoicePermission for PlatformVoicePermission {
    fn check(&self) -> Result<(), String> {
        if cfg!(target_os = "macos") {
            Ok(())
        } else {
            Err("voice capture is available on macOS only".into())
        }
    }
}

pub trait ListenerProcess: Send {
    fn try_wait(&mut self) -> Result<bool, String>;
    fn kill(&mut self) -> Result<(), String>;
}

pub struct RunningListener {
    pub input: Box<dyn Write + Send>,
    pub output: Box<dyn Read + Send>,
    pub process: Box<dyn ListenerProcess>,
}

pub trait ListenerLauncher: Send + Sync {
    fn spawn(&self, config: &VoiceConfig) -> Result<RunningListener, String>;
}

pub struct ProcessListenerLauncher;

struct ChildProcess(Child);

impl ListenerProcess for ChildProcess {
    fn try_wait(&mut self) -> Result<bool, String> {
        self.0
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|error| error.to_string())
    }

    fn kill(&mut self) -> Result<(), String> {
        match self.0.try_wait() {
            Ok(Some(_)) => Ok(()),
            Ok(None) => self.0.kill().map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        }
    }
}

impl ListenerLauncher for ProcessListenerLauncher {
    fn spawn(&self, config: &VoiceConfig) -> Result<RunningListener, String> {
        if !config.listener_path.is_file() {
            return Err("voice listener executable is not installed".into());
        }
        if !config.model_path.is_file() {
            return Err("local voice model is not installed".into());
        }
        let mut child = ProcessCommand::new(&config.listener_path)
            .arg("--input")
            .arg(&config.input)
            .arg("--model")
            .arg(&config.model_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("voice listener unavailable: {error}"))?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| "voice listener stdin unavailable".to_string())?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| "voice listener stdout unavailable".to_string())?;
        Ok(RunningListener {
            input: Box::new(input),
            output: Box::new(output),
            process: Box::new(ChildProcess(child)),
        })
    }
}

struct Inner {
    status: VoiceStatus,
    desired: bool,
    generation: u64,
    input: Option<Box<dyn Write + Send>>,
    process: Option<Arc<Mutex<Box<dyn ListenerProcess>>>>,
    failures: VecDeque<Instant>,
    session_id: Option<i64>,
    accepted_utterances: HashSet<u64>,
    active_utterance: Option<u64>,
    next_line_id: u64,
    spoken: Option<SpokenTracker>,
    last_spoken: Option<SpokenLine>,
}

struct SpokenTracker {
    line: SpokenLine,
    phrase_word_ends: Vec<usize>,
    started_at: Instant,
}

struct Shared {
    config: VoiceConfig,
    log: EventLog,
    launcher: Arc<dyn ListenerLauncher>,
    permission: Arc<dyn VoicePermission>,
    owner_turns: mpsc::SyncSender<VoiceOwnerTurn>,
    accounting: mpsc::SyncSender<VoiceAccountingEvent>,
    inner: Mutex<Inner>,
}

pub struct VoiceSupervisor {
    shared: Arc<Shared>,
    owner_turns: Mutex<Option<mpsc::Receiver<VoiceOwnerTurn>>>,
    accounting: Mutex<Option<mpsc::Receiver<VoiceAccountingEvent>>>,
}

impl VoiceSupervisor {
    pub fn new(config: VoiceConfig, log: EventLog) -> Self {
        Self::with_components(
            config,
            log,
            Arc::new(ProcessListenerLauncher),
            Arc::new(PlatformVoicePermission),
        )
    }

    pub fn with_components(
        config: VoiceConfig,
        log: EventLog,
        launcher: Arc<dyn ListenerLauncher>,
        permission: Arc<dyn VoicePermission>,
    ) -> Self {
        let (owner_turns, owner_turn_receiver) = mpsc::sync_channel(OWNER_TURN_CAPACITY);
        let (accounting, accounting_receiver) = mpsc::sync_channel(ACCOUNTING_CAPACITY);
        Self {
            shared: Arc::new(Shared {
                config,
                log,
                launcher,
                permission,
                owner_turns,
                accounting,
                inner: Mutex::new(Inner {
                    status: VoiceStatus::default(),
                    desired: false,
                    generation: 0,
                    input: None,
                    process: None,
                    failures: VecDeque::new(),
                    session_id: None,
                    accepted_utterances: HashSet::new(),
                    active_utterance: None,
                    next_line_id: 1,
                    spoken: None,
                    last_spoken: None,
                }),
            }),
            owner_turns: Mutex::new(Some(owner_turn_receiver)),
            accounting: Mutex::new(Some(accounting_receiver)),
        }
    }

    pub fn take_owner_turns(&self) -> Option<mpsc::Receiver<VoiceOwnerTurn>> {
        self.owner_turns.lock().unwrap().take()
    }

    pub fn take_accounting(&self) -> Option<mpsc::Receiver<VoiceAccountingEvent>> {
        self.accounting.lock().unwrap().take()
    }

    pub fn start(&self) -> Result<VoiceStatus, String> {
        let generation = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.desired {
                return Ok(inner.status.clone());
            }
            inner.desired = true;
            inner.failures.clear();
            inner.generation = inner.generation.wrapping_add(1);
            inner.generation
        };
        if let Err(reason) = self.shared.permission.check() {
            transition_generation(
                &self.shared,
                generation,
                VoiceState::Unavailable,
                Some(reason.clone()),
            );
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.generation == generation {
                inner.desired = false;
            }
            return Err(reason);
        }
        let session_id = match self
            .shared
            .log
            .append("voice", "voice.session.started", &json!({}))
        {
            Ok(event) => event.seq,
            Err(error) => {
                let reason = format!("voice session could not be recorded: {error}");
                transition_generation(
                    &self.shared,
                    generation,
                    VoiceState::Error,
                    Some(reason.clone()),
                );
                let mut inner = self.shared.inner.lock().unwrap();
                if inner.generation == generation {
                    inner.desired = false;
                }
                return Err(reason);
            }
        };
        {
            let mut inner = self.shared.inner.lock().unwrap();
            if !inner.desired || inner.generation != generation {
                return Ok(inner.status.clone());
            }
            inner.session_id = Some(session_id);
            inner.accepted_utterances.clear();
            inner.active_utterance = None;
            inner.status.partial_words = None;
            inner.status.speaking_line = None;
        }
        transition_generation(&self.shared, generation, VoiceState::Starting, None);
        if let Err(reason) = launch(&self.shared, generation) {
            {
                let inner = self.shared.inner.lock().unwrap();
                if !inner.desired || inner.generation != generation {
                    return Ok(inner.status.clone());
                }
            }
            transition_generation(
                &self.shared,
                generation,
                VoiceState::Unavailable,
                Some(reason.clone()),
            );
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.generation == generation {
                inner.desired = false;
            }
            return Err(reason);
        }
        Ok(self.state())
    }

    pub fn stop(&self) -> Result<VoiceStatus, String> {
        let (mut input, process, spoken) = {
            let mut inner = self.shared.inner.lock().unwrap();
            inner.desired = false;
            inner.generation = inner.generation.wrapping_add(1);
            inner.session_id = None;
            inner.active_utterance = None;
            inner.status.partial_words = None;
            inner.status.speaking_line = None;
            let spoken = inner.spoken.take();
            (inner.input.take(), inner.process.take(), spoken)
        };
        if let Some(spoken) = spoken {
            record_speech_completion(&self.shared, &spoken, VoiceOperationStatus::Cancelled);
        }
        if let Some(writer) = input.as_mut() {
            if let Ok(line) = encode_line(&Command::Quit) {
                let _ = writer.write_all(line.as_bytes());
                let _ = writer.flush();
            }
        }
        drop(input);
        if let Some(process) = process {
            let deadline = Instant::now() + STOP_TIMEOUT;
            loop {
                let exited = process.lock().unwrap().try_wait().unwrap_or(true);
                if exited {
                    break;
                }
                if Instant::now() >= deadline {
                    process.lock().unwrap().kill()?;
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        transition(&self.shared, VoiceState::Off, None);
        Ok(self.state())
    }

    pub fn send(&self, command: &Command) -> Result<(), String> {
        if command.fixture_only() {
            return Err("fixture-only voice command refused by supervisor".into());
        }
        let line = encode_line(command).map_err(|error| error.to_string())?;
        let mut inner = self.shared.inner.lock().unwrap();
        let input = inner
            .input
            .as_mut()
            .ok_or_else(|| "voice listener is not running".to_string())?;
        input
            .write_all(line.as_bytes())
            .and_then(|()| input.flush())
            .map_err(|error| format!("voice listener command failed: {error}"))
    }

    pub fn state(&self) -> VoiceStatus {
        self.shared.inner.lock().unwrap().status.clone()
    }

    pub fn speak(&self, task_id: TaskId, text: String) -> Result<SpokenLine, String> {
        let phrase_word_ends = mote_voice_macos::speak::phrases(&text)
            .into_iter()
            .scan(0_usize, |total, phrase| {
                *total += mote_voice_core::normalize(&phrase).len();
                Some(*total)
            })
            .collect::<Vec<_>>();
        if phrase_word_ends.is_empty() {
            return Err("speech line has no playable words".into());
        }
        let (line_id, line) = {
            let mut inner = self.shared.inner.lock().unwrap();
            if !inner.desired || inner.input.is_none() {
                return Err("voice listener is not running".into());
            }
            let line_id = inner.next_line_id;
            inner.next_line_id = inner.next_line_id.saturating_add(1);
            let line = SpokenLine::new(line_id, task_id, text.clone());
            inner.status.speaking_line = Some(visible_status_text(&text));
            inner.status.partial_words = None;
            inner.spoken = Some(SpokenTracker {
                line: line.clone(),
                phrase_word_ends,
                started_at: Instant::now(),
            });
            (line_id, line)
        };
        if self
            .shared
            .accounting
            .try_send(VoiceAccountingEvent::SpeechStarted {
                line_id,
                input_tokens: mote_context::count_tokens(&text) as i64,
            })
            .is_err()
        {
            self.shared.inner.lock().unwrap().spoken = None;
            self.shared.inner.lock().unwrap().status.speaking_line = None;
            return Err("voice accounting ingress unavailable".into());
        }
        if let Err(error) = self.send(&Command::Speak {
            line: line_id,
            text,
        }) {
            let spoken = self.shared.inner.lock().unwrap().spoken.take();
            if let Some(spoken) = spoken.filter(|spoken| spoken.line.line_id == line_id) {
                record_speech_completion(&self.shared, &spoken, VoiceOperationStatus::Failed);
            }
            self.shared.inner.lock().unwrap().status.speaking_line = None;
            return Err(error);
        }
        record_spoken_line(&self.shared, &line, "queued");
        Ok(line)
    }

    pub fn owner_turn_finished(&self, session_id: i64, utterance_id: u64, waiting: bool) {
        let generation = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.session_id != Some(session_id) || inner.active_utterance != Some(utterance_id)
            {
                return;
            }
            inner.active_utterance = None;
            inner.generation
        };
        transition_generation(
            &self.shared,
            generation,
            if waiting {
                VoiceState::Paused
            } else {
                VoiceState::Listening
            },
            None,
        );
    }

    pub fn owner_turn_failed(&self, session_id: i64, utterance_id: u64) {
        let generation = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.session_id != Some(session_id) || inner.active_utterance != Some(utterance_id)
            {
                return;
            }
            inner.active_utterance = None;
            inner.generation
        };
        transition_generation(
            &self.shared,
            generation,
            VoiceState::Error,
            Some("voice task failed".into()),
        );
    }
}

impl Drop for VoiceSupervisor {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn launch(shared: &Arc<Shared>, generation: u64) -> Result<(), String> {
    let running = shared.launcher.spawn(&shared.config)?;
    let process = Arc::new(Mutex::new(running.process));
    let accepted = {
        let mut inner = shared.inner.lock().unwrap();
        if !inner.desired || inner.generation != generation {
            false
        } else {
            inner.input = Some(running.input);
            inner.process = Some(Arc::clone(&process));
            true
        }
    };
    if !accepted {
        let _ = process.lock().unwrap().kill();
        return Ok(());
    }
    let shared = Arc::clone(shared);
    std::thread::spawn(move || monitor(shared, generation, running.output));
    Ok(())
}

fn monitor(shared: Arc<Shared>, generation: u64, output: Box<dyn Read + Send>) {
    let reader = BufReader::new(output);
    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => {
                fail(&shared, generation, "voice listener output failed");
                return;
            }
        };
        match decode_line(&line) {
            Ok(Frame::Event(event)) => apply_event(&shared, generation, event),
            _ => {
                fail(
                    &shared,
                    generation,
                    "voice listener emitted a malformed event",
                );
                return;
            }
        }
    }
    fail(&shared, generation, "voice listener exited");
}

fn apply_event(shared: &Arc<Shared>, generation: u64, event: Event) {
    if shared.inner.lock().unwrap().generation != generation {
        return;
    }
    match event {
        Event::Ready { .. } | Event::Resumed { .. } => {
            transition_generation(shared, generation, VoiceState::Listening, None)
        }
        Event::Gate { open: true, .. } => {
            transition_generation(shared, generation, VoiceState::Hearing, None)
        }
        Event::Words { text, .. } => {
            set_partial_words(shared, generation, Some(text));
            transition_generation(shared, generation, VoiceState::Hearing, None)
        }
        Event::Gate { open: false, .. } | Event::Dropped { .. } => {
            set_partial_words(shared, generation, None);
            transition_generation(shared, generation, VoiceState::Listening, None)
        }
        Event::Utterance {
            id,
            text,
            complete,
            start_ms,
            end_ms,
            ..
        } if complete => {
            set_partial_words(shared, generation, None);
            let turn = {
                let mut inner = shared.inner.lock().unwrap();
                if inner.generation != generation || !inner.desired {
                    return;
                }
                let Some(session_id) = inner.session_id else {
                    return;
                };
                if !inner.accepted_utterances.insert(id) {
                    return;
                }
                inner.active_utterance = Some(id);
                VoiceOwnerTurn {
                    session_id,
                    utterance_id: id,
                    text,
                    start_ms,
                    end_ms,
                    played_context: inner.last_spoken.clone(),
                }
            };
            let accounting = VoiceAccountingEvent::Recognized {
                session_id: turn.session_id,
                utterance_id: turn.utterance_id,
                output_tokens: mote_context::count_tokens(&turn.text) as i64,
                audio_ms: turn.end_ms.saturating_sub(turn.start_ms) as i64,
            };
            if shared.accounting.try_send(accounting).is_err() {
                let mut inner = shared.inner.lock().unwrap();
                inner.accepted_utterances.remove(&id);
                if inner.active_utterance == Some(id) {
                    inner.active_utterance = None;
                }
                drop(inner);
                transition_generation(
                    shared,
                    generation,
                    VoiceState::Error,
                    Some("voice accounting ingress unavailable".into()),
                );
                return;
            }
            if shared.owner_turns.try_send(turn).is_err() {
                let mut inner = shared.inner.lock().unwrap();
                inner.accepted_utterances.remove(&id);
                if inner.active_utterance == Some(id) {
                    inner.active_utterance = None;
                }
                drop(inner);
                transition_generation(
                    shared,
                    generation,
                    VoiceState::Error,
                    Some("voice task ingress unavailable".into()),
                );
                return;
            }
            transition_generation(shared, generation, VoiceState::Thinking, None)
        }
        Event::Utterance { .. } => {}
        Event::Barge { .. } => transition_generation(shared, generation, VoiceState::Hearing, None),
        Event::Spoke {
            line,
            event,
            phrase,
            ..
        } => apply_spoke(shared, generation, line, &event, phrase),
        Event::Paused { .. } => transition_generation(shared, generation, VoiceState::Paused, None),
        Event::Error { message, .. } => {
            finish_active_speech(shared, VoiceOperationStatus::Failed);
            set_partial_words(shared, generation, None);
            transition_generation(shared, generation, VoiceState::Error, Some(message))
        }
        Event::End { .. } | Event::Level { .. } => {}
    }
}

fn apply_spoke(shared: &Arc<Shared>, generation: u64, line_id: u64, event: &str, phrase: usize) {
    let (line, terminal, completed) = {
        let mut inner = shared.inner.lock().unwrap();
        if inner.generation != generation || !inner.desired {
            return;
        }
        let Some(spoken) = inner
            .spoken
            .as_mut()
            .filter(|spoken| spoken.line.line_id == line_id)
        else {
            return;
        };
        let Some(terminal) = update_spoken(spoken, event, phrase) else {
            return;
        };
        let line = spoken.line.clone();
        let completed = if terminal {
            inner.last_spoken = Some(line.clone());
            inner.status.speaking_line = None;
            inner.spoken.take()
        } else {
            None
        };
        (line, terminal, completed)
    };
    if let Some(spoken) = completed {
        let status = if event == "finished" {
            VoiceOperationStatus::Ok
        } else {
            VoiceOperationStatus::Stopped
        };
        record_speech_completion(shared, &spoken, status);
    }
    record_spoken_line(shared, &line, event);
    let state = match event {
        "stopped" => VoiceState::Hearing,
        "finished" => VoiceState::Listening,
        "lowered" => VoiceState::Hearing,
        "started" | "restored" | "phrase_finished" => VoiceState::Speaking,
        _ => return,
    };
    transition_generation(shared, generation, state, None);
    if terminal {
        let mut inner = shared.inner.lock().unwrap();
        inner.active_utterance = None;
    }
}

fn update_spoken(spoken: &mut SpokenTracker, event: &str, phrase: usize) -> Option<bool> {
    match event {
        "phrase_finished" => {
            if let Some(words) = spoken.phrase_word_ends.get(phrase).copied() {
                spoken.line.mark_played_through(words);
            }
        }
        "finished" => {
            let words = spoken.line.word_count();
            spoken.line.mark_played_through(words);
        }
        "started" | "lowered" | "restored" | "stopped" => {}
        _ => return None,
    }
    Some(matches!(event, "finished" | "stopped"))
}

fn record_spoken_line(shared: &Shared, line: &SpokenLine, event: &str) {
    let _ = shared.log.append(
        "voice",
        "voice.spoken_line",
        &json!({"event":event,"line":line}),
    );
}

fn record_speech_completion(shared: &Shared, spoken: &SpokenTracker, status: VoiceOperationStatus) {
    let latency_ms = spoken
        .started_at
        .elapsed()
        .as_millis()
        .min(i64::MAX as u128) as i64;
    let _ = shared
        .accounting
        .try_send(VoiceAccountingEvent::SpeechFinished {
            line_id: spoken.line.line_id,
            latency_ms,
            status,
        });
}

fn finish_active_speech(shared: &Shared, status: VoiceOperationStatus) {
    let spoken = {
        let mut inner = shared.inner.lock().unwrap();
        inner.status.speaking_line = None;
        inner.spoken.take()
    };
    if let Some(spoken) = spoken {
        record_speech_completion(shared, &spoken, status);
    }
}

fn set_partial_words(shared: &Shared, generation: u64, text: Option<String>) {
    let mut inner = shared.inner.lock().unwrap();
    if inner.generation == generation && inner.desired {
        inner.status.partial_words = text.map(|value| visible_status_text(&value));
    }
}

fn fail(shared: &Arc<Shared>, generation: u64, reason: &str) {
    finish_active_speech(shared, VoiceOperationStatus::Failed);
    let (restart, process, delay) = {
        let mut inner = shared.inner.lock().unwrap();
        if !inner.desired || inner.generation != generation {
            return;
        }
        inner.input = None;
        let process = inner.process.take();
        let now = Instant::now();
        while inner
            .failures
            .front()
            .is_some_and(|at| now.duration_since(*at) > RAPID_FAILURE_WINDOW)
        {
            inner.failures.pop_front();
        }
        inner.failures.push_back(now);
        let restart = inner.failures.len() < MAX_RAPID_FAILURES;
        let delay = Duration::from_millis(250 * inner.failures.len() as u64);
        (restart, process, delay)
    };
    if let Some(process) = process {
        let _ = process.lock().unwrap().kill();
    }
    let state = if restart {
        VoiceState::Error
    } else {
        VoiceState::Unavailable
    };
    transition_generation(shared, generation, state, Some(reason.to_string()));
    if !restart {
        let mut inner = shared.inner.lock().unwrap();
        if inner.generation == generation {
            inner.desired = false;
        }
    }
    if restart {
        let shared = Arc::clone(shared);
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            if !shared.inner.lock().unwrap().desired {
                return;
            }
            let generation = shared.inner.lock().unwrap().generation;
            transition_generation(&shared, generation, VoiceState::Starting, None);
            if let Err(error) = launch(&shared, generation) {
                fail(&shared, generation, &error);
            }
        });
    }
}

fn transition(shared: &Shared, state: VoiceState, reason: Option<String>) {
    let reason = reason.map(|value| visible_status_text(&value));
    let status = {
        let mut inner = shared.inner.lock().unwrap();
        inner.status.state = state;
        inner.status.reason = reason.clone();
        inner.status.clone()
    };
    let _ = shared.log.append(
        "voice",
        "voice.state",
        &serde_json::to_value(status).unwrap_or_default(),
    );
}

fn transition_generation(
    shared: &Shared,
    generation: u64,
    state: VoiceState,
    reason: Option<String>,
) {
    let reason = reason.map(|value| visible_status_text(&value));
    let status = {
        let mut inner = shared.inner.lock().unwrap();
        if !inner.desired || inner.generation != generation {
            return;
        }
        inner.status.state = state;
        inner.status.reason = reason.clone();
        inner.status.clone()
    };
    let _ = shared.log.append(
        "voice",
        "voice.state",
        &serde_json::to_value(status).unwrap_or_default(),
    );
}

fn visible_status_text(value: &str) -> String {
    mote_redact::redact(value)
        .chars()
        .take(VISIBLE_STATUS_TEXT_LIMIT)
        .collect()
}

pub fn default_listener_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join("mote-voice-listener"))
        })
        .unwrap_or_else(|| PathBuf::from("mote-voice-listener"))
}

pub fn default_model_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("models").join("ggml-base.en.bin")
}

#[cfg(test)]
mod spoken_line_tests {
    use super::*;

    fn tracker() -> SpokenTracker {
        SpokenTracker {
            line: SpokenLine::new(
                8,
                TaskId::new("voice-1-1").unwrap(),
                "First phrase, second phrase here.".into(),
            ),
            phrase_word_ends: vec![2, 5],
            started_at: Instant::now(),
        }
    }

    #[test]
    fn interruption_keeps_only_completed_phrases() {
        let mut spoken = tracker();
        assert_eq!(update_spoken(&mut spoken, "started", 0), Some(false));
        assert_eq!(
            update_spoken(&mut spoken, "phrase_finished", 0),
            Some(false)
        );
        assert_eq!(update_spoken(&mut spoken, "stopped", 1), Some(true));
        assert_eq!(spoken.line.played_through_word, 2);
        assert_eq!(spoken.line.played_text(), "First phrase,");
    }

    #[test]
    fn finished_line_marks_every_word_played() {
        let mut spoken = tracker();
        assert_eq!(update_spoken(&mut spoken, "finished", 1), Some(true));
        assert_eq!(spoken.line.played_through_word, 5);
    }
}
