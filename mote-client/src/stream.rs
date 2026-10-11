use mote_eventlog::Event;
use mote_ipc::Client;
use std::io;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

const MIN_RETRY: Duration = Duration::from_millis(20);
const MAX_RETRY: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct UnixHostSession {
    path: std::path::PathBuf,
    directory: std::sync::Arc<std::sync::Mutex<Option<mote_host_client::HostDirectory>>>,
}

impl UnixHostSession {
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            directory: Default::default(),
        }
    }

    pub fn call(&self, method: &str, params: serde_json::Value) -> Result<(), String> {
        let transport = self.adapter().map_err(|error| error.message)?;
        transport.call(method, params, |update| {
            if let Ok(mut selected) = self.directory.lock() {
                if let Some(directory) = selected.as_mut() {
                    let _ = directory.reduce_update(update.clone());
                }
            }
        })
    }

    fn adapter(&self) -> Result<UnixHostAdapter, mote_protocol::HostError> {
        let client = self.verified_client()?;
        drop(client);
        let directory = self.directory.lock().map_err(host_error)?;
        let host_id = directory
            .as_ref()
            .and_then(|directory| directory.selected_host())
            .cloned()
            .ok_or_else(|| host_error("local host is not selected"))?;
        Ok(UnixHostAdapter {
            host_id,
            path: self.path.clone(),
        })
    }

    pub fn verified_client(&self) -> Result<Client, mote_protocol::HostError> {
        let mut client = Client::connect(&self.path).map_err(host_error)?;
        let hello = client
            .call("hello", serde_json::json!({}))
            .map_err(host_error)?
            .map_err(|error| host_error(error.message))?;
        let current: mote_protocol::HostId =
            serde_json::from_value(hello["host_id"].clone()).map_err(host_error)?;
        let mut selected = self.directory.lock().map_err(host_error)?;
        if let Some(directory) = selected.as_mut() {
            let id = directory
                .selected_host()
                .cloned()
                .ok_or_else(|| host_error("local host is not selected"))?;
            if id != current {
                let error = mote_protocol::HostError::new(
                    mote_protocol::HostErrorCode::IdentityChanged,
                    "Unix host identity changed",
                );
                let _ = directory.reduce_update(mote_host_client::HostUpdate::Failed {
                    host_id: id,
                    error: error.clone(),
                });
                return Err(error);
            }
        } else {
            *selected = Some(mote_host_client::HostDirectory::trusted_local_projection(
                current,
            ));
        }
        Ok(client)
    }

    fn project(&self, update: &StreamUpdate) {
        use mote_host_client::{ConnectionState, HostUpdate};
        use mote_protocol::HostFrame;
        let Ok(mut selected) = self.directory.lock() else {
            return;
        };
        let Some(directory) = selected.as_mut() else {
            return;
        };
        let Some(id) = directory.selected_host().cloned() else {
            return;
        };
        let update = match update {
            StreamUpdate::Event { event, .. } => HostUpdate::Frame {
                host_id: id.clone(),
                frame: HostFrame::Event {
                    host_id: id,
                    event: event.clone(),
                },
            },
            StreamUpdate::Connected { .. } => HostUpdate::Connection {
                host_id: id,
                state: ConnectionState::Online,
            },
            StreamUpdate::Disconnected => {
                if directory
                    .state(&id)
                    .is_ok_and(|state| state.connection() == &ConnectionState::IdentityChanged)
                {
                    return;
                }
                HostUpdate::Connection {
                    host_id: id,
                    state: ConnectionState::Offline,
                }
            }
        };
        let _ = directory.reduce_update(update);
    }
}

pub struct UnixHostAdapter {
    host_id: mote_protocol::HostId,
    path: std::path::PathBuf,
}

fn host_error(error: impl std::fmt::Display) -> mote_protocol::HostError {
    mote_protocol::HostError::new(mote_protocol::HostErrorCode::Internal, error.to_string())
}

