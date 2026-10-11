use mote_gateway::GatewayConfig;
use mote_host_client::{CredentialStore, HostCommand, HostDirectory, HostUpdate, PairedHost};
use mote_ios::{keychain::StoredCredential, live::NetworkHost};
use mote_ipc::Dispatch;
use mote_protocol::{ClientFrame, HostFrame, RequestId, VersionRange};
use mote_testkit::ScriptedServer;
use serde_json::json;
use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};

#[derive(Default)]
struct MemoryStore(BTreeMap<mote_protocol::HostId, StoredCredential>);
impl CredentialStore for MemoryStore {
    type Credential = StoredCredential;
    fn load(&self, id: &mote_protocol::HostId) -> io::Result<Option<StoredCredential>> {
        Ok(self.0.get(id).cloned())
    }
    fn save(&mut self, id: &mote_protocol::HostId, value: &StoredCredential) -> io::Result<()> {
        self.0.insert(id.clone(), value.clone());
        Ok(())
    }
    fn delete(&mut self, id: &mote_protocol::HostId) -> io::Result<()> {
        self.0.remove(id);
        Ok(())
    }
}

#[test]
fn pairs_real_daemon_and_resumes_without_duplicate_chat() {
    let models = ScriptedServer::start().unwrap();
    models.set_chat_chunks(vec![json!({"message":{"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"greeting\"}"},"done":true}).to_string()]);
    let host = moted::test_fixture::start(&models).unwrap();
    let store = Arc::new(Mutex::new(MemoryStore::default()));
    let config = GatewayConfig::new(host.root());
    let endpoint = host.endpoint().unwrap();
    let offer = host.open_pairing().unwrap();
    let mut network = std::thread::scope(|scope| {
        let pair =
            scope.spawn(|| NetworkHost::pair(endpoint, config, &offer, "iPad", store.clone()));
        while !pair.is_finished() {
            host.confirm_pending(true).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        pair.join().unwrap().unwrap()
    });
    let id = host.host_id().clone();
    let mut directory = HostDirectory::default();
    directory
        .register(
            PairedHost::new(*host.public_key(), "Mac", None, VersionRange::current()).unwrap(),
        )
        .unwrap();
    directory.select_host(&id).unwrap();
    let frames = network
        .request(HostCommand {
            host_id: id.clone(),
            frame: ClientFrame::Call {
                request_id: RequestId::new("first-chat").unwrap(),
                method: "turn".into(),
                params: json!({"thread":"main","message":"hello"}),
            },
        })
        .unwrap();
    assert!(frames
        .iter()
        .any(|frame| matches!(frame, HostFrame::Acknowledgement { .. })));
    host.daemon().call("matrix.render", json!({"thread":"main","source":"screen { text \"Live dashboard\" select \"Talk model\" models.talk options model_options }"})).unwrap();
    let committed = network
        .snapshot()
        .unwrap()
        .surfaces
        .into_iter()
        .find(|surface| surface.thread == "main")
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !host
        .events_after(0)
        .unwrap()
        .iter()
        .any(|event| event.kind == "talk.done")
    {
        assert!(
            std::time::Instant::now() < deadline,
            "chat did not reach its ledger boundary"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let calls_before = models
        .recorded()
        .iter()
        .filter(|request| matches!(request.path.as_str(), "/api/chat" | "/api/generate"))
        .count();
    let interaction = ClientFrame::Interaction {
        request_id: RequestId::new("change-talk").unwrap(),
        interaction: mote_protocol::TypedInteraction {
            host_id: id.clone(),
            thread: "main".into(),
            surface_seq: committed.surface_seq,
            surface_contract: 1,
            node_path: vec![1],
            event: "change".into(),
            args: json!({"field":"models.talk","value":"qwen3:0.6b"})
                .as_object()
                .unwrap()
                .clone(),
        },
    };
    assert!(matches!(
        network
            .request(HostCommand {
                host_id: id.clone(),
                frame: interaction
            })
            .unwrap()[0],
        HostFrame::Result { .. }
    ));
    assert_eq!(
        models
            .recorded()
            .iter()
            .filter(|request| matches!(request.path.as_str(), "/api/chat" | "/api/generate"))
            .count(),
        calls_before
    );
    let wrong = mote_protocol::HostId::from_public_key(&[99; 32]);
    assert!(network
        .request(HostCommand {
            host_id: wrong,
            frame: ClientFrame::Call {
                request_id: RequestId::new("wrong-host").unwrap(),
                method: "turn".into(),
                params: json!({"thread":"main","message":"must not execute"})
            }
        })
        .is_err());
    let mut stream = network
        .subscribe(directory.state(&id).unwrap().cursor())
        .unwrap();
    while directory
        .state(&id)
        .unwrap()
        .threads()
        .get("main")
        .is_none_or(|thread| thread.messages.len() < 2)
    {
        directory
            .reduce_update(HostUpdate::Frame {
                host_id: id.clone(),
                frame: stream.receive().unwrap(),
            })
            .unwrap();
    }
    let cursor = directory.state(&id).unwrap().cursor().clone();
    let persisted = serde_json::to_vec(&directory).unwrap();
    let mut restored: HostDirectory = serde_json::from_slice(&persisted).unwrap();
    let mut relaunched = network.reopen().unwrap();
    let snapshot = relaunched.snapshot().unwrap();
    restored
        .reduce_update(HostUpdate::Frame {
            host_id: id.clone(),
            frame: HostFrame::Snapshot { snapshot },
        })
        .unwrap();
    assert!(restored.state(&id).unwrap().cursor().seq >= cursor.seq);
    let messages = &restored.state(&id).unwrap().threads()["main"].messages;
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.text == "hello")
            .count(),
        1
    );
    assert_eq!(
        restored.state(&id).unwrap().surfaces()["main"]
            .last_key_value()
            .unwrap()
            .1
            .tree["version"],
        1
    );
    assert_eq!(
        host.events_after(0)
            .unwrap()
            .iter()
            .filter(|event| event.kind == "turn.user")
            .count(),
        1
    );
    let credential = store
        .lock()
        .unwrap()
        .load(&id)
        .unwrap()
        .unwrap()
        .restore(&id)
        .unwrap();
    let devices = mote_gateway::DeviceStore::open(&host.root().join("data/mote.db"), &id).unwrap();
    assert!(devices.revoke(credential.public_key()).unwrap());
    assert!(network.reopen().is_err());
    store.lock().unwrap().delete(&id).unwrap();
    assert!(network.reopen().is_err());
}

#[test]
fn discovered_port_moves_only_after_identity_proof() {
    use mote_gateway::discovery::DiscoveryRecord;
    let models = ScriptedServer::start().unwrap();
    let mut host = moted::test_fixture::start(&models).unwrap();
    let impostor = moted::test_fixture::start(&models).unwrap();
    let store = Arc::new(Mutex::new(MemoryStore::default()));
    let offer = host.open_pairing().unwrap();
    let mut network = std::thread::scope(|scope| {
        let pair = scope.spawn(|| {
            NetworkHost::pair(
                host.endpoint().unwrap(),
                GatewayConfig::new(host.root()),
                &offer,
                "iPad",
                store,
            )
        });
        while !pair.is_finished() {
            host.confirm_pending(true).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        pair.join().unwrap().unwrap()
    });
    let original = network.endpoint();
    let mut command_connection = network.reopen().unwrap();
    let forged = DiscoveryRecord::new(impostor.endpoint().unwrap(), 1, host.public_key()).unwrap();
    assert!(network.rediscover(&forged).is_err());
    assert_eq!(network.endpoint(), original);
    host.rotate_endpoint().unwrap();
    let moved = DiscoveryRecord::new(host.endpoint().unwrap(), 1, host.public_key()).unwrap();
    network.rediscover(&moved).unwrap();
    assert_ne!(network.endpoint(), original);
    assert_eq!(network.snapshot().unwrap().host_id, *host.host_id());
    let outcome = command_connection
        .request(HostCommand {
            host_id: host.host_id().clone(),
            frame: ClientFrame::Call {
                request_id: RequestId::new("after-port-move").unwrap(),
                method: "ping".into(),
                params: json!({}),
            },
        })
        .unwrap();
    assert!(matches!(outcome[0], HostFrame::Result { .. }));
}
