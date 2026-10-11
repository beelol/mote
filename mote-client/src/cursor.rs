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
