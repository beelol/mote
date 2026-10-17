//! launchd service management (docs/rfcs/v0.md section 16). `moted install` writes the
//! `com.mote.moted` LaunchAgent with KeepAlive and log paths under the data directory and loads
//! it; `moted uninstall` unloads and removes it.
//!
//! Loading the agent is a system change; the owner runs `moted install`. The daemon never loads
//! it on its own.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub const LABEL: &str = "com.mote.moted";

struct CommandResult {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostAction {
    Start,
    Status,
    Restart,
    Stop,
}

/// Path of the LaunchAgent plist.
pub fn plist_path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
    home.join("Library/LaunchAgents").join(format!("{LABEL}.plist"))
}

/// Generate the plist XML for the agent.
pub fn generate_plist(moted: &Path, data_dir: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{moted}</string>
    </array>
    <key>KeepAlive</key>
    <true/>
    <key>RunAtLoad</key>
    <true/>
    <key>StandardOutPath</key>
    <string>{data}/moted.out.log</string>
    <key>StandardErrorPath</key>
    <string>{data}/moted.err.log</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>MOTE_HOME</key>
        <string>{data}</string>
    </dict>
</dict>
</plist>
"#,
        moted = moted.display(),
        data = data_dir.display(),
    )
}

/// Write the plist and load it with launchctl.
pub fn install() -> Result<(), String> {
    let moted = std::env::current_exe().map_err(|e| e.to_string())?;
    let data = mote_paths::data_dir();
    let plist = plist_path();
    if let Some(parent) = plist.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&plist, generate_plist(&moted, &data)).map_err(|e| e.to_string())?;
    let out = std::process::Command::new("launchctl")
        .args(["load", "-w"])
        .arg(&plist)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    println!("loaded {LABEL} from {}", plist.display());
    Ok(())
}

