use mote_eventlog::EventLog;
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceOwnerTurn {
    pub session_id: i64,
    pub utterance_id: u64,
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
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
}

struct Shared {
    config: VoiceConfig,
    log: EventLog,
    launcher: Arc<dyn ListenerLauncher>,
    permission: Arc<dyn VoicePermission>,
    owner_turns: mpsc::SyncSender<VoiceOwnerTurn>,
    inner: Mutex<Inner>,
}

pub struct VoiceSupervisor {
    shared: Arc<Shared>,
    owner_turns: Mutex<Option<mpsc::Receiver<VoiceOwnerTurn>>>,
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
        Self {
            shared: Arc::new(Shared {
                config,
                log,
                launcher,
                permission,
                owner_turns,
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
                }),
            }),
            owner_turns: Mutex::new(Some(owner_turn_receiver)),
        }
    }

    pub fn take_owner_turns(&self) -> Option<mpsc::Receiver<VoiceOwnerTurn>> {
        self.owner_turns.lock().unwrap().take()
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
        }
        transition_generation(
            &self.shared,
            generation,
            VoiceState::Starting,
            None,
        );
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
        let (mut input, process) = {
            let mut inner = self.shared.inner.lock().unwrap();
            inner.desired = false;
            inner.generation = inner.generation.wrapping_add(1);
            inner.session_id = None;
            inner.active_utterance = None;
            (inner.input.take(), inner.process.take())
        };
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

    pub fn owner_turn_finished(
        &self,
        session_id: i64,
        utterance_id: u64,
        waiting: bool,
    ) {
        let generation = {
            let mut inner = self.shared.inner.lock().unwrap();
            if inner.session_id != Some(session_id)
                || inner.active_utterance != Some(utterance_id)
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
            if inner.session_id != Some(session_id)
                || inner.active_utterance != Some(utterance_id)
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
                fail(&shared, generation, "voice listener emitted a malformed event");
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
        Event::Gate { open: true, .. } | Event::Words { .. } => {
            transition_generation(shared, generation, VoiceState::Hearing, None)
        }
        Event::Gate { open: false, .. } | Event::Dropped { .. } => {
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
                }
            };
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
        Event::Barge { .. } => {
            transition_generation(shared, generation, VoiceState::Hearing, None)
        }
        Event::Spoke { .. } => {
            transition_generation(shared, generation, VoiceState::Speaking, None)
        }
        Event::Paused { .. } => {
            transition_generation(shared, generation, VoiceState::Paused, None)
        }
        Event::Error { message, .. } => {
            transition_generation(shared, generation, VoiceState::Error, Some(message))
        }
        Event::End { .. } | Event::Level { .. } => {}
    }
}

fn fail(shared: &Arc<Shared>, generation: u64, reason: &str) {
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
    let reason = reason.map(|value| mote_redact::redact(&value));
    {
        let mut inner = shared.inner.lock().unwrap();
        inner.status = VoiceStatus {
            state,
            reason: reason.clone(),
        };
    }
    let _ = shared.log.append(
        "voice",
        "voice.state",
        &json!({"state":state,"reason":reason}),
    );
}

fn transition_generation(
    shared: &Shared,
    generation: u64,
    state: VoiceState,
    reason: Option<String>,
) {
    let reason = reason.map(|value| mote_redact::redact(&value));
    {
        let mut inner = shared.inner.lock().unwrap();
        if !inner.desired || inner.generation != generation {
            return;
        }
        inner.status = VoiceStatus {
            state,
            reason: reason.clone(),
        };
    }
    let _ = shared.log.append(
        "voice",
        "voice.state",
        &json!({"state":state,"reason":reason}),
    );
}

pub fn default_listener_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("mote-voice-listener")))
        .unwrap_or_else(|| PathBuf::from("mote-voice-listener"))
}

pub fn default_model_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("models").join("ggml-base.en.bin")
}
