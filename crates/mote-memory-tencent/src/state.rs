use mote_memory::MemoryError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Record {
    pub account_id: String,
    pub realm: String,
    pub session_id: String,
    pub candidate_id: String,
    pub text: String,
    pub version: u64,
    pub forgotten: bool,
    #[serde(default)]
    pub unresolved_addition: bool,
    #[serde(default)]
    pub pending_deletions: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct State {
    pub next_id: u64,
    pub records: BTreeMap<String, Record>,
}

pub(crate) struct StateStore {
    path: PathBuf,
    pub state: State,
}

impl StateStore {
    pub(crate) fn open(path: PathBuf) -> Result<Self, MemoryError> {
        let state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| MemoryError::backend(format!("Tencent state JSON: {error}")))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(error) => {
                return Err(MemoryError::backend(format!(
                    "read Tencent state {}: {error}",
                    path.display()
                )))
            }
        };
        Ok(Self { path, state })
    }

    pub(crate) fn persist(&self) -> Result<(), MemoryError> {
        let parent = self.path.parent().ok_or_else(|| {
            MemoryError::malformed("Tencent state path must have a parent directory")
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            MemoryError::backend(format!("create Tencent state directory: {error}"))
        })?;
        let bytes = serde_json::to_vec(&self.state)
            .map_err(|error| MemoryError::backend(format!("encode Tencent state: {error}")))?;
        let temporary = temporary_path(&self.path);
        fs::write(&temporary, bytes)
            .map_err(|error| MemoryError::backend(format!("write Tencent state: {error}")))?;
        fs::rename(&temporary, &self.path)
            .map_err(|error| MemoryError::backend(format!("replace Tencent state: {error}")))
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(".tmp");
    PathBuf::from(temporary)
}
