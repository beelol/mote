//! V0-30: with local_only true and the think harness set to Claude or Codex, moted refuses at
//! startup with a logged reason.

use std::process::{Command, Stdio};

#[test]
fn refuses_subscription_think_harness_at_startup() {
    let home = std::env::temp_dir().join(format!("moted-lo-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();

    // Pre-write a config that names a subscription harness for think.
    let config = "\
[models]
local_only = true
talk = \"qwen3:4b\"
router = \"qwen3:0.6b\"
think = \"claude-opus-4-8\"

[budgets]
talk_first_token = 300
router = 150
think_first_token = 3000

[packs]
talk = 600
think = 8000
";
    std::fs::write(home.join("config.toml"), config).unwrap();

    let bin = env!("CARGO_BIN_EXE_moted");
    let out = Command::new(bin)
        .env("MOTE_HOME", &home)
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .output()
        .unwrap();

    assert!(!out.status.success(), "moted must refuse and exit non-zero");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("subscription harness"),
        "stderr must name the refusal reason, got: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&home);
}
