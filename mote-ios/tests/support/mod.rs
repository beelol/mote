use std::{
    path::PathBuf,
    process::{Child, Command},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const LIVE_FILES: [&str; 3] = ["live-state.json", "live-hosts.json", "live-network.json"];
const PRIOR_FILES_MANIFEST: &str = "prior-live-files.json";

pub struct NativeTestChild(pub Child);

impl Drop for NativeTestChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub struct SimulatorStateGuard {
    device: Option<String>,
    host_id: Option<String>,
    documents: PathBuf,
    backup: PathBuf,
    artifacts: PathBuf,
    backed_up: Vec<&'static str>,
    backup_complete: bool,
    files_restored: bool,
    credential_deleted: bool,
    restored: bool,
}

impl SimulatorStateGuard {
    pub fn resume(
        device: String,
        host_id: String,
        documents: PathBuf,
        backup: PathBuf,
        artifacts: PathBuf,
    ) -> Self {
        assert!(backup.is_dir(), "interrupted native backup must exist");
        let manifest = match std::fs::read(backup.join(PRIOR_FILES_MANIFEST)) {
            Ok(bytes) => {
                let names: Vec<String> = serde_json::from_slice(&bytes)
                    .expect("interrupted native manifest must contain original file names");
                assert!(
                    names.iter().all(|name| LIVE_FILES.contains(&name.as_str())),
                    "interrupted native manifest contains an unknown file"
                );
                Some(names)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => panic!("interrupted native manifest unavailable: {error}"),
        };
        let backed_up = LIVE_FILES
            .into_iter()
            .filter(|name| {
                manifest.as_ref().map_or_else(
                    || backup.join(name).is_file(),
                    |names| names.iter().any(|original| original == name),
                )
            })
            .collect();
        Self {
            device: Some(device),
            host_id: Some(host_id),
            documents,
            backup,
            artifacts,
            backed_up,
            backup_complete: manifest.is_some(),
            files_restored: false,
            credential_deleted: false,
            restored: false,
        }
    }

    pub fn new(
        device: Option<String>,
        host_id: Option<String>,
        documents: PathBuf,
        backup: PathBuf,
        artifacts: PathBuf,
    ) -> Self {
        std::fs::create_dir(&backup).expect("prior Simulator backup must not be overwritten");
        Self {
            device,
            host_id,
            documents,
            backup,
            artifacts,
            backed_up: Vec::new(),
            backup_complete: false,
            files_restored: false,
            credential_deleted: false,
            restored: false,
        }
    }

    pub fn backup_files(&mut self) -> std::io::Result<()> {
        let originals: Vec<_> = LIVE_FILES
            .into_iter()
            .filter(|name| self.documents.join(name).exists())
            .collect();
        let mut manifest = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.backup.join(PRIOR_FILES_MANIFEST))?;
        serde_json::to_writer(&mut manifest, &originals)?;
        manifest.sync_all()?;
        std::fs::File::open(&self.backup)?.sync_all()?;
        self.backed_up = originals;
        for name in &self.backed_up {
            let source = self.documents.join(name);
            std::fs::rename(source, self.backup.join(name))?;
        }
        self.backup_complete = true;
        Ok(())
    }

    pub fn refresh_documents(&mut self) -> std::io::Result<PathBuf> {
        if let Some(device) = &self.device {
            let output = Command::new("xcrun")
                .args([
                    "simctl",
                    "get_app_container",
                    device,
                    "local.mote.simulator",
                    "data",
                ])
                .output()?;
            if !output.status.success() {
                return Err(std::io::Error::other(
                    "current Simulator container unavailable",
                ));
            }
            let container = String::from_utf8(output.stdout).map_err(std::io::Error::other)?;
            let documents = PathBuf::from(container.trim()).join("Documents");
            if !documents.is_absolute() || !documents.is_dir() {
                return Err(std::io::Error::other(
                    "current Simulator Documents unavailable",
                ));
            }
            let evidence = self.artifacts.join("container-paths.json");
            if documents != self.documents || !evidence.exists() {
                let row = serde_json::json!({"device":device,"bundle_id":"local.mote.simulator","original_documents":self.documents,"current_documents":documents,"backup":self.backup,"observed_at_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis()});
                std::fs::write(evidence, serde_json::to_vec_pretty(&row)?)?;
            }
            if documents != self.documents {
                self.files_restored = false;
                self.documents = documents;
            }
        }
        Ok(self.documents.clone())
    }

    pub fn finish(&mut self) -> std::io::Result<()> {
        if self.restored {
            return Ok(());
        }
        let refresh_result = self.refresh_documents();
        let credential_was_deleted = self.credential_deleted;
        let credential_result = if credential_was_deleted {
            Ok(())
        } else {
            self.delete_temporary_credential()
        };
        self.credential_deleted |= credential_result.is_ok();
        let restore_result = if self.files_restored && credential_was_deleted {
            Ok(())
        } else {
            self.restore_files()
        };
        self.files_restored |= restore_result.is_ok() && refresh_result.is_ok();
        if self.files_restored && self.credential_deleted {
            for name in &self.backed_up {
                let source = self.backup.join(name);
                if source.exists() {
                    std::fs::remove_file(source)?;
                }
            }
            let manifest = self.backup.join(PRIOR_FILES_MANIFEST);
            if manifest.exists() {
                std::fs::remove_file(manifest)?;
            }
            std::fs::remove_dir(&self.backup)?;
            self.restored = true;
        }
        restore_result
            .and(credential_result)
            .and(refresh_result.map(|_| ()))
    }

    fn delete_temporary_credential(&self) -> std::io::Result<()> {
        let (Some(device), Some(host_id)) = (&self.device, &self.host_id) else {
            return Ok(());
        };
        let _ = Command::new("xcrun")
            .args(["simctl", "terminate", device, "local.mote.simulator"])
            .output();
        let result = (|| {
            let launched = Command::new("xcrun")
                .args(["simctl", "launch", device, "local.mote.simulator"])
                .env("SIMCTL_CHILD_MOTE_SIMULATOR_KEYCHAIN_DELETE", host_id)
                .output()?;
            if !launched.status.success() {
                return Err(std::io::Error::other(
                    "temporary credential deletion launch failed",
                ));
            }
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                let deleted = std::fs::File::open(self.documents.join("live-network.json"))
                    .ok()
                    .and_then(|file| {
                        serde_json::from_reader::<_, Vec<serde_json::Value>>(file).ok()
                    })
                    .and_then(|rows| {
                        rows.into_iter().find(|row| {
                            row["kind"] == "keychain_deleted"
                                && row["host_id"] == host_id.as_str()
                                && row["missing"] == true
                        })
                    });
                if let Some(deleted) = deleted {
                    std::fs::write(
                        self.artifacts.join("credential-deletion.json"),
                        serde_json::to_vec_pretty(&deleted)?,
                    )?;
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(std::io::Error::other(
                        "temporary credential deletion timeout",
                    ));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        })();
        let _ = Command::new("xcrun")
            .args(["simctl", "terminate", device, "local.mote.simulator"])
            .output();
        result
    }

    fn restore_files(&self) -> std::io::Result<()> {
        for name in LIVE_FILES {
            let destination = self.documents.join(name);
            let source = self.backup.join(name);
            if self.backed_up.contains(&name) && source.exists() {
                if destination.exists() {
                    std::fs::remove_file(&destination)?;
                }
                std::fs::copy(source, destination)?;
            } else if self.backed_up.contains(&name) {
                if !destination.exists() {
                    return Err(std::io::Error::other("prior Simulator file is missing"));
                }
            } else if self.backup_complete && destination.exists() {
                std::fs::remove_file(destination)?;
            }
        }
        Ok(())
    }
}

impl Drop for SimulatorStateGuard {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!(
                "Simulator cleanup failed; preserved backup at {}: {error}",
                self.backup.display()
            );
        }
    }
}

#[test]
fn native_state_guard_resumes_only_preserved_files_into_the_current_container() {
    let root = tempfile::tempdir().unwrap();
    let documents = root.path().join("current-Documents");
    let artifacts = root.path().join("artifacts");
    let backup = artifacts.join("prior-live-state");
    std::fs::create_dir(&documents).unwrap();
    std::fs::create_dir(&artifacts).unwrap();
    std::fs::create_dir(&backup).unwrap();
    std::fs::write(backup.join("live-state.json"), "preserved prior state").unwrap();
    std::fs::write(backup.join(PRIOR_FILES_MANIFEST), r#"["live-state.json"]"#).unwrap();
    std::fs::write(documents.join("live-state.json"), "temporary test state").unwrap();
    std::fs::write(documents.join("live-network.json"), "test network").unwrap();
    std::fs::write(documents.join("unrelated.txt"), "keep").unwrap();
    let mut guard = SimulatorStateGuard::resume(
        "not-a-simulator-device".into(),
        "temporary-host".into(),
        documents.clone(),
        backup.clone(),
        artifacts,
    );
    guard.device = None;
    guard.host_id = None;
    guard.finish().unwrap();
    assert_eq!(
        std::fs::read_to_string(documents.join("live-state.json")).unwrap(),
        "preserved prior state"
    );
    assert_eq!(
        std::fs::read_to_string(documents.join("unrelated.txt")).unwrap(),
        "keep"
    );
    assert!(!documents.join("live-network.json").exists());
    assert!(!backup.exists());
}

#[test]
fn native_state_guard_retries_credentials_without_erasing_restored_files() {
    let root = tempfile::tempdir().unwrap();
    let documents = root.path().join("Documents");
    let artifacts = root.path().join("artifacts");
    std::fs::create_dir(&documents).unwrap();
    std::fs::create_dir(&artifacts).unwrap();
    std::fs::write(documents.join("live-state.json"), "prior").unwrap();
    let backup = artifacts.join("prior-live-state");
    let mut guard = SimulatorStateGuard::new(
        Some("not-a-simulator-device".into()),
        Some("temporary-test-host".into()),
        documents.clone(),
        backup.clone(),
        artifacts,
    );
    guard.backup_files().unwrap();
    std::fs::write(documents.join("live-state.json"), "fixture").unwrap();
    assert!(guard.finish().is_err());
    assert!(!guard.restored, "credential failure must remain retryable");
    assert_eq!(
        std::fs::read_to_string(documents.join("live-state.json")).unwrap(),
        "prior"
    );
    guard.device = None;
    guard.host_id = None;
    std::fs::write(documents.join("live-network.json"), "cleanup retry output").unwrap();
    guard.finish().unwrap();
    assert!(guard.restored);
    assert_eq!(
        std::fs::read_to_string(documents.join("live-state.json")).unwrap(),
        "prior"
    );
    assert!(!backup.exists());
    assert!(!documents.join("live-network.json").exists());
}

#[test]
fn native_state_guard_restores_prior_files_after_failure() {
    let root = tempfile::tempdir().unwrap();
    let documents = root.path().join("Documents");
    let artifacts = root.path().join("artifacts");
    std::fs::create_dir(&documents).unwrap();
    std::fs::create_dir(&artifacts).unwrap();
    std::fs::write(documents.join("live-state.json"), "prior").unwrap();
    let result = std::panic::catch_unwind(|| {
        let mut guard = SimulatorStateGuard::new(
            None,
            None,
            documents.clone(),
            artifacts.join("prior-live-state"),
            artifacts.clone(),
        );
        guard.backup_files().unwrap();
        std::fs::write(documents.join("live-state.json"), "fixture").unwrap();
        std::fs::write(documents.join("live-network.json"), "fixture").unwrap();
        panic!("test failure after Simulator state displacement");
    });
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(documents.join("live-state.json")).unwrap(),
        "prior"
    );
    assert!(!documents.join("live-network.json").exists());
    assert!(!artifacts.join("prior-live-state").exists());
}

#[test]
fn native_state_guard_keeps_already_restored_files_when_a_later_restore_retries() {
    let root = tempfile::tempdir().unwrap();
    let documents = root.path().join("Documents");
    let artifacts = root.path().join("artifacts");
    std::fs::create_dir(&documents).unwrap();
    std::fs::create_dir(&artifacts).unwrap();
    std::fs::write(documents.join("live-state.json"), "prior state").unwrap();
    std::fs::write(documents.join("live-hosts.json"), "prior hosts").unwrap();
    let backup = artifacts.join("prior-live-state");
    let mut guard =
        SimulatorStateGuard::new(None, None, documents.clone(), backup.clone(), artifacts);
    guard.backup_files().unwrap();
    std::fs::write(documents.join("live-state.json"), "temporary state").unwrap();
    std::fs::create_dir(documents.join("live-hosts.json")).unwrap();
    assert!(guard.finish().is_err());
    assert!(!guard.restored);
    assert_eq!(
        std::fs::read_to_string(documents.join("live-state.json")).unwrap(),
        "prior state"
    );
    assert_eq!(
        std::fs::read_to_string(backup.join("live-state.json")).unwrap(),
        "prior state"
    );
    assert_eq!(
        std::fs::read_to_string(backup.join("live-hosts.json")).unwrap(),
        "prior hosts"
    );
    std::fs::remove_dir(documents.join("live-hosts.json")).unwrap();
    guard.finish().unwrap();
    assert_eq!(
        std::fs::read_to_string(documents.join("live-state.json")).unwrap(),
        "prior state"
    );
    assert_eq!(
        std::fs::read_to_string(documents.join("live-hosts.json")).unwrap(),
        "prior hosts"
    );
    assert!(!backup.exists());
}

#[test]
fn native_state_guard_resumes_after_partial_restoration_without_losing_prior_files() {
    let root = tempfile::tempdir().unwrap();
    let documents = root.path().join("Documents");
    let artifacts = root.path().join("artifacts");
    std::fs::create_dir(&documents).unwrap();
    std::fs::create_dir(&artifacts).unwrap();
    std::fs::write(documents.join("live-state.json"), "original state").unwrap();
    std::fs::write(documents.join("live-hosts.json"), "original hosts").unwrap();
    let backup = artifacts.join("prior-live-state");
    {
        let mut guard = SimulatorStateGuard::new(
            None,
            None,
            documents.clone(),
            backup.clone(),
            artifacts.clone(),
        );
        guard.backup_files().unwrap();
        std::fs::create_dir(documents.join("live-hosts.json")).unwrap();
        assert!(guard.finish().is_err());
    }
    assert_eq!(
        std::fs::read_to_string(documents.join("live-state.json")).unwrap(),
        "original state"
    );
    std::fs::remove_dir(documents.join("live-hosts.json")).unwrap();
    let mut guard = SimulatorStateGuard::resume(
        "not-a-simulator-device".into(),
        "temporary-host".into(),
        documents.clone(),
        backup.clone(),
        artifacts,
    );
    guard.device = None;
    guard.host_id = None;
    guard.finish().unwrap();
    assert_eq!(
        std::fs::read_to_string(documents.join("live-state.json")).unwrap(),
        "original state"
    );
    assert_eq!(
        std::fs::read_to_string(documents.join("live-hosts.json")).unwrap(),
        "original hosts"
    );
    assert!(!backup.exists());
}

#[test]
fn native_state_guard_preserves_unmoved_originals_when_resuming_partial_backup() {
    for with_manifest in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let documents = root.path().join("Documents");
        let artifacts = root.path().join("artifacts");
        let backup = artifacts.join("prior-live-state");
        std::fs::create_dir(&documents).unwrap();
        std::fs::create_dir(&artifacts).unwrap();
        std::fs::create_dir(&backup).unwrap();
        std::fs::write(documents.join("live-state.json"), "original state").unwrap();
        std::fs::write(documents.join("live-hosts.json"), "unmoved original hosts").unwrap();
        std::fs::rename(
            documents.join("live-state.json"),
            backup.join("live-state.json"),
        )
        .unwrap();
        if with_manifest {
            std::fs::write(
                backup.join("prior-live-files.json"),
                r#"["live-state.json","live-hosts.json"]"#,
            )
            .unwrap();
        }
        let mut guard = SimulatorStateGuard::resume(
            "not-a-simulator-device".into(),
            "temporary-host".into(),
            documents.clone(),
            backup.clone(),
            artifacts,
        );
        guard.device = None;
        guard.host_id = None;
        let result = guard.finish();
        assert_eq!(
            std::fs::read_to_string(documents.join("live-hosts.json")).unwrap(),
            "unmoved original hosts"
        );
        result.unwrap();
        assert_eq!(
            std::fs::read_to_string(documents.join("live-state.json")).unwrap(),
            "original state"
        );
        assert!(!backup.exists());
    }
}

#[test]
fn native_state_guard_rejects_invalid_resume_manifest_without_changing_files() {
    for manifest in ["not json", r#"["../live-hosts.json"]"#] {
        let root = tempfile::tempdir().unwrap();
        let documents = root.path().join("Documents");
        let artifacts = root.path().join("artifacts");
        let backup = artifacts.join("prior-live-state");
        std::fs::create_dir(&documents).unwrap();
        std::fs::create_dir(&artifacts).unwrap();
        std::fs::create_dir(&backup).unwrap();
        std::fs::write(documents.join("live-hosts.json"), "keep original hosts").unwrap();
        std::fs::write(backup.join("prior-live-files.json"), manifest).unwrap();
        let result = std::panic::catch_unwind(|| {
            SimulatorStateGuard::resume(
                "not-a-simulator-device".into(),
                "temporary-host".into(),
                documents.clone(),
                backup.clone(),
                artifacts,
            )
        });
        assert!(result.is_err(), "invalid cleanup authority must be refused");
        assert_eq!(
            std::fs::read_to_string(documents.join("live-hosts.json")).unwrap(),
            "keep original hosts"
        );
        assert_eq!(
            std::fs::read_to_string(backup.join("prior-live-files.json")).unwrap(),
            manifest
        );
    }
}

#[test]
fn native_state_guard_preserves_backup_when_restore_fails() {
    let root = tempfile::tempdir().unwrap();
    let documents = root.path().join("Documents");
    let artifacts = root.path().join("artifacts");
    std::fs::create_dir(&documents).unwrap();
    std::fs::create_dir(&artifacts).unwrap();
    std::fs::write(documents.join("live-state.json"), "prior").unwrap();
    let backup = artifacts.join("prior-live-state");
    {
        let mut guard =
            SimulatorStateGuard::new(None, None, documents.clone(), backup.clone(), artifacts);
        guard.backup_files().unwrap();
        std::fs::create_dir(documents.join("live-state.json")).unwrap();
        assert!(guard.finish().is_err());
    }
    assert_eq!(
        std::fs::read_to_string(backup.join("live-state.json")).unwrap(),
        "prior"
    );
}

pub fn command(program: &str, args: &[&str]) -> String {
    let output = Command::new(program).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
