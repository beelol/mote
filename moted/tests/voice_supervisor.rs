#[path = "../src/voice.rs"]
#[allow(dead_code)]
mod voice;

use mote_eventlog::EventLog;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use voice::{
    ListenerLauncher, ListenerProcess, ProcessListenerLauncher, RunningListener, VoiceConfig,
    VoicePermission, VoiceState, VoiceSupervisor,
};

struct Permission(Result<(), String>);

impl VoicePermission for Permission {
    fn check(&self) -> Result<(), String> {
        self.0.clone()
    }
}

#[derive(Clone)]
struct Script {
    output: Vec<u8>,
    hold: bool,
}

struct FakeLauncher {
    scripts: Mutex<VecDeque<Script>>,
    spawns: AtomicUsize,
}

impl FakeLauncher {
    fn new(scripts: impl IntoIterator<Item = Script>) -> Self {
        Self {
            scripts: Mutex::new(scripts.into_iter().collect()),
            spawns: AtomicUsize::new(0),
        }
    }
}

impl ListenerLauncher for FakeLauncher {
    fn spawn(&self, _config: &VoiceConfig) -> Result<RunningListener, String> {
        self.spawns.fetch_add(1, Ordering::SeqCst);
        let script = self
            .scripts
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(Script {
                output: Vec::new(),
                hold: false,
            });
        let killed = Arc::new(AtomicBool::new(false));
        Ok(RunningListener {
            input: Box::new(Sink),
            output: Box::new(ScriptedOutput {
                bytes: script.output,
                offset: 0,
                hold: script.hold,
                killed: Arc::clone(&killed),
            }),
            process: Box::new(FakeProcess { killed }),
        })
    }
}

struct Sink;

impl Write for Sink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CaptureWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct CapturingLauncher {
    input: Arc<Mutex<Vec<u8>>>,
}

impl ListenerLauncher for CapturingLauncher {
    fn spawn(&self, _config: &VoiceConfig) -> Result<RunningListener, String> {
        let killed = Arc::new(AtomicBool::new(false));
        Ok(RunningListener {
            input: Box::new(CaptureWriter(self.input.clone())),
            output: Box::new(ScriptedOutput {
                bytes: ready(true).output,
                offset: 0,
                hold: true,
                killed: killed.clone(),
            }),
            process: Box::new(FakeProcess { killed }),
        })
    }
}

struct ScriptedOutput {
    bytes: Vec<u8>,
    offset: usize,
    hold: bool,
    killed: Arc<AtomicBool>,
}

impl Read for ScriptedOutput {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.offset < self.bytes.len() {
            let count = output.len().min(self.bytes.len() - self.offset);
            output[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
            self.offset += count;
            return Ok(count);
        }
        while self.hold && !self.killed.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(0)
    }
}

struct FakeProcess {
    killed: Arc<AtomicBool>,
}

struct BlockingLauncher {
    entered: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    killed: Arc<AtomicBool>,
}

impl ListenerLauncher for BlockingLauncher {
    fn spawn(&self, _config: &VoiceConfig) -> Result<RunningListener, String> {
        self.entered.send(()).unwrap();
        self.release.lock().unwrap().recv().unwrap();
        Ok(RunningListener {
            input: Box::new(Sink),
            output: Box::new(ScriptedOutput {
                bytes: ready(true).output,
                offset: 0,
                hold: true,
                killed: Arc::clone(&self.killed),
            }),
            process: Box::new(FakeProcess {
                killed: Arc::clone(&self.killed),
            }),
        })
    }
}

impl ListenerProcess for FakeProcess {
    fn try_wait(&mut self) -> Result<bool, String> {
        Ok(self.killed.load(Ordering::SeqCst))
    }