impl UnixHostAdapter {
    fn verified_client(&self) -> Result<Client, mote_protocol::HostError> {
        let mut client = Client::connect(&self.path).map_err(host_error)?;
        let hello = client.call("hello", serde_json::json!({})).map_err(host_error)?.map_err(|error| host_error(error.message))?;
        let current: mote_protocol::HostId = serde_json::from_value(hello["host_id"].clone()).map_err(host_error)?;
        if current != self.host_id { return Err(mote_protocol::HostError::new(mote_protocol::HostErrorCode::IdentityChanged, "Unix host identity changed")); }
        Ok(client)
    }
    #[cfg(test)]
    pub fn connect(path: &Path) -> Result<Self, mote_protocol::HostError> {
        let mut client = Client::connect(path).map_err(host_error)?;
        let hello = client.call("hello", serde_json::json!({})).map_err(host_error)?.map_err(|error| host_error(error.message))?;
        let host_id = serde_json::from_value(hello["host_id"].clone()).map_err(host_error)?;
        Ok(Self { host_id, path: path.to_owned() })
    }

    pub fn dispatch(self, frame: mote_protocol::ClientFrame) -> Result<std::sync::mpsc::Receiver<mote_host_client::HostUpdate>, mote_protocol::HostError> {
        let command = mote_host_client::HostCommand { host_id: self.host_id.clone(), frame };
        let (commands, requests) = std::sync::mpsc::channel();
        let (updates, replies) = std::sync::mpsc::channel();
        mote_host_client::command_worker(self, requests, updates, || {});
        commands.send(command).map_err(host_error)?;
        Ok(replies)
    }

