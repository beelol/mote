use mote_host_client::{HostDirectory, PairedHost};
use mote_protocol::{Event, HostId, HostSnapshot, HostStatus, PendingApproval, VersionRange};
use serde_json::{json, Value};

fn directory() -> (HostDirectory, HostId) {
    let host = PairedHost::new([17; 32], "Approval host", None, VersionRange::current()).unwrap();
    let id = host.host_id().clone();
    let mut directory = HostDirectory::default();
    directory.register(host).unwrap();
    directory.select_host(&id).unwrap();
    (directory, id)
}

fn record() -> Value {
    json!({
        "approval_id": "opaque-browser-request",
        "thread": "main", "kind": "browser",
        "payload": {"site": "http://127.0.0.1", "effect": "submit"},
        "action_hash": "0000000000000000000000000000000000000000000000000000000000000000",
        "expires_at": 4102444800000_i64
    })
}

fn event(host: &HostId, sequence: i64, kind: &str, status: &str) -> Event {
    Event {
        seq: sequence,
        ts: 0,
        scope_id: "main".into(),
        kind: kind.into(),
        payload: json!({"host_id": host, "approval": record(), "status": status}),
    }
}

fn pending_snapshot(directory: &mut HostDirectory, host: &HostId) {
    directory
        .install_snapshot(
            host,
            HostSnapshot {
                host_id: host.clone(),
                cursor: mote_protocol::Cursor::new(host.clone(), 0).unwrap(),
                threads: Vec::new(),
                surfaces: Vec::new(),
                pending_approvals: vec![
                    serde_json::from_value::<PendingApproval>(record()).unwrap()
                ],
                status: HostStatus::Online,
            },
        )
        .unwrap();
}

#[test]
fn streamed_pending_approval_survives_directory_reload_without_a_duplicate() {
    let (mut directory, host) = directory();
    let created = event(&host, 1, "approval.created", "pending");
    assert!(directory
        .apply_event(&host, &host, created.clone())
        .unwrap());
    assert!(!directory.apply_event(&host, &host, created).unwrap());
    let reloaded: HostDirectory =
        serde_json::from_value(serde_json::to_value(&directory).unwrap()).unwrap();
    let pending = reloaded.state(&host).unwrap().pending_approvals();
    assert_eq!(pending.len(), 1);
    let approval = pending.values().next().unwrap();
    assert_eq!(approval.approval_id.as_str(), "opaque-browser-request");
    assert_eq!(approval.thread, "main");
    assert_eq!(approval.expires_at, 4102444800000_i64);
}

#[test]
fn every_terminal_stream_update_removes_the_pending_decision() {
    for status in ["approved", "denied", "expired", "failed"] {
        let (mut directory, host) = directory();
        pending_snapshot(&mut directory, &host);
        assert!(directory
            .apply_event(&host, &host, event(&host, 1, "approval.updated", status))
            .unwrap());
        assert!(
            directory
                .state(&host)
                .unwrap()
                .pending_approvals()
                .is_empty(),
            "{status}"
        );
    }
}

#[test]
fn foreign_payload_host_cannot_create_an_approval_or_advance_the_cursor() {
    let (mut directory, host) = directory();
    let foreign = HostId::from_public_key(&[18; 32]);
    assert!(directory
        .apply_event(
            &host,
            &host,
            event(&foreign, 1, "approval.created", "pending")
        )
        .is_err());
    assert!(directory
        .state(&host)
        .unwrap()
        .pending_approvals()
        .is_empty());
    assert_eq!(directory.state(&host).unwrap().cursor().seq, 0);
}

#[test]
fn malformed_approval_envelopes_are_atomic_refusals() {
    for (field, value) in [
        ("status", json!("unknown")),
        ("thread", json!("other")),
        ("expires_at", json!(-1)),
    ] {
        let (mut directory, host) = directory();
        let mut created = event(&host, 1, "approval.created", "pending");
        if field == "status" {
            created.payload[field] = value;
        } else {
            created.payload["approval"][field] = value;
        }
        assert!(
            directory.apply_event(&host, &host, created).is_err(),
            "{field}"
        );
        assert!(directory
            .state(&host)
            .unwrap()
            .pending_approvals()
            .is_empty());
        assert_eq!(directory.state(&host).unwrap().cursor().seq, 0);
    }
}

#[test]
fn an_existing_approval_cannot_silently_change_its_decision_authority() {
    let (mut directory, host) = directory();
    pending_snapshot(&mut directory, &host);
    let mut changed = event(&host, 1, "approval.updated", "pending");
    changed.payload["approval"]["action_hash"] =
        json!("1111111111111111111111111111111111111111111111111111111111111111");
    assert!(directory.apply_event(&host, &host, changed).is_err());
    assert_eq!(
        directory
            .state(&host)
            .unwrap()
            .pending_approvals()
            .values()
            .next()
            .unwrap()
            .action_hash
            .as_str(),
        "0000000000000000000000000000000000000000000000000000000000000000"
    );
    assert_eq!(directory.state(&host).unwrap().cursor().seq, 0);
}

