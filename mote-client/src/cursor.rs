use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const RECOVERY_WARNING: &str =
    "Saved event position could not be read. Replaying from the beginning.";
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, PartialEq, Eq)]
pub struct LoadedCursor {
    pub seq: i64,
    pub warning: Option<String>,
}

pub fn path() -> PathBuf {
    mote_paths::data_dir().join("client_cursor")
}

pub fn host_path(data_dir: &Path, host: &mote_protocol::HostId, scope: &str) -> PathBuf {
    let scope: String = scope.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect();
    data_dir.join("hosts").join(host.as_str()).join("cursors").join(scope)
}

pub fn migrate_legacy(data_dir: &Path, host: &mote_protocol::HostId) -> io::Result<LoadedCursor> {
    let target = host_path(data_dir, host, "main");
    if target.is_file() { return Ok(load(&target)); }
    let record = data_dir.join("client_cursor.migration");
    let (owner, legacy_seq) = if record.exists() {
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&record)?).map_err(io::Error::other)?;
        let owner: mote_protocol::HostId = serde_json::from_value(value["host_id"].clone()).map_err(io::Error::other)?;
        let seq = value["seq"].as_i64().filter(|seq| *seq >= 0).ok_or_else(|| io::Error::other("invalid cursor migration"))?;
        (owner, seq)
    } else {
        let loaded = load(&data_dir.join("client_cursor"));
        if loaded.warning.is_some() { return Ok(loaded); }
        let bytes = serde_json::to_vec(&serde_json::json!({"host_id":host,"seq":loaded.seq})).map_err(io::Error::other)?;
        let (temporary, mut file) = create_temp(data_dir, record.file_name())?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        let claim = std::fs::hard_link(&temporary, &record);
        std::fs::remove_file(&temporary)?;
        match claim {
            Ok(()) => File::open(data_dir)?.sync_all()?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return migrate_legacy(data_dir, host),
            Err(error) => return Err(error),
        }
        (host.clone(), loaded.seq)
    };
    if owner != *host { return Ok(load(&target)); }
    std::fs::create_dir_all(target.parent().unwrap())?;
    store(&target, legacy_seq)?;
    Ok(load(&target))
}

pub fn local_host_cursor(socket: &Path) -> io::Result<(PathBuf, LoadedCursor)> {
    local_host_path_in(socket, &mote_paths::data_dir())
}

fn local_host_path_in(socket: &Path, data_dir: &Path) -> io::Result<(PathBuf, LoadedCursor)> {
    let mut client = mote_ipc::Client::connect(socket)?;
    let hello = client.call("hello", serde_json::json!({}))?.map_err(|error| io::Error::other(error.message))?;
    let host: mote_protocol::HostId = serde_json::from_value(hello["host_id"].clone()).map_err(io::Error::other)?;
    let loaded = migrate_legacy(data_dir, &host)?;
    Ok((host_path(data_dir, &host, "main"), loaded))
}

pub fn load(path: &Path) -> LoadedCursor {
    match std::fs::read_to_string(path) {
        Ok(contents) => match contents.trim().parse::<i64>() {
            Ok(seq) if seq >= 0 => LoadedCursor { seq, warning: None },
            _ => recovered_cursor(),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => LoadedCursor {
            seq: 0,
            warning: None,
        },
        Err(_) => recovered_cursor(),
    }
}

fn recovered_cursor() -> LoadedCursor {
    LoadedCursor {
        seq: 0,
        warning: Some(RECOVERY_WARNING.into()),
    }
}

pub fn store(path: &Path, seq: i64) -> io::Result<()> {
    store_with(path, seq, |from, to| std::fs::rename(from, to))
}

fn store_with(
    path: &Path,
    seq: i64,
    rename: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    if seq < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cursor must be nonnegative",
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "cursor path has no parent directory",
        )
    })?;
    let (temp_path, mut temp) = create_temp(parent, path.file_name())?;
    let write_result = temp
        .write_all(seq.to_string().as_bytes())
        .and_then(|()| temp.sync_all());
    drop(temp);
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }
    if let Err(error) = rename(&temp_path, path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }
    File::open(parent)?.sync_all()
}