    fn call(self, method: &str, params: serde_json::Value, mut observe: impl FnMut(&mote_host_client::HostUpdate)) -> Result<(), String> {
        let id = mote_protocol::RequestId::new(format!(
            "local-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
        .map_err(|error| error.message)?;
        let replies = self
            .dispatch(mote_protocol::ClientFrame::Call {
                request_id: id,
                method: method.into(),
                params,
            })
            .map_err(|error| error.message)?;
        let mut outcome = Ok(());
        for update in replies {
            observe(&update);
            match update {
                mote_host_client::HostUpdate::Failed { error, .. } => return Err(error.message),
                mote_host_client::HostUpdate::Frame { frame: mote_protocol::HostFrame::Error { error, .. }, .. } => outcome = Err(error.message),
                mote_host_client::HostUpdate::Frame { frame: mote_protocol::HostFrame::Acknowledgement { .. }, .. } => return outcome,
                _ => {}
            }
        }
        Err("host command ended without acknowledgement".into())
    }
}

pub struct UnixHostStream {
    host_id: mote_protocol::HostId,
    stream: mote_ipc::EventStream,
}

impl mote_host_client::HostEventStream for UnixHostStream {
    fn next(&mut self, host: &mote_protocol::HostId) -> Result<Option<mote_protocol::HostFrame>, mote_protocol::HostError> {
        if host != &self.host_id { return Err(mote_protocol::HostError::new(mote_protocol::HostErrorCode::IdentityChanged, "stream belongs to another host")); }
        self.stream.read_event().map(|event| event.map(|event| mote_protocol::HostFrame::Event { host_id: host.clone(), event })).map_err(host_error)
    }
}

impl mote_host_client::HostRequestTransport for UnixHostAdapter {
    fn request(&mut self, command: mote_host_client::HostCommand) -> Result<Vec<mote_protocol::HostFrame>, mote_protocol::HostError> {
        use mote_protocol::{ClientFrame, HostError, HostErrorCode, HostFrame};
        if command.host_id != self.host_id { return Err(HostError::new(HostErrorCode::IdentityChanged, "command belongs to another host")); }
        let (request_id, method, params) = match command.frame {
            ClientFrame::Call { request_id, method, params } => (request_id, method, params),
            ClientFrame::Interaction { request_id, interaction } if interaction.host_id == self.host_id => (request_id, "ui.event".into(), serde_json::json!({"thread":interaction.thread,"surface_seq":interaction.surface_seq,"surface_contract":interaction.surface_contract,"node_path":interaction.node_path,"event":interaction.event,"args":interaction.args})),
            _ => return Err(HostError::new(HostErrorCode::InvalidRequest, "unsupported local command")),
        };
        let mut client = self.verified_client()?;
        let result = client.call(&method, params).map_err(host_error)?;
        let frame = match result {
            Ok(result) => HostFrame::Result { request_id: request_id.clone(), result },
            Err(error) => HostFrame::Error { request_id: Some(request_id.clone()), error: host_error(error.message) },
        };
        Ok(vec![frame, HostFrame::Acknowledgement { host_id: self.host_id.clone(), request_id, cursor: mote_protocol::Cursor::new(self.host_id.clone(), 0)? }])
    }
}

impl mote_host_client::HostTransport for UnixHostAdapter {
    type EventStream = UnixHostStream;
    fn send(&mut self, host: &mote_protocol::HostId, frame: &mote_protocol::ClientFrame) -> Result<(), mote_protocol::HostError> {
        use mote_host_client::HostRequestTransport;
        self.request(mote_host_client::HostCommand { host_id: host.clone(), frame: frame.clone() }).map(|_| ())
    }
    fn subscribe(&mut self, host: &mote_protocol::HostId, cursor: &mote_protocol::Cursor) -> Result<Self::EventStream, mote_protocol::HostError> {
        if host != &self.host_id || &cursor.host_id != host { return Err(mote_protocol::HostError::new(mote_protocol::HostErrorCode::IdentityChanged, "cursor belongs to another host")); }
        let stream = self.verified_client()?.subscribe("main", cursor.seq).map_err(host_error)?;
        Ok(UnixHostStream { host_id: host.clone(), stream })
    }
}

pub enum StreamUpdate {
    Event { event: Event, arrived_ns: u64 },
    Connected { replay_through: i64 },
    Disconnected,
}

impl From<Event> for StreamUpdate {
    fn from(event: Event) -> Self {
        Self::timed(event, 0)
    }
}

impl StreamUpdate {
    pub fn timed(event: Event, arrived_ns: u64) -> Self {
        Self::Event { event, arrived_ns }
    }
}

#[cfg(test)]
pub fn run(
    path: &Path,
    scope_id: &str,
    cursor: i64,
    tx: Sender<StreamUpdate>,
    clock: crate::display_latency::SharedClock,
    repaint: impl FnMut(),
) {
    run_after_connect(path, scope_id, cursor, tx, clock, repaint, || {});
}

#[cfg(test)]
pub fn run_after_connect(
    path: &Path,
    scope_id: &str,
    cursor: i64,
    tx: Sender<StreamUpdate>,
    clock: crate::display_latency::SharedClock,
    repaint: impl FnMut(),
    on_connected: impl FnOnce(),
) {
    run_session(
        None,
        path,
        scope_id,
        cursor,
        tx,
        clock,
        repaint,
        on_connected,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn run_pinned(
    host: UnixHostSession,
    path: &Path,
    scope_id: &str,
    cursor: i64,
    tx: Sender<StreamUpdate>,
    clock: crate::display_latency::SharedClock,
    repaint: impl FnMut(),
    on_connected: impl FnOnce(),
) {
    run_session(
        Some(host),
        path,
        scope_id,
        cursor,
        tx,
        clock,
        repaint,
        on_connected,
    );
}

#[allow(clippy::too_many_arguments)]
fn run_session(
    host: Option<UnixHostSession>,
    path: &Path,
    scope_id: &str,
    cursor: i64,
    tx: Sender<StreamUpdate>,
    clock: crate::display_latency::SharedClock,
    repaint: impl FnMut(),
    on_connected: impl FnOnce(),
) {
    let mut on_connected = Some(on_connected);
    run_with(
        cursor,
        |cursor| {
            let mut client = if let Some(host) = &host {
                host.verified_client()
                    .map_err(|error| io::Error::other(error.message))?
            } else {
                Client::connect(path)?
            };
            let status = client
                .call("status", serde_json::json!({}))?
                .map_err(|error| io::Error::other(error.message))?;
            let replay_through = status["last_seq"].as_i64().filter(|seq| *seq >= 0)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "status has no valid last_seq"))?;
            Ok((client.subscribe(scope_id, cursor)?, replay_through))
        },
        |stream| {
            if let Some(on_connected) = on_connected.take() {
                on_connected();
            }
            stream.read_event()
        },
        || clock.now_ns(),
        |event| {
            if let Some(host) = &host {
                host.project(&event);
            }
            tx.send(event).is_ok()
        },
        repaint,
        std::thread::sleep,
    );
}

pub fn run_benchmark(
    host: &UnixHostSession,
    tx: Sender<StreamUpdate>,
    clock: crate::display_latency::SharedClock,
    repaint: impl FnMut(),
) {
    run_with(
        0,
        |cursor| {
            let client = host
                .verified_client()
                .map_err(|error| io::Error::other(error.message))?;
            Ok((client.subscribe("bench.talk", cursor)?, 0))
        },
        |stream| stream.read_event(),
        || clock.now_ns(),
        |update| match update {
            StreamUpdate::Event { .. } => tx.send(update).is_ok(),
            StreamUpdate::Connected { .. } | StreamUpdate::Disconnected => true,
        },
        repaint,
        std::thread::sleep,
    );
}

fn run_with<S>(
    initial_cursor: i64,
    mut connect: impl FnMut(i64) -> io::Result<(S, i64)>,
    mut read: impl FnMut(&mut S) -> io::Result<Option<Event>>,
    mut now_ns: impl FnMut() -> u64,
    mut deliver: impl FnMut(StreamUpdate) -> bool,
    mut wake: impl FnMut(),
    mut wait: impl FnMut(Duration),
) {
    let mut cursor = initial_cursor;
    let mut retry = MIN_RETRY;

    loop {
        let (mut source, replay_through) = match connect(cursor) {
            Ok(connection) => connection,
            Err(_) => {
                wait(retry);
                retry = (retry * 2).min(MAX_RETRY);
                continue;
            }
        };
        if !deliver_and_wake(StreamUpdate::Connected { replay_through }, &mut deliver, &mut wake) {
            return;
        }

        loop {
            match read(&mut source) {
                Ok(None) => {}
                Ok(Some(event)) => {
                    let arrived_ns = now_ns();
                    if event.kind == "resync" {
                        if !deliver_and_wake(StreamUpdate::Disconnected, &mut deliver, &mut wake) {
                            return;
                        }
                        break;
                    }
                    if event.seq <= cursor {
                        continue;
                    }
                    let delivered_cursor = event.seq;
                    if !deliver_and_wake(
                        StreamUpdate::timed(event, arrived_ns), &mut deliver, &mut wake,
                    ) {
                        return;
                    }
                    cursor = delivered_cursor;
                    retry = MIN_RETRY;
                }
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {}
                Err(_) => {
                    if !deliver_and_wake(StreamUpdate::Disconnected, &mut deliver, &mut wake) {
                        return;
                    }
                    wait(retry);
                    retry = (retry * 2).min(MAX_RETRY);
                    break;
                }
            }
        }
    }
}

fn deliver_and_wake(
    update: StreamUpdate,
    mut deliver: impl FnMut(StreamUpdate) -> bool,
    mut wake: impl FnMut(),
) -> bool {
    let delivered = deliver(update);
    if delivered {
        wake();
    }
    delivered
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_eventlog::Event;
    use std::collections::VecDeque;
    use std::io;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn shell_session_rejects_socket_replacement_between_actions() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Dispatch(mote_protocol::HostId, Arc<AtomicUsize>);
        impl mote_ipc::Dispatch for Dispatch {
            fn call(
                &self,
                method: &str,
                _: serde_json::Value,
            ) -> Result<serde_json::Value, mote_ipc::RpcError> {
                if method == "hello" {
                    return Ok(serde_json::json!({"host_id":self.0}));
                }
                self.1.fetch_add(1, Ordering::SeqCst);
                Ok(serde_json::json!({"ok":true}))
            }
        }
        let path = std::env::temp_dir().join(format!("mote-shell-pin-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        let first = mote_protocol::HostId::from_public_key(&[41; 32]);
        let second = mote_protocol::HostId::from_public_key(&[42; 32]);
        let effects = Arc::new(AtomicUsize::new(0));
        let server = mote_ipc::Server::bind(
            &path.join("host.sock"),
            Arc::new(Dispatch(first, effects.clone())),
        )
        .unwrap();
        let session = UnixHostSession::new(server.path());
        session.call("turn", serde_json::json!({})).unwrap();
        assert_eq!(effects.load(Ordering::SeqCst), 1);
        drop(server);
        let replacement = mote_ipc::Server::bind(
            &path.join("host.sock"),
            Arc::new(Dispatch(second, effects.clone())),
        )
        .unwrap();
        assert!(session
            .call("ui.event", serde_json::json!({}))
            .unwrap_err()
            .contains("identity changed"));
        assert_eq!(effects.load(Ordering::SeqCst), 1);
        let directory = session.directory.lock().unwrap();
        let directory = directory.as_ref().unwrap();
        let id = directory.selected_host().unwrap();
        assert_eq!(directory.state(id).unwrap().connection(), &mote_host_client::ConnectionState::IdentityChanged);
        assert!(serde_json::to_vec(directory).is_err());
        drop(replacement);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn pinned_shell_stream_projects_live_events_and_preserves_timestamps() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;
        let path = std::env::temp_dir().join(format!("mote-shell-stream-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        let socket = path.join("host.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let id = mote_protocol::HostId::from_public_key(&[51;32]);
        let (finish, finish_rx) = std::sync::mpsc::channel();
        let server_id = id.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            for (method, result) in [("hello", serde_json::json!({"host_id":server_id})), ("status", serde_json::json!({"last_seq":0}))] {
                let mut line = String::new(); reader.read_line(&mut line).unwrap();
                let request: serde_json::Value = serde_json::from_str(&line).unwrap();
                assert_eq!(request["method"], method);
                writeln!(stream, "{}", serde_json::json!({"jsonrpc":"2.0","id":request["id"],"result":result})).unwrap();
            }
            let mut line = String::new(); reader.read_line(&mut line).unwrap();
            let request: serde_json::Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["method"], "events.subscribe");
            assert_eq!(request["params"]["cursor"], 0);
            for seq in [1,2] {
                if seq == 2 { finish_rx.recv().unwrap(); }
                let event = Event { seq, ts:0, scope_id:"main".into(), kind:"turn.user".into(), payload:serde_json::json!({"text":format!("message {seq}")}) };
                writeln!(stream, "{}", serde_json::json!({"event":event})).unwrap();
            }
        });
        let host = UnixHostSession::new(&socket);
        let (tx, rx) = std::sync::mpsc::channel();
        let worker_host = host.clone();
        let worker_socket = socket.clone();
        let worker = std::thread::spawn(move || run_pinned(worker_host, &worker_socket, "main", 0, tx, crate::display_latency::InstantClock::shared(), || {}, || {}));
        assert!(matches!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), StreamUpdate::Connected { replay_through:0 }));
        let update = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(update, StreamUpdate::Event { event: Event {seq:1, ..}, arrived_ns } if arrived_ns > 0));
        {
            let selected = host.directory.lock().unwrap();
            let directory = selected.as_ref().unwrap();
            assert_eq!(directory.selected_host(), Some(&id));
            let state = directory.state(&id).unwrap();
            assert_eq!(state.connection(), &mote_host_client::ConnectionState::Online);
            assert_eq!(state.cursor().seq, 1);
            assert_eq!(state.threads()["main"].messages[0].text, "message 1");
        }
        drop(rx); finish.send(()).unwrap();
        worker.join().unwrap(); server.join().unwrap();
        std::fs::remove_file(socket).unwrap();
        std::fs::remove_dir(path).unwrap();
    }

    #[test]
    fn unix_and_fixture_frames_reduce_to_the_same_host_state() {
        use mote_host_client::{
            HostDirectory, HostEventStream, HostTransport, HostUpdate, PairedHost,
        };
        use mote_protocol::{HostFrame, VersionRange};
        let public_key = [7; 32];
        let paired = PairedHost::new(public_key, "Fixture", None, VersionRange::current()).unwrap();
        let id = paired.host_id().clone();
        let path = std::env::temp_dir().join(format!("mote-host-parity-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        struct Dispatch(mote_protocol::HostId, mote_eventlog::EventLog);
        impl mote_ipc::Dispatch for Dispatch {
            fn call(
                &self,
                method: &str,
                _: serde_json::Value,
            ) -> Result<serde_json::Value, mote_ipc::RpcError> {
                match method {
                    "hello" => Ok(serde_json::json!({"host_id":self.0})),
                    _ => Err(mote_ipc::RpcError::invalid_params("unknown method")),
                }
            }
            fn subscribe(&self, scope: &str, cursor: i64) -> Option<mote_eventlog::Subscription> {
                self.1.subscribe(scope, cursor).ok()
            }
        }
        let log = mote_eventlog::EventLog::open(&path.join("events.db")).unwrap();
        let first = log.append("main", "turn.user", &serde_json::json!({"text":"hello"})).unwrap();
        let second = log.append("main", "talk.delta", &serde_json::json!({"text":"reply"})).unwrap();
        let server = mote_ipc::Server::bind(&path.join("host.sock"), std::sync::Arc::new(Dispatch(id.clone(), log))).unwrap();
        let mut fixture = HostDirectory::default(); fixture.register(paired.clone()).unwrap(); fixture.select_host(&id).unwrap();
        let mut unix = fixture.clone();
        let mut adapter = UnixHostAdapter::connect(server.path()).unwrap();
        let mut stream = adapter.subscribe(&id, unix.state(&id).unwrap().cursor()).unwrap();
        for event in [first, second] {
            fixture.reduce_update(HostUpdate::Frame { host_id: id.clone(), frame: HostFrame::Event { host_id: id.clone(), event } }).unwrap();
            unix.reduce_update(HostUpdate::Frame { host_id: id.clone(), frame: stream.next(&id).unwrap().unwrap() }).unwrap();
        }
        assert_eq!(unix.state(&id).unwrap().threads()["main"].messages[0].text, "hello");
        assert_eq!(unix.state(&id).unwrap().threads()["main"].messages[1].text, "reply");
        assert_eq!(unix, fixture);
        let replies = UnixHostAdapter::connect(server.path()).unwrap().dispatch(mote_protocol::ClientFrame::Call {
            request_id: mote_protocol::RequestId::new("helper-error").unwrap(),
            method: "unknown".into(), params: serde_json::json!({}),
        }).unwrap();
        let update = replies.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(update.host_id(), &id);
        assert!(matches!(
            update,
            HostUpdate::Frame {
                frame: HostFrame::Error { .. },
                ..
            }
        ));
        assert!(
            matches!(replies.recv_timeout(Duration::from_secs(1)).unwrap(), HostUpdate::Frame { host_id, frame: HostFrame::Acknowledgement { .. } } if host_id == id)
        );
        assert!(UnixHostSession::new(server.path())
            .call("unknown", serde_json::json!({}))
            .unwrap_err()
            .contains("unknown method"));
        drop(server);
        let other = mote_protocol::HostId::from_public_key(&[8; 32]);
        let replacement = mote_ipc::Server::bind(&path.join("host.sock"), std::sync::Arc::new(Dispatch(other, mote_eventlog::EventLog::open(&path.join("events.db")).unwrap()))).unwrap();
        use mote_host_client::HostRequestTransport;
        let error = adapter.request(mote_host_client::HostCommand { host_id: id, frame: mote_protocol::ClientFrame::Call { request_id: mote_protocol::RequestId::new("changed-host").unwrap(), method:"effect".into(), params:serde_json::json!({}) } }).unwrap_err();
        assert_eq!(error.code, mote_protocol::HostErrorCode::IdentityChanged);
        drop(replacement);
        std::fs::remove_dir_all(path).unwrap();
    }

    struct FakeSource {
        reads: VecDeque<io::Result<Option<Event>>>,
    }

    fn event(seq: i64, kind: &str) -> Event {
        Event {
            seq,
            ts: 0,
            scope_id: "main".into(),
            kind: kind.into(),
            payload: serde_json::json!({"seq": seq}),
        }
    }

    fn source(reads: Vec<io::Result<Option<Event>>>) -> FakeSource {
        FakeSource {
            reads: reads.into(),
        }
    }

    #[test]
    fn reconnects_from_the_last_delivered_cursor_after_disconnect_and_resync() {
        let plans = Arc::new(Mutex::new(VecDeque::from([
            Err(io::Error::new(io::ErrorKind::NotFound, "not ready")),
            Ok(source(vec![
                Ok(Some(event(1, "turn.user"))),
                Err(io::Error::new(io::ErrorKind::UnexpectedEof, "dropped")),
            ])),
            Ok(source(vec![
                Ok(Some(event(1, "turn.user"))),
                Ok(Some(event(5, "resync"))),
            ])),
            Ok(source(vec![
                Ok(Some(event(2, "talk.delta"))),
                Ok(Some(event(3, "talk.done"))),
            ])),
        ])));
        let cursors = Arc::new(Mutex::new(Vec::new()));
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let waits = Arc::new(Mutex::new(Vec::new()));

        run_with(
            0,
            {
                let plans = plans.clone();
                let cursors = cursors.clone();
                move |cursor| {
                    cursors.lock().unwrap().push(cursor);
                    plans.lock().unwrap().pop_front().unwrap().map(|source| (source, 0))
                }
            },
            |stream: &mut FakeSource| stream.reads.pop_front().unwrap(),
            || 0,
            {
                let delivered = delivered.clone();
                move |update| {
                    let event = match update {
                        StreamUpdate::Event { event, .. } => event,
                        StreamUpdate::Disconnected => {
                            delivered.lock().unwrap().push(-1);
                            return true;
                        }
                        StreamUpdate::Connected { .. } => {
                            delivered.lock().unwrap().push(-2);
                            return true;
                        }
                    };
                    if event.seq == 3 {
                        return false;
                    }
                    delivered.lock().unwrap().push(event.seq);
                    true
                }
            },
            || {},
            {
                let waits = waits.clone();
                move |delay| waits.lock().unwrap().push(delay)
            },
        );

        assert_eq!(*cursors.lock().unwrap(), vec![0, 0, 1, 1]);
        assert_eq!(*delivered.lock().unwrap(), vec![-2, 1, -1, -2, -1, -2, 2]);
        assert_eq!(
            *waits.lock().unwrap(),
            vec![Duration::from_millis(20), Duration::from_millis(20)]
        );
    }

    #[test]
    fn malformed_frame_keeps_the_connection_and_committed_surface() {
        let mut reads = VecDeque::from([
            Ok(Some(event(1, "ui.surface"))),
            Err(io::Error::new(io::ErrorKind::InvalidData, "malformed frame")),
            Ok(Some(event(2, "ui.surface"))),
        ]);
        let mut connected = 0;
        let mut delivered = Vec::new();
        run_with(0, |_| { connected += 1; Ok(((), 0)) }, |_| reads.pop_front().unwrap(), || 0,
            |update| match update {
                StreamUpdate::Connected { .. } => true,
                StreamUpdate::Disconnected => panic!("malformed candidate invalidated committed display"),
                StreamUpdate::Event { event, .. } => { delivered.push(event.seq); event.seq != 2 }
            }, || {}, |_| {});
        assert_eq!(connected, 1);
        assert_eq!(delivered, [1, 2]);
    }

    #[test]
    fn reconnect_backoff_is_capped() {
        let failures = Arc::new(Mutex::new(0usize));
        let waits = Arc::new(Mutex::new(Vec::new()));

        run_with(
            0,
            {
                let failures = failures.clone();
                move |_| {
                    let mut failures = failures.lock().unwrap();
                    if *failures < 8 {
                        *failures += 1;
                        Err(io::Error::new(io::ErrorKind::NotFound, "not ready"))
                    } else {
                        Ok((source(vec![Ok(Some(event(1, "turn.user")))]), 0))
                    }
                }
            },
            |stream: &mut FakeSource| stream.reads.pop_front().unwrap(),
            || 0,
            |_| false,
            || {},
            {
                let waits = waits.clone();
                move |delay| waits.lock().unwrap().push(delay)
            },
        );

        let waits = waits.lock().unwrap();
        assert_eq!(waits.first(), Some(&Duration::from_millis(20)));
        assert_eq!(waits.last(), Some(&Duration::from_secs(1)));
        assert!(waits.iter().all(|delay| *delay <= Duration::from_secs(1)));
    }

    #[test]
    fn delivered_update_wakes_the_renderer_after_queueing() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let delivered = order.clone();
        let woken = order.clone();
        assert!(deliver_and_wake(
            StreamUpdate::Disconnected,
            move |_| { delivered.lock().unwrap().push("deliver"); true },
            move || woken.lock().unwrap().push("wake"),
        ));
        assert_eq!(*order.lock().unwrap(), ["deliver", "wake"]);
    }

    #[test]
    fn decoded_event_carries_the_clock_offset_before_delivery() {
        let update = StreamUpdate::timed(event(9, "ui.surface"), 123_456);
        match update {
            StreamUpdate::Event { event, arrived_ns } => {
                assert_eq!(event.seq, 9);
                assert_eq!(arrived_ns, 123_456);
            }
            _ => panic!("expected event"),
        }
    }

    #[test]
    fn stream_timestamps_decoded_events_and_wakes_after_successful_delivery() {
        let mut reads = VecDeque::from([
            Ok(Some(event(1, "ui.surface"))),
            Ok(Some(event(2, "ui.surface"))),
        ]);
        let mut times = VecDeque::from([123, 456]);
        let mut arrivals = Vec::new();
        let mut wakes = 0;
        run_with(
            0, |_| Ok(((), 0)), |_| reads.pop_front().unwrap(),
            || times.pop_front().unwrap(),
            |update| match update {
                StreamUpdate::Connected { .. } => true,
                StreamUpdate::Event { event, arrived_ns } => {
                    arrivals.push((event.seq, arrived_ns));
                    event.seq != 2
                }
                StreamUpdate::Disconnected => true,
            },
            || wakes += 1, |_| {},
        );
        assert_eq!(arrivals, [(1, 123), (2, 456)]);
        assert_eq!(wakes, 2, "connected and first queued event must wake");
    }
}
