//! Platform boundary for mote's filesystem locations.
//!
//! On macOS durable state lives under `~/Library/Application Support/mote`, mode 0700. The
//! control socket is `moted.sock` inside that directory. `MOTE_HOME` overrides the root, used
//! by tests and fixtures.

use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// Root for durable state. `MOTE_HOME` overrides the platform default.
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("MOTE_HOME") {
        return PathBuf::from(dir);
    }
    if cfg!(target_os = "macos") {
        home().join("Library/Application Support/mote")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join(".local/share"))
            .join("mote")
    }
}

/// The Unix socket path inside the data directory.
pub fn socket_path() -> PathBuf {
    data_dir().join("moted.sock")
}

/// The single SQLite database file.
pub fn db_path() -> PathBuf {
    data_dir().join("mote.db")
}

/// The config file.
pub fn config_path() -> PathBuf {
    data_dir().join("config.toml")
}

/// Create a directory and its parents, readable only by the current user (mode 0700).
pub fn ensure_private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

/// Create the data directory with mode 0700 and return it.
pub fn ensure_data_dir() -> std::io::Result<PathBuf> {
    let dir = data_dir();
    ensure_private_dir(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    // One test to avoid a race on the process-global MOTE_HOME env var.
    #[test]
    fn paths_and_mode() {
        let tmp = std::env::temp_dir().join(format!("mote-paths-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("MOTE_HOME", &tmp);

        assert_eq!(data_dir(), tmp);
        assert_eq!(socket_path(), tmp.join("moted.sock"));
        assert_eq!(db_path(), tmp.join("mote.db"));
        assert_eq!(config_path(), tmp.join("config.toml"));

        let dir = ensure_data_dir().expect("create data dir");
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "data dir must be owner-only");

        std::env::remove_var("MOTE_HOME");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
