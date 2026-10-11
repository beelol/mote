use super::*;
use mote_host_client::{ConnectionState, HostDirectory, PairedHost};
use mote_protocol::{
    ApprovalDecision, ApprovalVerdict, ClientFrame, Cursor, HostFrame, HostSnapshot, RequestId,
    TypedInteraction, VersionRange,
};
use mote_testkit::gateway::LoopbackHost;

fn fixture(models: &mote_testkit::ScriptedServer) -> LoopbackHost<Daemon> {
    LoopbackHost::start(|data| {
        gateway::prepare_identity(data).map_err(std::io::Error::other)?;
        let mut host = daemon(data);
        host.log = EventLog::open(&data.join("mote.db"))?;
        host.watchers = Mutex::new(Watchers::open(rusqlite::Connection::open(
            data.join("mote.db"),
        )?)?);
        host.host_id = mote_gateway::HostIdentity::load_or_create(data)?
            .host_id()
            .clone();
        host.client = Ollama::new(models.base_url());
        let log = host.log.clone();
        Ok((Arc::new(host), log))
    })
    .unwrap()
}

fn call(id: &str, method: &str, params: Value) -> ClientFrame {
    ClientFrame::Call {
        request_id: RequestId::new(id).unwrap(),
        method: method.into(),
        params,
    }
}

fn snapshot(client: &mut mote_testkit::gateway::PairedClient, id: &str) -> HostSnapshot {
    let HostFrame::Result { result, .. } =
        client.exchange(&call(id, "snapshot", json!({}))).unwrap()
    else {
        panic!("snapshot result");
    };
    serde_json::from_value(result).unwrap()
}

fn subscribe(
    client: &mut mote_testkit::gateway::PairedClient,
    host: &mote_protocol::HostId,
    seq: i64,
) {
    client
        .send(&ClientFrame::Subscribe {
            request_id: RequestId::new("subscription").unwrap(),
            scope_id: "main".into(),
            cursor: Cursor::new(host.clone(), seq).unwrap(),
        })
        .unwrap();
}

#[test]
fn paired_loopback_chat_surface_interaction_and_approval_journey() {
    let models = mote_testkit::ScriptedServer::start().unwrap();
    models.set_chat_chunks(vec![json!({"message":{"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"greeting\"}"},"done":true}).to_string()]);
    let mut host = fixture(&models);
    assert_eq!(host.endpoint().unwrap().ip().to_string(), "127.0.0.1");
    let offer = host.open_pairing().unwrap();
    assert!(offer.starts_with("MOTE1-"));
    let mut client = host.pair(&offer, "Acceptance iPad").unwrap();
    let mut stream = host
        .pair(&host.open_pairing().unwrap(), "Event observer")
        .unwrap();
    let host_id = host.host_id().clone();
    assert!(
        matches!(client.exchange(&ClientFrame::Hello { versions: VersionRange::current() }).unwrap(), HostFrame::Hello { host_id: actual, protocol_major: 1, .. } if actual == host_id)
    );
    subscribe(&mut stream, &host_id, 0);
    assert!(
        matches!(client.exchange(&call("turn", "turn", json!({"thread":"main","message":"hello sk-abcdefghijklmnopqrstuvwxyz"}))).unwrap(), HostFrame::Result { result, .. } if result["status"] == "done")
    );
    host.daemon().call("matrix.render", json!({"thread":"main","source":"screen { text \"Ready\" button \"Refresh\" -> refresh }"})).unwrap();
    let events = host.events_after(0).unwrap();
    assert!(events.iter().any(|event| event.kind == "talk.delta"));
    assert!(events.iter().any(|event| event.kind == "talk.done"));
    let committed = events
        .iter()
        .find(|event| event.kind == "ui.surface")
        .unwrap();
    mote_ui::v1::validate_envelope(&serde_json::to_vec(&committed.payload).unwrap()).unwrap();
    let mut cursor = 0;
    for expected in &events {
        let HostFrame::Event {
            host_id: actual_host,
            event,
        } = stream.receive().unwrap()
        else {
            panic!("event");
        };
        assert_eq!(actual_host, host_id);
        assert_eq!(event.seq, expected.seq);
        assert_eq!(event.kind, expected.kind);
        if event.kind == "ui.surface" {
            mote_ui::v1::validate_envelope(&serde_json::to_vec(&event.payload).unwrap()).unwrap();
        }
        if event.kind == "talk.delta" {
            assert_eq!(
                event.payload["text"],
                "{\"route\":\"talk\",\"reason\":\"greeting\"}"
            );
        }
        assert!(event.seq > cursor);
        cursor = event.seq;
    }
    assert!(!serde_json::to_string(&events)
        .unwrap()
        .contains("sk-abcdefghijklmnopqrstuvwxyz"));
    assert_eq!(
        host.daemon().store.lock().unwrap().count_ledger().unwrap(),
        2
    );
    assert!(matches!(
        client
            .exchange(&ClientFrame::Interaction {
                request_id: RequestId::new("refresh").unwrap(),
                interaction: TypedInteraction {
                    host_id: host_id.clone(),
                    thread: "main".into(),
                    surface_seq: committed.seq,
                    surface_contract: 1,
                    node_path: vec![1],
                    event: "refresh".into(),
                    args: Default::default()
                }
            })
            .unwrap(),
        HostFrame::Result { .. }
    ));
    let after = host.events_after(cursor).unwrap();
    assert_eq!(
        after
            .iter()
            .filter(|event| event.kind == "ui.event")
            .count(),
        1
    );
    stream.force_disconnect();
    host.reconnect(&mut stream).unwrap();
    subscribe(&mut stream, &host_id, cursor);
    for expected in &after {
        assert!(
            matches!(stream.receive().unwrap(), HostFrame::Event { event, .. } if event.seq == expected.seq && event.seq > cursor)
        );
    }
    let target = host.root().join("approved.txt");
    host.daemon()
        .call(
            "file.write",
            json!({"thread":"main","path":target,"content":"approved once"}),
        )
        .unwrap();
    assert!(!target.exists());
    let view = snapshot(&mut client, "approval-snapshot");
    let approval = &view.pending_approvals[0];
    let decision = ApprovalDecision {
        host_id: host_id.clone(),
        thread: "main".into(),
        approval_id: approval.approval_id.clone(),
        action_hash: approval.action_hash.clone(),
        expires_at: approval.expires_at,
        decision: ApprovalVerdict::Approve,
    };
    let mut wrong = decision.clone();
    wrong.host_id = mote_protocol::HostId::from_public_key(&[99; 32]);
    assert!(client
        .exchange(&ClientFrame::ApprovalDecision {
            request_id: RequestId::new("wrong-host").unwrap(),
            decision: wrong
        })
        .is_err());
    assert!(!target.exists());
    host.reconnect(&mut client).unwrap();
    let request = ClientFrame::ApprovalDecision {
        request_id: RequestId::new("approval").unwrap(),
        decision,
    };
    let result = client.exchange(&request).unwrap();
    assert!(matches!(result, HostFrame::Result { .. }));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "approved once");
    let before = host.events_after(0).unwrap().len();
    client.force_disconnect();
    let old = host.endpoint().unwrap();
    host.rotate_endpoint().unwrap();
    assert_ne!(old, host.endpoint().unwrap());
    host.reconnect(&mut client).unwrap();
    assert_eq!(client.host_id(), &host_id);
    assert_eq!(client.exchange(&request).unwrap(), result);
    assert_eq!(host.events_after(0).unwrap().len(), before);
}