fn create_temp(parent: &Path, file_name: Option<&std::ffi::OsStr>) -> io::Result<(PathBuf, File)> {
    let file_name = file_name
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("cursor");
    loop {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{file_name}.tmp.{}.{id}", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn desktop_reads_the_daemon_public_key_identity_before_migrating() {
        struct IdentityDispatch(mote_protocol::HostId);
        impl mote_ipc::Dispatch for IdentityDispatch {
            fn call(&self, method: &str, _: serde_json::Value) -> Result<serde_json::Value, mote_ipc::RpcError> {
                assert_eq!(method, "hello");
                Ok(serde_json::json!({"host_id":self.0,"protocol_major":1}))
            }
        }
        let dir = TestDir::new("real-host");
        let data = dir.0.join("host");
        let identity = mote_gateway::HostIdentity::load_or_create(&data).unwrap();
        store(&data.join("client_cursor"), 88).unwrap();
        let socket = PathBuf::from("/tmp").join(format!("mote-cursor-identity-{}.sock", std::process::id()));
        let server = mote_ipc::Server::bind(&socket, std::sync::Arc::new(IdentityDispatch(identity.host_id().clone()))).unwrap();
        let (target, loaded) = local_host_path_in(server.path(), &data).unwrap();
        assert_eq!(loaded.seq, 88);
        assert_eq!(target, host_path(&data, identity.host_id(), "main"));
        assert_eq!(load(&target).seq, 88);
        assert_eq!(load(&data.join("client_cursor")).seq, 88);
    }

    #[test]
    fn legacy_cursor_migrates_once_under_the_authenticated_host_and_main_scope() {
        let dir = TestDir::new("host-migration");
        store(&dir.cursor(), 42).unwrap();
        let home = mote_protocol::HostId::from_public_key(&[1;32]);
        let office = mote_protocol::HostId::from_public_key(&[2;32]);
        assert_eq!(migrate_legacy(&dir.0, &home).unwrap().seq, 42);
        let home_path = host_path(&dir.0, &home, "main");
        assert_eq!(load(&home_path).seq, 42);
        store(&home_path, 43).unwrap();
        assert_eq!(migrate_legacy(&dir.0, &home).unwrap().seq, 43);
        assert_eq!(migrate_legacy(&dir.0, &office).unwrap().seq, 0);
        assert_eq!(load(&host_path(&dir.0, &home, "other")).seq, 0);
        assert_eq!(load(&dir.cursor()).seq, 42);
    }

    #[test]
    fn failed_host_cursor_write_preserves_legacy_and_retries_only_its_host() {
        let dir = TestDir::new("host-migration-failure");
        store(&dir.cursor(), 17).unwrap();
        let home = mote_protocol::HostId::from_public_key(&[1;32]);
        let office = mote_protocol::HostId::from_public_key(&[2;32]);
        let target = host_path(&dir.0, &home, "main");
        std::fs::create_dir_all(&target).unwrap();
        assert!(migrate_legacy(&dir.0, &home).is_err());
        assert_eq!(load(&dir.cursor()).seq, 17);
        assert_eq!(migrate_legacy(&dir.0, &office).unwrap().seq, 0);
        std::fs::remove_dir(&target).unwrap();
        assert_eq!(migrate_legacy(&dir.0, &home).unwrap().seq, 17);
    }

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("mote-cursor-{name}-{}-{id}", std::process::id()));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn cursor(&self) -> PathBuf {
            self.0.join("client_cursor")
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn missing_cursor_starts_at_zero_without_a_warning() {
        let dir = TestDir::new("missing");

        assert_eq!(
            load(&dir.cursor()),
            LoadedCursor {
                seq: 0,
                warning: None
            }
        );
    }

    #[test]
    fn valid_nonnegative_cursor_is_loaded() {
        let dir = TestDir::new("valid");
        for (contents, expected) in [("0\n", 0), ("42", 42), ("9223372036854775807", i64::MAX)] {
            std::fs::write(dir.cursor(), contents).unwrap();
            assert_eq!(
                load(&dir.cursor()),
                LoadedCursor {
                    seq: expected,
                    warning: None
                }
            );
        }
    }

    #[test]
    fn invalid_or_unreadable_cursor_recovers_without_leaking_contents() {
        let dir = TestDir::new("invalid");
        for contents in ["", "-1", "9223372036854775808", "private cursor contents"] {
            std::fs::write(dir.cursor(), contents).unwrap();
            let loaded = load(&dir.cursor());
            assert_eq!(loaded.seq, 0);
            let warning = loaded
                .warning
                .expect("invalid cursor needs a visible warning");
            if !contents.is_empty() {
                assert!(!warning.contains(contents));
            }
        }

        std::fs::remove_file(dir.cursor()).unwrap();
        std::fs::create_dir(dir.cursor()).unwrap();
        let loaded = load(&dir.cursor());
        assert_eq!(loaded.seq, 0);
        assert!(loaded.warning.is_some());
    }

    #[test]
    fn two_complete_replacements_leave_only_the_committed_cursor() {
        let dir = TestDir::new("replacement");

        store(&dir.cursor(), 10).unwrap();
        store(&dir.cursor(), 11).unwrap();

        assert_eq!(std::fs::read_to_string(dir.cursor()).unwrap(), "11");
        let names = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(names, ["client_cursor"]);
    }

    #[test]
    fn rename_failure_preserves_committed_cursor_and_removes_owned_temp() {
        let dir = TestDir::new("rename-failure");
        std::fs::write(dir.cursor(), "9").unwrap();

        let error = store_with(&dir.cursor(), 10, |_, _| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected rename failure",
            ))
        })
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(std::fs::read_to_string(dir.cursor()).unwrap(), "9");
        let names = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(names, ["client_cursor"]);
    }
}
