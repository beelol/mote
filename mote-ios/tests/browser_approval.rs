use mote_host_client::{ApprovalStatus, HostDirectory};
use mote_ipc::Dispatch;
use mote_protocol::PendingApproval;
use mote_testkit::{browser_fixture::BrowserFixture, gateway::LoopbackHost, ScriptedServer};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod support;
use support::{command, NativeTestChild, SimulatorStateGuard, LIVE_FILES};

#[test]
#[ignore = "explicit interrupted native test recovery only"]
fn native_recover_interrupted_simulator_state() {
    assert_eq!(std::env::var("MOTE_RECOVERY").as_deref(), Ok("1"));
    let device = std::env::var("MOTE_SIMULATOR_ID").unwrap();
    let host_id = std::env::var("MOTE_RECOVERY_HOST_ID").unwrap();
    let artifacts = PathBuf::from(std::env::var("MOTE_ARTIFACTS").unwrap());
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
    for name in LIVE_FILES {
        if documents.join(name).is_file() {
            std::fs::copy(
                documents.join(name),
                artifacts.join(format!("interrupted-{name}")),
            )
            .unwrap();
        }
    }
    let backup = artifacts.join("prior-live-state");
    let prior: Vec<_> = LIVE_FILES
        .into_iter()
        .filter(|name| backup.join(name).is_file())
        .map(|name| (name, std::fs::read(backup.join(name)).unwrap()))
        .collect();
    let mut cleanup =
        SimulatorStateGuard::resume(device, host_id, documents.clone(), backup, artifacts);
    cleanup.finish().unwrap();
    for (name, bytes) in prior {
        assert_eq!(std::fs::read(documents.join(name)).unwrap(), bytes);
    }
    println!("Interrupted native state restored byte-for-byte and temporary credential verified missing.");
}

