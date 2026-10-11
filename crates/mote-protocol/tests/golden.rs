use mote_protocol::{ClientFrame, Cursor, HostFrame, HostId, RequestId, VersionRange};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

const HOST: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn round_trip<T: DeserializeOwned + Serialize>(fixture: &str) {
    let expected: Value = serde_json::from_str(fixture).unwrap();
    let frame: T = serde_json::from_str(fixture).unwrap();
    assert_eq!(serde_json::to_value(frame).unwrap(), expected);
}

#[test]
fn client_frames_preserve_the_golden_wire_contract() {
    for fixture in [
        include_str!("golden/hello-client.json"),
        include_str!("golden/call.json"),
        include_str!("golden/subscribe.json"),
        include_str!("golden/interaction.json"),
        include_str!("golden/approval-decision.json"),
    ] {
        round_trip::<ClientFrame>(fixture);
    }
}

#[test]
fn host_frames_preserve_the_golden_wire_contract() {
    for fixture in [
        include_str!("golden/hello-host.json"),
        include_str!("golden/result.json"),
        include_str!("golden/event.json"),
        include_str!("golden/committed-surface.json"),
        include_str!("golden/snapshot.json"),
        include_str!("golden/acknowledgement.json"),
        include_str!("golden/error.json"),
        include_str!("golden/request-pending.json"),
        include_str!("golden/request-indeterminate.json"),
    ] {
        round_trip::<HostFrame>(fixture);
    }
}

#[test]
fn request_states_are_typed_correlated_and_fail_closed() {
    for state in ["pending", "indeterminate"] {
        round_trip::<HostFrame>(
            &json!({"type":"request_state","request_id":"sk-abcdefghijklmnopqrstuv","state":state})
                .to_string(),
        );
    }
    for malformed in [
        json!({"type":"request_state","request_id":"id","state":"failed"}),
        json!({"type":"request_state","request_id":"","state":"pending"}),
        json!({"type":"request_state","request_id":"id","state":"pending","retry":true}),
    ] {
        assert!(serde_json::from_value::<HostFrame>(malformed).is_err());
    }
}

#[test]
fn hello_rejects_unsupported_major_versions_without_downgrading() {
    for versions in [json!({"min":2,"max":2}), json!({"min":0,"max":0})] {
        assert!(
            serde_json::from_value::<ClientFrame>(json!({"type":"hello","versions":versions}))
                .is_err()
        );
    }
    let offered = VersionRange::new(2, 3).unwrap();
    let error = VersionRange::current().negotiate(offered).unwrap_err();
    assert_eq!(
        serde_json::to_value(error).unwrap(),
        json!({
            "code":"incompatible_version", "message":"no supported protocol major in common",
            "supported":{"min":1,"max":1}, "offered":{"min":2,"max":3}
        })
    );
    assert_eq!(
        VersionRange::current()
            .negotiate(VersionRange::new(0, 2).unwrap())
            .unwrap(),
        1
    );
    assert!(VersionRange::new(3, 2).is_err());
    let mut hello: Value = serde_json::from_str(include_str!("golden/hello-host.json")).unwrap();
    hello["protocol_major"] = json!(2);
    assert!(serde_json::from_value::<HostFrame>(hello).is_err());
}

#[test]
fn cursors_reject_negative_fractional_and_overflow_sequences() {
    for seq in [json!(-1), json!(0.5), json!(9223372036854775808_u64)] {
        assert!(serde_json::from_value::<Cursor>(json!({"host_id":HOST,"seq":seq})).is_err());
    }
    assert!(Cursor::new(HostId::new(HOST).unwrap(), -1).is_err());
    assert!(Cursor::new(HostId::new(HOST).unwrap(), 0).is_ok());
}

#[test]
fn identities_use_full_sha256_and_request_ids_are_nonempty() {
    assert_eq!(HostId::from_public_key(b"abc").as_str(), HOST);
    for value in [
        "",
        "abc",
        "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD",
        "ga7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    ] {
        assert!(HostId::new(value).is_err());
        assert!(serde_json::from_value::<HostId>(json!(value)).is_err());
    }
    assert!(RequestId::new("").is_err());
    assert!(serde_json::from_value::<RequestId>(json!("")).is_err());
    assert_eq!(
        RequestId::new("opaque / request").unwrap().as_str(),
        "opaque / request"
    );
    assert!(serde_json::from_value::<RequestId>(json!(7)).is_err());
}

#[test]
fn interactions_reject_missing_or_malformed_contract_fields() {
    let frame: Value = serde_json::from_str(include_str!("golden/interaction.json")).unwrap();
    for key in [
        "host_id",
        "thread",
        "surface_seq",
        "surface_contract",
        "node_path",
        "event",
        "args",
    ] {
        let mut incomplete = frame.clone();
        incomplete["interaction"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            serde_json::from_value::<ClientFrame>(incomplete).is_err(),
            "missing {key}"
        );
    }
    for (key, value) in [
        ("surface_contract", json!(0)),
        ("surface_contract", json!(2)),
        ("surface_seq", json!(-1)),
        ("node_path", json!([-1])),
        ("thread", json!("")),
        ("event", json!("")),
        ("args", json!([])),
        ("unexpected", json!(true)),
    ] {
        let mut malformed = frame.clone();
        malformed["interaction"][key] = value;
        assert!(
            serde_json::from_value::<ClientFrame>(malformed).is_err(),
            "malformed {key}"
        );
    }
    let mut root = frame;
    root["interaction"]["node_path"] = json!([]);
    assert!(serde_json::from_value::<ClientFrame>(root).is_ok());
}

