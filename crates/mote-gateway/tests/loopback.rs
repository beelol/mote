use mote_eventlog::{EventLog, Subscription};
use mote_gateway::{ClientCredential, GatewayClient, GatewayConfig, GatewayServer};
use mote_ipc::{Actor, Dispatch, RpcError};
use mote_protocol::{ClientFrame, Cursor, HostFrame, RequestId, VersionRange};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

struct Fixture {
    log: EventLog,
    calls: AtomicUsize,
}
impl Dispatch for Fixture {
    fn call(&self, _: &str, _: Value) -> Result<Value, RpcError> {
        panic!("missing authenticated actor")
    }
    fn call_with_context(
        &self,
        actor: &Actor,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        assert!(matches!(actor, Actor::PairedDevice { .. }));
        self.calls.fetch_add(1, Ordering::SeqCst);
        if method == "turn" {
            self.log.append("main", "turn.user", &params).unwrap();
        }
        Ok(json!({"method":method,"params":params}))
    }
    fn subscribe(&self, scope: &str, cursor: i64) -> Option<Subscription> {
        self.log.subscribe(scope, cursor).ok()
    }
}
fn fixture() -> (
    tempfile::TempDir,
    GatewayServer,
    GatewayConfig,
    Arc<Fixture>,
) {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let mut config = GatewayConfig::new(data);
    config.bind = "127.0.0.1:0".parse().unwrap();
    let dispatch = Arc::new(Fixture {
        log: EventLog::open_in_memory().unwrap(),
        calls: AtomicUsize::new(0),
    });
    let server = GatewayServer::bind_loopback(config.clone(), dispatch.clone()).unwrap();
    (dir, server, config, dispatch)
}
fn pair(server: &GatewayServer, config: &GatewayConfig) -> (GatewayClient, ClientCredential) {
    let code = server.open_pairing().unwrap();
    assert!(code.starts_with("MOTE1-"));
    let credential = ClientCredential::from_pairing_code(config, &code).unwrap();
    let client = std::thread::scope(|scope| {
        let pending = scope.spawn(|| {
            GatewayClient::pair(server.local_addr(), config, &credential, "Fixture iPad", 1)
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(device) = server.pending_devices().first() {
                server.confirm_pairing(&device.client_id, true).unwrap();
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        pending.join().unwrap().unwrap()
    });
    (client, credential)
}
fn call(id: &str, method: &str, params: Value) -> ClientFrame {
    ClientFrame::Call {
        request_id: RequestId::new(id).unwrap(),
        method: method.into(),
        params,
    }
}
#[test]
fn authenticated_pair_hello_status_turn_and_subscribe() {
    let (_dir, server, config, dispatch) = fixture();
    let (mut client, _) = pair(&server, &config);
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 0);
    assert!(
        matches!(client.exchange(&ClientFrame::Hello { versions: VersionRange::current() }).unwrap(), HostFrame::Hello { host_id, .. } if host_id == *server.host_id())
    );
    assert!(matches!(
        client
            .exchange(&call("status", "status", json!({})))
            .unwrap(),
        HostFrame::Result { .. }
    ));
    assert!(matches!(
        client
            .exchange(&call(
                "turn",
                "turn",
                json!({"thread":"main","text":"hello"})
            ))
            .unwrap(),
        HostFrame::Result { .. }
    ));
    client
        .send(&ClientFrame::Subscribe {
            request_id: RequestId::new("sub").unwrap(),
            scope_id: "main".into(),
            cursor: Cursor::new(server.host_id().clone(), 0).unwrap(),
        })
        .unwrap();
    assert!(
        matches!(client.receive().unwrap(), HostFrame::Event { event, .. } if event.seq == 1 && event.payload["text"] == "hello")
    );
}
#[test]
fn replayed_request_returns_recorded_outcome() {
    let (_dir, server, config, dispatch) = fixture();
    let (mut client, credential) = pair(&server, &config);
    let request = call("once", "turn", json!({"thread":"main","text":"once"}));
    let first = client.exchange(&request).unwrap();
    drop(client);
    let mut second = GatewayClient::connect(server.local_addr(), &config, &credential, 2).unwrap();
    assert_eq!(second.exchange(&request).unwrap(), first);
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        second
            .exchange(&call("once", "turn", json!({"text":"changed"})))
            .unwrap(),
        HostFrame::Error { .. }
    ));
    assert_eq!(dispatch.log.max_seq().unwrap(), 1);
}
#[test]
fn plaintext_and_raw_mutations_never_dispatch() {
    use std::io::Write;
    let (_dir, server, config, dispatch) = fixture();
    let mut raw = std::net::TcpStream::connect(server.local_addr()).unwrap();
    raw.write_all(b"{\"method\":\"turn\",\"params\":{}}\n")
        .unwrap();
    drop(raw);
    let (mut client, _) = pair(&server, &config);
    for method in [
        "approve",
        "deny",
        "ui.event",
        "config.set",
        "config.revert",
        "file.write",
        "surface.render",
        "matrix.render",
        "matrix.candidate",
        "schedule.add",
        "interaction",
        "approval.decision",
    ] {
        assert!(
            matches!(
                client.exchange(&call(method, method, json!({}))).unwrap(),
                HostFrame::Error { .. }
            ),
            "{method}"
        );
    }
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn expired_reused_wrong_identity_replayed_counter_and_revoked_sessions_never_dispatch() {
    let (_dir, server, config, dispatch) = fixture();
    let (mut client, credential) = pair(&server, &config);
    assert!(GatewayClient::pair(server.local_addr(), &config, &credential, "Reused", 2).is_err());
    let mut next = GatewayClient::connect(server.local_addr(), &config, &credential, 2).unwrap();
    assert!(GatewayClient::connect(server.local_addr(), &config, &credential, 2).is_err());
    let wrong_host =
        ClientCredential::restore([5; 32], *credential.private_key_for_secure_storage()).unwrap();
    assert!(GatewayClient::connect(server.local_addr(), &config, &wrong_host, 3).is_err());
    server.revoke(credential.public_key()).unwrap();
    assert!(GatewayClient::connect(server.local_addr(), &config, &credential, 3).is_err());
    assert!(next
        .exchange(&call("revoked", "turn", json!({"text":"blocked"})))
        .is_err());
    assert!(client
        .exchange(&call("revoked-old", "turn", json!({"text":"blocked"})))
        .is_err());
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 0);
    let expired_dir = tempfile::tempdir().unwrap();
    let mut expired_config = GatewayConfig::new(expired_dir.path().join("data"));
    expired_config.bind = "127.0.0.1:0".parse().unwrap();
    expired_config.pairing_window = Duration::from_millis(1);
    let expired_server =
        GatewayServer::bind_loopback(expired_config.clone(), dispatch.clone()).unwrap();
    let expired = ClientCredential::from_pairing_code(
        &expired_config,
        &expired_server.open_pairing().unwrap(),
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(5));
    assert!(GatewayClient::pair(
        expired_server.local_addr(),
        &expired_config,
        &expired,
        "Expired",
        1
    )
    .is_err());
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn tampered_encrypted_frame_never_dispatches() {
    use std::io::{Read, Write};
    let (_dir, server, config, dispatch) = fixture();
    let (_, credential) = pair(&server, &config);
    let (handshake, first) = credential.begin_session(&config, 2).unwrap();
    let mut socket = std::net::TcpStream::connect(server.local_addr()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    socket.write_all(&[0]).unwrap();
    socket
        .write_all(&(first.len() as u32).to_be_bytes())
        .unwrap();
    socket.write_all(&first).unwrap();
    let mut size = [0; 4];
    socket.read_exact(&mut size).unwrap();
    let mut second = vec![0; u32::from_be_bytes(size) as usize];
    socket.read_exact(&mut second).unwrap();
    let mut session = handshake.finish(&second).unwrap();
    let mut frames = session
        .write_client(&call("tampered", "turn", json!({"text":"blocked"})))
        .unwrap();
    frames[0][20] ^= 1;
    socket
        .write_all(&(frames[0].len() as u32).to_be_bytes())
        .unwrap();
    socket.write_all(&frames[0]).unwrap();
    assert!(socket.read_exact(&mut size).is_err());
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn durable_replay_survives_gateway_restart() {
    let (_dir, server, config, dispatch) = fixture();
    let (mut client, credential) = pair(&server, &config);
    let frame = call("durable", "turn", json!({"thread":"main","text":"once"}));
    let outcome = client.exchange(&frame).unwrap();
    let host_id = server.host_id().clone();
    drop(client);
    drop(server);
    let restarted = GatewayServer::bind_loopback(config.clone(), dispatch.clone()).unwrap();
    assert_eq!(restarted.host_id(), &host_id);
    let mut client =
        GatewayClient::connect(restarted.local_addr(), &config, &credential, 2).unwrap();
    assert_eq!(client.exchange(&frame).unwrap(), outcome);
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn remote_outcomes_and_events_redact_secrets_and_local_paths() {
    let (_dir, server, config, dispatch) = fixture();
    let (mut client, _) = pair(&server, &config);
    let params = json!({"thread":"main","text":"secret sk-abcdefghijklmnopqrstuvwxyz at /Users/owner/private.txt","path":"/tmp/private.txt","password":"do-not-send"});
    let frame = call("redacted", "turn", params);
    let response = client.exchange(&frame).unwrap();
    let encoded = serde_json::to_string(&response).unwrap();
    assert!(!encoded.contains("sk-abcdefghijklmnopqrstuvwxyz"));
    assert!(!encoded.contains("do-not-send"));
    assert!(!encoded.contains("/Users/owner/private.txt"));
    assert!(!encoded.contains("/tmp/private.txt"));
    assert_eq!(client.exchange(&frame).unwrap(), response);
    client
        .send(&ClientFrame::Subscribe {
            request_id: RequestId::new("sub").unwrap(),
            scope_id: "main".into(),
            cursor: Cursor::new(server.host_id().clone(), 0).unwrap(),
        })
        .unwrap();
    let event = serde_json::to_string(&client.receive().unwrap()).unwrap();
    assert!(!event.contains("sk-abcdefghijklmnopqrstuvwxyz"));
    assert!(!event.contains("/Users/owner/private.txt"));
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 1);
}
