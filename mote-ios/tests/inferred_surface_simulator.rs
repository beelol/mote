use mote_host_client::HostDirectory;
use mote_ipc::Dispatch;
use mote_protocol::{Event, ThreadProjection};
use mote_testkit::ScriptedServer;
use serde_json::json;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

mod support;
use support::{command, NativeTestChild, SimulatorStateGuard, LIVE_FILES};

fn queue(server: &ScriptedServer, model: &str, content: &str) {
    server.queue_chat_chunks(
        model,
        vec![json!({"message":{"role":"assistant","content":content},"done":true}).to_string()],
    );
}

#[test]
#[ignore = "requires MOTE_LIVE=1 and the native iPad Simulator"]
fn native_inferred_success_retry_preservation_and_prose_fallback() {
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    let device = std::env::var("MOTE_SIMULATOR_ID").unwrap();
    let artifacts = PathBuf::from(std::env::var("MOTE_ARTIFACTS").unwrap());
    std::fs::create_dir_all(&artifacts).unwrap();
    let _ = Command::new("xcrun")
        .args(["simctl", "terminate", &device, "local.mote.simulator"])
        .output();
    let build_log = std::fs::File::create(artifacts.join("native-build.log")).unwrap();
    let status = Command::new("xcodebuild")
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
        .args([
            "-project",
            "ios/Mote/Mote.xcodeproj",
            "-scheme",
            "Mote",
            "-destination",
            &format!("id={device}"),
            "-derivedDataPath",
            "/private/tmp/mote-inferred-derived",
            "build",
        ])
        .stdout(Stdio::from(build_log.try_clone().unwrap()))
        .stderr(Stdio::from(build_log))
        .status()
        .unwrap();
    assert!(status.success(), "native build failed");
    command(
        "xcrun",
        &[
            "simctl",
            "install",
            &device,
            "/private/tmp/mote-inferred-derived/Build/Products/Debug-iphonesimulator/Mote.app",
        ],
    );
    let models = ScriptedServer::start().unwrap();
    let attestation_path = std::env::var("MOTE_SURFACE_ATTESTATION").unwrap();
    let attestation = std::fs::read(attestation_path).unwrap();
    let identity: serde_json::Value = serde_json::from_slice(&attestation).unwrap();
    models.set_models(vec![("qwen3:4b".into(), 1), ("qwen3:0.6b".into(), 1)]);
    models.set_model_digest("qwen3:4b", identity["model_digest"].as_str().unwrap());
    for (prose, decisions) in [
        (
            "Success prose.",
            vec![
                json!({"kind":"matrix","source":"screen { text \"Success panel\" button \"Refresh\" -> refresh }"}),
            ],
        ),
        (
            "Recovery prose.",
            vec![
                json!({"kind":"matrix","source":"screen { text missing_binding }"}),
                json!({"kind":"matrix","source":"screen { text \"Recovered panel\" button \"Refresh\" -> refresh }"}),
            ],
        ),
        (
            "Preservation prose.",
            vec![
                json!({"kind":"matrix","source":"screen { text missing_binding }"}),
                json!({"kind":"matrix","source":"screen { button \"Invented\" -> invented_action }"}),
            ],
        ),
        ("Ordinary prose fallback.", vec![json!({"kind":"none"})]),
    ] {
        queue(
            &models,
            "qwen3:0.6b",
            r#"{"route":"talk","reason":"fixture"}"#,
        );
        queue(&models, "qwen3:4b", prose);
        for decision in decisions {
            queue(&models, "qwen3:4b", &decision.to_string());
        }
    }
    let host = moted::test_fixture::start(&models).unwrap();
    let data = host.root().join("data");
    std::fs::write(data.join("surface-activation.json"), attestation).unwrap();
    std::fs::set_permissions(
        data.join("surface-activation.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    host.daemon().call("matrix.render", json!({"thread":"main","source":"screen { text \"Prior panel\" button \"Refresh\" -> refresh }"})).unwrap();
    let container = PathBuf::from(command(
        "xcrun",
        &[
            "simctl",
            "get_app_container",
            &device,
            "local.mote.simulator",
            "data",
        ],
    ));
    let documents = container.join("Documents");
    let backup = artifacts.join("prior-live-state");
    let mut cleanup = SimulatorStateGuard::new(
        Some(device.clone()),
        Some(host.host_id().as_str().into()),
        documents.clone(),
        backup,
        artifacts.clone(),
    );
    cleanup.backup_files().unwrap();
    let _ = Command::new("xcrun")
        .args(["simctl", "terminate", &device, "local.mote.simulator"])
        .output();
    let configs = json!([{"name":"Inference","public_key":host.public_key(),"endpoint":host.endpoint().unwrap(),"offer":host.open_pairing().unwrap()}]);
    let launched = Command::new("xcrun")
        .args(["simctl", "launch", &device, "local.mote.simulator"])
        .env("SIMCTL_CHILD_MOTE_SIMULATOR_HOSTS", configs.to_string())
        .output()
        .unwrap();
    assert!(launched.status.success());
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        host.confirm_pending(true).unwrap();
        if HostDirectory::load(&documents.join("live-state.json"))
            .ok()
            .is_some_and(|directory| {
                directory
                    .state(host.host_id())
                    .is_ok_and(|state| state.cursor().seq >= 1)
            })
        {
            break;
        }
        assert!(Instant::now() < deadline, "native pairing timeout");
        std::thread::sleep(Duration::from_millis(20));
    }
    let log = std::fs::File::create(artifacts.join("native-xctest.log")).unwrap();
    let mut test = NativeTestChild(Command::new("xcodebuild").current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
        .args(["-project", "ios/Mote/Mote.xcodeproj", "-scheme", "Mote", "-destination", &format!("id={device}"), "-derivedDataPath", "/private/tmp/mote-inferred-derived", "-resultBundlePath"])
        .arg(artifacts.join("native.xcresult"))
        .args(["-only-testing:MoteUITests/MoteUITests/testInferredSurfaceSuccessRetryPreservationAndFallback", "test"])
        .stdout(Stdio::from(log.try_clone().unwrap())).stderr(Stdio::from(log)).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(420);
    while test.0.try_wait().unwrap().is_none() {
        host.confirm_pending(true).unwrap();
        assert!(
            Instant::now() < deadline,
            "native inference journey timeout"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let status = test.0.wait().unwrap();
    let events = host.events_after(0).unwrap();
    std::fs::write(
        artifacts.join("events.json"),
        serde_json::to_vec_pretty(&events).unwrap(),
    )
    .unwrap();
    assert!(
        status.success(),
        "native inference XCUITest failed: {}",
        artifacts.join("native-xctest.log").display()
    );
    let directory = HostDirectory::load(&documents.join("live-state.json")).unwrap();
    let state = directory.state(host.host_id()).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == "turn.user")
            .count(),
        4
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == "talk.done")
            .count(),
        4
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == "ui.diagnostic")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == "ui.surface")
            .count(),
        4
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == "ui.event")
            .count(),
        1
    );
    let committed = events
        .iter()
        .filter(|event| event.kind == "ui.surface")
        .next_back()
        .unwrap();
    assert!(committed.payload.to_string().contains("Recovered panel"));
    let preserved = state.surfaces()["main"].last_key_value().unwrap().1;
    assert_eq!(preserved.surface_seq, committed.seq);
    assert!(state.threads()["main"]
        .messages
        .iter()
        .any(|message| message.text.contains("Matrix source rejected")));
    let call_count = models
        .recorded()
        .iter()
        .filter(|request| request.path == "/api/chat")
        .count();
    assert_eq!(call_count, 14);
    let requests = models.recorded();
    let surface_requests = requests
        .iter()
        .filter(|request| request.body["format"].get("oneOf").is_some())
        .collect::<Vec<_>>();
    assert_eq!(surface_requests.len(), 6);
    for retry in [2, 4] {
        let messages = surface_requests[retry].body["messages"].as_array().unwrap();
        let envelope: serde_json::Value =
            serde_json::from_str(messages.last().unwrap()["content"].as_str().unwrap()).unwrap();
        let diagnostics: serde_json::Value =
            serde_json::from_str(envelope["diagnostics"].as_str().unwrap()).unwrap();
        assert_eq!(diagnostics[0]["code"], "E_RESOLVE_PATH");
        assert_eq!(diagnostics[0]["message"], "Unknown Value path");
    }
    let mut client = host
        .pair(&host.open_pairing().unwrap(), "typed-rejection-proof")
        .unwrap();
    for (id, sequence, event, args) in [
        ("stale", 1, "refresh", json!({})),
        ("invented", committed.seq, "invented_action", json!({})),
        ("extra", committed.seq, "refresh", json!({"execute":true})),
    ] {
        let interaction = mote_protocol::TypedInteraction {
            host_id: host.host_id().clone(),
            thread: "main".into(),
            surface_seq: sequence,
            surface_contract: 1,
            node_path: vec![1],
            event: event.into(),
            args: args.as_object().unwrap().clone(),
        };
        let reply = client
            .exchange(&mote_protocol::ClientFrame::Interaction {
                request_id: mote_protocol::RequestId::new(id).unwrap(),
                interaction,
            })
            .unwrap();
        assert!(
            matches!(reply, mote_protocol::HostFrame::Error { error, .. } if error.code == mote_protocol::HostErrorCode::InvalidParams)
        );
    }
    assert_eq!(
        models
            .recorded()
            .iter()
            .filter(|request| request.path == "/api/chat")
            .count(),
        call_count
    );
    let counts = command("sqlite3", &[data.join("mote.db").to_str().unwrap(), "select lane,model,status,count(*) from ledger group by lane,model,status; select count(*) from ledger; select count(*) from packs;"]);
    assert!(counts.contains("router|qwen3:0.6b|ok|4"));
    assert!(counts.contains("surface|qwen3:4b|ok|6"));
    assert!(counts.contains("talk|qwen3:4b|ok|4"));
    assert!(counts.ends_with("14\n14"));
    std::fs::write(artifacts.join("ledger.txt"), &counts).unwrap();
    for name in LIVE_FILES {
        std::fs::copy(documents.join(name), artifacts.join(name)).unwrap();
    }
    cleanup.finish().unwrap();
    println!("Native inferred success, one retry, stable prior surface, visible terminal diagnostic and prose fallback passed. 14 calls, 14 ledger rows, 14 packs; 3 authenticated typed refusals. Host {}", host.host_id().as_str());
}

#[test]
fn terminal_inference_diagnostic_remains_visible_in_paired_thread() {
    let mut projection = ThreadProjection::new("main".into());
    let event: Event = serde_json::from_value(json!({
        "seq":3,"ts":3,"scope_id":"main","kind":"ui.diagnostic",
        "payload":{"diagnostics":[{"code":"E_RESOLVE_ACTION","message":"Unknown action","span":{"start":1,"end":2},"hint":"Use a host-owned action"}]}
    })).unwrap();
    assert!(ThreadProjection::handles(&event.kind));
    projection.apply(&event);
    assert_eq!(projection.thread.messages.len(), 1);
    assert!(projection.thread.messages[0]
        .text
        .contains("E_RESOLVE_ACTION"));
    assert!(projection.thread.messages[0]
        .text
        .contains("Unknown action"));
    assert!(projection.thread.messages[0]
        .text
        .contains("Use a host-owned action"));
}
