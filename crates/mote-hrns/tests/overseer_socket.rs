use mote_hrns::{
    Cursor, DelegatedEventKind, DelegatedTask, OverseerDelegate, PermissionDecision,
    SocketOverseerTransport,
};
use mote_ipc::overseer::Client;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::Duration;

fn socket_path(name: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/mote-ovs-{}-{name}.sock", std::process::id()))
}

fn read(reader: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

fn write(stream: &mut UnixStream, value: Value) {
    writeln!(stream, "{value}").unwrap();
}

fn accept_hello(listener: &UnixListener) -> (UnixStream, BufReader<UnixStream>) {
    let (mut stream, _) = listener.accept().unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let hello = read(&mut reader);
    assert_eq!(hello["method"], "hello");
    assert_eq!(hello["params"]["client"], "mote");
    assert_eq!(hello["params"]["version"], "0.0.0-test");
    write(
        &mut stream,
        json!({"id": hello["id"], "result": {"protocol": 1}}),
    );
    (stream, reader)
}

fn scripted_server(path: PathBuf) -> JoinHandle<()> {
    std::fs::remove_file(&path).ok();
    let listener = UnixListener::bind(&path).unwrap();
    std::thread::spawn(move || {
        let (mut stream, mut reader) = accept_hello(&listener);
        let create = read(&mut reader);
        assert_eq!(create["method"], "task.create");
        assert_eq!(create["params"]["workspace_mode"], "worktree");
        assert_eq!(create["params"]["permission_mode"], "acceptEdits");
        write(
            &mut stream,
            json!({
                "id": create["id"],
                "result": {
                    "task": {"id": "task-1"},
                    "run": {"id": "run-1"},
                    "workspace": {"id": "workspace-1", "path": "/tmp/worktree"}
                }
            }),
        );

        let (mut stream, mut reader) = accept_hello(&listener);
        let subscribe = read(&mut reader);
        assert_eq!(subscribe["method"], "events.subscribe");
        assert_eq!(subscribe["params"], json!({"after": 3, "run_id": "run-1"}));
        write(
            &mut stream,
            json!({"id": subscribe["id"], "result": {"subscribed": true, "after": 3, "history_truncated": false}}),
        );
        write(
            &mut stream,
            json!({"method": "event", "params": {"seq": 4, "run_id": "run-1", "kind": "status", "payload": {"status": "running", "why": "reading"}}}),
        );
        write(
            &mut stream,
            json!({"method": "event", "params": {"seq": 5, "run_id": "run-1", "kind": "permission", "payload": {"request_id": "req-1", "tool": "Bash", "input": {"command": "cargo test"}}}}),
        );
        write(
            &mut stream,
            json!({"method": "event", "params": {"seq": 6, "run_id": "run-1", "kind": "turn_done", "payload": {"ok": true, "summary": "done"}}}),
        );
        write(
            &mut stream,
            json!({"method": "replayed", "params": {"cursor": 6}}),
        );

        let (mut stream, mut reader) = accept_hello(&listener);
        let permission = read(&mut reader);
        assert_eq!(permission["method"], "run.permission");
        assert_eq!(permission["params"]["run_id"], "run-1");
        assert_eq!(permission["params"]["request_id"], "req-1");
        assert_eq!(permission["params"]["allow"], false);
        write(
            &mut stream,
            json!({"id": permission["id"], "result": {"ok": true}}),
        );
        std::fs::remove_file(path).ok();
    })
}

#[test]
fn socket_transport_uses_overseer_v1_for_submit_replay_and_permission() {
    let path = socket_path("journey");
    let server = scripted_server(path.clone());
    let client = Client::new(&path, "0.0.0-test").with_timeout(Duration::from_secs(2));
    let mut delegate = OverseerDelegate::new(SocketOverseerTransport::new(client));

    let id = delegate
        .submit(DelegatedTask {
            repo: "/Users/owner/projects/mote".into(),
            title: "Fix context ordering".into(),
            prompt: "Fix the context ordering bug.".into(),
        })
        .unwrap();
    let events = delegate.poll(&id, Cursor::new(3)).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(
        events[0].kind(),
        &DelegatedEventKind::Progress {
            status: "running".into(),
            message: "reading".into()
        }
    );
    assert!(matches!(
        events[1].kind(),
        DelegatedEventKind::Permission { .. }
    ));
    assert_eq!(
        events[2].kind(),
        &DelegatedEventKind::Completed {
            summary: "done".into(),
            changed_paths: Vec::new()
        }
    );
    delegate
        .answer_permission(
            &id,
            PermissionDecision {
                request_id: "req-1".into(),
                allow: false,
                message: "not needed".into(),
            },
        )
        .unwrap();
    server.join().unwrap();
}

#[test]
fn socket_client_refuses_an_unknown_protocol() {
    let path = socket_path("protocol");
    std::fs::remove_file(&path).ok();
    let listener = UnixListener::bind(&path).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let hello = read(&mut reader);
        write(
            &mut stream,
            json!({"id": hello["id"], "result": {"protocol": 2}}),
        );
    });
    let client = Client::new(&path, "0.0.0-test").with_timeout(Duration::from_secs(2));
    assert!(client.connect().is_err());
    server.join().unwrap();
    std::fs::remove_file(path).ok();
}
