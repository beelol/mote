//! Unix socket server and client for the daemon control plane.
//!
//! Framing is newline-delimited JSON. A request is `{"id","method","params"}`. A reply is
//! `{"id","result"}` or `{"id","error":{"code","message"}}`. The method `events.subscribe`
//! streams `{"event":{...}}` lines until the connection closes. The socket is created mode
//! 0600, a peer with another uid is refused before any request is read, and a second server on
//! the same live socket fails naming the path.

use mote_eventlog::{Event, Subscription};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// Maximum request frame size.
pub const MAX_REQUEST: usize = 1024 * 1024;

pub mod codes {
    pub const PARSE_ERROR: &str = "parse_error";
    pub const INVALID_REQUEST: &str = "invalid_request";
    pub const INVALID_PARAMS: &str = "invalid_params";
    pub const INTERNAL: &str = "internal";
}

/// An error returned to the client.
#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: String,
    pub message: String,
}

impl RpcError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), message: message.into() }
    }
    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(codes::INVALID_PARAMS, message)
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(codes::INTERNAL, message)
    }
}

/// The daemon's method table. Implementors answer unary calls and may offer event streams.
pub trait Dispatch: Send + Sync + 'static {
    fn call(&self, method: &str, params: Value) -> Result<Value, RpcError>;
    fn subscribe(&self, _scope_id: &str, _cursor: i64) -> Option<Subscription> {
        None
    }
}

/// The uid of a connected peer (macOS/BSD `getpeereid`).
pub fn peer_uid(fd: RawFd) -> io::Result<u32> {
    let mut uid: libc::uid_t = 0;
    let mut gid: libc::gid_t = 0;
    let r = unsafe { libc::getpeereid(fd, &mut uid, &mut gid) };
    if r != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(uid as u32)
}

/// Whether a connected peer's uid matches the expected uid.
pub fn peer_allowed(fd: RawFd, expected_uid: u32) -> bool {
    peer_uid(fd).map(|u| u == expected_uid).unwrap_or(false)
}

fn our_uid() -> u32 {
    unsafe { libc::geteuid() as u32 }
}

/// A running server. Dropping it stops the accept loop.
#[derive(Debug)]
pub struct Server {
    path: PathBuf,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Server {
    /// Bind the socket at `path` and start accepting. Fails naming the path if another live
    /// server already holds it.
    pub fn bind<D: Dispatch>(path: &Path, dispatch: Arc<D>) -> io::Result<Server> {
        if path.exists() {
            // A successful connect means a live server already owns the socket.
            if UnixStream::connect(path).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("another moted is already listening on {}", path.display()),
                ));
            }
            // Stale socket file; remove it before binding.
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;

        let running = Arc::new(AtomicBool::new(true));
        let run_flag = running.clone();
        let expected = our_uid();
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if !run_flag.load(Ordering::Acquire) {
                    break;
                }
                let stream = match stream {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                // Refuse a foreign peer before reading any request.
                if !peer_allowed(stream.as_raw_fd(), expected) {
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    continue;
                }
                let d = dispatch.clone();
                std::thread::spawn(move || handle_connection(stream, d));
            }
        });

        Ok(Server { path: path.to_path_buf(), running, handle: Some(handle) })
    }

    /// The socket path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        // Unblock the accept loop with a throwaway connection.
        let _ = UnixStream::connect(&self.path);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Read one newline-delimited frame, enforcing the size cap. Returns None at EOF.
fn read_frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, RpcError> {
    let mut buf = Vec::new();
    let n = reader
        .read_until(b'\n', &mut buf)
        .map_err(|e| RpcError::internal(e.to_string()))?;
    if n == 0 {
        return Ok(None);
    }
    if buf.last() == Some(&b'\n') {
        buf.pop();
    }
    if buf.len() > MAX_REQUEST {
        return Err(RpcError::new(codes::INVALID_REQUEST, "request exceeds 1 MiB"));
    }
    Ok(Some(buf))
}

