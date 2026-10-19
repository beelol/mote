//! Platform boundary for mote's filesystem locations.
//!
//! On macOS durable state lives under `~/Library/Application Support/mote`, mode 0700. The
//! control socket is `moted.sock` inside that directory. `MOTE_HOME` overrides the root, used
//! by tests and fixtures.

use std::path::{Path, PathBuf};

const MAX_UNIX_SOCKET_PATH_BYTES: usize = 100;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
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

/// The socket exposed by the owner's local Overseer daemon.
pub fn overseer_socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("OVERSEER_SOCKET") {
        return PathBuf::from(path);
    }
    let configured_home = std::env::var_os("OVERSEER_HOME").map(PathBuf::from);
    let data = configured_home.clone().unwrap_or_else(|| {
        if cfg!(target_os = "macos") {
            home().join("Library/Application Support/Overseer")
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home().join(".local/share"))
                .join("overseer")
        }
    });
    let runtime = if configured_home.is_some() {
        data.join("run")
    } else if cfg!(target_os = "linux") {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .map(|path| path.join("overseer"))
            .unwrap_or_else(|| data.join("run"))
    } else {
        data.join("run")
    };
    overseer_socket_path_for(&data, &runtime, unsafe { libc::getuid() as u32 })
}

fn overseer_socket_path_for(data_dir: &Path, runtime_dir: &Path, uid: u32) -> PathBuf {
    let preferred = runtime_dir.join("overseerd.sock");
    if preferred.as_os_str().len() < MAX_UNIX_SOCKET_PATH_BYTES {
        return preferred;
    }
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(data_dir.display().to_string().as_bytes());
    let fingerprint = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let key = &fingerprint[..8];
    PathBuf::from(format!("/tmp/overseer-{uid}/{key}-overseerd.sock"))
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

    #[test]
    fn overseer_socket_matches_short_and_fallback_layouts() {
        assert_eq!(
            overseer_socket_path_for(
                Path::new("/tmp/overseer-home"),
                Path::new("/tmp/overseer-home/run"),
                501
            ),
            PathBuf::from("/tmp/overseer-home/run/overseerd.sock")
        );
        let long = PathBuf::from(format!("/tmp/{}", "a".repeat(110)));
        let socket = overseer_socket_path_for(&long, &long.join("run"), 501);
        assert_eq!(socket.parent(), Some(Path::new("/tmp/overseer-501")));
        assert!(socket
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("-overseerd.sock"));
    }
}
