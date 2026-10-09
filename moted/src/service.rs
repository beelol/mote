//! launchd service management (docs/rfcs/v0.md section 16). `moted install` writes the
//! `com.mote.moted` LaunchAgent with KeepAlive and log paths under the data directory and loads
//! it; `moted uninstall` unloads and removes it.
//!
//! Loading the agent is a system change; the owner runs `moted install`. The daemon never loads
//! it on its own.

use std::path::{Path, PathBuf};

pub const LABEL: &str = "com.mote.moted";

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
    let _ = std::process::Command::new("launchctl").args(["unload", "-w"]).arg(&plist).output();
    if plist.exists() {
        std::fs::remove_file(&plist).map_err(|e| e.to_string())?;
    }
    println!("removed {LABEL}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
