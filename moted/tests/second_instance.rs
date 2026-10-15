//! V0-08: the socket is mode 0600 and a second daemon exits non-zero naming the socket path.

use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn wait_for_socket(path: &std::path::Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn second_instance_exits_naming_socket() {
    let home = std::env::temp_dir().join(format!("moted-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let socket = home.join("moted.sock");

    let bin = env!("CARGO_BIN_EXE_moted");

    // First instance stays alive in the background.
    let mut first = Command::new(bin)
        .env("MOTE_HOME", &home)
        .env("MOTE_DISCOVERY", "0")
        .env("MOTE_GATEWAY_BIND", "127.0.0.1:0")
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    assert!(wait_for_socket(&socket, Duration::from_secs(10)), "first daemon never bound");

    let mode = std::fs::metadata(&socket).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "socket must be mode 0600");

    // Second instance must fail naming the path.
    let second = Command::new(bin)
        .env("MOTE_HOME", &home)
        .env("MOTE_DISCOVERY", "0")
        .env("MOTE_GATEWAY_BIND", "127.0.0.1:0")
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let out = second.wait_with_output().unwrap();
    assert!(!out.status.success(), "second daemon must exit non-zero");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(socket.to_str().unwrap()),
        "second daemon must name the socket path, got: {stderr}"
    );

    first.kill().ok();
    first.wait().ok();
    let _ = std::fs::remove_dir_all(&home);
}
