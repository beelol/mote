use mote_eventlog::{EventLog, Replay};
use mote_gateway::{ClientCredential, GatewayClient, GatewayConfig, GatewayServer};
use mote_host_client::{ConnectionState, HostDirectory, PairedHost};
use mote_ipc::{Actor, Dispatch, RpcError};
use mote_protocol::{ClientFrame, Cursor, HostFrame, HostId, RequestId, VersionRange};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

struct Host {
    log: EventLog,
}
impl Dispatch for Host {
    fn call(&self, _: &str, _: serde_json::Value) -> Result<serde_json::Value, RpcError> {
        unreachable!()
    }
    fn call_with_context(
        &self,
        actor: &Actor,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, RpcError> {
        let Actor::PairedDevice { host_id, device_id } = actor else {
            unreachable!()
        };
        if method == "snapshot" {
            return Ok(serde_json::to_value(
                self.log
                    .host_snapshot_for_device(host_id, device_id, 0)
                    .unwrap(),
            )
            .unwrap());
        }
        self.log.append("main", "turn.user", &params).unwrap();
        Ok(json!({"ok":true}))
    }
    fn subscribe_checked(&self, scope: &str, cursor: i64) -> Option<Replay> {
        self.log.subscribe_checked(scope, cursor).ok()
    }
    fn subscribe_host(&self, cursor: i64) -> Option<Replay> {
        self.log.subscribe_host(cursor).ok()
    }
}
fn pair(server: &GatewayServer, config: &GatewayConfig) -> (GatewayClient, ClientCredential) {
    let credential =
        ClientCredential::from_pairing_code(config, &server.open_pairing().unwrap()).unwrap();
    let client = std::thread::scope(|scope| {
        let join = scope
            .spawn(|| GatewayClient::pair(server.local_addr(), config, &credential, "Recovery", 1));
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(device) = server.pending_devices().first() {
                server.confirm_pairing(&device.client_id, true).unwrap();
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        join.join().unwrap().unwrap()
    });
    (client, credential)
}

fn acknowledgement(mut client: GatewayClient, host: &HostId) -> GatewayClient {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let frame = client.receive();
        let _ = tx.send((client, frame));
    });
    let (client, frame) = rx
        .recv_timeout(Duration::from_secs(3))
        .expect("durable outcome must be followed by acknowledgement");
    assert!(
        matches!(frame.unwrap(), HostFrame::Acknowledgement { host_id, .. } if host_id == *host)
    );
    client
}

