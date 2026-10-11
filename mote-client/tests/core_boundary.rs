use serde_json::Value;
use std::process::Command;

#[test]
fn stored_events_reach_the_core_without_changing_the_wire_payload() {
    let stored = mote_eventlog::Event {
        seq: 17,
        ts: 1234,
        scope_id: "main".into(),
        kind: "ui.surface".into(),
        payload: serde_json::json!({"surface_id":"matrix","surface":{"type":"action","label":"Refresh","event":"refresh"}}),
    };
    let protocol: mote_protocol::Event = stored.clone();
    let expected = serde_json::json!({"seq":17,"ts":1234,"scope_id":"main","kind":"ui.surface","payload":{"surface_id":"matrix","surface":{"type":"action","label":"Refresh","event":"refresh"}}});
    assert_eq!(serde_json::to_value(stored).unwrap(), expected);
    assert_eq!(serde_json::to_value(&protocol).unwrap(), expected);
    let mut chat = mote_client_core::ChatState::default();
    assert!(chat.reduce(&protocol).is_some());
    assert_eq!(chat.messages[0].surface_seq, Some(17));
    assert_eq!(
        chat.messages[0].surface.as_ref().unwrap().display_v0(),
        &mote_ui::Node::Action {
            label: "Refresh".into(),
            event: "refresh".into(),
        }
    );
}

#[test]
fn shared_core_has_only_platform_neutral_dependencies() {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version=1", "--no-deps", "--offline"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let core = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "mote-client-core")
        .expect("shared Matrix core must be a workspace package");
    let allowed = [
        "egui",
        "accesskit",
        "mote-ui",
        "mote-protocol",
        "serde",
        "serde_json",
    ];
    for dependency in core["dependencies"].as_array().unwrap() {
        if dependency["kind"] == "dev" {
            continue;
        }
        let name = dependency["name"].as_str().unwrap();
        assert!(
            allowed.contains(&name),
            "core dependency crosses the platform boundary: {name}"
        );
    }

    let tree = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--offline",
            "-p",
            "mote-client-core",
            "--target",
            "aarch64-apple-ios-sim",
            "--edges",
            "normal,build",
            "--prefix",
            "none",
            "--format",
            "{p}",
        ])
        .output()
        .unwrap();
    assert!(
        tree.status.success(),
        "{}",
        String::from_utf8_lossy(&tree.stderr)
    );
    for package in String::from_utf8(tree.stdout).unwrap().lines() {
        let name = package.split_whitespace().next().unwrap();
        assert!(
            !matches!(
                name,
                "eframe" | "mote-ipc" | "mote-eventlog" | "rusqlite" | "libsqlite3-sys" | "winit"
            ) && !name.starts_with("objc")
                && !name.starts_with("core-foundation")
                && !name.starts_with("core-graphics")
                && !name.starts_with("metal"),
            "core transitively depends on platform/service package: {name}"
        );
    }
}
