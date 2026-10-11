use mote_host_client::{
    ConnectionState, CredentialStore, HostDirectory, HostSessionState, Outbox, PairedHost,
};
use mote_protocol::{
    ActionDigest, ApprovalDecision, ApprovalVerdict, ChatMessage, ClientFrame, CommittedSurface,
    Cursor, Event, HostErrorCode, HostId, HostSnapshot, MessageRole, MessageStatus,
    PendingApproval, RequestId, ThreadSnapshot, TypedInteraction, VersionRange,
};
use serde_json::json;
use std::collections::BTreeMap;
use std::io;

#[derive(Default)]
struct Credentials(BTreeMap<HostId, Vec<u8>>);

impl CredentialStore for Credentials {
    type Credential = Vec<u8>;

    fn load(&self, host: &HostId) -> io::Result<Option<Self::Credential>> {
        Ok(self.0.get(host).cloned())
    }

    fn save(&mut self, host: &HostId, credential: &Self::Credential) -> io::Result<()> {
        self.0.insert(host.clone(), credential.clone());
        Ok(())
    }

    fn delete(&mut self, host: &HostId) -> io::Result<()> {
        self.0.remove(host);
        Ok(())
    }
}

fn directory() -> (HostDirectory, HostId, HostId) {
    let mut directory = HostDirectory::default();
    let a = PairedHost::new([1; 32], "Home Mac", None, VersionRange::current()).unwrap();
    let b = PairedHost::new(
        [2; 32],
        "Office Mac",
        Some("Office".into()),
        VersionRange::current(),
    )
    .unwrap();
    let a_id = a.host_id().clone();
    let b_id = b.host_id().clone();
    directory.register(a).unwrap();
    directory.register(b).unwrap();
    (directory, a_id, b_id)
}

fn snapshot(host: &HostId, label: &str) -> HostSnapshot {
    HostSnapshot {
        status: mote_protocol::HostStatus::Online,
        host_id: host.clone(),
        cursor: Cursor::new(host.clone(), 7).unwrap(),
        threads: vec![ThreadSnapshot {
            thread: "shared-thread".into(),
            messages: vec![ChatMessage {
                id: "shared-message".into(),
                role: MessageRole::Assistant,
                text: label.into(),
                status: MessageStatus::Delivered,
            }],
        }],
        surfaces: vec![CommittedSurface {
            thread: "shared-thread".into(),
            surface_seq: 4,
            surface_contract: 1,
            tree: json!({"text": label}).as_object().unwrap().clone(),
        }],
        pending_approvals: vec![PendingApproval {
            approval_id: RequestId::new("shared-approval").unwrap(),
            thread: "shared-thread".into(),
            kind: "fixture".into(),
            payload: json!({"label": label}).as_object().unwrap().clone(),
            action_hash: ActionDigest::new(if label == "home" { "a" } else { "b" }.repeat(64))
                .unwrap(),
            expires_at: 1000,
        }],
    }
}

fn decision(host: &HostId, label: &str) -> ClientFrame {
    ClientFrame::ApprovalDecision {
        request_id: RequestId::new("shared-request").unwrap(),
        decision: ApprovalDecision {
            host_id: host.clone(),
            thread: "shared-thread".into(),
            approval_id: RequestId::new("shared-approval").unwrap(),
            action_hash: ActionDigest::new(if label == "home" { "a" } else { "b" }.repeat(64))
                .unwrap(),
            expires_at: 1000,
            decision: ApprovalVerdict::Approve,
        },
    }
}

