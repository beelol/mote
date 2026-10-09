//! Charter: `house.md` and `characters/<name>.md`. Each file starts with a TOML front matter
//! block (delimited by `+++`) the daemon enforces, followed by prose that becomes the first
//! pack part. Files are watched; a change emits a `charter.changed` event. See docs/rfcs/v0.md
//! section 11. The model has no tool that writes these files.

use mote_eventlog::EventLog;
use notify::{RecursiveMode, Watcher};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The default tool allowlist for v0 characters.
pub const DEFAULT_TOOLS: [&str; 4] = ["memory_search", "memory_write", "file_read", "file_list"];

/// The scope under which charter events are stored.
pub const CHARTER_SCOPE: &str = "charter";

/// The enforced front matter block.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FrontMatter {
    pub name: String,
    pub tools: Vec<String>,
    pub realms: Vec<String>,
}

/// A parsed charter file.
#[derive(Debug, Clone, PartialEq)]
pub struct Charter {
    pub front: FrontMatter,
    pub prose: String,
}

impl Charter {
    /// Whether a tool is allowed by this charter.
    pub fn allows_tool(&self, tool: &str) -> bool {
        self.front.tools.iter().any(|t| t == tool)
    }
}

#[derive(Debug)]
pub enum CharterError {
    MissingFrontMatter,
    Toml(String),
    Io(String),
}

impl std::fmt::Display for CharterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CharterError::MissingFrontMatter => write!(f, "charter is missing its +++ front matter"),
            CharterError::Toml(e) => write!(f, "charter front matter is invalid: {e}"),
            CharterError::Io(e) => write!(f, "charter io error: {e}"),
        }
    }
}

/// Parse a charter file's text into front matter and prose.
pub fn parse(text: &str) -> Result<Charter, CharterError> {
    let rest = text.strip_prefix("+++").ok_or(CharterError::MissingFrontMatter)?;
    let end = rest.find("+++").ok_or(CharterError::MissingFrontMatter)?;
    let toml_src = &rest[..end];
    let prose = rest[end + 3..].trim().to_string();
    let front: FrontMatter =
        toml::from_str(toml_src.trim()).map_err(|e| CharterError::Toml(e.to_string()))?;
    Ok(Charter { front, prose })
}

fn default_file(name: &str, prose: &str) -> String {
    let tools = DEFAULT_TOOLS
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "+++\nname = \"{name}\"\ntools = [{tools}]\nrealms = [\"personal\"]\n+++\n\n{prose}\n"
    )
}

/// Paths of the two charter files in a data directory.
pub fn house_path(data_dir: &Path) -> PathBuf {
    data_dir.join("house.md")
}
pub fn character_path(data_dir: &Path, name: &str) -> PathBuf {
    data_dir.join("characters").join(format!("{name}.md"))
}

/// Create `house.md` and `characters/mote.md` with the enforced front matter if they are absent.
pub fn create_defaults(data_dir: &Path) -> Result<(), CharterError> {
    let house = house_path(data_dir);
    if !house.exists() {
        std::fs::write(&house, default_file("house", "House rules for all characters."))
            .map_err(|e| CharterError::Io(e.to_string()))?;
    }
    let chars = data_dir.join("characters");
    std::fs::create_dir_all(&chars).map_err(|e| CharterError::Io(e.to_string()))?;
    let mote = character_path(data_dir, "mote");
    if !mote.exists() {
        std::fs::write(&mote, default_file("mote", "I am mote, an always-on personal agent."))
            .map_err(|e| CharterError::Io(e.to_string()))?;
    }
    Ok(())
}

/// Load and enforce a charter file.
pub fn load(path: &Path) -> Result<Charter, CharterError> {
    let text = std::fs::read_to_string(path).map_err(|e| CharterError::Io(e.to_string()))?;
    parse(&text)
}

