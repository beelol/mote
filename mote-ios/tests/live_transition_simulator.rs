use mote_eventlog::{EventLog, Replay};
use mote_host_client::HostDirectory;
use mote_ipc::{Actor, Dispatch, RpcError};
use mote_testkit::{gateway::LoopbackHost, ScriptedServer};
use serde_json::{json, Value};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

mod support;
use support::{command, NativeTestChild, SimulatorStateGuard};

const BEFORE: &str = "screen { header { button \"Stable\" -> refresh text \"Header\" } button \"Before\" -> refresh toggle \"Local only\" models.local_only footer { text \"Footer\" } }";
const AFTER: &str = "screen { header { button \"Stable\" -> refresh text \"Header\" text \"Second\" text \"Third\" } button \"Replacement\" -> refresh toggle \"Local only\" models.local_only footer { text \"Footer\" text \"Second\" text \"Third\" } }";

fn wait(mut predicate: impl FnMut() -> bool, message: &str) {
    let deadline = Instant::now() + Duration::from_secs(90);
    while !predicate() {
        assert!(Instant::now() < deadline, "{message}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn capture<D: Dispatch>(host: &LoopbackHost<D>, name: &str, replace: impl FnOnce()) {
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
    let launched = Command::new("xcrun")
        .args([
            "simctl",
            "launch",
            &device,
            "local.mote.simulator",
            "--mote-live-transition-probe",
        ])
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
    assert!(!first["immediate_action_target"].is_null());
    assert!(
        first["emitted_events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event.as_str().unwrap().contains("refresh")),
        "first-frame action did not reach renderer"
    );
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
    assert!(first["particle_count"].as_u64().unwrap() > 0);
    assert!(midpoint["particle_count"].as_u64().unwrap() > 0);
    assert_eq!(settled["particle_count"], 0);
    let before = frames
        .iter()
        .rev()
        .find(|frame| frame["revision"].as_i64().unwrap() < replacement)
        .unwrap();
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
    assert_ne!(
        old_border["points"], new_border["points"],
        "changed button border must move"
    );
    assert!(
        new_border["points"][0][1].as_f64().unwrap() > old_border["points"][0][1].as_f64().unwrap(),
        "changed button border must move below the taller header"
    );
    assert_eq!(old_border["painted"], true);
    assert_eq!(new_border["painted"], true);
    assert_eq!(new_border["particle_count"], 0);
    for frame in [first, midpoint] {
        let materializing = border(frame);
        assert_eq!(materializing["path"], json!([1]));
        assert_eq!(materializing["points"], new_border["points"]);
        assert_eq!(materializing["previous_points"], old_border["points"]);
        assert_eq!(
            materializing["painted"], false,
            "changed solid button border must be hidden while materializing"
        );
        assert!(
            materializing["particle_count"].as_u64().unwrap() > 0,
            "button-owned particles must appear independently of rules and rails"
        );
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
    assert_eq!(
        old_lines.len(),
        if name == "typed-slider-fixture" { 3 } else { 2 }
    );
    assert_eq!(old_lines.len(), new_lines.len());
    assert!(
        old_lines
            .iter()
            .zip(new_lines)
            .all(|(before, after)| before != after),
        "every header/footer rule and slider rail must move"
    );
    for (name, frame) in [
        ("before", before),
        ("first", first),
        ("midpoint", midpoint),
        ("settled", settled),
    ] {
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
            .arg(&video_path)
            .args(["-vf", &filter, "-fps_mode", "passthrough", "-frames:v", "1"])
            .arg(artifacts.join(format!("{name}.png")))
            .status()
            .unwrap();
        assert!(status.success(), "video frame extraction failed");
    }
    let actions: Vec<_> = events
        .iter()
        .filter(|event| event.kind == "ui.event")
        .collect();
    assert_eq!(
        actions
            .iter()
            .filter(|event| event.seq > replacement)
            .count(),
        1,
        "first-frame action must emit exactly once"
    );
    assert_eq!(actions[0].payload["event"], "refresh");
    if name == "typed-slider-fixture" {
        assert_eq!(actions.last().unwrap().payload["surface_seq"], replacement);
        assert_eq!(actions.last().unwrap().payload["node_path"], json!([1]));
    }
    std::fs::write(artifacts.join("selected-frames.json"), serde_json::to_vec_pretty(&json!({"before":before,"first":first,"midpoint":midpoint,"settled":settled,"unchanged_rectangle":stable})).unwrap()).unwrap();
    cleanup.finish().unwrap();
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
    let models = ScriptedServer::start().unwrap();
    let host = moted::test_fixture::start(&models).unwrap();
    host.daemon()
        .call("matrix.render", json!({"thread":"main","source":BEFORE}))
        .unwrap();
    capture(&host, "real-daemon", || {
        host.daemon()
            .call("matrix.render", json!({"thread":"main","source":AFTER}))
            .unwrap();
    });
    let slider = LoopbackHost::start(|data| {
        let log = EventLog::open(&data.join("mote.db"))?;
        std::fs::set_permissions(data.join("mote.db"), std::fs::Permissions::from_mode(0o600))?;
        Ok((Arc::new(SliderFixture { log: log.clone() }), log))
    })
    .unwrap();
    slider.daemon().commit(false);
    capture(&slider, "typed-slider-fixture", || {
        slider.daemon().commit(true)
    });
}

struct SliderFixture {
    log: EventLog,
}
impl SliderFixture {
    fn commit(&self, changed: bool) {
        let tree = json!({"version":1,"root":{"type":"screen","children":[
            {"type":"header","children":[{"type":"button","label":"Stable","event":"refresh"},{"type":"text","value":if changed {"Header\nSecond\nThird"} else {"Header"}}]},
            {"type":"button","label":if changed {"Replacement"} else {"Before"},"event":"refresh"},
            {"type":"slider","label":"Volume","field":"volume","value":4,"min":0,"max":10},
            {"type":"footer","children":[{"type":"text","value":if changed {"Footer\nSecond\nThird"} else {"Footer"}}]}
        ]}});
        mote_ui::v1::validate(&serde_json::to_vec(&tree).unwrap()).unwrap();
        self.log
            .append(
                "main",
                "ui.surface",
                &json!({"matrix_surface":tree,"surface":{"type":"text","value":"Matrix fixture"}}),
            )
            .unwrap();
    }
}
impl Dispatch for SliderFixture {
    fn call(&self, _: &str, _: Value) -> Result<Value, RpcError> {
        Err(RpcError::invalid_params("authenticated fixture only"))
    }
    fn call_with_context(
        &self,
        actor: &Actor,
        method: &str,
        params: Value,
    ) -> Result<Value, RpcError> {
        let Actor::PairedDevice { host_id, device_id } = actor else {
            return self.call(method, params);
        };
        if method == "snapshot" {
            return Ok(serde_json::to_value(
                self.log
                    .host_snapshot_for_device(host_id, device_id, 0)
                    .unwrap(),
            )
            .unwrap());
        }
        let committed = (1..=self.log.max_seq().unwrap()).rev().find_map(|seq| {
            self.log
                .get(seq)
                .unwrap()
                .filter(|event| event.kind == "ui.surface")
        });
        if method == "interaction"
            && params["host_id"] == json!(host_id)
            && params["thread"] == "main"
            && params["surface_contract"] == 1
            && committed
                .as_ref()
                .is_some_and(|surface| params["surface_seq"] == surface.seq)
            && params["args"] == json!({})
            && [json!([1]), json!([0, 0])].contains(&params["node_path"])
            && params["event"] == "refresh"
        {
            self.log.append("main", "ui.event", &params).unwrap();
            return Ok(json!({"accepted":true}));
        }
        Err(RpcError::invalid_params("fixture action rejected"))
    }
    fn subscribe_host(&self, cursor: i64) -> Option<Replay> {
        self.log.subscribe_host(cursor).ok()
    }
}