/// Unload and remove the agent.
pub fn uninstall() -> Result<(), String> {
    let plist = plist_path();
    uninstall_with(&plist, |program, args| {
        let output = std::process::Command::new(program).args(args).output()?;
        Ok(CommandResult {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    })?;
    println!("removed {LABEL}");
    Ok(())
}

pub fn host(action: HostAction) -> Result<String, String> {
    let uid = current_uid()?;
    host_with(action, &plist_path(), uid, |program, args| {
        let output = std::process::Command::new(program).args(args).output()?;
        Ok(CommandResult {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    })
}

fn current_uid() -> Result<u32, String> {
    let output = std::process::Command::new("id")
        .arg("-u")
        .output()
        .map_err(|error| format!("could not determine user id: {error}"))?;
    if !output.status.success() {
        return Err("could not determine user id".to_string());
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|error| format!("invalid user id: {error}"))
}

fn host_with<F>(
    action: HostAction,
    plist: &Path,
    uid: u32,
    mut run: F,
) -> Result<String, String>
where
    F: FnMut(&str, &[&OsStr]) -> std::io::Result<CommandResult>,
{
    let domain = format!("gui/{uid}");
    let target = format!("{domain}/{LABEL}");
    let args = match action {
        HostAction::Start => vec![
            OsStr::new("bootstrap"),
            OsStr::new(&domain),
            plist.as_os_str(),
        ],
        HostAction::Status => vec![OsStr::new("print"), OsStr::new(&target)],
        HostAction::Restart => vec![
            OsStr::new("kickstart"),
            OsStr::new("-k"),
            OsStr::new(&target),
        ],
        HostAction::Stop => vec![OsStr::new("bootout"), OsStr::new(&target)],
    };
    let result = run("launchctl", &args)
        .map_err(|error| format!("could not run launchctl: {error}"))?;
    if !result.success {
        let detail = String::from_utf8_lossy(&result.stderr);
        let detail = detail.trim();
        return if detail.is_empty() {
            Err(format!("launchctl {} failed", action.name()))
        } else {
            Err(format!("launchctl {} failed: {detail}", action.name()))
        };
    }
    let stdout = String::from_utf8_lossy(&result.stdout).trim().to_string();
    Ok(if stdout.is_empty() {
        format!("{} {LABEL}", action.past_tense())
    } else {
        stdout
    })
}

impl HostAction {
    pub fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Status => "status",
            Self::Restart => "restart",
            Self::Stop => "stop",
        }
    }

    fn past_tense(self) -> &'static str {
        match self {
            Self::Start => "started",
            Self::Status => "reported",
            Self::Restart => "restarted",
            Self::Stop => "stopped",
        }
    }
}

fn uninstall_with<F>(plist: &Path, run: F) -> Result<(), String>
where
    F: FnOnce(&str, &[&OsStr]) -> std::io::Result<CommandResult>,
{
    let args = [OsStr::new("unload"), OsStr::new("-w"), plist.as_os_str()];
    let result = run("launchctl", &args)
        .map_err(|error| format!("could not run launchctl unload: {error}"))?;
    if !result.success {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let detail = stderr.trim();
        return if detail.is_empty() {
            Err("launchctl unload failed without an error message".to_string())
        } else {
            Err(format!("launchctl unload failed: {detail}"))
        };
    }
    if plist.exists() {
        std::fs::remove_file(plist).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> CommandResult {
        CommandResult {
            success: true,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    fn plist_fixture(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mote-service-{name}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::write(&path, "recovery state").unwrap();
        path
    }

    #[test]
    fn plist_has_label_keepalive_and_log_paths() {
        let p = generate_plist(Path::new("/usr/local/bin/moted"), Path::new("/data/mote"));
        assert!(p.contains("<string>com.mote.moted</string>"));
        assert!(p.contains("<key>KeepAlive</key>"));
        assert!(p.contains("/usr/local/bin/moted"));
        assert!(p.contains("/data/mote/moted.out.log"));
        assert!(p.contains("/data/mote/moted.err.log"));
        assert!(p.contains("<key>RunAtLoad</key>"));
    }

    #[test]
    fn uninstall_preserves_plist_when_launchctl_reports_stderr() {
        let plist = plist_fixture("nonzero-stderr");

        let error = uninstall_with(&plist, |program, args| {
            assert_eq!(program, "launchctl");
            assert_eq!(
                args,
                [OsStr::new("unload"), OsStr::new("-w"), plist.as_os_str()]
            );
            Ok(CommandResult {
                success: false,
                stdout: Vec::new(),
                stderr: b"service is busy\n".to_vec(),
            })
        })
        .unwrap_err();

        assert!(plist.exists());
        assert!(error.contains("service is busy"));
        std::fs::remove_file(plist).unwrap();
    }

    #[test]
    fn uninstall_preserves_plist_when_launchctl_reports_no_stderr() {
        let plist = plist_fixture("nonzero-empty");

        let error = uninstall_with(&plist, |program, args| {
            assert_eq!(program, "launchctl");
            assert_eq!(
                args,
                [OsStr::new("unload"), OsStr::new("-w"), plist.as_os_str()]
            );
            Ok(CommandResult {
                success: false,
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap_err();

        assert!(plist.exists());
        assert!(!error.trim().is_empty());
        std::fs::remove_file(plist).unwrap();
    }

    #[test]
    fn uninstall_preserves_plist_when_launchctl_cannot_start() {
        let plist = plist_fixture("spawn-error");

        let error = uninstall_with(&plist, |program, args| {
            assert_eq!(program, "launchctl");
            assert_eq!(
                args,
                [OsStr::new("unload"), OsStr::new("-w"), plist.as_os_str()]
            );
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "launchctl missing",
            ))
        })
        .unwrap_err();

        assert!(plist.exists());
        assert!(error.contains("launchctl missing"));
        std::fs::remove_file(plist).unwrap();
    }

    #[test]
    fn uninstall_removes_plist_after_launchctl_succeeds() {
        let plist = plist_fixture("success");

        uninstall_with(&plist, |program, args| {
            assert_eq!(program, "launchctl");
            assert_eq!(
                args,
                [OsStr::new("unload"), OsStr::new("-w"), plist.as_os_str()]
            );
            Ok(CommandResult {
                success: true,
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();

        assert!(!plist.exists());

        uninstall_with(&plist, |program, args| {
            assert_eq!(program, "launchctl");
            assert_eq!(
                args,
                [OsStr::new("unload"), OsStr::new("-w"), plist.as_os_str()]
            );
            Ok(CommandResult {
                success: true,
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
        .unwrap();
    }

    #[test]
    fn host_start_bootstraps_only_the_mote_launch_agent() {
        let plist = Path::new("/Users/test/Library/LaunchAgents/com.mote.moted.plist");
        let mut calls = Vec::new();

        host_with(HostAction::Start, plist, 501, |program, args| {
            calls.push((
                program.to_string(),
                args.iter().map(|arg| arg.to_os_string()).collect::<Vec<_>>(),
            ));
            Ok(ok())
        })
        .unwrap();

        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "launchctl");
        assert_eq!(
            calls[0].1,
            [OsStr::new("bootstrap"), OsStr::new("gui/501"), plist.as_os_str()]
        );
    }

    #[test]
    fn host_stop_boots_out_only_the_mote_launch_agent() {
        let plist = Path::new("/Users/test/Library/LaunchAgents/com.mote.moted.plist");
        let mut calls = Vec::new();

        host_with(HostAction::Stop, plist, 501, |program, args| {
            calls.push((
                program.to_string(),
                args.iter().map(|arg| arg.to_os_string()).collect::<Vec<_>>(),
            ));
            Ok(ok())
        })
        .unwrap();

        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].1,
            [OsStr::new("bootout"), OsStr::new("gui/501/com.mote.moted")]
        );
    }

    #[test]
    fn host_restart_kickstarts_only_the_mote_launch_agent() {
        let plist = Path::new("/Users/test/Library/LaunchAgents/com.mote.moted.plist");
        let mut calls = Vec::new();

        host_with(HostAction::Restart, plist, 501, |program, args| {
            calls.push((
                program.to_string(),
                args.iter().map(|arg| arg.to_os_string()).collect::<Vec<_>>(),
            ));
            Ok(ok())
        })
        .unwrap();

        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].1,
            [
                OsStr::new("kickstart"),
                OsStr::new("-k"),
                OsStr::new("gui/501/com.mote.moted")
            ]
        );
    }

    #[test]
    fn host_status_prints_only_the_mote_launch_agent() {
        let plist = Path::new("/Users/test/Library/LaunchAgents/com.mote.moted.plist");
        let mut calls = Vec::new();

        host_with(HostAction::Status, plist, 501, |program, args| {
            calls.push((
                program.to_string(),
                args.iter().map(|arg| arg.to_os_string()).collect::<Vec<_>>(),
            ));
            Ok(ok())
        })
        .unwrap();

        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].1,
            [OsStr::new("print"), OsStr::new("gui/501/com.mote.moted")]
        );
    }

    #[test]
    fn host_actions_preserve_the_plist_when_launchctl_fails() {
        for action in [
            HostAction::Start,
            HostAction::Status,
            HostAction::Restart,
            HostAction::Stop,
        ] {
            let plist = plist_fixture(action.name());
            let error = host_with(action, &plist, 501, |_program, _args| {
                Ok(CommandResult {
                    success: false,
                    stdout: Vec::new(),
                    stderr: b"not permitted\n".to_vec(),
                })
            })
            .unwrap_err();

            assert!(plist.exists());
            assert!(error.contains("not permitted"));
            std::fs::remove_file(plist).unwrap();
        }
    }
}
