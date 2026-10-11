use mote_gateway::discovery::{Advertiser, DiscoveryRecord, DnsSdAdvertiser};
use mote_host_client::HostDirectory;
use mote_ios::live::HostConfig;
use mote_ipc::Dispatch;
use mote_testkit::ScriptedServer;
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn simctl(args: &[&str]) -> String {
    let output = Command::new("xcrun")
        .arg("simctl")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "simctl {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn load(path: &Path) -> Option<HostDirectory> {
    HostDirectory::load(path).ok()
}

struct ImpostorAdvertisement(std::process::Child);
impl Drop for ImpostorAdvertisement {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn advertise_impostor(record: &DiscoveryRecord) -> ImpostorAdvertisement {
    let name = format!("mote-impostor-{}", record.endpoint.port());
    let child = Command::new("/usr/bin/dns-sd")
        .args([
            "-lo",
            "-P",
            &name,
            "_mote._tcp",
            "local",
            &record.endpoint.port().to_string(),
            &format!("{name}.local"),
            "127.0.0.1",
        ])
        .args(record.txt())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    ImpostorAdvertisement(child)
}

#[test]
#[ignore = "requires MOTE_LIVE=1 and an installed iPad Simulator app"]
fn native_pair_chat_edit_relaunch_discover_and_keychain_delete() {
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    let device = std::env::var("MOTE_SIMULATOR_ID").expect("MOTE_SIMULATOR_ID");
    let artifacts = PathBuf::from(std::env::var("MOTE_ARTIFACTS").expect("MOTE_ARTIFACTS"));
    std::fs::create_dir_all(&artifacts).unwrap();
    let container = PathBuf::from(simctl(&[
        "get_app_container",
        &device,
        "local.mote.simulator",
        "data",
    ]));
    let documents = container.join("Documents");
    let backup = artifacts.join("prior-live-state");
    std::fs::create_dir_all(&backup).unwrap();
    for name in ["live-state.json", "live-hosts.json", "live-network.json"] {
        let prior = documents.join(name);
        if prior.exists() {
            std::fs::rename(prior, backup.join(name)).unwrap();
        }
    }
    let models = ScriptedServer::start().unwrap();
    models.set_chat_chunks(vec![json!({"message":{"role":"assistant","content":"{\"route\":\"talk\",\"reason\":\"greeting\"}"},"done":true}).to_string()]);
    let mut home = moted::test_fixture::start(&models).unwrap();
    let office = moted::test_fixture::start(&models).unwrap();
    let impostor = moted::test_fixture::start(&models).unwrap();
    for (host, label) in [(&home, "Home dashboard"), (&office, "Office dashboard")] {
        host.daemon().call("matrix.render", json!({"thread":"main","source":format!("screen {{ header {{ text \"{label}\" }} col {{ select \"Talk model\" models.talk options model_options }} }}")})).unwrap();
    }
    let advertised_home =
        DiscoveryRecord::new(home.endpoint().unwrap(), 1, home.public_key()).unwrap();
    let mut home_advertisement = DnsSdAdvertiser.start(&advertised_home).unwrap();
    let _office_advertisement = DnsSdAdvertiser
        .start(&DiscoveryRecord::new(office.endpoint().unwrap(), 1, office.public_key()).unwrap())
        .unwrap();
    let impostor_endpoint = impostor.endpoint().unwrap();
    let _impostor_advertisement =
        advertise_impostor(&DiscoveryRecord::new(impostor_endpoint, 1, home.public_key()).unwrap());
    let configs = vec![
        HostConfig {
            name: "Home".into(),
            public_key: *home.public_key(),
            endpoint: home.endpoint().unwrap(),
            offer: Some(home.open_pairing().unwrap()),
        },
        HostConfig {
            name: "Office".into(),
            public_key: *office.public_key(),
            endpoint: office.endpoint().unwrap(),
            offer: Some(office.open_pairing().unwrap()),
        },
    ];
    let launch_configs = configs.iter().map(|config| json!({"name":config.name,"public_key":config.public_key,"endpoint":config.endpoint,"offer":config.offer})).collect::<Vec<_>>();
    let _ = Command::new("xcrun")
        .args(["simctl", "terminate", &device, "local.mote.simulator"])
        .output();
    let output = Command::new("xcrun")
        .args(["simctl", "launch", &device, "local.mote.simulator"])
        .env(
            "SIMCTL_CHILD_MOTE_SIMULATOR_HOSTS",
            serde_json::to_string(&launch_configs).unwrap(),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "launch failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let initial_endpoint = home.endpoint().unwrap();
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        home.confirm_pending(true).unwrap();
        office.confirm_pending(true).unwrap();
        if load(&documents.join("live-state.json")).is_some_and(|directory| {
            directory.hosts().count() == 2
                && directory.state(home.host_id()).unwrap().cursor().seq >= 1
                && directory.state(office.host_id()).unwrap().cursor().seq >= 1
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Simulator did not pair with both real fixture hosts"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let before = load(&documents.join("live-state.json")).unwrap();
    let initial_cursor = before.state(home.host_id()).unwrap().cursor().seq;
    let log = std::fs::File::create(artifacts.join("native-xctest.log")).unwrap();
    let mut test = Command::new("xcodebuild")
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
        .args([
            "-project",
            "ios/Mote/Mote.xcodeproj",
            "-scheme",
            "Mote",
            "-destination",
            &format!("id={device}"),
            "-derivedDataPath",
            "/private/tmp/mote-task7-derived",
            "-resultBundlePath",
        ])
        .arg(artifacts.join("native.xcresult"))
        .args([
            "-only-testing:MoteUITests/MoteUITests/testLivePairedHostsChatDashboardAndRelaunch",
            "test",
        ])
        .stdout(Stdio::from(log.try_clone().unwrap()))
        .stderr(Stdio::from(log))
        .spawn()
        .unwrap();
    let mut rotated = false;
    while test.try_wait().unwrap().is_none() {
        home.confirm_pending(true).unwrap();
        office.confirm_pending(true).unwrap();
        let events = home.events_after(0).unwrap();
        if !rotated && events.iter().any(|event| event.kind == "talk.done") {
            home.rotate_endpoint().unwrap();
            drop(home_advertisement);
            home_advertisement = DnsSdAdvertiser
                .start(
                    &DiscoveryRecord::new(home.endpoint().unwrap(), 1, home.public_key()).unwrap(),
                )
                .unwrap();
            rotated = true;
        }
        assert!(
            Instant::now() < deadline + Duration::from_secs(240),
            "native journey timed out"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let status = test.wait().unwrap();
    let documents = PathBuf::from(simctl(&[
        "get_app_container",
        &device,
        "local.mote.simulator",
        "data",
    ]))
    .join("Documents");
    for name in ["live-state.json", "live-hosts.json", "live-network.json"] {
        if documents.join(name).exists() {
            std::fs::copy(documents.join(name), artifacts.join(name)).unwrap();
        }
    }
    std::fs::write(
        artifacts.join("home-events.json"),
        serde_json::to_vec_pretty(&home.events_after(0).unwrap()).unwrap(),
    )
    .unwrap();
    std::fs::write(
        artifacts.join("office-events.json"),
        serde_json::to_vec_pretty(&office.events_after(0).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(
        status.success(),
        "native XCUITest failed; inspect {}",
        artifacts.join("native-xctest.log").display()
    );
    assert!(rotated);
    let directory = load(&documents.join("live-state.json")).unwrap();
    let messages = &directory.state(home.host_id()).unwrap().threads()["main"].messages;
    assert_eq!(
        messages
            .iter()
            .filter(|message| message.text == "hello")
            .count(),
        1
    );
    assert!(directory.state(office.host_id()).unwrap().threads()["main"]
        .messages
        .iter()
        .all(|message| message.text != "hello"));
    assert_eq!(directory.selected_host(), Some(home.host_id()));
    let events = home.events_after(0).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == "turn.user")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == "ui.event")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == "talk.done" && event.payload["ledger_id"].is_number())
            .count(),
        1
    );
    let network: Vec<serde_json::Value> =
        serde_json::from_reader(std::fs::File::open(documents.join("live-network.json")).unwrap())
            .unwrap();
    assert!(network.iter().any(|row| row["kind"] == "identity_proof"
        && row["candidate"] == impostor_endpoint.to_string()
        && row["accepted"] == false
        && row["prior"] == row["current"]));
    assert!(network.iter().any(|row| row["kind"] == "keychain_restored"
        && row["host_id"] == home.host_id().as_str()
        && row["saved_cursor"]
            .as_i64()
            .is_some_and(|cursor| cursor > initial_cursor)));
    assert_ne!(home.endpoint().unwrap(), initial_endpoint);
    let _ = Command::new("xcrun")
        .args(["simctl", "terminate", &device, "local.mote.simulator"])
        .output();
    let deleted = Command::new("xcrun")
        .args(["simctl", "launch", &device, "local.mote.simulator"])
        .env(
            "SIMCTL_CHILD_MOTE_SIMULATOR_KEYCHAIN_DELETE",
            home.host_id().as_str(),
        )
        .output()
        .unwrap();
    assert!(deleted.status.success());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let network: Vec<serde_json::Value> = serde_json::from_reader(
            std::fs::File::open(documents.join("live-network.json")).unwrap(),
        )
        .unwrap();
        if network.iter().any(|row| {
            row["kind"] == "keychain_deleted"
                && row["host_id"] == home.host_id().as_str()
                && row["missing"] == true
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Simulator Keychain deletion did not complete"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    std::fs::copy(
        documents.join("live-network.json"),
        artifacts.join("live-network-after-delete.json"),
    )
    .unwrap();
    simctl(&[
        "io",
        &device,
        "screenshot",
        artifacts.join("credential-deleted.png").to_str().unwrap(),
    ]);
    println!("Simulator paired two hosts, completed native chat and typed dashboard edit, resumed cursor without duplicates, moved {} to {}, rejected {}, restored and deleted native Keychain credentials. Home cursor {} -> {}.", initial_endpoint, home.endpoint().unwrap(), impostor_endpoint, initial_cursor, directory.state(home.host_id()).unwrap().cursor().seq);
    drop(home_advertisement);
}
