//! Daemon configuration: defaults, runtime `config.set`, `config.changed` events and
//! `config.revert`. See docs/rfcs/v0.md section 4.

use mote_eventlog::EventLog;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

/// The exact default config written on first start. Kept byte-for-byte equal to the block in
/// docs/rfcs/v0.md section 4 so the file can be diffed against the contract.
pub const DEFAULT_CONFIG_TOML: &str = r#"[models]
local_only = true
talk = "qwen3:4b"
router = "qwen3:0.6b"
think = "qwen3-coder:30b-64k"

[budgets]            # milliseconds, p50
talk_first_token = 300
router = 150
think_first_token = 3000

[packs]              # tokens
talk = 600
think = 8000
"#;

/// The scope under which config events are stored.
pub const CONFIG_SCOPE: &str = "config";

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Ty {
    Bool,
    Str,
    Int,
}

/// Known keys and their types. An unknown key is rejected.
fn schema(key: &str) -> Option<Ty> {
    match key {
        "models.local_only" => Some(Ty::Bool),
        "models.talk" | "models.router" | "models.think" | "agent.harness" => Some(Ty::Str),
        "budgets.talk_first_token" | "budgets.router" | "budgets.think_first_token" => {
            Some(Ty::Int)
        }
        "packs.talk" | "packs.think" => Some(Ty::Int),
        _ => None,
    }
}

/// A gate that decides whether a model named in config can load. V0-14 backs this with the
/// memory gate; by default every model is allowed.
pub trait ModelGate: Send + Sync {
    /// Ok to use the model, or Err with a reason naming free memory and model size.
    fn check(&self, model: &str) -> Result<(), String>;
}

/// Allows every model. A test gate.
#[cfg(test)]
pub struct AllowAll;
#[cfg(test)]
impl ModelGate for AllowAll {
    fn check(&self, _model: &str) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum SetError {
    UnknownKey(String),
    WrongType { key: String, want: &'static str },
    Rejected(String),
    Io(String),
}

impl std::fmt::Display for SetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SetError::UnknownKey(k) => write!(f, "unknown config key: {k}"),
            SetError::WrongType { key, want } => write!(f, "config key {key} wants {want}"),
            SetError::Rejected(r) => write!(f, "{r}"),
            SetError::Io(e) => write!(f, "config io error: {e}"),
        }
    }
}

/// In-memory config backed by a TOML file and an event log.
pub struct ConfigStore {
    value: toml::Value,
    path: std::path::PathBuf,
    log: EventLog,
    gate: Arc<dyn ModelGate>,
}

impl ConfigStore {
    /// Load the config, writing the exact defaults if the file does not exist.
    pub fn load(path: &Path, log: EventLog, gate: Arc<dyn ModelGate>) -> Result<Self, SetError> {
        if !path.exists() {
            std::fs::write(path, DEFAULT_CONFIG_TOML).map_err(|e| SetError::Io(e.to_string()))?;
        }
        let text = std::fs::read_to_string(path).map_err(|e| SetError::Io(e.to_string()))?;
        let value: toml::Value = toml::from_str(&text)
            .map_err(|e| SetError::Io(format!("parse {}: {e}", path.display())))?;
        Ok(Self { value, path: path.to_path_buf(), log, gate })
    }

    /// Current value of a dotted key as a TOML value.
    pub fn get(&self, key: &str) -> Option<toml::Value> {
        let mut cur = &self.value;
        for part in key.split('.') {
            cur = cur.get(part)?;
        }
        Some(cur.clone())
    }

