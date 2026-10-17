use mote_host_client::HostDirectory;
use mote_ipc::Dispatch;
use mote_testkit::{gateway::LoopbackHost, ScriptedServer};
use serde_json::{json, Value};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

mod support;
use support::{command, NativeTestChild, SimulatorStateGuard};

const BEFORE: &str = "screen { header { button \"Stable\" -> refresh text \"Header\" } button \"Before\" -> refresh toggle \"Local only\" models.local_only footer { text \"Footer\" } }";
const AFTER: &str = "screen { header { button \"Stable\" -> refresh text \"Header\" text \"Second\" text \"Third\" } button \"Replacement\" -> refresh slider \"Router budget\" budgets.router min 50 max 1000 step 10 footer { text \"Footer\" text \"Second\" text \"Third\" } }";

#[derive(Clone, Copy)]
enum Motion {
    Normal,
    Reduced,
}

#[derive(Clone, Copy)]
enum Journey {
    Transition,
    Action,
}

fn wait(mut predicate: impl FnMut() -> bool, message: &str) {
    let deadline = Instant::now() + Duration::from_secs(90);
    while !predicate() {
        assert!(Instant::now() < deadline, "{message}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn capture<D: Dispatch>(
    host: &LoopbackHost<D>,
    name: &str,
    motion: Motion,
    journey: Journey,
    replace: impl FnOnce(),
) -> i64 {
    let device = std::env::var("MOTE_SIMULATOR_ID").unwrap();
    let artifacts = PathBuf::from(std::env::var("MOTE_ARTIFACTS").unwrap()).join(name);
    std::fs::create_dir_all(&artifacts).unwrap();
    let _ = Command::new("xcrun")
        .args(["simctl", "terminate", &device, "local.mote.simulator"])
        .output();
    let documents = PathBuf::from(command(
        "xcrun",
        &[
            "simctl",
            "get_app_container",
            &device,
            "local.mote.simulator",
            "data",
        ],
    ))
    .join("Documents");
    let mut cleanup = SimulatorStateGuard::new(
        Some(device.clone()),
        Some(host.host_id().as_str().into()),
        documents.clone(),
        artifacts.join("prior-live-state"),
        artifacts.clone(),
    );
    cleanup.backup_files().unwrap();
    let old_probe = documents.join("mote-live-transition");
    if old_probe.exists() {
        std::fs::rename(&old_probe, artifacts.join("prior-probe")).unwrap();
    }
    let configs = json!([{"name":"Transition","public_key":host.public_key(),"endpoint":host.endpoint().unwrap(),"offer":host.open_pairing().unwrap()}]);
    let video_path = artifacts.join("journey.mp4");
    let video_log = artifacts.join("video.log");
    let mut video = NativeTestChild(
        Command::new("xcrun")
            .args(["simctl", "io", &device, "recordVideo", "--codec=h264"])
            .arg(&video_path)
            .stdout(Stdio::null())
            .stderr(Stdio::from(std::fs::File::create(&video_log).unwrap()))
            .spawn()
            .unwrap(),
    );
    wait(
        || {
            std::fs::read_to_string(&video_log)
                .unwrap_or_default()
                .contains("Recording started")
        },
        "Simulator video did not start",
    );
    let mut launch = Command::new("xcrun");
    launch.args([
        "simctl",
        "launch",
        &device,
        "local.mote.simulator",
        "--mote-live-transition-probe",
    ]);
    if matches!(motion, Motion::Reduced) {
        launch.arg("--mote-reduced-motion");
    }
    if matches!(journey, Journey::Action) {
        launch.arg("--mote-live-transition-action");
    }
    let launched = launch
        .env("SIMCTL_CHILD_MOTE_SIMULATOR_HOSTS", configs.to_string())
        .output()
        .unwrap();
    assert!(
        launched.status.success(),
        "{}",
        String::from_utf8_lossy(&launched.stderr)
    );
    wait(
        || {
            host.confirm_pending(true).unwrap();
            HostDirectory::load(&documents.join("live-state.json"))
                .ok()
                .is_some_and(|directory| {
                    directory
                        .state(host.host_id())
                        .is_ok_and(|state| state.cursor().seq > 0)
                })
        },
        "paired baseline did not arrive",
    );
    wait(
        || old_probe.join("ready").is_file(),
        "baseline GPU capture did not warm up",
    );
    wait(
        || {
            let events = host.events_after(0).unwrap();
            events
                .iter()
                .find(|event| event.kind == "ui.event")
                .is_some_and(|action| {
                    events
                        .iter()
                        .any(|event| event.kind == "ui.surface" && event.seq > action.seq)
                })
        },
        "baseline interaction status did not settle",
    );
    std::thread::sleep(Duration::from_millis(500));
    replace();
    wait(
        || old_probe.join("report.json").is_file(),
        "live transition probe did not finish",
    );
    unsafe {
        libc::kill(video.0.id() as i32, libc::SIGINT);
    }
    assert!(video.0.wait().unwrap().success(), "video recording failed");
    std::fs::rename(&old_probe, artifacts.join("capture")).unwrap();
    let events = host.events_after(0).unwrap();
    std::fs::write(
        artifacts.join("events.json"),
        serde_json::to_vec_pretty(&events).unwrap(),
    )
    .unwrap();
    let mut report: Value =
        serde_json::from_slice(&std::fs::read(artifacts.join("capture/report.json")).unwrap())
            .unwrap();
    report["captures"] = decode_video(&video_path, &report);
    std::fs::write(
        artifacts.join("video-map.json"),
        serde_json::to_vec_pretty(&report["captures"]).unwrap(),
    )
    .unwrap();
    let replacement = report["replacement"][0].as_i64().unwrap();
    let start = report["replacement"][1].as_f64().unwrap();
    let frames = report["frames"].as_array().unwrap();
    let captured = report["captures"].as_array().unwrap();
    let images = |frame: &&Value| {
        captured
            .iter()
            .any(|capture| capture["sequence"] == frame["sequence"])
    };
    let first = frames
        .iter()
        .find(|frame| frame["revision"] == replacement)
        .unwrap();
    assert!(
        images(&first),
        "actual first replacement render was not captured"
    );
    assert_eq!(report["inject_action"], matches!(journey, Journey::Action));
    assert_eq!(
        first["slider_target"],
        json!({"path":[2],"field":"budgets.router","surface_value":150.0,"value":160.0})
    );
    let before = frames
        .iter()
        .rev()
        .find(|frame| frame["revision"].as_i64().unwrap() < replacement)
        .unwrap();
    let actions: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "ui.event")
        .collect();
    let generated = events
        .iter()
        .find(|event| event.seq == replacement && event.kind == "ui.surface")
        .expect("replacement revision was not the generated production surface");
    assert_eq!(
        generated.payload["matrix_surface"]["root"]["children"][2],
        json!({
            "type":"slider","label":"Router budget","field":"budgets.router",
            "value":150.0,"min":50.0,"max":1000.0,"step":10.0
        })
    );
    match journey {
        Journey::Transition => {
            assert!(first["immediate_action_target"].is_null());
            assert!(first["emitted_events"].as_array().unwrap().is_empty());
            assert_eq!(
                actions
                    .iter()
                    .filter(|event| event.seq > replacement)
                    .count(),
                0,
                "transition-only journey must not mutate the replacement"
            );
            assert!(!events.iter().any(|event| {
                event.seq > replacement
                    && matches!(event.kind.as_str(), "config.changed" | "ui.surface")
            }));
            let midpoint = frames
                .iter()
                .filter(images)
                .find(|frame| {
                    let elapsed = frame["time"].as_f64().unwrap() - start;
                    (0.040..0.100).contains(&elapsed)
                })
                .expect("no captured midpoint frame");
            let settled = frames
                .iter()
                .filter(images)
                .find(|frame| {
                    let elapsed = frame["time"].as_f64().unwrap() - start;
                    (0.120..0.150).contains(&elapsed)
                })
                .expect("no captured settled frame");
            match motion {
                Motion::Normal => {
                    assert!(first["particle_count"].as_u64().unwrap() > 0);
                    assert!(midpoint["particle_count"].as_u64().unwrap() > 0);
                    assert_eq!(settled["particle_count"], 0);
                    assert_eq!(first["pulse_count"], 0);
                    assert_eq!(midpoint["pulse_count"], 0);
                    assert_eq!(settled["pulse_count"], 0);
                }
                Motion::Reduced => {
                    assert_eq!(first["particle_count"], 0);
                    assert_eq!(midpoint["particle_count"], 0);
                    assert_eq!(settled["particle_count"], 0);
                    assert!(first["pulse_count"].as_u64().unwrap() > 0);
                    assert!(midpoint["pulse_count"].as_u64().unwrap() > 0);
                    assert_eq!(settled["pulse_count"], 0);
                }
            }
            let border = |frame: &Value| {
                frame
                    .get("changed_button")
                    .expect("missing path [1] button materialization observation")
                    .clone()
            };
            let old_border = border(before);
            let new_border = border(settled);
            assert_eq!(old_border["path"], json!([1]));
            assert_eq!(new_border["path"], json!([1]));
            assert_eq!(old_border["points"].as_array().unwrap().len(), 5);
            assert_eq!(new_border["points"].as_array().unwrap().len(), 5);
            assert_ne!(old_border["points"], new_border["points"]);
            assert!(
                new_border["points"][0][1].as_f64().unwrap()
                    > old_border["points"][0][1].as_f64().unwrap()
            );
            if old_border["painted"] == true {
                assert_eq!(old_border["particle_count"], 0);
            } else {
                assert!(old_border["previous_points"].is_array());
                assert!(old_border["particle_count"].as_u64().unwrap() > 0);
            }
            assert_eq!(new_border["painted"], true);
            assert_eq!(new_border["particle_count"], 0);
            for frame in [first, midpoint] {
                let materializing = border(frame);
                assert_eq!(materializing["path"], json!([1]));
                assert_eq!(materializing["points"], new_border["points"]);
                assert_eq!(materializing["previous_points"], old_border["points"]);
                match motion {
                    Motion::Normal => {
                        assert_eq!(materializing["painted"], false);
                        assert!(materializing["particle_count"].as_u64().unwrap() > 0);
                    }
                    Motion::Reduced => {
                        assert_eq!(materializing["painted"], true);
                        assert_eq!(materializing["particle_count"], 0);
                    }
                }
                for name in ["header_rule", "footer_rule"] {
                    let stroke = frame.get(name).expect("missing changed structural stroke");
                    assert!(stroke["previous_points"].is_array());
                    match motion {
                        Motion::Normal => {
                            assert_eq!(stroke["painted"], false);
                            assert!(stroke["particle_count"].as_u64().unwrap() > 0);
                        }
                        Motion::Reduced => {
                            assert_eq!(stroke["painted"], true);
                            assert_eq!(stroke["particle_count"], 0);
                        }
                    }
                }
                let rail = frame
                    .get("slider_rail")
                    .expect("missing newly introduced slider rail");
                assert!(rail["previous_points"].is_null());
                match motion {
                    Motion::Normal => {
                        assert_eq!(rail["painted"], false);
                        assert!(rail["particle_count"].as_u64().unwrap() > 0);
                    }
                    Motion::Reduced => {
                        assert_eq!(rail["painted"], true);
                        assert_eq!(rail["particle_count"], 0);
                    }
                }
            }
            let stable = before["rectangles"]
                .as_array()
                .unwrap()
                .iter()
                .find(|rect| {
                    [first, midpoint, settled]
                        .iter()
                        .all(|frame| frame["rectangles"].as_array().unwrap().contains(rect))
                })
                .expect("no unchanged stroke remained painted");
            let old_lines = before["line_segments"].as_array().unwrap();
            let new_lines = settled["line_segments"].as_array().unwrap();
            assert_eq!(old_lines.len(), 2);
            assert_eq!(new_lines.len(), 3);
            assert!(old_lines.iter().all(|before| !new_lines.contains(before)));
            for (name, frame) in [
                ("before", before),
                ("first", first),
                ("midpoint", midpoint),
                ("settled", settled),
            ] {
                extract_frame(&video_path, &artifacts, captured, name, frame);
            }
            std::fs::write(artifacts.join("selected-frames.json"), serde_json::to_vec_pretty(&json!({"before":before,"first":first,"midpoint":midpoint,"settled":settled,"unchanged_rectangle":stable})).unwrap()).unwrap();
        }
        Journey::Action => {
            assert!(!first["immediate_action_target"].is_null());
            assert!(first["emitted_events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| {
                    let event = event.as_str().unwrap();
                    event.contains("Change")
                        && event.contains("budgets.router")
                        && event.contains("160.0")
                        && event.contains("path: [2]")
                }));
            assert_eq!(
                actions
                    .iter()
                    .filter(|event| event.seq > replacement)
                    .count(),
                1
            );
            let action = actions
                .iter()
                .find(|event| event.seq > replacement)
                .unwrap();
            assert_eq!(action.payload["event"], "change");
            assert_eq!(
                action.payload["args"],
                json!({"field":"budgets.router","value":160.0})
            );
            let config = events
                .iter()
                .find(|event| event.seq > replacement && event.kind == "config.changed")
                .expect("slider change did not persist config");
            assert_eq!(config.payload["key"], "budgets.router");
            assert_eq!(config.payload["old"], 150);
            assert_eq!(config.payload["new"], 160);
            let refreshed = events
                .iter()
                .find(|event| event.seq > action.seq && event.kind == "ui.surface")
                .expect("accepted slider change did not refresh the retained surface");
            assert_eq!(
                refreshed.payload["matrix_surface"]["root"]["children"][2]["value"],
                160.0
            );
            let refreshed_frame = frames
                .iter()
                .find(|frame| frame["revision"] == refreshed.seq)
                .expect("refreshed revision was not rendered before the probe completed");
            assert_eq!(refreshed_frame["slider_target"]["surface_value"], 160.0);
            assert!(refreshed_frame["immediate_action_target"].is_null());
            assert!(refreshed_frame["emitted_events"]
                .as_array()
                .unwrap()
                .is_empty());
            extract_frame(&video_path, &artifacts, captured, "before", before);
            extract_frame(&video_path, &artifacts, captured, "first", first);
            std::fs::write(
                artifacts.join("selected-frames.json"),
                serde_json::to_vec_pretty(
                    &json!({"before":before,"first":first,"refreshed":refreshed_frame}),
                )
                .unwrap(),
            )
            .unwrap();
        }
    }
    cleanup.finish().unwrap();
    replacement
}