/// Check a tool call against a character's charter. On refusal, logs a `tool.refused` event and
/// returns an error.
pub fn check_tool(charter: &Charter, tool: &str, log: &EventLog) -> Result<(), String> {
    if charter.allows_tool(tool) {
        return Ok(());
    }
    let reason = format!("tool {tool} is not in character {}'s allowlist", charter.front.name);
    let _ = log.append(
        CHARTER_SCOPE,
        "tool.refused",
        &json!({ "tool": tool, "character": charter.front.name, "reason": reason }),
    );
    Err(reason)
}

/// Watches the character file, re-parses on change, updates the shared charter and emits a
/// `charter.changed` event. The returned watcher must be kept alive.
pub struct CharterWatcher {
    _watcher: notify::RecommendedWatcher,
    pub current: Arc<Mutex<Charter>>,
}

impl CharterWatcher {
    pub fn start(path: &Path, log: EventLog) -> Result<CharterWatcher, CharterError> {
        let charter = load(path)?;
        let current = Arc::new(Mutex::new(charter));
        let shared = current.clone();
        let watch_path = path.to_path_buf();

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                if matches!(event.kind, notify::EventKind::Modify(_) | notify::EventKind::Create(_)) {
                    if let Ok(new) = load(&watch_path) {
                        let name = new.front.name.clone();
                        let mut guard = shared.lock().unwrap_or_else(|p| p.into_inner());
                        *guard = new.clone();
                        drop(guard);
                        let _ = log.append(
                            CHARTER_SCOPE,
                            "charter.changed",
                            &json!({ "character": name, "prose": new.prose }),
                        );
                    }
                }
            }
        })
        .map_err(|e| CharterError::Io(e.to_string()))?;

        watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(|e| CharterError::Io(e.to_string()))?;
        Ok(CharterWatcher { _watcher: watcher, current })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mote-charter-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn first_start_creates_files_with_front_matter() {
        let dir = temp_dir("create");
        create_defaults(&dir).unwrap();

        let house = load(&house_path(&dir)).unwrap();
        assert_eq!(house.front.name, "house");
        assert_eq!(house.front.realms, vec!["personal"]);

        let mote = load(&character_path(&dir, "mote")).unwrap();
        assert_eq!(mote.front.name, "mote");
        assert_eq!(mote.front.tools, DEFAULT_TOOLS.to_vec());
        assert_eq!(mote.front.realms, vec!["personal"]);
        assert!(!mote.prose.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unlisted_tool_is_refused_and_logged() {
        let dir = temp_dir("tool");
        create_defaults(&dir).unwrap();
        let charter = load(&character_path(&dir, "mote")).unwrap();
        let log = EventLog::open_in_memory().unwrap();

        assert!(check_tool(&charter, "memory_search", &log).is_ok());
        let err = check_tool(&charter, "shell_exec", &log).unwrap_err();
        assert!(err.contains("shell_exec"));

        // The refusal was logged.
        let sub = log.subscribe(CHARTER_SCOPE, 0).unwrap();
        assert!(sub.replay_remaining() >= 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn editing_charter_emits_changed_and_updates_prose() {
        let dir = temp_dir("watch");
        create_defaults(&dir).unwrap();
        let path = character_path(&dir, "mote");
        let log = EventLog::open_in_memory().unwrap();
        let mut sub = log.subscribe(CHARTER_SCOPE, 0).unwrap();

        let watcher = CharterWatcher::start(&path, log.clone()).unwrap();

        // Edit the file.
        std::fs::write(&path, default_file("mote", "New prose after editing.")).unwrap();

        // Wait for the charter.changed event.
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut changed = false;
        while std::time::Instant::now() < deadline {
            if let Some(ev) = sub.recv_timeout(Duration::from_millis(200)) {
                if ev.kind == "charter.changed" {
                    changed = true;
                    break;
                }
            }
        }
        assert!(changed, "editing the charter must emit charter.changed");
        assert_eq!(watcher.current.lock().unwrap().prose, "New prose after editing.");

        std::fs::remove_dir_all(&dir).ok();
    }
}