#[test]
fn colliding_host_ids_are_isolated() {
    let (mut directory, a, b) = directory();
    let mut credentials = Credentials::default();
    credentials.save(&a, &b"home-secret".to_vec()).unwrap();
    credentials.save(&b, &b"office-secret".to_vec()).unwrap();
    for (host, label, thread) in [
        (&a, "home", "home-selection"),
        (&b, "office", "office-selection"),
    ] {
        directory
            .install_snapshot(host, snapshot(host, label))
            .unwrap();
        directory.select_thread(host, Some(thread.into())).unwrap();
        directory.enqueue(host, decision(host, label)).unwrap();
    }
    for _ in 0..5 {
        for (host, label, thread, secret) in [
            (&a, "home", "home-selection", "home-secret"),
            (&b, "office", "office-selection", "office-secret"),
        ] {
            directory.select_host(host).unwrap();
            assert_eq!(directory.selected_host(), Some(host));
            let state = directory.state(host).unwrap();
            assert_eq!(state.cursor().host_id, *host);
            assert_eq!(state.cursor().seq, 7);
            assert_eq!(state.selected_thread(), Some(thread));
            assert_eq!(state.threads()["shared-thread"].messages[0].text, label);
            assert_eq!(state.surfaces()["shared-thread"][&4].tree["text"], label);
            assert_eq!(
                state.pending_approvals()[&RequestId::new("shared-approval").unwrap()].payload
                    ["label"],
                label
            );
            assert_eq!(
                state.outbox().pending(host).unwrap(),
                vec![&decision(host, label)]
            );
            assert_eq!(credentials.load(host).unwrap().unwrap(), secret.as_bytes());
        }
    }
    let event = Event {
        seq: 8,
        ts: 0,
        scope_id: "main".into(),
        kind: "fixture".into(),
        payload: json!({}),
    };
    assert!(directory.apply_event(&a, &a, event.clone()).unwrap());
    assert!(!directory.apply_event(&a, &a, event.clone()).unwrap());
    assert_eq!(directory.state(&b).unwrap().cursor().seq, 7);
    assert!(directory.apply_event(&b, &b, event).unwrap());
    directory
        .acknowledge(&a, &RequestId::new("shared-request").unwrap())
        .unwrap();
    assert!(directory
        .state(&a)
        .unwrap()
        .outbox()
        .pending(&a)
        .unwrap()
        .is_empty());
    assert_eq!(
        directory
            .state(&b)
            .unwrap()
            .outbox()
            .pending(&b)
            .unwrap()
            .len(),
        1
    );
    credentials.delete(&a).unwrap();
    assert_eq!(credentials.load(&a).unwrap(), None);
    assert_eq!(credentials.load(&b).unwrap().unwrap(), b"office-secret");
}

#[test]
fn directory_roundtrip_preserves_host_state_without_credentials() {
    let (mut directory, a, b) = directory();
    let mut credentials = Credentials::default();
    credentials
        .save(&a, &b"private-key-material".to_vec())
        .unwrap();
    directory
        .install_snapshot(&a, snapshot(&a, "home"))
        .unwrap();
    directory.select_host(&b).unwrap();
    let serialized = serde_json::to_string(&directory).unwrap();
    assert!(!serialized.contains("private-key-material"));
    assert!(!serialized.contains("credential"));
    let restored: HostDirectory = serde_json::from_str(&serialized).unwrap();
    assert_eq!(restored.selected_host(), Some(&b));
    assert_eq!(restored.state(&a).unwrap().cursor().seq, 7);
    assert_eq!(restored.host(&a).unwrap().public_key(), &[1; 32]);
    assert_eq!(restored.host(&b).unwrap().name(), "Office Mac");
}

#[test]
fn foreign_host_reducers_and_actions_fail_without_mutation() {
    let (mut directory, a, b) = directory();
    directory
        .install_snapshot(&a, snapshot(&a, "home"))
        .unwrap();
    directory
        .install_snapshot(&b, snapshot(&b, "office"))
        .unwrap();
    let before = serde_json::to_value(&directory).unwrap();
    assert_eq!(
        directory
            .install_snapshot(&a, snapshot(&b, "office"))
            .unwrap_err()
            .code,
        HostErrorCode::IdentityChanged
    );
    assert_eq!(
        directory
            .enqueue(&a, decision(&b, "office"))
            .unwrap_err()
            .code,
        HostErrorCode::IdentityChanged
    );
    assert_eq!(
        directory
            .apply_event(
                &a,
                &b,
                Event {
                    seq: 8,
                    ts: 0,
                    scope_id: "main".into(),
                    kind: "fixture".into(),
                    payload: json!({})
                }
            )
            .unwrap_err()
            .code,
        HostErrorCode::IdentityChanged
    );
    assert!(directory.state(&a).unwrap().outbox().pending(&b).is_err());
    assert_eq!(serde_json::to_value(&directory).unwrap(), before);
}

#[test]
fn corrupted_directory_identity_and_cursor_are_rejected() {
    let (directory, a, b) = directory();
    let mut value = serde_json::to_value(&directory).unwrap();
    value["hosts"][a.as_str()]["host"]["public_key"] = json!([2; 32].to_vec());
    assert!(serde_json::from_value::<HostDirectory>(value).is_err());
    let mut value = serde_json::to_value(&directory).unwrap();
    value["hosts"][a.as_str()]["state"]["cursor"]["host_id"] = json!(b);
    assert!(serde_json::from_value::<HostDirectory>(value).is_err());
}

#[test]
fn unpaired_hosts_and_duplicate_registration_do_not_reset_state() {
    let (mut directory, a, _) = directory();
    directory
        .install_snapshot(&a, snapshot(&a, "home"))
        .unwrap();
    assert!(directory
        .register(PairedHost::new([1; 32], "Duplicate", None, VersionRange::current()).unwrap())
        .is_err());
    assert_eq!(directory.state(&a).unwrap().cursor().seq, 7);
    let unknown = HostId::from_public_key(&[3; 32]);
    assert!(directory.select_host(&unknown).is_err());
    assert!(directory
        .set_connection(&unknown, ConnectionState::Online)
        .is_err());
    assert_eq!(directory.selected_host(), None);
}