    fn kill(&mut self) -> Result<(), String> {
        self.killed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

fn config() -> VoiceConfig {
    VoiceConfig {
        listener_path: PathBuf::from("unused"),
        input: "mic".into(),
        model_path: PathBuf::from("/local/model.bin"),
    }
}

fn ready(hold: bool) -> Script {
    Script {
        output: br#"{"type":"ready","input":"mic","model":"local.bin","t_ms":0}
"#
        .to_vec(),
        hold,
    }
}

fn events(lines: &[&str], hold: bool) -> Script {
    Script {
        output: format!("{}\n", lines.join("\n")).into_bytes(),
        hold,
    }
}

fn wait_for(supervisor: &VoiceSupervisor, expected: VoiceState) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if supervisor.state().state == expected {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "voice state never became {expected:?}; was {:?}",
        supervisor.state()
    );
}

#[test]
fn controls_are_available_without_starting_capture() {
    let launcher = Arc::new(FakeLauncher::new([]));
    let supervisor = VoiceSupervisor::with_components(
        config(),
        EventLog::open_in_memory().unwrap(),
        launcher.clone(),
        Arc::new(Permission(Ok(()))),
    );

    assert_eq!(supervisor.state().state, VoiceState::Off);
    assert_eq!(launcher.spawns.load(Ordering::SeqCst), 0);
}

#[test]
fn permission_denial_never_starts_capture_and_keeps_typed_events_healthy() {
    let log = EventLog::open_in_memory().unwrap();
    let launcher = Arc::new(FakeLauncher::new([]));
    let supervisor = VoiceSupervisor::with_components(
        config(),
        log.clone(),
        launcher.clone(),
        Arc::new(Permission(Err(
            "permission denied for sk-abcdefghijklmnopqrstuvwxyz".into(),
        ))),
    );

    assert!(supervisor.start().is_err());
    assert_eq!(supervisor.state().state, VoiceState::Unavailable);
    assert_eq!(supervisor.state().reason.as_deref(), Some("permission denied for [redacted]"));
    assert_eq!(launcher.spawns.load(Ordering::SeqCst), 0);

    log.append("main", "turn.user", &serde_json::json!({"text":"typed still works"}))
        .unwrap();
    assert_eq!(
        log.recent_kind_since("turn.user", i64::MIN, 1).unwrap()[0].payload["text"],
        "typed still works"
    );
}

#[test]
fn explicit_start_reaches_listening_and_clean_stop_returns_off() {
    let launcher = Arc::new(FakeLauncher::new([ready(true)]));
    let supervisor = VoiceSupervisor::with_components(
        config(),
        EventLog::open_in_memory().unwrap(),
        launcher.clone(),
        Arc::new(Permission(Ok(()))),
    );

    supervisor.start().unwrap();
    wait_for(&supervisor, VoiceState::Listening);
    supervisor.stop().unwrap();
    assert_eq!(supervisor.state().state, VoiceState::Off);
    assert_eq!(launcher.spawns.load(Ordering::SeqCst), 1);
}

#[test]
fn completed_task_reply_queues_a_numbered_listener_speech_command() {
    let input = Arc::new(Mutex::new(Vec::new()));
    let supervisor = VoiceSupervisor::with_components(
        config(),
        EventLog::open_in_memory().unwrap(),
        Arc::new(CapturingLauncher {
            input: input.clone(),
        }),
        Arc::new(Permission(Ok(()))),
    );
    supervisor.start().unwrap();
    wait_for(&supervisor, VoiceState::Listening);
    let spoken = supervisor
        .speak(
            mote_hrns::TaskId::new("voice-task").unwrap(),
            "Speak this locally.".into(),
        )
        .unwrap();
    assert_eq!(spoken.line_id, 1);
    let commands = String::from_utf8(input.lock().unwrap().clone()).unwrap();
    assert!(commands.contains(
        r#"{"cmd":"speak","line":1,"text":"Speak this locally."}"#
    ));
    supervisor.stop().unwrap();
}

#[test]
fn final_utterance_routes_once_across_listener_reconnect_and_partials_do_not_route() {
    let fixture = [
        r#"{"type":"ready","input":"mic","model":"local.bin","t_ms":0}"#,
        r#"{"type":"level","source":"owner","value":0.5,"t_ms":10}"#,
        r#"{"type":"words","id":9,"text":"hel","t_ms":20}"#,
        r#"{"type":"utterance","id":9,"text":"ignored partial","complete":false,"start_ms":10,"end_ms":20,"t_ms":20}"#,
        r#"{"type":"utterance","id":9,"text":"hello mote","complete":true,"start_ms":10,"end_ms":30,"t_ms":30}"#,
    ];
    let launcher = Arc::new(FakeLauncher::new([
        events(&fixture, false),
        events(&fixture, true),
    ]));
    let supervisor = VoiceSupervisor::with_components(
        config(),
        EventLog::open_in_memory().unwrap(),
        launcher,
        Arc::new(Permission(Ok(()))),
    );
    let turns = supervisor.take_owner_turns().unwrap();

    supervisor.start().unwrap();
    let turn = turns.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(turn.utterance_id, 9);
    assert_eq!(turn.text, "hello mote");
    assert_eq!(turn.start_ms, 10);
    assert_eq!(turn.end_ms, 30);
    assert!(turns.recv_timeout(Duration::from_millis(900)).is_err());
    supervisor.stop().unwrap();
}

#[test]
fn concurrent_start_calls_spawn_only_one_listener() {
    let launcher = Arc::new(FakeLauncher::new([ready(true)]));
    let supervisor = Arc::new(VoiceSupervisor::with_components(
        config(),
        EventLog::open_in_memory().unwrap(),
        launcher.clone(),
        Arc::new(Permission(Ok(()))),
    ));
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let threads = (0..2)
        .map(|_| {
            let supervisor = Arc::clone(&supervisor);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                supervisor.start()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for thread in threads {
        thread.join().unwrap().unwrap();
    }

    wait_for(&supervisor, VoiceState::Listening);
    assert_eq!(launcher.spawns.load(Ordering::SeqCst), 1);
}

#[test]
fn stop_wins_while_a_listener_spawn_is_in_flight() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let killed = Arc::new(AtomicBool::new(false));
    let launcher = Arc::new(BlockingLauncher {
        entered: entered_tx,
        release: Mutex::new(release_rx),
        killed: Arc::clone(&killed),
    });
    let supervisor = Arc::new(VoiceSupervisor::with_components(
        config(),
        EventLog::open_in_memory().unwrap(),
        launcher,
        Arc::new(Permission(Ok(()))),
    ));
    let starting = {
        let supervisor = Arc::clone(&supervisor);
        std::thread::spawn(move || supervisor.start())
    };
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    supervisor.stop().unwrap();
    release_tx.send(()).unwrap();
    starting.join().unwrap().unwrap();

    wait_for(&supervisor, VoiceState::Off);
    assert!(killed.load(Ordering::SeqCst));
}

#[test]
fn production_launcher_refuses_missing_local_files_without_leaking_paths() {
    let launcher = ProcessListenerLauncher;
    let missing_listener = VoiceConfig {
        listener_path: PathBuf::from("/private/no-such-mote-listener"),
        input: "mic".into(),
        model_path: PathBuf::from("/private/no-such-mote-model"),
    };
    let listener_error = launcher.spawn(&missing_listener).err().unwrap();
    assert_eq!(listener_error, "voice listener executable is not installed");
    assert!(!listener_error.contains("/private"));

    let missing_model = VoiceConfig {
        listener_path: std::env::current_exe().unwrap(),
        input: "mic".into(),
        model_path: PathBuf::from("/private/no-such-mote-model"),
    };
    let model_error = launcher.spawn(&missing_model).err().unwrap();
    assert_eq!(model_error, "local voice model is not installed");
    assert!(!model_error.contains("/private"));
}

#[test]
fn malformed_output_is_bounded_and_becomes_unavailable() {
    let bad = Script {
        output: b"not json\n".to_vec(),
        hold: false,
    };
    let launcher = Arc::new(FakeLauncher::new([bad.clone(), bad.clone(), bad]));
    let supervisor = VoiceSupervisor::with_components(
        config(),
        EventLog::open_in_memory().unwrap(),
        launcher.clone(),
        Arc::new(Permission(Ok(()))),
    );

    supervisor.start().unwrap();
    wait_for(&supervisor, VoiceState::Unavailable);
    assert_eq!(launcher.spawns.load(Ordering::SeqCst), 3);
    assert_eq!(
        supervisor.state().reason.as_deref(),
        Some("voice listener emitted a malformed event")
    );
}

#[test]
fn rapid_child_crashes_stop_after_the_bounded_restart_budget() {
    let crash = Script {
        output: Vec::new(),
        hold: false,
    };
    let launcher = Arc::new(FakeLauncher::new([
        crash.clone(),
        crash.clone(),
        crash,
    ]));
    let supervisor = VoiceSupervisor::with_components(
        config(),
        EventLog::open_in_memory().unwrap(),
        launcher.clone(),
        Arc::new(Permission(Ok(()))),
    );

    supervisor.start().unwrap();
    wait_for(&supervisor, VoiceState::Unavailable);
    assert_eq!(launcher.spawns.load(Ordering::SeqCst), 3);
    assert_eq!(supervisor.state().reason.as_deref(), Some("voice listener exited"));
}