#[test]
fn interleaved_threads_reconnect_in_one_host_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = GatewayConfig::new(dir.path().join("data"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    let host = Arc::new(Host {
        log: EventLog::open_in_memory().unwrap(),
    });
    for _ in 0..9 {
        host.log.append("earlier", "heartbeat", &json!({})).unwrap();
    }
    host.log
        .append("A", "turn.user", &json!({"text":"A10"}))
        .unwrap();
    host.log
        .append("B", "turn.user", &json!({"text":"B11"}))
        .unwrap();
    let server = GatewayServer::bind_loopback(config.clone(), host.clone()).unwrap();
    let (mut client, credential) = pair(&server, &config);
    client
        .send(&ClientFrame::Subscribe {
            request_id: RequestId::new("B").unwrap(),
            scope_id: "B".into(),
            cursor: Cursor::new(server.host_id().clone(), 9).unwrap(),
        })
        .unwrap();
    let HostFrame::Event { event, .. } = client.receive().unwrap() else {
        panic!("event")
    };
    assert_eq!(
        (event.seq, event.scope_id.as_str()),
        (10, "A"),
        "a host cursor must not advance past an unseen scope"
    );
    drop(client);
    let mut client = GatewayClient::connect(server.local_addr(), &config, &credential, 2).unwrap();
    client
        .send(&ClientFrame::Subscribe {
            request_id: RequestId::new("A").unwrap(),
            scope_id: "A".into(),
            cursor: Cursor::new(server.host_id().clone(), event.seq).unwrap(),
        })
        .unwrap();
    let HostFrame::Event { event, .. } = client.receive().unwrap() else {
        panic!("event")
    };
    assert_eq!((event.seq, event.scope_id.as_str()), (11, "B"));
    let a = host
        .log
        .append("A", "turn.user", &json!({"text":"A12"}))
        .unwrap();
    let b = host
        .log
        .append("B", "turn.user", &json!({"text":"B13"}))
        .unwrap();
    for expected in [a, b] {
        let HostFrame::Event { event, .. } = client.receive().unwrap() else {
            panic!("event")
        };
        assert_eq!(
            (event.seq, event.scope_id),
            (expected.seq, expected.scope_id)
        );
    }
}

#[test]
fn snapshot_larger_than_noise_limit_completes_call_and_resync() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = GatewayConfig::new(dir.path().join("data"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    let host = Arc::new(Host {
        log: EventLog::open_in_memory().unwrap(),
    });
    for index in 0..8 {
        host.log
            .append(
                "main",
                "turn.user",
                &json!({"text":format!("{index}{}", "x".repeat(140_000))}),
            )
            .unwrap();
    }
    let server = GatewayServer::bind_loopback(config.clone(), host.clone()).unwrap();
    let (mut client, _) = pair(&server, &config);
    let expected = host.log.host_snapshot(server.host_id()).unwrap();
    assert!(serde_json::to_vec(&expected).unwrap().len() > config.max_message);
    let result = client
        .exchange(&ClientFrame::Call {
            request_id: RequestId::new("large").unwrap(),
            method: "snapshot".into(),
            params: json!({}),
        })
        .expect("large snapshot call must complete");
    let HostFrame::Result { result, .. } = result else {
        panic!("result")
    };
    assert_eq!(
        serde_json::from_value::<mote_protocol::HostSnapshot>(result).unwrap(),
        expected
    );
    client
        .send(&ClientFrame::Subscribe {
            request_id: RequestId::new("resync").unwrap(),
            scope_id: "main".into(),
            cursor: Cursor::new(server.host_id().clone(), 9).unwrap(),
        })
        .unwrap();
    assert!(
        matches!(client.receive().unwrap(), HostFrame::Event { event, .. } if event.kind == "resync")
    );
    let snapshot = client.receive().expect("large resync must complete");
    assert_eq!(snapshot, HostFrame::Snapshot { snapshot: expected });
    let event = host
        .log
        .append("other", "turn.user", &json!({"text":"after snapshot"}))
        .unwrap();
    assert!(
        matches!(client.receive().unwrap(), HostFrame::Event {event:actual,..} if actual.seq == event.seq)
    );
}

#[test]
fn snapshot_chunks_fit_small_noise_limits_with_escaped_unicode() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = GatewayConfig::new(dir.path().join("data"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    config.max_message = 4096;
    let host = Arc::new(Host {
        log: EventLog::open_in_memory().unwrap(),
    });
    let text = "λ\"\\\n".repeat(20_000);
    host.log
        .append("main", "turn.user", &json!({"text":text}))
        .unwrap();
    let server = GatewayServer::bind_loopback(config.clone(), host).unwrap();
    let (mut client, _) = pair(&server, &config);
    let HostFrame::Result { result, .. } = client
        .exchange(&ClientFrame::Call {
            request_id: RequestId::new("small-limit").unwrap(),
            method: "snapshot".into(),
            params: json!({}),
        })
        .unwrap()
    else {
        panic!("result")
    };
    let snapshot: mote_protocol::HostSnapshot = serde_json::from_value(result).unwrap();
    assert_eq!(snapshot.threads[0].messages[0].text, text);
    assert_eq!(snapshot.cursor.seq, 1);
}

#[test]
fn gateway_ack_and_resync_are_authoritative_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = GatewayConfig::new(dir.path().join("data"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    let host = Arc::new(Host {
        log: EventLog::open_in_memory().unwrap(),
    });
    let server = GatewayServer::bind_loopback(config.clone(), host.clone()).unwrap();
    let (mut client, credential) = pair(&server, &config);
    let frame = ClientFrame::Call {
        request_id: RequestId::new("once").unwrap(),
        method: "turn".into(),
        params: json!({"text":"once"}),
    };
    client.send(&frame).unwrap();
    assert!(matches!(
        client.receive().unwrap(),
        HostFrame::Result { .. }
    ));
    client = acknowledgement(client, server.host_id());
    drop(client);
    drop(server);
    let server = GatewayServer::bind_loopback(config.clone(), host.clone()).unwrap();
    let mut client = GatewayClient::connect(server.local_addr(), &config, &credential, 2).unwrap();
    client.send(&frame).unwrap();
    assert!(matches!(
        client.receive().unwrap(),
        HostFrame::Result { .. }
    ));
    client = acknowledgement(client, server.host_id());
    assert_eq!(host.log.count("main").unwrap(), 1);
    for _ in 0..5001 {
        host.log.append("main", "heartbeat", &json!({})).unwrap();
    }
    client
        .send(&ClientFrame::Subscribe {
            request_id: RequestId::new("sub").unwrap(),
            scope_id: "main".into(),
            cursor: Cursor::new(server.host_id().clone(), 0).unwrap(),
        })
        .unwrap();
    assert!(
        matches!(client.receive().unwrap(), HostFrame::Event { event, .. } if event.kind == "resync")
    );
    let HostFrame::Snapshot { snapshot } = client.receive().unwrap() else {
        panic!("expected snapshot")
    };
    assert_eq!(snapshot.threads[0].messages[0].text, "once");
    let event = host
        .log
        .append("main", "turn.user", &json!({"text":"after snapshot"}))
        .unwrap();
    assert!(
        matches!(client.receive().unwrap(), HostFrame::Event { event: actual, .. } if actual.seq == event.seq)
    );
}

fn directory() -> (HostDirectory, HostId, HostId) {
    let mut directory = HostDirectory::default();
    let home = PairedHost::new([1; 32], "Home", None, VersionRange::current()).unwrap();
    let office = PairedHost::new([2; 32], "Office", None, VersionRange::current()).unwrap();
    let ids = (home.host_id().clone(), office.host_id().clone());
    directory.register(home).unwrap();
    directory.register(office).unwrap();
    directory.select_host(&ids.0).unwrap();
    (directory, ids.0, ids.1)
}

#[test]
fn expired_cursor_replaces_all_visible_state_for_one_host() {
    let (mut directory, home, office) = directory();
    let log = EventLog::open_in_memory().unwrap();
    log.append("old", "turn.user", &json!({"text":"old message"}))
        .unwrap();
    directory
        .install_snapshot(&home, log.host_snapshot(&home).unwrap())
        .unwrap();
    directory
        .install_snapshot(&office, log.host_snapshot(&office).unwrap())
        .unwrap();
    let office_before = directory.state(&office).unwrap().clone();
    directory
        .select_thread(&home, Some("removed".into()))
        .unwrap();
    log.append("new", "turn.user", &json!({"text":"retained message"}))
        .unwrap();
    for _ in 0..5001 {
        log.append("new", "heartbeat", &json!({})).unwrap();
    }
    assert!(matches!(
        log.subscribe_checked("new", 0).unwrap(),
        Replay::ResyncRequired { .. }
    ));
    let snapshot = log.host_snapshot(&home).unwrap();
    assert_eq!(snapshot.threads.len(), 2);
    assert_eq!(snapshot.threads[1].messages[0].text, "old message");
    directory
        .receive_frame(&home, HostFrame::Snapshot { snapshot })
        .unwrap();
    assert_eq!(directory.state(&home).unwrap().selected_thread(), None);
    assert_eq!(directory.state(&office).unwrap(), &office_before);
    assert_eq!(directory.selected_host(), Some(&home));
}

#[test]
fn incompatible_or_changed_host_never_fails_over() {
    let (mut directory, home, office) = directory();
    directory
        .set_connection(&home, ConnectionState::Connecting)
        .unwrap();
    assert!(directory
        .receive_frame(
            &home,
            HostFrame::Hello {
                host_id: office,
                protocol_major: 1,
                versions: VersionRange::current()
            }
        )
        .is_err());
    assert_eq!(
        directory.state(&home).unwrap().connection(),
        &ConnectionState::IdentityChanged
    );
    assert_eq!(directory.selected_host(), Some(&home));
    assert!(directory
        .receive_frame(
            &home,
            HostFrame::Hello {
                host_id: home.clone(),
                protocol_major: 2,
                versions: VersionRange::new(2, 2).unwrap()
            }
        )
        .is_err());
    assert!(matches!(
        directory.state(&home).unwrap().connection(),
        ConnectionState::Incompatible { .. }
    ));
    assert_eq!(directory.selected_host(), Some(&home));
}

#[test]
fn acknowledged_outbox_survives_restart_and_result_without_ack() {
    let (mut directory, home, office) = directory();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hosts.json");
    let id = RequestId::new("same-id").unwrap();
    let frame = ClientFrame::Call {
        request_id: id.clone(),
        method: "turn".into(),
        params: json!({"text":"once"}),
    };
    directory
        .enqueue_durable(&path, &home, frame.clone())
        .unwrap();
    let mut directory = HostDirectory::load(&path).unwrap();
    directory
        .receive_frame_durable(
            &path,
            &home,
            HostFrame::Result {
                request_id: id.clone(),
                result: json!({"ok":true}),
            },
        )
        .unwrap();
    assert_eq!(
        directory
            .state(&home)
            .unwrap()
            .outbox()
            .pending(&home)
            .unwrap(),
        [&frame]
    );
    assert!(directory
        .receive_frame_durable(
            &path,
            &home,
            HostFrame::Acknowledgement {
                host_id: office.clone(),
                request_id: id.clone(),
                cursor: Cursor::new(office, 1).unwrap()
            }
        )
        .is_err());
    directory
        .receive_frame_durable(
            &path,
            &home,
            HostFrame::Acknowledgement {
                host_id: home.clone(),
                request_id: id,
                cursor: Cursor::new(home.clone(), 1).unwrap(),
            },
        )
        .unwrap();
    let directory = HostDirectory::load(&path).unwrap();
    assert!(directory
        .state(&home)
        .unwrap()
        .outbox()
        .pending(&home)
        .unwrap()
        .is_empty());
    assert_eq!(
        directory.state(&home).unwrap().cursor().seq,
        0,
        "an ack must not skip unseen events"
    );
}

#[test]
fn durable_snapshot_survives_restart_and_keeps_last_committed_surface() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.db");
    let host = HostId::from_public_key(&[3; 32]);
    let log = EventLog::open(&path).unwrap();
    log.append("main", "turn.user", &json!({"text":"hello"}))
        .unwrap();
    log.append("main", "talk.delta", &json!({"text":"one "}))
        .unwrap();
    log.append("main", "talk.delta", &json!({"text":"reply"}))
        .unwrap();
    log.append("main", "talk.done", &json!({})).unwrap();
    log.append(
        "main",
        "ui.surface",
        &json!({"matrix_surface":{"type":"text","text":"committed"}}),
    )
    .unwrap();
    log.append(
        "main",
        "ui.candidate",
        &json!({"matrix_surface":{"type":"text","text":"partial"}}),
    )
    .unwrap();
    drop(log);
    let log = EventLog::open(&path).unwrap();
    let snapshot = log.host_snapshot(&host).unwrap();
    assert_eq!(snapshot.cursor.seq, 6);
    assert_eq!(snapshot.threads[0].messages.len(), 2);
    assert_eq!(snapshot.threads[0].messages[1].text, "one reply");
    assert_eq!(snapshot.surfaces[0].tree["text"], "committed");
}

#[test]
fn pending_approvals_and_status_survive_daemon_database_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.db");
    let host = HostId::from_public_key(&[4; 32]);
    let log = EventLog::open(&path).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE approvals(id INTEGER PRIMARY KEY, thread_id TEXT, kind TEXT, payload TEXT, status TEXT, action_hash TEXT, expires_at INTEGER);
        CREATE TABLE approval_devices(host_id TEXT,device_id TEXT,approval_id INTEGER,action_hash TEXT,expires_at INTEGER,PRIMARY KEY(host_id,device_id,approval_id));").unwrap();
    db.execute("INSERT INTO approvals VALUES(1,'main','file_write','{\"filename\":\"report.txt\"}','pending',?1,9999999999999)", ["a".repeat(64)]).unwrap();
    log.append("main", "turn.user", &json!({"text":"waiting"}))
        .unwrap();
    let snapshot = log.host_snapshot_for_device(&host, "device", 10).unwrap();
    assert_eq!(snapshot.pending_approvals.len(), 1);
    assert_eq!(snapshot.status, mote_protocol::HostStatus::Online);
    drop(log);
    let restarted = EventLog::open(&path).unwrap();
    assert_eq!(
        restarted
            .host_snapshot_for_device(&host, "device", 10)
            .unwrap(),
        snapshot
    );
    db.execute("UPDATE approvals SET status='denied'", [])
        .unwrap();
    assert!(restarted
        .host_snapshot_for_device(&host, "device", 10)
        .unwrap()
        .pending_approvals
        .is_empty());
}

#[test]
fn partial_encrypted_requests_retry_the_same_id_without_duplicate_turns() {
    use tokio_tungstenite::tungstenite::{client, Message};
    let dir = tempfile::tempdir().unwrap();
    let mut config = GatewayConfig::new(dir.path().join("data"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    let host = Arc::new(Host {
        log: EventLog::open_in_memory().unwrap(),
    });
    let server = GatewayServer::bind_loopback(config.clone(), host.clone()).unwrap();
    let (paired, credential) = pair(&server, &config);
    drop(paired);
    let mut counter = 2;
    for cut in 0..=3 {
        let request = ClientFrame::Call {
            request_id: RequestId::new(format!("cut-{cut}")).unwrap(),
            method: "turn".into(),
            params: json!({"text":"x".repeat(140_000)}),
        };
        let (handshake, first) = credential.begin_session(&config, counter).unwrap();
        counter += 1;
        let tcp = std::net::TcpStream::connect(server.local_addr()).unwrap();
        tcp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let (mut socket, _) = client(format!("ws://{}/session", server.local_addr()), tcp).unwrap();
        socket.send(Message::Binary(first.into())).unwrap();
        let Message::Binary(second) = socket.read().unwrap() else {
            panic!("handshake")
        };
        let mut session = handshake.finish(&second).unwrap();
        let frames = session.write_client(&request).unwrap();
        assert_eq!(frames.len(), 3);
        for bytes in frames.into_iter().take(cut) {
            socket.send(Message::Binary(bytes.into())).unwrap();
        }
        if cut == 3 {
            let _ = socket.read().unwrap();
        }
        drop(socket);
        let mut retry =
            GatewayClient::connect(server.local_addr(), &config, &credential, counter).unwrap();
        counter += 1;
        assert!(matches!(
            retry.exchange(&request).unwrap(),
            HostFrame::Result { .. }
        ));
        assert_eq!(host.log.count("main").unwrap(), cut as i64 + 1);
    }
}

#[test]
fn snapshot_then_duplicate_and_live_tokens_keep_one_reply() {
    let (mut directory, home, _) = directory();
    let log = EventLog::open_in_memory().unwrap();
    log.append("main", "turn.user", &json!({"text":"question"}))
        .unwrap();
    let first = log
        .append("main", "talk.delta", &json!({"text":"one "}))
        .unwrap();
    directory
        .install_snapshot(&home, log.host_snapshot(&home).unwrap())
        .unwrap();
    for event in [
        first,
        log.append("main", "talk.delta", &json!({"text":"reply"}))
            .unwrap(),
    ] {
        directory
            .receive_frame(
                &home,
                HostFrame::Event {
                    host_id: home.clone(),
                    event: serde_json::from_value(serde_json::to_value(event).unwrap()).unwrap(),
                },
            )
            .unwrap();
    }
    let messages = &directory.state(&home).unwrap().threads()["main"].messages;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].text, "one reply");
}

#[test]
fn future_surface_snapshot_is_rejected_atomically() {
    let (mut directory, home, _) = directory();
    let log = EventLog::open_in_memory().unwrap();
    log.append("main", "turn.user", &json!({"text":"prior"}))
        .unwrap();
    let snapshot = log.host_snapshot(&home).unwrap();
    directory.install_snapshot(&home, snapshot.clone()).unwrap();
    let before = directory.clone();
    let mut invalid = snapshot;
    invalid.surfaces.push(mote_protocol::CommittedSurface {
        thread: "main".into(),
        surface_seq: 2,
        surface_contract: 1,
        tree: json!({"type":"text"}).as_object().unwrap().clone(),
    });
    assert!(directory.install_snapshot(&home, invalid).is_err());
    assert_eq!(directory, before);
}

#[test]
fn resync_keeps_visible_state_until_snapshot_and_exposes_connection_states() {
    let (mut directory, home, _) = directory();
    let log = EventLog::open_in_memory().unwrap();
    log.append("main", "turn.user", &json!({"text":"visible"}))
        .unwrap();
    directory
        .receive_frame(
            &home,
            HostFrame::Snapshot {
                snapshot: log.host_snapshot(&home).unwrap(),
            },
        )
        .unwrap();
    assert_eq!(
        directory.state(&home).unwrap().connection(),
        &ConnectionState::Online
    );
    directory
        .receive_frame(
            &home,
            HostFrame::Event {
                host_id: home.clone(),
                event: mote_protocol::Event {
                    seq: 2,
                    ts: 0,
                    scope_id: "main".into(),
                    kind: "resync".into(),
                    payload: json!(null),
                },
            },
        )
        .unwrap();
    assert_eq!(directory.state(&home).unwrap().cursor().seq, 1);
    assert_eq!(
        directory.state(&home).unwrap().threads()["main"].messages[0].text,
        "visible"
    );
    assert_eq!(
        directory.state(&home).unwrap().connection(),
        &ConnectionState::Connecting
    );
    directory
        .set_connection(&home, ConnectionState::Offline)
        .unwrap();
    assert_eq!(directory.selected_host(), Some(&home));
}

#[test]
fn remote_snapshot_excludes_paths_credentials_and_cookies() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = GatewayConfig::new(dir.path().join("data"));
    config.bind = "127.0.0.1:0".parse().unwrap();
    let host = Arc::new(Host {
        log: EventLog::open_in_memory().unwrap(),
    });
    host.log
        .append(
            "main",
            "turn.user",
            &json!({"text":"sk-abcdefghijklmnopqrstuvwxyz /Users/private/report"}),
        )
        .unwrap();
    host.log.append("main", "ui.surface", &json!({"matrix_surface":{"type":"text","value":"safe","cookie":"raw-cookie","credentials":{"value":"private-credential"}}})).unwrap();
    let server = GatewayServer::bind_loopback(config.clone(), host).unwrap();
    let (mut client, _) = pair(&server, &config);
    let result = client
        .exchange(&ClientFrame::Call {
            request_id: RequestId::new("snapshot").unwrap(),
            method: "snapshot".into(),
            params: json!({}),
        })
        .unwrap();
    let encoded = serde_json::to_string(&result).unwrap();
    for secret in [
        "sk-abcdefghijklmnopqrstuvwxyz",
        "/Users/private/report",
        "raw-cookie",
        "private-credential",
    ] {
        assert!(!encoded.contains(secret), "snapshot leaked {secret}");
    }
}