#[test]
fn selection_does_not_route_requests_and_request_ids_are_immutable() {
    let (mut directory, a, b) = directory();
    directory.select_host(&b).unwrap();
    let call = ClientFrame::Call {
        request_id: RequestId::new("call").unwrap(),
        method: "status".into(),
        params: json!({}),
    };
    directory.enqueue(&a, call.clone()).unwrap();
    directory.enqueue(&a, call).unwrap();
    let altered = ClientFrame::Call {
        request_id: RequestId::new("call").unwrap(),
        method: "turn".into(),
        params: json!({"text": "changed"}),
    };
    assert!(directory.enqueue(&a, altered).is_err());
    assert_eq!(
        directory
            .state(&a)
            .unwrap()
            .outbox()
            .pending(&a)
            .unwrap()
            .len(),
        1
    );
    assert!(directory
        .state(&b)
        .unwrap()
        .outbox()
        .pending(&b)
        .unwrap()
        .is_empty());
    directory
        .set_connection(&a, ConnectionState::Offline)
        .unwrap();
    directory
        .set_connection(&b, ConnectionState::IdentityChanged)
        .unwrap();
    assert_eq!(directory.selected_host(), Some(&b));
    assert_eq!(
        directory.state(&a).unwrap().connection(),
        &ConnectionState::Offline
    );
    assert_eq!(
        directory.state(&b).unwrap().connection(),
        &ConnectionState::IdentityChanged
    );
}

#[test]
fn interactions_and_subscriptions_cannot_cross_hosts() {
    let (mut directory, a, b) = directory();
    let interaction = ClientFrame::Interaction {
        request_id: RequestId::new("tap").unwrap(),
        interaction: TypedInteraction {
            host_id: b.clone(),
            thread: "shared-thread".into(),
            surface_seq: 4,
            surface_contract: 1,
            node_path: vec![0],
            event: "tap".into(),
            args: Default::default(),
        },
    };
    assert!(directory.enqueue(&a, interaction).is_err());
    assert!(directory
        .enqueue(
            &a,
            ClientFrame::Subscribe {
                request_id: RequestId::new("subscribe").unwrap(),
                scope_id: "main".into(),
                cursor: Cursor::new(b, 0).unwrap()
            }
        )
        .is_err());
    assert!(directory
        .enqueue(
            &a,
            ClientFrame::Hello {
                versions: VersionRange::current()
            }
        )
        .is_err());
    assert!(directory
        .state(&a)
        .unwrap()
        .outbox()
        .pending(&a)
        .unwrap()
        .is_empty());
}

#[test]
fn standalone_host_deserialization_cannot_bypass_identity_checks() {
    let (directory, a, b) = directory();
    let mut host = serde_json::to_value(directory.host(&a).unwrap()).unwrap();
    host["host_id"] = json!(b);
    assert!(serde_json::from_value::<PairedHost>(host).is_err());
}

#[test]
fn standalone_outbox_deserialization_cannot_bypass_identity_checks() {
    let (_, a, b) = directory();
    let mut outbox = Outbox::new(&a);
    outbox.enqueue(&a, decision(&a, "home")).unwrap();
    let mut serialized = serde_json::to_value(&outbox).unwrap();
    serialized["host_id"] = json!(b);
    assert!(serde_json::from_value::<Outbox>(serialized).is_err());
}

#[test]
fn standalone_session_deserialization_cannot_bypass_identity_checks() {
    let (directory, a, b) = directory();
    let mut state = serde_json::to_value(directory.state(&a).unwrap()).unwrap();
    state["outbox"]["host_id"] = json!(b);
    assert!(serde_json::from_value::<HostSessionState>(state).is_err());
}

#[test]
fn outbox_preserves_action_order_when_request_ids_sort_differently() {
    let (_, a, _) = directory();
    let mut outbox = Outbox::new(&a);
    for (id, method) in [("z-first", "turn"), ("a-second", "status")] {
        outbox
            .enqueue(
                &a,
                ClientFrame::Call {
                    request_id: RequestId::new(id).unwrap(),
                    method: method.into(),
                    params: json!({}),
                },
            )
            .unwrap();
    }
    let restored: Outbox = serde_json::from_value(serde_json::to_value(&outbox).unwrap()).unwrap();
    let methods: Vec<_> = restored
        .pending(&a)
        .unwrap()
        .into_iter()
        .map(|frame| match frame {
            ClientFrame::Call { method, .. } => method.as_str(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(methods, ["turn", "status"]);
}