#[test]
fn paired_loopback_suspend_reconnect_and_retention_resync() {
    let models = mote_testkit::ScriptedServer::start().unwrap();
    let mut host = fixture(&models);
    let mut client = host
        .pair(&host.open_pairing().unwrap(), "Recovery iPad")
        .unwrap();
    let host_id = host.host_id().clone();
    host.daemon()
        .log
        .append("main", "turn.user", &json!({"text":"before suspension"}))
        .unwrap();
    let target = host.root().join("pending.txt");
    host.daemon()
        .call(
            "file.write",
            json!({"thread":"main","path":target,"content":"pending"}),
        )
        .unwrap();
    host.daemon()
        .call(
            "matrix.render",
            json!({"thread":"main","source":"screen { text \"Retained\" }"}),
        )
        .unwrap();
    let initial = snapshot(&mut client, "initial");
    assert_eq!(initial.pending_approvals.len(), 1);
    assert_eq!(initial.surfaces.len(), 1);
    assert_eq!(initial.surfaces[0].tree["version"], 1);
    let mut directory = HostDirectory::default();
    directory
        .register(
            PairedHost::new(*host.public_key(), "Fixture", None, VersionRange::current()).unwrap(),
        )
        .unwrap();
    directory.select_host(&host_id).unwrap();
    directory
        .install_snapshot(&host_id, initial.clone())
        .unwrap();
    subscribe(&mut client, &host_id, initial.cursor.seq);
    host.suspend();
    assert!(client.receive().is_err());
    directory
        .set_connection(&host_id, ConnectionState::Offline)
        .unwrap();
    host.fill_retention("main", 5001).unwrap();
    host.daemon()
        .log
        .append("other", "turn.user", &json!({"text":"during suspension"}))
        .unwrap();
    host.resume().unwrap();
    directory
        .set_connection(&host_id, ConnectionState::Connecting)
        .unwrap();
    host.reconnect(&mut client).unwrap();
    assert_eq!(client.host_id(), &host_id);
    subscribe(&mut client, &host_id, initial.cursor.seq);
    let resync = client.receive().unwrap();
    assert!(matches!(&resync, HostFrame::Event { event, .. } if event.kind == "resync"));
    directory.receive_frame(&host_id, resync).unwrap();
    let frame = client.receive().unwrap();
    let HostFrame::Snapshot { snapshot } = &frame else {
        panic!("authoritative snapshot");
    };
    assert_eq!(snapshot.pending_approvals, initial.pending_approvals);
    assert_eq!(snapshot.threads.len(), 2);
    assert_eq!(snapshot.threads[0].messages[0].text, "before suspension");
    assert_eq!(snapshot.threads[1].messages[0].text, "during suspension");
    assert_eq!(snapshot.surfaces, initial.surfaces);
    directory.receive_frame(&host_id, frame).unwrap();
    assert_eq!(
        directory.state(&host_id).unwrap().connection(),
        &ConnectionState::Online
    );
    assert_eq!(directory.selected_host(), Some(&host_id));
    let seq = directory.state(&host_id).unwrap().cursor().seq;
    let live = host
        .daemon()
        .log
        .append("main", "turn.user", &json!({"text":"after recovery"}))
        .unwrap();
    let frame = client.receive().unwrap();
    assert!(
        matches!(&frame, HostFrame::Event { event, .. } if event.seq == live.seq && event.seq > seq)
    );
    directory.receive_frame(&host_id, frame.clone()).unwrap();
    directory.receive_frame(&host_id, frame).unwrap();
    assert_eq!(
        directory.state(&host_id).unwrap().threads()["main"]
            .messages
            .len(),
        2
    );
    assert!(!target.exists());
}
