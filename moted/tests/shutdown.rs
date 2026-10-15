use mote_gateway::discovery::{discover, DnsSdBrowser};
use mote_gateway::HostIdentity;
use std::{
    fs,
    io::Read,
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    process::{Child, Command, ExitStatus, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

static DAEMON_LISTENER: Mutex<()> = Mutex::new(());

struct DaemonProcess {
    child: Child,
    data: PathBuf,
    advertiser: Option<i32>,
}

impl DaemonProcess {
    fn start(label: &str, discovery: bool) -> Self {
        let data =
            std::env::temp_dir().join(format!("moted-shutdown-{}-{label}", std::process::id()));
        fs::DirBuilder::new().mode(0o700).create(&data).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_moted"))
            .env("MOTE_HOME", &data)
            .env("MOTE_DISCOVERY", if discovery { "1" } else { "0" })
            .env(
                "MOTE_GATEWAY_BIND",
                if discovery {
                    "0.0.0.0:47810"
                } else {
                    "127.0.0.1:0"
                },
            )
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut daemon = Self {
            child,
            data,
            advertiser: None,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(mut client) = mote_ipc::Client::connect(&daemon.data.join("moted.sock")) {
                if client
                    .call("ping", serde_json::json!({}))
                    .is_ok_and(|result| result.is_ok())
                {
                    break;
                }
            }
            if let Some(status) = daemon.child.try_wait().unwrap() {
                let mut stderr = String::new();
                daemon
                    .child
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut stderr)
                    .unwrap();
                panic!("daemon exited before startup: {status}: {stderr}");
            }
            assert!(Instant::now() < deadline, "daemon never became available");
            std::thread::sleep(Duration::from_millis(20));
        }
        daemon
    }

    fn find_advertiser(&mut self) -> i32 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let output = Command::new("/usr/bin/pgrep")
                .args(["-P", &self.child.id().to_string(), "-x", "dns-sd"])
                .output()
                .unwrap();
            if output.status.success() {
                let children = String::from_utf8(output.stdout).unwrap();
                let ids: Vec<i32> = children.lines().map(|line| line.parse().unwrap()).collect();
                assert_eq!(ids.len(), 1, "expected exactly one advertiser child");
                self.advertiser = Some(ids[0]);
                return ids[0];
            }
            assert!(
                Instant::now() < deadline,
                "daemon did not start its advertiser"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn signal_and_wait(&mut self, signal: i32) -> ExitStatus {
        assert_eq!(unsafe { libc::kill(self.child.id() as i32, signal) }, 0);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "daemon did not terminate");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for DaemonProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(pid) = self.advertiser {
            if unsafe { libc::kill(pid, 0) } == 0 {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
        let _ = fs::remove_dir_all(&self.data);
    }
}

#[test]
fn sigterm_and_sigint_release_the_unix_listener() {
    let _listener = DAEMON_LISTENER.lock().unwrap();
    for (label, signal) in [("term", libc::SIGTERM), ("int", libc::SIGINT)] {
        let mut daemon = DaemonProcess::start(label, false);
        let status = daemon.signal_and_wait(signal);
        assert!(
            status.success(),
            "signal {signal} bypassed graceful cleanup: {status}"
        );
        assert!(
            !daemon.data.join("moted.sock").exists(),
            "Unix listener was not released"
        );
        println!("signal {signal}: daemon exited normally and released its Unix listener");
    }
}

#[test]
#[ignore = "requires MOTE_LIVE=1 and the Mac Bonjour stack"]
fn live_sigterm_reaps_advertiser_and_removes_registration() {
    let _listener = DAEMON_LISTENER.lock().unwrap();
    assert_eq!(std::env::var("MOTE_LIVE").as_deref(), Ok("1"));
    let mut daemon = DaemonProcess::start("live-term", true);
    let advertiser = daemon.find_advertiser();
    let identity = HostIdentity::load_or_create(&daemon.data).unwrap();
    let fingerprint = identity.host_id().as_str();
    let records = discover(true, &DnsSdBrowser, Duration::from_secs(2)).unwrap();
    assert!(
        records
            .iter()
            .any(|record| record.fingerprint == fingerprint),
        "daemon registration not discovered: {records:?}"
    );
    println!(
        "Before SIGTERM: daemon {} owns live advertiser {advertiser}",
        daemon.child.id()
    );
    let status = daemon.signal_and_wait(libc::SIGTERM);
    let child_exists = unsafe { libc::kill(advertiser, 0) } == 0;
    let records = discover(true, &DnsSdBrowser, Duration::from_secs(2)).unwrap();
    let registration_exists = records
        .iter()
        .any(|record| record.fingerprint == fingerprint);
    println!("After SIGTERM: status={status}, child_exists={child_exists}, registration_exists={registration_exists}");
    assert!(!child_exists, "SIGTERM orphaned advertiser {advertiser}");
    assert!(!registration_exists, "stopped gateway is still advertised");
    assert!(
        status.success(),
        "SIGTERM bypassed graceful cleanup: {status}"
    );
    assert!(!daemon.data.join("moted.sock").exists());
}
