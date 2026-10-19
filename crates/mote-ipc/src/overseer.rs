//! Bounded client for Overseer's version 1 owner-only Unix socket protocol.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

const PROTOCOL_VERSION: u64 = 1;
const MAX_FRAME_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Client {
    socket: PathBuf,
    version: String,
    timeout: Duration,
}

impl Client {
    pub fn new(socket: impl Into<PathBuf>, version: impl Into<String>) -> Self {
        Self {
            socket: socket.into(),
            version: version.into(),
            timeout: Duration::from_secs(2),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    pub fn connect(&self) -> Result<Connection, Error> {
        let stream = UnixStream::connect(&self.socket).map_err(|_| Error::Unavailable)?;
        let owner_uid = unsafe { libc::geteuid() as u32 };
        if !crate::peer_allowed(stream.as_raw_fd(), owner_uid) {
            return Err(Error::Unavailable);
        }
        stream
            .set_read_timeout(Some(self.timeout))
            .map_err(|_| Error::Unavailable)?;
        stream
            .set_write_timeout(Some(self.timeout))
            .map_err(|_| Error::Unavailable)?;
        let reader = BufReader::new(stream.try_clone().map_err(|_| Error::Unavailable)?);
        let mut connection = Connection {
            reader,
            writer: stream,
            next_id: 1,
        };
        let hello =
            connection.request("hello", json!({"client": "mote", "version": self.version}))?;
        if hello.get("protocol").and_then(Value::as_u64) != Some(PROTOCOL_VERSION) {
            return Err(Error::Protocol);
        }
        Ok(connection)
    }
}

pub struct Connection {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: u64,
}

impl Connection {
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(Error::Protocol)?;
        self.write(&json!({"id": id, "method": method, "params": params}))?;
        let message = self.read()?;
        if message.get("id").and_then(Value::as_u64) != Some(id) {
            return Err(Error::Protocol);
        }
        if message.get("error").is_some_and(|error| !error.is_null()) {
            return Err(Error::Rejected);
        }
        message.get("result").cloned().ok_or(Error::Protocol)
    }

    pub fn replay(&mut self, run_id: &str, after: u64, limit: usize) -> Result<Vec<Value>, Error> {
        if limit == 0 || limit > 1_000 {
            return Err(Error::Protocol);
        }
        let id = self.next_id;
        self.next_id = self.next_id.checked_add(1).ok_or(Error::Protocol)?;
        self.write(&json!({
            "id": id,
            "method": "events.subscribe",
            "params": {"after": after, "run_id": run_id}
        }))?;
        let acknowledgement = self.read()?;
        if acknowledgement.get("id").and_then(Value::as_u64) != Some(id)
            || acknowledgement
                .pointer("/result/subscribed")
                .and_then(Value::as_bool)
                != Some(true)
            || acknowledgement
                .pointer("/result/after")
                .and_then(Value::as_u64)
                != Some(after)
            || acknowledgement
                .pointer("/result/history_truncated")
                .and_then(Value::as_bool)
                != Some(false)
        {
            return Err(Error::Protocol);
        }

        let mut events = Vec::new();
        let mut last_cursor = after;
        loop {
            let message = self.read()?;
            match message.get("method").and_then(Value::as_str) {
                Some("event") => {
                    if events.len() == limit {
                        return Err(Error::LimitExceeded);
                    }
                    let event = message.get("params").cloned().ok_or(Error::Protocol)?;
                    let cursor = event
                        .get("seq")
                        .and_then(Value::as_u64)
                        .filter(|cursor| *cursor > last_cursor)
                        .ok_or(Error::Protocol)?;
                    last_cursor = cursor;
                    events.push(event);
                }
                Some("replayed") => {
                    if message.pointer("/params/cursor").and_then(Value::as_u64)
                        != Some(last_cursor)
                    {
                        return Err(Error::Protocol);
                    }
                    return Ok(events);
                }
                Some("resync") => return Err(Error::Resync),
                _ => return Err(Error::Protocol),
            }
        }
    }

    fn write(&mut self, message: &Value) -> Result<(), Error> {
        let mut frame = serde_json::to_vec(message).map_err(|_| Error::Protocol)?;
        if frame.len() as u64 > MAX_FRAME_BYTES {
            return Err(Error::LimitExceeded);
        }
        frame.push(b'\n');
        self.writer
            .write_all(&frame)
            .map_err(|_| Error::Unavailable)
    }

    fn read(&mut self) -> Result<Value, Error> {
        let mut frame = Vec::new();
        let bytes = (&mut self.reader)
            .take(MAX_FRAME_BYTES + 2)
            .read_until(b'\n', &mut frame)
            .map_err(|_| Error::Unavailable)?;
        if bytes == 0 {
            return Err(Error::Unavailable);
        }
        if frame.last() == Some(&b'\n') {
            frame.pop();
        }
        if frame.len() as u64 > MAX_FRAME_BYTES {
            return Err(Error::LimitExceeded);
        }
        serde_json::from_slice(&frame).map_err(|_| Error::Protocol)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Unavailable,
    Protocol,
    Rejected,
    Resync,
    LimitExceeded,
}