fn handle_connection<D: Dispatch>(stream: UnixStream, dispatch: Arc<D>) {
    let read_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut reader = BufReader::new(read_stream);
    let mut writer = stream;

    loop {
        let frame = match read_frame(&mut reader) {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                let _ = write_line(&mut writer, &error_reply(Value::Null, &e));
                break;
            }
        };

        let parsed: Result<Value, _> = serde_json::from_slice(&frame);
        let value = match parsed {
            Ok(v) => v,
            Err(_) => {
                let e = RpcError::new(codes::PARSE_ERROR, "request is not valid JSON");
                if write_line(&mut writer, &error_reply(Value::Null, &e)).is_err() {
                    break;
                }
                continue;
            }
        };

        let id = value.get("id").cloned().unwrap_or(Value::Null);
        let method = match value.get("method").and_then(|m| m.as_str()) {
            Some(m) => m.to_string(),
            None => {
                let e = RpcError::new(codes::INVALID_REQUEST, "missing method");
                if write_line(&mut writer, &error_reply(id, &e)).is_err() {
                    break;
                }
                continue;
            }
        };
        let params = value.get("params").cloned().unwrap_or(Value::Null);

        if method == "events.subscribe" {
            if stream_events(&mut writer, &dispatch, &params).is_err() {
                break;
            }
            // The subscribe method owns the connection until it closes.
            break;
        }

        let reply = match dispatch.call(&method, params) {
            Ok(result) => success_envelope(id, result),
            Err(e) => {
                // An unknown method surfaces as invalid_request; the handler chooses otherwise.
                error_reply(id, &e)
            }
        };
        if write_line(&mut writer, &reply).is_err() {
            break;
        }
    }
}

fn stream_events<D: Dispatch>(
    writer: &mut UnixStream,
    dispatch: &Arc<D>,
    params: &Value,
) -> io::Result<()> {
    let scope_id = params.get("scope_id").and_then(|v| v.as_str()).unwrap_or("");
    let cursor = params.get("cursor").and_then(|v| v.as_i64()).unwrap_or(0);
    let mut sub = match dispatch.subscribe(scope_id, cursor) {
        Some(s) => s,
        None => {
            let e = RpcError::new(codes::INVALID_PARAMS, "unknown scope");
            return write_line(writer, &error_reply(Value::Null, &e));
        }
    };
    loop {
        if let Some(ev) = sub.recv_timeout(Duration::from_millis(250)) {
            write_line(writer, &event_envelope(&ev))?;
        }
        // On idle, loop again; a dropped peer is detected on the next real write.
    }
}

fn error_reply(id: Value, e: &RpcError) -> Value {
    json!({ "id": id, "error": { "code": e.code, "message": e.message } })
}

pub fn request_envelope(id: Value, method: &str, params: Value) -> Value {
    json!({ "id": id, "method": method, "params": params })
}

pub fn success_envelope(id: Value, result: Value) -> Value {
    json!({ "id": id, "result": result })
}

pub fn event_envelope(event: &Event) -> Value {
    json!({ "event": event })
}

pub fn encode_frame(value: &Value) -> io::Result<Vec<u8>> {
    let mut line = serde_json::to_vec(value).map_err(io::Error::other)?;
    line.push(b'\n');
    Ok(line)
}

fn write_line(writer: &mut UnixStream, value: &Value) -> io::Result<()> {
    writer.write_all(&encode_frame(value)?)?;
    writer.flush()
}

/// A connected client.
pub struct Client {
    path: PathBuf,
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
}

impl Client {
    pub fn connect(path: &Path) -> io::Result<Client> {
        let writer = UnixStream::connect(path)?;
        let reader = BufReader::new(writer.try_clone()?);
        Ok(Client { path: path.to_path_buf(), reader, writer, next_id: 1 })
    }

    /// Make a unary call.
    pub fn call(&mut self, method: &str, params: Value) -> io::Result<Result<Value, RpcError>> {
        let id = self.next_id;
        self.next_id += 1;
        let req = request_envelope(json!(id), method, params);
        write_line(&mut self.writer, &req)?;
        let mut line = String::new();
        let n = self.reader.read_line(&mut line)?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed"));
        }
        let v: Value = serde_json::from_str(line.trim_end())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        if let Some(err) = v.get("error") {
            let code = err.get("code").and_then(|c| c.as_str()).unwrap_or("").to_string();
            let message = err.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
            return Ok(Err(RpcError { code, message }));
        }
        Ok(Ok(v.get("result").cloned().unwrap_or(Value::Null)))
    }

    /// Consume this connection into a subscription without reconnecting. Use `read_event`
    /// when unary state must be refreshed after a disconnect.
    pub fn subscribe(mut self, scope_id: &str, cursor: i64) -> io::Result<EventStream> {
        self.writer.set_read_timeout(Some(Duration::from_millis(200)))?;
        let request = request_envelope(
            json!(self.next_id),
            "events.subscribe",
            json!({ "scope_id": scope_id, "cursor": cursor }),
        );
        write_line(&mut self.writer, &request)?;
        Ok(EventStream {
            path: self.path,
            scope_id: scope_id.to_string(),
            cursor,
            reader: self.reader,
            stream: self.writer,
            decoder: EventDecoder::default(),
        })
    }
}