    fn set_dotted(&mut self, key: &str, new: toml::Value) {
        let parts: Vec<&str> = key.split('.').collect();
        let mut cur = &mut self.value;
        for part in &parts[..parts.len() - 1] {
            cur = cur
                .as_table_mut()
                .unwrap()
                .entry(part.to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
        }
        if let Some(t) = cur.as_table_mut() {
            t.insert(parts[parts.len() - 1].to_string(), new);
        }
    }

    fn remove_dotted(&mut self, key: &str) {
        let parts: Vec<&str> = key.split('.').collect();
        let mut cur = &mut self.value;
        for part in &parts[..parts.len() - 1] {
            let Some(next) = cur.get_mut(part) else {
                return;
            };
            cur = next;
        }
        if let Some(table) = cur.as_table_mut() {
            table.remove(parts[parts.len() - 1]);
        }
    }

    fn persist(&self) -> Result<(), SetError> {
        let text = toml::to_string(&self.value).map_err(|e| SetError::Io(e.to_string()))?;
        std::fs::write(&self.path, text).map_err(|e| SetError::Io(e.to_string()))
    }

    fn type_ok(ty: Ty, v: &toml::Value) -> bool {
        match ty {
            Ty::Bool => v.is_bool(),
            Ty::Str => v.is_str(),
            Ty::Int => v.is_integer(),
        }
    }

    /// Validate and apply a change, record a `config.changed` event, and return its seq.
    pub fn set(&mut self, key: &str, new: toml::Value) -> Result<i64, SetError> {
        let ty = schema(key).ok_or_else(|| SetError::UnknownKey(key.to_string()))?;
        if !Self::type_ok(ty, &new) {
            let want = match ty {
                Ty::Bool => "a boolean",
                Ty::Str => "a string",
                Ty::Int => "an integer",
            };
            return Err(SetError::WrongType { key: key.to_string(), want });
        }
        // A model key is gated by the memory gate.
        if matches!(key, "models.talk" | "models.router" | "models.think") {
            let model = new.as_str().unwrap();
            self.gate.check(model).map_err(SetError::Rejected)?;
        }
        if key == "agent.harness" && !matches!(new.as_str(), Some("opencode" | "mote")) {
            return Err(SetError::Rejected(
                "agent.harness must be opencode or mote".into(),
            ));
        }
        let old = self.get(key);
        self.set_dotted(key, new.clone());
        self.persist()?;
        let payload = json!({
            "key": key,
            "old": old.as_ref().map(toml_to_json).unwrap_or(serde_json::Value::Null),
            "new": toml_to_json(&new),
        });
        let ev = self
            .log
            .append(CONFIG_SCOPE, "config.changed", &payload)
            .map_err(|e| SetError::Io(e.to_string()))?;
        Ok(ev.seq)
    }

    /// Set from a JSON value (as it arrives over IPC).
    pub fn set_json(&mut self, key: &str, v: &serde_json::Value) -> Result<i64, SetError> {
        self.set(key, json_to_toml(v))
    }

    /// Current value of a key as JSON.
    pub fn get_json(&self, key: &str) -> Option<serde_json::Value> {
        self.get(key).map(|v| toml_to_json(&v))
    }

    /// Restore the value recorded as `old` in the `config.changed` event at `seq`.
    pub fn revert(&mut self, seq: i64) -> Result<i64, SetError> {
        let ev = self
            .log
            .get(seq)
            .map_err(|e| SetError::Io(e.to_string()))?
            .ok_or_else(|| SetError::Rejected(format!("no event at seq {seq}")))?;
        if ev.kind != "config.changed" {
            return Err(SetError::Rejected(format!("seq {seq} is not a config.changed event")));
        }
        let key = ev.payload["key"].as_str().unwrap_or("").to_string();
        if ev.payload["old"].is_null() {
            schema(&key).ok_or_else(|| SetError::UnknownKey(key.clone()))?;
            let current = self.get_json(&key).unwrap_or(serde_json::Value::Null);
            self.remove_dotted(&key);
            self.persist()?;
            return self
                .log
                .append(
                    CONFIG_SCOPE,
                    "config.changed",
                    &json!({"key":key,"old":current,"new":serde_json::Value::Null}),
                )
                .map(|event| event.seq)
                .map_err(|error| SetError::Io(error.to_string()));
        }
        let old = json_to_toml(&ev.payload["old"]);
        self.set(&key, old)
    }
}

fn toml_to_json(v: &toml::Value) -> serde_json::Value {
    match v {
        toml::Value::String(s) => json!(s),
        toml::Value::Integer(i) => json!(i),
        toml::Value::Boolean(b) => json!(b),
        toml::Value::Float(f) => json!(f),
        other => json!(other.to_string()),
    }
}

fn json_to_toml(v: &serde_json::Value) -> toml::Value {
    match v {
        serde_json::Value::String(s) => toml::Value::String(s.clone()),
        serde_json::Value::Bool(b) => toml::Value::Boolean(*b),
        serde_json::Value::Number(n) if n.is_i64() => toml::Value::Integer(n.as_i64().unwrap()),
        serde_json::Value::Number(n) => toml::Value::Float(n.as_f64().unwrap()),
        other => toml::Value::String(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &Path) -> ConfigStore {
        let log = EventLog::open_in_memory().unwrap();
        ConfigStore::load(&dir.join("config.toml"), log, Arc::new(AllowAll)).unwrap()
    }

    #[test]
    fn first_start_writes_defaults_matching_the_rfc_block() {
        let dir = std::env::temp_dir().join(format!("mote-cfg-def-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        let _ = store(&dir);
        let written = std::fs::read_to_string(&path).unwrap();

        // Extract the toml block under "## 4. Config" in the contract and diff against it.
        let rfc = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/rfcs/v0.md"),
        )
        .unwrap();
        let after = rfc.split("## 4. Config").nth(1).expect("section 4");
        let block = after
            .split("```toml")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .expect("toml block")
            .trim_start_matches('\n');
        assert_eq!(written, block, "written config must equal the RFC block");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn set_records_change_and_revert_restores() {
        let dir = std::env::temp_dir().join(format!("mote-cfg-set-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut cs = store(&dir);

        assert_eq!(cs.get_json("models.talk").unwrap(), "qwen3:4b");
        let seq = cs.set("models.talk", toml::Value::String("qwen3:0.6b".into())).unwrap();
        assert_eq!(cs.get_json("models.talk").unwrap(), "qwen3:0.6b");

        // The change event carries old and new.
        let ev = cs.log.get(seq).unwrap().unwrap();
        assert_eq!(ev.kind, "config.changed");
        assert_eq!(ev.payload["key"], "models.talk");
        assert_eq!(ev.payload["old"], "qwen3:4b");
        assert_eq!(ev.payload["new"], "qwen3:0.6b");

        // Revert restores the old value and records a second event.
        let rev = cs.revert(seq).unwrap();
        assert_eq!(cs.get_json("models.talk").unwrap(), "qwen3:4b");
        let ev2 = cs.log.get(rev).unwrap().unwrap();
        assert_eq!(ev2.payload["new"], "qwen3:4b");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_key_and_wrong_type_are_rejected() {
        let dir = std::env::temp_dir().join(format!("mote-cfg-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut cs = store(&dir);
        assert!(matches!(
            cs.set("models.nope", toml::Value::String("x".into())),
            Err(SetError::UnknownKey(_))
        ));
        assert!(matches!(
            cs.set("budgets.router", toml::Value::String("x".into())),
            Err(SetError::WrongType { .. })
        ));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn optional_harness_defaults_to_opencode_and_validates_explicit_values() {
        let dir = std::env::temp_dir().join(format!("mote-cfg-harness-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut cs = store(&dir);

        assert_eq!(cs.get_json("agent.harness"), None);
        assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), DEFAULT_CONFIG_TOML);
        cs.set("agent.harness", toml::Value::String("mote".into())).unwrap();
        assert_eq!(cs.get_json("agent.harness").unwrap(), "mote");
        assert!(matches!(
            cs.set("agent.harness", toml::Value::String("other".into())),
            Err(SetError::Rejected(_))
        ));
        assert_eq!(cs.get_json("agent.harness").unwrap(), "mote");

        let changed = cs
            .log
            .recent_kind_since("config.changed", i64::MIN, 1)
            .unwrap()
            .pop()
            .unwrap();
        cs.revert(changed.seq).unwrap();
        assert_eq!(cs.get_json("agent.harness"), None);

        std::fs::remove_dir_all(&dir).ok();
    }
}