#[test]
fn a_terminal_status_is_retained_and_cannot_return_to_pending() {
    let (mut directory, host) = directory();
    directory
        .apply_event(&host, &host, event(&host, 1, "approval.created", "pending"))
        .unwrap();
    directory
        .apply_event(&host, &host, event(&host, 2, "approval.updated", "denied"))
        .unwrap();
    let serialized = serde_json::to_value(&directory).unwrap();
    assert!(serialized.to_string().contains("\"status\":\"denied\""));
    let mut reloaded: HostDirectory = serde_json::from_value(serialized).unwrap();
    assert!(reloaded
        .apply_event(&host, &host, event(&host, 3, "approval.created", "pending"))
        .is_err());
    assert!(reloaded
        .state(&host)
        .unwrap()
        .pending_approvals()
        .is_empty());
    assert_eq!(reloaded.state(&host).unwrap().cursor().seq, 2);
}

#[test]
fn a_terminal_status_cannot_be_replaced_by_a_different_outcome() {
    let (mut directory, host) = directory();
    directory
        .apply_event(
            &host,
            &host,
            event(&host, 1, "approval.updated", "approved"),
        )
        .unwrap();
    assert!(directory
        .apply_event(&host, &host, event(&host, 2, "approval.updated", "failed"))
        .is_err());
    assert_eq!(directory.state(&host).unwrap().cursor().seq, 1);
}

#[test]
fn an_update_cannot_replace_the_displayed_site_under_the_same_authority() {
    let (mut directory, host) = directory();
    pending_snapshot(&mut directory, &host);
    let mut changed = event(&host, 1, "approval.updated", "pending");
    changed.payload["approval"]["payload"]["site"] = json!("https://other.example");
    assert!(directory.apply_event(&host, &host, changed).is_err());
    assert_eq!(
        directory
            .state(&host)
            .unwrap()
            .pending_approvals()
            .values()
            .next()
            .unwrap()
            .payload["site"],
        "http://127.0.0.1"
    );
    assert_eq!(directory.state(&host).unwrap().cursor().seq, 0);
}

#[test]
fn a_stale_snapshot_cannot_resurrect_a_terminal_approval() {
    let (mut directory, host) = directory();
    directory
        .apply_event(&host, &host, event(&host, 1, "approval.updated", "expired"))
        .unwrap();
    let before = directory.clone();
    assert!(directory
        .install_snapshot(
            &host,
            HostSnapshot {
                host_id: host.clone(),
                cursor: mote_protocol::Cursor::new(host.clone(), 0).unwrap(),
                threads: vec![],
                surfaces: vec![],
                pending_approvals: vec![serde_json::from_value(record()).unwrap()],
                status: HostStatus::Online
            }
        )
        .is_err());
    assert_eq!(directory, before);
}

#[test]
fn legacy_pending_state_hydrates_the_same_card_without_credentials() {
    let (mut directory, host) = directory();
    pending_snapshot(&mut directory, &host);
    let mut value = serde_json::to_value(directory.state(&host).unwrap()).unwrap();
    value.as_object_mut().unwrap().remove("approval_states");
    let reloaded: mote_host_client::HostSessionState = serde_json::from_value(value).unwrap();
    let card = reloaded.approval_states().values().next().unwrap();
    assert_eq!(card.status, mote_host_client::ApprovalStatus::Pending);
    assert_eq!(
        card.approval,
        reloaded
            .pending_approvals()
            .values()
            .next()
            .unwrap()
            .clone()
    );
    assert_eq!(
        reloaded.connection(),
        &mote_host_client::ConnectionState::Offline
    );
}

#[test]
fn a_legacy_decision_event_does_not_erase_a_terminal_tombstone() {
    let (mut directory, host) = directory();
    directory
        .apply_event(&host, &host, event(&host, 1, "approval.updated", "denied"))
        .unwrap();
    directory
        .apply_event(
            &host,
            &host,
            Event {
                seq: 2,
                ts: 0,
                scope_id: "main".into(),
                kind: "approval.decided".into(),
                payload: json!({"approval_id":"opaque-browser-request"}),
            },
        )
        .unwrap();
    assert_eq!(directory.state(&host).unwrap().approval_states().len(), 1);
    assert!(directory
        .apply_event(&host, &host, event(&host, 3, "approval.created", "pending"))
        .is_err());
}

#[test]
fn a_pending_snapshot_cannot_change_the_record_used_for_a_safe_retry() {
    let (mut directory, host) = directory();
    pending_snapshot(&mut directory, &host);
    let before = directory.clone();
    let mut changed = record();
    changed["payload"]["site"] = json!("https://other.example");
    assert!(directory
        .install_snapshot(
            &host,
            HostSnapshot {
                host_id: host.clone(),
                cursor: mote_protocol::Cursor::new(host.clone(), 1).unwrap(),
                threads: vec![],
                surfaces: vec![],
                pending_approvals: vec![serde_json::from_value(changed).unwrap()],
                status: HostStatus::Online
            }
        )
        .is_err());
    assert_eq!(directory, before);
}

#[test]
fn repeated_stale_pending_snapshots_cannot_roll_back_the_retry_watermark() {
    let (mut directory, host) = directory();
    directory
        .apply_event(&host, &host, event(&host, 2, "approval.created", "pending"))
        .unwrap();
    let before = directory.clone();
    for _ in 0..2 {
        assert!(directory
            .install_snapshot(
                &host,
                HostSnapshot {
                    host_id: host.clone(),
                    cursor: mote_protocol::Cursor::new(host.clone(), 1).unwrap(),
                    threads: vec![],
                    surfaces: vec![],
                    pending_approvals: vec![serde_json::from_value(record()).unwrap()],
                    status: HostStatus::Online
                }
            )
            .is_err());
        assert_eq!(directory, before);
    }
}