/// A resilient subscription: it tracks its cursor and resubscribes from it after a dropped
/// connection, so no event is delivered twice.
pub struct EventStream {
    path: PathBuf,
    scope_id: String,
    cursor: i64,
    reader: BufReader<UnixStream>,
    stream: UnixStream,
    decoder: EventDecoder,
}

/// Retains incomplete newline-delimited event bytes across read timeouts.
#[derive(Default)]
pub struct EventDecoder {
    pending: Vec<u8>,
}

impl EventDecoder {
    pub fn read(&mut self, reader: &mut impl BufRead) -> io::Result<Option<Event>> {
        match reader.read_until(b'\n', &mut self.pending) {
            Ok(0) => {
                self.pending.clear();
                Err(io::Error::new(io::ErrorKind::UnexpectedEof, "subscription closed"))
            }
            Ok(_) if self.pending.last() != Some(&b'\n') => {
                self.pending.clear();
                Err(io::Error::new(io::ErrorKind::UnexpectedEof, "incomplete subscription frame"))
            }
            Ok(_) => {
                let line = std::mem::take(&mut self.pending);
                let frame: Value = serde_json::from_slice(&line)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                let value = frame.get("event").ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "subscription frame has no event")
                })?;
                serde_json::from_value(value.clone()).map(Some)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
            }
            Err(error) if matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

impl EventStream {
    pub fn connect(path: &Path, scope_id: &str, cursor: i64) -> io::Result<EventStream> {
        let (reader, stream) = Self::open(path, scope_id, cursor)?;
        Ok(EventStream {
            path: path.to_path_buf(),
            scope_id: scope_id.to_string(),
            cursor,
            reader,
            stream,
            decoder: EventDecoder::default(),
        })
    }