fn extract_frame(
    video_path: &Path,
    artifacts: &Path,
    captured: &[Value],
    name: &str,
    frame: &Value,
) {
    let capture = captured
        .iter()
        .find(|capture| capture["sequence"] == frame["sequence"])
        .expect("selected frame missing from video");
    let filter = format!(
        "select=eq(n\\,{})",
        capture["video_index"].as_u64().unwrap()
    );
    let status = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-i"])
        .arg(video_path)
        .args(["-vf", &filter, "-fps_mode", "passthrough", "-frames:v", "1"])
        .arg(artifacts.join(format!("{name}.png")))
        .status()
        .unwrap();
    assert!(status.success(), "video frame extraction failed");
}

fn decode_video(path: &Path, report: &Value) -> Value {
    let frame = report["frames"].as_array().unwrap().last().unwrap();
    let x = frame["marker_origin"][0].as_u64().unwrap();
    let y = frame["marker_origin"][1].as_u64().unwrap();
    let stride = frame["marker_stride"].as_u64().unwrap() as usize;
    let width = stride * 31 + 1;
    let filter = format!("format=rgb24,crop={width}:1:{x}:{y}");
    let output = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-i"])
        .arg(path)
        .args([
            "-vf",
            &filter,
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut captures = Vec::new();
    for (video_index, row) in output.stdout.chunks_exact(width * 3).enumerate() {
        let mut sequence = 0_u32;
        let mut valid = true;
        for bit in 0..32 {
            let pixel = &row[bit * stride * 3..][..3];
            if i16::from(pixel[1]) > i16::from(pixel[0]) + 40 {
                sequence |= 1 << bit;
            } else if i16::from(pixel[0]) <= i16::from(pixel[1]) + 40 {
                valid = false;
                break;
            }
        }
        if valid
            && !captures
                .iter()
                .any(|capture: &Value| capture["sequence"] == sequence)
        {
            captures.push(json!({"sequence":sequence,"video_index":video_index}));
        }
    }
    json!(captures)
}

fn queue(server: &ScriptedServer, model: &str, content: &str) {
    server.queue_chat_chunks(
        model,
        vec![json!({"message":{"role":"assistant","content":content},"done":true}).to_string()],
    );
}

fn production_host(models: &ScriptedServer, attestation: &[u8]) -> LoopbackHost<impl Dispatch> {
    let identity: Value = serde_json::from_slice(attestation).unwrap();
    models.set_models(vec![("qwen3:4b".into(), 1), ("qwen3:0.6b".into(), 1)]);
    models.set_model_digest("qwen3:4b", identity["model_digest"].as_str().unwrap());
    queue(
        models,
        "qwen3:0.6b",
        r#"{"route":"talk","reason":"fixture"}"#,
    );
    queue(models, "qwen3:4b", "Here is the requested control.");
    queue(
        models,
        "qwen3:4b",
        &json!({"kind":"matrix","source":AFTER}).to_string(),
    );
    let host = moted::test_fixture::start(models).unwrap();
    let activation = host.root().join("data/surface-activation.json");
    std::fs::write(&activation, attestation).unwrap();
    std::fs::set_permissions(&activation, std::fs::Permissions::from_mode(0o600)).unwrap();
    host.daemon()
        .call("matrix.render", json!({"thread":"main","source":BEFORE}))
        .unwrap();
    host
}

fn reject_stale_revision<D: Dispatch>(host: &LoopbackHost<D>, stale_seq: i64) {
    let before = host.events_after(0).unwrap();
    let mut client = host
        .pair(&host.open_pairing().unwrap(), "stale-slider-proof")
        .unwrap();
    let interaction = mote_protocol::TypedInteraction {
        host_id: host.host_id().clone(),
        thread: "main".into(),
        surface_seq: stale_seq,
        surface_contract: 1,
        node_path: vec![2],
        event: "change".into(),
        args: json!({"field":"budgets.router","value":170.0})
            .as_object()
            .unwrap()
            .clone(),
    };
    let reply = client
        .exchange(&mote_protocol::ClientFrame::Interaction {
            request_id: mote_protocol::RequestId::new("stale-slider").unwrap(),
            interaction,
        })
        .unwrap();
    assert!(matches!(
        reply,
        mote_protocol::HostFrame::Error { error, .. }
            if error.code == mote_protocol::HostErrorCode::InvalidParams
    ));
    assert_eq!(host.events_after(0).unwrap(), before);
    assert_eq!(
        host.daemon()
            .call("config.get", json!({"key":"budgets.router"}))
            .unwrap()["value"],
        160
    );
}

#[test]
#[ignore = "requires MOTE_LIVE=1 and native iPad Simulator"]
fn paired_committed_replacement_captures_frames_and_first_frame_action() {
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    let device = std::env::var("MOTE_SIMULATOR_ID").unwrap();
    let artifacts = PathBuf::from(std::env::var("MOTE_ARTIFACTS").unwrap());
    std::fs::create_dir_all(&artifacts).unwrap();
    let log = std::fs::File::create(artifacts.join("build.log")).unwrap();
    let derived = artifacts.join("derived");
    let mut build = NativeTestChild(
        Command::new("xcodebuild")
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
            .args([
                "-quiet",
                "-project",
                "ios/Mote/Mote.xcodeproj",
                "-scheme",
                "Mote",
                "-destination",
                &format!("id={device}"),
                "-derivedDataPath",
            ])
            .arg(&derived)
            .arg("build")
            .stdout(Stdio::from(log.try_clone().unwrap()))
            .stderr(Stdio::from(log))
            .spawn()
            .unwrap(),
    );
    let status = build.0.wait().unwrap();
    assert!(status.success(), "native build failed");
    command(
        "xcrun",
        &[
            "simctl",
            "install",
            &device,
            derived
                .join("Build/Products/Debug-iphonesimulator/Mote.app")
                .to_str()
                .unwrap(),
        ],
    );
    let attestation = std::fs::read(std::env::var("MOTE_SURFACE_ATTESTATION").unwrap()).unwrap();
    for (motion_name, motion) in [
        ("normal", Motion::Normal),
        ("reduced-motion", Motion::Reduced),
    ] {
        for (journey_name, journey) in [
            ("transition", Journey::Transition),
            ("action", Journey::Action),
        ] {
            let models = ScriptedServer::start().unwrap();
            let host = production_host(&models, &attestation);
            let name = format!("{motion_name}-{journey_name}");
            let replacement = capture(&host, &name, motion, journey, || {
                host.daemon()
                    .call(
                        "turn",
                        json!({"thread":"main","message":"Show the router budget control"}),
                    )
                    .unwrap();
            });
            assert_eq!(
                models
                    .recorded()
                    .iter()
                    .filter(|request| request.path == "/api/chat")
                    .count(),
                3
            );
            if matches!(journey, Journey::Action) {
                reject_stale_revision(&host, replacement);
            }
        }
    }
}