fn save(path: &Path, value: &impl serde::Serialize) {
    std::fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn submit<D: Dispatch>(
    host: &LoopbackHost<D>,
    fixture: &BrowserFixture,
    artifacts: &Path,
    name: &str,
) -> PendingApproval {
    let opened = host
        .daemon()
        .call(
            "browser.open",
            json!({"thread":"main","url":fixture.url("/read")}),
        )
        .unwrap();
    let nodes = opened["snapshot"]["nodes"].as_array().unwrap();
    let reference =
        |role: &str| nodes.iter().find(|node| node["role"] == role).unwrap()["reference"].clone();
    let button = reference("button");
    let field = reference("textbox");
    let heading = reference("heading");
    let repeated = host
        .daemon()
        .call("browser.snapshot", json!({"thread":"main"}))
        .unwrap();
    assert!(repeated["snapshot"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["reference"] == button && node["name"] == "Submit"));
    let read = host
        .daemon()
        .call("browser.text", json!({"thread":"main","reference":heading}))
        .unwrap();
    assert_eq!(
        read["text"],
        "<untrusted_browser_page>\nFixture read\n</untrusted_browser_page>"
    );
    let value = format!("Native {name} fixture");
    host.daemon()
        .call(
            "browser.type",
            json!({"thread":"main","reference":field,"value":value}),
        )
        .unwrap();
    let filled = host
        .daemon()
        .call("browser.snapshot", json!({"thread":"main"}))
        .unwrap();
    assert!(filled["snapshot"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|node| node["reference"] == field && node["value"] == value));
    save(
        &artifacts.join(format!("{name}-read-fill.json")),
        &json!({"opened":opened,"repeated":repeated,"read":read,"filled":filled}),
    );
    let previous = host
        .events_after(0)
        .unwrap()
        .last()
        .map_or(0, |event| event.seq);
    let clicked = host
        .daemon()
        .call("browser.click", json!({"thread":"main","reference":button}))
        .unwrap();
    assert_eq!(clicked["status"], "approval_required");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(event) = host
            .events_after(previous)
            .unwrap()
            .into_iter()
            .find(|event| event.kind == "approval.created")
        {
            assert_eq!(event.payload["host_id"], json!(host.host_id()));
            assert_eq!(event.payload["status"], "pending");
            let approval = serde_json::from_value(event.payload["approval"].clone()).unwrap();
            save(&artifacts.join(format!("{name}-pending.json")), &event);
            return approval;
        }
        assert!(Instant::now() < deadline, "durable pending request missing");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "requires MOTE_LIVE=1, pinned Chrome and the native iPad Simulator"]
fn native_paired_deny_approve_repeat_and_natural_expiry() {
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    let device = std::env::var("MOTE_SIMULATOR_ID").unwrap();
    let chrome = std::env::var("MOTE_CHROME").unwrap();
    let artifacts = PathBuf::from(std::env::var("MOTE_ARTIFACTS").unwrap());
    std::fs::create_dir(&artifacts).expect("native evidence must not overwrite an earlier run");
    let derived = artifacts.join("derived");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let _ = Command::new("xcrun")
        .args(["simctl", "terminate", &device, "local.mote.simulator"])
        .output();
    let build_log = std::fs::File::create(artifacts.join("native-build.log")).unwrap();
    let built = Command::new("xcodebuild")
        .current_dir(root)
        .args([
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
        .stdout(Stdio::from(build_log.try_clone().unwrap()))
        .stderr(Stdio::from(build_log))
        .status()
        .unwrap();
    assert!(built.success(), "native build failed");
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
    let fixture = BrowserFixture::start().unwrap();
    let models = ScriptedServer::start().unwrap();
    let host = moted::test_fixture::start(&models).unwrap();
    let data = host.root().join("data");
    let mut cleanup = SimulatorStateGuard::new(
        Some(device.clone()),
        Some(host.host_id().as_str().into()),
        documents.clone(),
        artifacts.join("prior-live-state"),
        artifacts.clone(),
    );
    cleanup.backup_files().unwrap();
    host.daemon()
        .call(
            "matrix.render",
            json!({"thread":"main","source":"screen { text \"Browser fixture ready\" }"}),
        )
        .unwrap();
    let configs = json!([{"name":"Browser","public_key":host.public_key(),"endpoint":host.endpoint().unwrap(),"offer":host.open_pairing().unwrap()}]);
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
        assert!(Instant::now() < deadline, "native browser pairing timeout");
        std::thread::sleep(Duration::from_millis(20));
    }
    host.daemon().call("browser.configure", json!({"thread":"main","executable":chrome,"origins":[fixture.base_url()],"read_paths":["/read","/navigation","/favicon.ico"]})).unwrap();
    let version = command(&chrome, &["--version"]);
    assert_eq!(version, "Google Chrome for Testing 155.0.8059.39");
    let profile = data.join("browser/mote/local");
    assert!(profile.is_dir());
    let processes = command("ps", &["-axo", "command"]);
    let profile_arg = format!("--user-data-dir={}", profile.display());
    let argv = processes
        .lines()
        .find(|line| line.contains(&profile_arg))
        .unwrap();
    for required in [
        "--remote-debugging-address=127.0.0.1",
        "--remote-debugging-port=0",
        "--use-mock-keychain",
        "--app=about:blank",
    ] {
        assert!(
            argv.contains(required),
            "missing actual headed process argument {required}"
        );
    }
    assert!(!argv.contains("--headless"));
    save(
        &artifacts.join("environment.json"),
        &json!({"date":command("date", &["+%Y-%m-%d %H:%M:%S %Z"]),"macos":command("sw_vers", &[]),"xcode":command("xcodebuild", &["-version"]),"rust":command("rustc", &["--version"]),"simulator":device,"original_documents":documents,"chrome_version":version,"chrome_executable":chrome,"actual_chrome_argv":argv,"profile":profile,"fixture":fixture.base_url(),"host_id":host.host_id()}),
    );
    let test_log = artifacts.join("native-xctest.log");
    let log = std::fs::File::create(&test_log).unwrap();
    let mut test = NativeTestChild(Command::new("xcodebuild")
        .current_dir(root)
        .args(["-project","ios/Mote/Mote.xcodeproj","-scheme","Mote","-destination",&format!("id={device}"),"-derivedDataPath"])
        .arg(&derived)
        .arg("-resultBundlePath")
        .arg(artifacts.join("native.xcresult"))
        .args(["-only-testing:MoteUITests/MoteUITests/testBrowserApprovalDenyApproveRepeatAndNaturalExpiry","test"])
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(480);
    let mut records: Vec<PendingApproval> = Vec::new();
    let mut expiry_started = None;
    loop {
        let text = std::fs::read_to_string(&test_log).unwrap();
        let marker = match records.len() {
            0 => "MOTE BROWSER READY",
            1 => "MOTE BROWSER DENIED OBSERVED",
            2 => "MOTE BROWSER APPROVED OBSERVED",
            _ => "",
        };
        if records.len() < 3 && text.contains(marker) {
            let expected_effects = usize::from(records.len() == 2);
            assert_eq!(fixture.count("submit"), expected_effects);
            if let Some(previous) = records.last() {
                let expected = if records.len() == 1 {
                    "denied"
                } else {
                    "approved"
                };
                assert!(host.events_after(0).unwrap().iter().any(|event| event.kind
                    == "approval.updated"
                    && event.payload["approval"]["approval_id"] == previous.approval_id.as_str()
                    && event.payload["status"] == expected));
            }
            let name = ["deny", "approve", "expiry"][records.len()];
            if records.len() == 2 {
                expiry_started = Some(Instant::now());
            }
            records.push(submit(&host, &fixture, &artifacts, name));
            assert_eq!(fixture.count("submit"), expected_effects);
        }
        save(
            &artifacts.join("events.json"),
            &host.events_after(0).unwrap(),
        );
        save(
            &artifacts.join("effects.json"),
            &json!({"submit":fixture.count("submit"),"requests":records}),
        );
        if let Some(status) = test.0.try_wait().unwrap() {
            assert!(
                status.success(),
                "native approval XCUITest failed: {}",
                test_log.display()
            );
            break;
        }
        assert!(Instant::now() < deadline, "native approval journey timeout");
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(records.len(), 3);
    let documents = cleanup.refresh_documents().unwrap();
    assert!(expiry_started.unwrap().elapsed() >= Duration::from_secs(120));
    assert_eq!(fixture.count("submit"), 1);
    let events = host.events_after(0).unwrap();
    for (record, status, view_status) in [
        (&records[0], "denied", ApprovalStatus::Denied),
        (&records[1], "approved", ApprovalStatus::Approved),
        (&records[2], "expired", ApprovalStatus::Expired),
    ] {
        let terminal: Vec<_> = events
            .iter()
            .filter(|event| {
                event.kind == "approval.updated"
                    && event.payload["approval"]["approval_id"] == record.approval_id.as_str()
            })
            .collect();
        assert_eq!(terminal.len(), 1);
        assert_eq!(terminal[0].payload["status"], status);
        assert_eq!(terminal[0].payload["approval"], json!(record));
        let directory = HostDirectory::load(&documents.join("live-state.json")).unwrap();
        assert_eq!(
            directory.state(host.host_id()).unwrap().approval_states()[&record.approval_id].status,
            view_status
        );
    }
    let result = events
        .iter()
        .rev()
        .find(|event| event.kind == "ui.surface")
        .unwrap();
    let validated =
        mote_ui::v1::validate_envelope(&serde_json::to_vec(&result.payload).unwrap()).unwrap();
    let tree = result.payload.to_string();
    assert!(tree.contains("Approved request completed"));
    assert!(tree.contains("HTTP 200"));
    assert!(tree.contains(&fixture.base_url()));
    let directory = HostDirectory::load(&documents.join("live-state.json")).unwrap();
    let committed = directory.state(host.host_id()).unwrap().surfaces()["main"]
        .last_key_value()
        .unwrap()
        .1;
    assert_eq!(committed.surface_seq, result.seq);
    assert_eq!(json!(committed.tree), json!(validated.matrix_surface));
    let old = records[0].payload.clone();
    assert!(!json!(old).to_string().contains("Native deny fixture"));
    let read = host
        .daemon()
        .call(
            "browser.open",
            json!({"thread":"main","url":fixture.url("/read")}),
        )
        .unwrap();
    let old_reference = read["snapshot"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["role"] == "textbox")
        .unwrap()["reference"]
        .clone();
    host.daemon()
        .call(
            "browser.open",
            json!({"thread":"main","url":fixture.url("/navigation")}),
        )
        .unwrap();
    let stale = host
        .daemon()
        .call(
            "browser.type",
            json!({"thread":"main","reference":old_reference,"value":"stale"}),
        )
        .unwrap_err();
    save(
        &artifacts.join("stale-ref.json"),
        &json!({"reference":old_reference,"error":{"code":stale.code,"message":stale.message},"submit_effects":fixture.count("submit")}),
    );
    save(
        &artifacts.join("events.json"),
        &host.events_after(0).unwrap(),
    );
    let counts = command("sqlite3", &[data.join("mote.db").to_str().unwrap(),"select kind,status,count(*) from approvals group by kind,status; select action,status,count(*) from tool_ledger group by action,status; select count(*) from ledger; select count(*) from packs;"]);
    assert!(counts.contains("browser|approved|1"));
    assert!(counts.contains("browser|denied|1"));
    assert!(counts.contains("browser|expired|1"));
    assert!(counts.contains("browser.approved_request|ok|1"));
    assert!(counts.ends_with("0\n0"));
    std::fs::write(artifacts.join("ledger.txt"), counts).unwrap();
    std::fs::create_dir(artifacts.join("browser-evidence")).unwrap();
    let evidence: Vec<_> = std::fs::read_dir(data.join("browser-evidence"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(
        evidence
            .iter()
            .filter(|path| path.extension().is_some_and(|extension| extension == "png"))
            .count(),
        2
    );
    for path in evidence {
        if path.extension().is_some_and(|extension| extension == "har") {
            let raw = std::fs::read_to_string(&path).unwrap();
            let _: Value = serde_json::from_str(&raw).unwrap();
            assert!(!raw.contains("Native approve fixture"));
            assert!(!raw.contains("fixture-secret"));
        }
        std::fs::copy(
            &path,
            artifacts
                .join("browser-evidence")
                .join(path.file_name().unwrap()),
        )
        .unwrap();
    }
    let redacted = serde_json::to_string(&events).unwrap();
    assert!(!redacted.contains("Native approve fixture"));
    for name in LIVE_FILES {
        std::fs::copy(documents.join(name), artifacts.join(name)).unwrap();
    }
    save(
        &artifacts.join("natural-expiry.json"),
        &json!({"elapsed_ms":expiry_started.unwrap().elapsed().as_millis(),"expires_at":records[2].expires_at,"observed_at":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis(),"submit_effects":fixture.count("submit"),"clock_or_database_mutated":false}),
    );
    cleanup.finish().unwrap();
    println!("Native paired browser journey passed: denied 0 effects, approved exactly 1 effect despite repeated pointer taps, natural 120s expiry with no extra effect, terminal cards and same validated Matrix result. Evidence {}", artifacts.display());
}