    fn open(path: &Path, scope_id: &str, cursor: i64) -> io::Result<(BufReader<UnixStream>, UnixStream)> {
        let stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(Duration::from_millis(200)))?;
        let mut w = stream.try_clone()?;
        let req = request_envelope(
            json!(0),
            "events.subscribe",
            json!({ "scope_id": scope_id, "cursor": cursor }),
        );
        write_line(&mut w, &req)?;
        Ok((BufReader::new(stream.try_clone()?), stream))
    }

    fn reconnect(&mut self) {
        let mut delay = Duration::from_millis(20);
        loop {
            match Self::open(&self.path, &self.scope_id, self.cursor) {
                Ok((r, s)) => {
                    self.reader = r;
                    self.stream = s;
                    self.decoder = EventDecoder::default();
                    return;
                }
                Err(_) => {
                    std::thread::sleep(delay);
                    delay = (delay * 2).min(Duration::from_secs(1));
                }
            }
        }
    }

    /// Force the current connection to drop; the next read reconnects from the cursor. For
    /// tests that simulate a mid-stream disconnect.
    pub fn force_reconnect(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

    /// Read once from this subscription. A timeout returns `Ok(None)`. A dropped connection is
    /// returned to the caller so a higher-level worker can reconnect from its delivered cursor.
    pub fn read_event(&mut self) -> io::Result<Option<Event>> {
        let event = self.decoder.read(&mut self.reader)?;
        if let Some(event) = &event {
            self.cursor = event.seq;
        }
        Ok(event)
    }

    /// Pull the next event, reconnecting from the cursor on a hard disconnect. Returns None on a
    /// read timeout with no event.
    pub fn next_event(&mut self) -> Option<Event> {
        loop {
            match self.read_event() {
                Ok(event) => return event,
                Err(error) if error.kind() == io::ErrorKind::InvalidData => continue,
                Err(_) => {
                    self.reconnect();
                }
            }
        }
    }

    pub fn cursor(&self) -> i64 {
        self.cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_eventlog::EventLog;

    #[test]
    fn fragmented_event_survives_timeouts_at_every_byte_and_requires_newline() {
        let event = Event { seq: 7, ts: 0, scope_id: "main".into(), kind: "ui.surface".into(),
            payload: json!({"surface":{"type":"text","value":"é"}}) };
        let frame = encode_frame(&event_envelope(&event)).unwrap();
        for cut in 0..frame.len() {
            let (reader, mut writer) = UnixStream::pair().unwrap();
            reader.set_read_timeout(Some(Duration::from_millis(1))).unwrap();
            let mut stream = EventStream {
                path: PathBuf::new(), scope_id: "main".into(), cursor: 0,
                reader: BufReader::new(reader.try_clone().unwrap()), stream: reader,
                decoder: EventDecoder::default(),
            };
            writer.write_all(&frame[..cut]).unwrap();
            assert!(stream.read_event().unwrap().is_none(), "cut {cut}");
            assert_eq!(stream.cursor(), 0);
            writer.write_all(&frame[cut..]).unwrap();
            let delivered = stream.read_event().unwrap().unwrap();
            assert_eq!(delivered.payload, event.payload, "cut {cut}");
            assert_eq!(stream.cursor(), 7);
        }
    }

    #[test]
    fn event_decoder_rejects_eof_before_newline_and_recovers_after_bad_frames() {
        let event = Event { seq: 9, ts: 0, scope_id: "main".into(), kind: "ui.surface".into(),
            payload: json!({"surface":{"type":"text","value":"complete"}}) };
        let frame = encode_frame(&event_envelope(&event)).unwrap();
        let mut decoder = EventDecoder::default();
        let error = decoder.read(&mut &frame[..frame.len() - 1]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        let mut coalesced = b"{bad}\n{\"event\":{}}\n".to_vec();
        coalesced.extend(&frame);
        coalesced.extend(&frame);
        let mut reader = coalesced.as_slice();
        for _ in 0..2 {
            assert_eq!(decoder.read(&mut reader).unwrap_err().kind(), io::ErrorKind::InvalidData);
        }
        for _ in 0..2 {
            assert_eq!(decoder.read(&mut reader).unwrap().unwrap().seq, 9);
        }
        assert_eq!(decoder.read(&mut reader).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }

    struct TestDispatch {
        log: EventLog,
    }
    impl Dispatch for TestDispatch {
        fn call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
            match method {
                "echo" => Ok(params),
                "append" => {
                    let ev = self
                        .log
                        .append("s", "test", &params)
                        .map_err(|e| RpcError::internal(e.to_string()))?;
                    Ok(json!({ "seq": ev.seq }))
                }
                _ => Err(RpcError::new(codes::INVALID_REQUEST, format!("unknown method: {method}"))),
            }
        }
        fn subscribe(&self, scope_id: &str, cursor: i64) -> Option<Subscription> {
            self.log.subscribe(scope_id, cursor).ok()
        }
    }

    fn temp_sock(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mote-ipc-{}-{}.sock", name, std::process::id()))
    }

    fn server(name: &str) -> (Server, PathBuf, EventLog) {
        let path = temp_sock(name);
        let _ = std::fs::remove_file(&path);
        let log = EventLog::open_in_memory().unwrap();
        let d = Arc::new(TestDispatch { log: log.clone() });
        let srv = Server::bind(&path, d).unwrap();
        (srv, path, log)
    }

    #[test]
    fn socket_mode_is_0600_and_second_bind_fails() {
        let (_srv, path, _log) = server("mode");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        // A second server on the live socket fails naming the path.
        let log2 = EventLog::open_in_memory().unwrap();
        let d2 = Arc::new(TestDispatch { log: log2 });
        let err = Server::bind(&path, d2).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AddrInUse);
        assert!(err.to_string().contains(path.to_str().unwrap()));
    }

    #[test]
    fn error_codes_for_bad_requests() {
        let (_srv, path, _log) = server("errors");

        // Malformed JSON -> parse_error.
        let mut raw = UnixStream::connect(&path).unwrap();
        raw.write_all(b"{not json}\n").unwrap();
        let mut r = BufReader::new(raw.try_clone().unwrap());
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        let v: Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(v["error"]["code"], codes::PARSE_ERROR);

        // Unknown method -> invalid_request.
        let mut c = Client::connect(&path).unwrap();
        let res = c.call("does_not_exist", json!({})).unwrap();
        assert_eq!(res.unwrap_err().code, codes::INVALID_REQUEST);

        // Oversize request -> invalid_request.
        let mut big = UnixStream::connect(&path).unwrap();
        let payload = vec![b'x'; MAX_REQUEST + 16];
        big.write_all(&payload).unwrap();
        big.write_all(b"\n").unwrap();
        let mut rb = BufReader::new(big.try_clone().unwrap());
        let mut lb = String::new();
        rb.read_line(&mut lb).unwrap();
        let vb: Value = serde_json::from_str(lb.trim_end()).unwrap();
        assert_eq!(vb["error"]["code"], codes::INVALID_REQUEST);
    }

    #[test]
    fn peer_credential_check() {
        // A connected peer on this machine is our own uid; a different uid is refused.
        let (a, b) = UnixStream::pair().unwrap();
        let uid = our_uid();
        assert_eq!(peer_uid(a.as_raw_fd()).unwrap(), uid);
        assert!(peer_allowed(a.as_raw_fd(), uid));
        assert!(!peer_allowed(b.as_raw_fd(), uid.wrapping_add(1)));
    }

    #[test]
    fn subscribe_streams_and_reconnects_without_duplicates() {
        let (_srv, path, log) = server("subscribe");
        for i in 0..5 {
            log.append("s", "test", &json!({ "n": i })).unwrap();
        }
        let mut stream = EventStream::connect(&path, "s", 0).unwrap();

        let mut got: Vec<i64> = Vec::new();
        // Read the first few.
        while got.len() < 3 {
            if let Some(ev) = stream.next_event() {
                got.push(ev.seq);
            }
        }
        // Kill the connection mid-stream, then append more.
        stream.force_reconnect();
        for i in 5..10 {
            log.append("s", "test", &json!({ "n": i })).unwrap();
        }
        // Collect the rest; the stream resubscribes from its cursor.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while got.len() < 10 && std::time::Instant::now() < deadline {
            if let Some(ev) = stream.next_event() {
                got.push(ev.seq);
            }
        }
        assert_eq!(got.len(), 10, "received all ten events");
        // Strictly increasing: in order and no duplicate.
        for w in got.windows(2) {
            assert!(w[1] > w[0], "no duplicate, in order: {got:?}");
        }
    }

    #[test]
    fn unary_client_transitions_to_subscription_on_the_same_socket() {
        let path = temp_sock("transition");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let status: Value = serde_json::from_slice(&read_frame(&mut reader).unwrap().unwrap()).unwrap();
            assert_eq!(status["method"], "status");
            write_line(&mut socket, &success_envelope(status["id"].clone(), json!({"last_seq":5}))).unwrap();
            let subscribe: Value = serde_json::from_slice(&read_frame(&mut reader).unwrap().unwrap()).unwrap();
            assert_eq!(subscribe["method"], "events.subscribe");
            assert_eq!(subscribe["params"], json!({"scope_id":"s","cursor":3}));
            let event = Event { seq:6, ts:0, scope_id:"s".into(), kind:"test".into(), payload:json!({}) };
            write_line(&mut socket, &event_envelope(&event)).unwrap();
        });
        let mut client = Client::connect(&path).unwrap();
        assert_eq!(client.call("status", json!({})).unwrap().unwrap()["last_seq"], 5);
        let mut stream = client.subscribe("s", 3).unwrap();
        assert_eq!(stream.read_event().unwrap().unwrap().seq, 6);
        assert_eq!(stream.cursor(), 6);
        server.join().unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn client_transition_cannot_move_to_a_replacement_server() {
        let path = temp_sock("transition-death");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket.try_clone().unwrap());
            let request: Value = serde_json::from_slice(&read_frame(&mut reader).unwrap().unwrap()).unwrap();
            write_line(&mut socket, &success_envelope(request["id"].clone(), json!({"last_seq":5}))).unwrap();
        });
        let mut client = Client::connect(&path).unwrap();
        assert_eq!(client.call("status", json!({})).unwrap().unwrap()["last_seq"], 5);
        server.join().unwrap();
        std::fs::remove_file(&path).unwrap();
        let replacement = UnixListener::bind(&path).unwrap();
        replacement.set_nonblocking(true).unwrap();
        if let Ok(mut stream) = client.subscribe("s", 0) {
            assert!(stream.read_event().is_err());
        }
        assert_eq!(replacement.accept().unwrap_err().kind(), io::ErrorKind::WouldBlock);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn single_subscription_read_reports_a_dropped_connection() {
        let (_srv, path, _log) = server("single-read-drop");
        let mut stream = EventStream::connect(&path, "s", 0).unwrap();

        stream.force_reconnect();

        let error = stream.read_event().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(stream.cursor(), 0);
    }
}