#[test]
fn approvals_reject_untyped_incomplete_and_extra_decisions() {
    let frame: Value = serde_json::from_str(include_str!("golden/approval-decision.json")).unwrap();
    for key in ["host_id", "thread", "approval_id", "decision"] {
        let mut incomplete = frame.clone();
        incomplete["decision"].as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<ClientFrame>(incomplete).is_err(),
            "missing {key}"
        );
    }
    for (key, value) in [
        ("decision", json!(true)),
        ("decision", json!("accept")),
        ("approval_id", json!("")),
        ("thread", json!("")),
        ("payload", json!({})),
    ] {
        let mut malformed = frame.clone();
        malformed["decision"][key] = value;
        assert!(
            serde_json::from_value::<ClientFrame>(malformed).is_err(),
            "malformed {key}"
        );
    }
    let mut deny = frame;
    deny["decision"]["decision"] = json!("deny");
    assert!(serde_json::from_value::<ClientFrame>(deny).is_ok());
}

#[test]
fn snapshots_and_acknowledgements_reject_a_cursor_from_another_host() {
    for fixture in [
        include_str!("golden/snapshot.json"),
        include_str!("golden/acknowledgement.json"),
    ] {
        let mut frame: Value = serde_json::from_str(fixture).unwrap();
        let body = if frame["type"] == "snapshot" {
            &mut frame["snapshot"]
        } else {
            &mut frame
        };
        body["cursor"]["host_id"] = json!("0".repeat(64));
        assert!(serde_json::from_value::<HostFrame>(frame).is_err());
    }
}

#[test]
fn unknown_frame_types_and_fields_fail_closed() {
    assert!(
        serde_json::from_value::<ClientFrame>(json!({"type":"execute","command":"anything"}))
            .is_err()
    );
    let mut frame: Value = serde_json::from_str(include_str!("golden/call.json")).unwrap();
    frame["unexpected"] = json!(true);
    assert!(serde_json::from_value::<ClientFrame>(frame).is_err());
}

#[test]
fn host_events_reject_unknown_nested_fields_without_changing_legacy_events() {
    let mut frame: Value = serde_json::from_str(include_str!("golden/event.json")).unwrap();
    frame["event"]["unexpected"] = json!(true);
    assert!(serde_json::from_value::<mote_protocol::Event>(frame["event"].clone()).is_ok());
    assert!(serde_json::from_value::<HostFrame>(frame).is_err());
}

#[test]
fn approval_decisions_require_a_complete_action_hash_and_expiry_binding() {
    let frame: Value = serde_json::from_str(include_str!("golden/approval-decision.json")).unwrap();
    assert!(serde_json::from_value::<ClientFrame>(frame.clone()).is_ok());
    for key in ["action_hash", "expires_at"] {
        let mut incomplete = frame.clone();
        incomplete["decision"].as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<ClientFrame>(incomplete).is_err(),
            "missing {key}"
        );
    }
    for (key, value) in malformed_approval_bindings() {
        let mut malformed = frame.clone();
        malformed["decision"][key] = value;
        assert!(
            serde_json::from_value::<ClientFrame>(malformed).is_err(),
            "malformed {key}"
        );
    }
}

#[test]
fn pending_approvals_require_a_complete_action_hash_and_expiry_binding() {
    let frame: Value = serde_json::from_str(include_str!("golden/snapshot.json")).unwrap();
    assert!(serde_json::from_value::<HostFrame>(frame.clone()).is_ok());
    for key in ["action_hash", "expires_at"] {
        let mut incomplete = frame.clone();
        incomplete["snapshot"]["pending_approvals"][0]
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert!(
            serde_json::from_value::<HostFrame>(incomplete).is_err(),
            "missing {key}"
        );
    }
    for (key, value) in malformed_approval_bindings() {
        let mut malformed = frame.clone();
        malformed["snapshot"]["pending_approvals"][0][key] = value;
        assert!(
            serde_json::from_value::<HostFrame>(malformed).is_err(),
            "malformed {key}"
        );
    }
}

fn malformed_approval_bindings() -> Vec<(&'static str, Value)> {
    vec![
        ("action_hash", json!("")),
        ("action_hash", json!("abc")),
        (
            "action_hash",
            json!("0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF"),
        ),
        (
            "action_hash",
            json!("g123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"),
        ),
        ("action_hash", json!(7)),
        ("action_hash", Value::Null),
        ("expires_at", json!(-1)),
        ("expires_at", json!(0.5)),
        ("expires_at", json!(9223372036854775808_u64)),
        ("expires_at", json!("1791763200000")),
        ("expires_at", Value::Null),
    ]
}
