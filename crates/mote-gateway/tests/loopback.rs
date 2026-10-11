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
        if params["slow"] == true {
            std::thread::sleep(Duration::from_millis(5_200));
        }
        if method == "turn" {
            self.log.append("main", "turn.user", &params).unwrap();
        }
        Ok(json!({"method":method,"params":params}))
    }
    fn subscribe(&self, scope: &str, cursor: i64) -> Option<Subscription> {
        self.log.subscribe(scope, cursor).ok()
    }
    fn subscribe_host(&self, cursor: i64) -> Option<mote_eventlog::Replay> {
        self.log.subscribe_host(cursor).ok()
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
    use tokio_tungstenite::tungstenite::{client, Message};
    let (_dir, server, config, dispatch) = fixture();
    let (_, credential) = pair(&server, &config);
    let (handshake, first) = credential.begin_session(&config, 2).unwrap();
    let socket = std::net::TcpStream::connect(server.local_addr()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    let (mut socket, _) = client(format!("ws://{}/session", server.local_addr()), socket).unwrap();
    socket.send(Message::Binary(first.into())).unwrap();
    let Message::Binary(second) = socket.read().unwrap() else {
        panic!("expected Noise handshake")
    };
    let mut session = handshake.finish(&second).unwrap();
    let mut frames = session
        .write_client(&call("tampered", "turn", json!({"text":"blocked"})))
        .unwrap();
    frames[0][20] ^= 1;
    socket
        .send(Message::Binary(frames.remove(0).into()))
        .unwrap();
    assert!(socket.read().is_err());
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
    let params = json!({"thread":"main","text":"secret sk-abcdefghijklmnopqrstuvwxyz at /Users/owner/private.txt","path":"/tmp/private.txt","password":"do-not-send","sk-abcdefghijklmnopqrstuvwxyz":"safe"});
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

#[test]
fn opaque_request_id_survives_redaction_restart_and_outbox_correlation() {
    let (_dir, server, config, dispatch) = fixture();
    let (mut client, credential) = pair(&server, &config);
    let host = server.host_id().clone();
    let request = call(
        "sk-abcdefghijklmnopqrstuv",
        "turn",
        json!({"thread":"main","text":"sk-abcdefghijklmnopqrstuvwxyz","password":"hidden"}),
    );
    let mut outbox = mote_host_client::Outbox::new(&host);
    outbox.enqueue(&host, request.clone()).unwrap();
    let first = client.exchange(&request).unwrap();
    let HostFrame::Result { request_id, result } = &first else {
        panic!("expected result")
    };
    assert_eq!(request_id.as_str(), "sk-abcdefghijklmnopqrstuv");
    assert!(!serde_json::to_string(result)
        .unwrap()
        .contains("sk-abcdefghijklmnopqrstuvwxyz"));
    assert!(!serde_json::to_string(result).unwrap().contains("hidden"));
    drop(client);
    drop(server);
    let restarted = GatewayServer::bind_loopback(config.clone(), dispatch.clone()).unwrap();
    let mut client =
        GatewayClient::connect(restarted.local_addr(), &config, &credential, 2).unwrap();
    assert_eq!(client.exchange(&request).unwrap(), first);
    assert!(outbox.acknowledge(&host, request_id).unwrap());
    assert!(outbox.pending(&host).unwrap().is_empty());
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn websocket_http_upgrade_and_binary_noise_frames_authenticate_before_dispatch() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{client_async, tungstenite::Message};
    let (_dir, server, config, dispatch) = fixture();
    let (_, credential) = pair(&server, &config);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let socket = tokio::net::TcpStream::connect(server.local_addr()).await.unwrap();
        let (mut websocket, upgrade) = client_async(format!("ws://{}/session", server.local_addr()), socket).await.unwrap();
        assert_eq!(upgrade.status(), 101);
        assert_eq!(dispatch.calls.load(Ordering::SeqCst), 0);
        let (handshake, first) = credential.begin_session(&config, 2).unwrap();
        websocket.send(Message::Binary(first.into())).await.unwrap();
        let Message::Binary(second) = websocket.next().await.unwrap().unwrap() else { panic!("expected binary Noise handshake") };
        let mut session = handshake.finish(&second).unwrap();
        for bytes in session.write_client(&call("binary", "turn", json!({"thread":"main","text":"hello"}))).unwrap() {
            websocket.send(Message::Binary(bytes.into())).await.unwrap();
        }
        let Message::Binary(bytes) = websocket.next().await.unwrap().unwrap() else { panic!("expected encrypted binary result") };
        assert!(matches!(session.read_host(&bytes).unwrap(), Some(HostFrame::Result { request_id, .. }) if request_id.as_str() == "binary"));
        assert_eq!(dispatch.calls.load(Ordering::SeqCst), 1);
        let socket = tokio::net::TcpStream::connect(server.local_addr()).await.unwrap();
        let (mut unauthenticated, _) = client_async(format!("ws://{}/session", server.local_addr()), socket).await.unwrap();
        unauthenticated.send(Message::Binary(serde_json::to_vec(&call("plaintext", "turn", json!({}))).unwrap().into())).await.unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(1), unauthenticated.next()).await.unwrap().is_none_or(|message| message.is_err()));
        assert_eq!(dispatch.calls.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn concurrent_slow_turn_retry_is_pending_without_timeout_or_redispatch() {
    let (_dir, server, config, dispatch) = fixture();
    let (mut client, credential) = pair(&server, &config);
    let request = call(
        "slow",
        "turn",
        json!({"thread":"main","text":"once","slow":true}),
    );
    let started = Instant::now();
    std::thread::scope(|scope| {
        let original = scope.spawn(|| client.exchange(&request));
        let deadline = Instant::now() + Duration::from_secs(2);
        while dispatch.calls.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut retry =
            GatewayClient::connect(server.local_addr(), &config, &credential, 2).unwrap();
        let pending = serde_json::to_value(retry.exchange(&request).unwrap()).unwrap();
        assert_eq!(
            pending,
            json!({"type":"request_state","request_id":"slow","state":"pending"})
        );
        let completed = original.join().unwrap().unwrap();
        assert!(started.elapsed() > Duration::from_secs(5));
        assert_eq!(retry.exchange(&request).unwrap(), completed);
    });
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 1);
    assert_eq!(dispatch.log.max_seq().unwrap(), 1);
}

#[test]
fn interrupted_claim_is_indeterminate_after_restart_and_fences_late_completion() {
    let (_dir, server, config, dispatch) = fixture();
    let (mut client, credential) = pair(&server, &config);
    let request = call(
        "interrupted",
        "turn",
        json!({"thread":"main","text":"once","slow":true}),
    );
    std::thread::scope(|scope| {
        let original = scope.spawn(|| client.exchange(&request));
        let deadline = Instant::now() + Duration::from_secs(2);
        while dispatch.calls.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(server);
        let restarted = GatewayServer::bind_loopback(config.clone(), dispatch.clone()).unwrap();
        let mut retry =
            GatewayClient::connect(restarted.local_addr(), &config, &credential, 2).unwrap();
        let expected =
            json!({"type":"request_state","request_id":"interrupted","state":"indeterminate"});
        assert_eq!(
            serde_json::to_value(retry.exchange(&request).unwrap()).unwrap(),
            expected
        );
        assert!(original.join().unwrap().is_err());
        assert_eq!(
            serde_json::to_value(retry.exchange(&request).unwrap()).unwrap(),
            expected
        );
        assert!(matches!(
            retry
                .exchange(&call("interrupted", "turn", json!({"text":"different"})))
                .unwrap(),
            HostFrame::Error { .. }
        ));
    });
    assert_eq!(dispatch.calls.load(Ordering::SeqCst), 1);
}
