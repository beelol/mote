//! The production model gate: ask Ollama for a model's file size and enforce the memory gate
//! against real free memory. See docs/rfcs/v0.md sections 0 and 4.

use crate::config::ModelGate;
use mote_localmodel::{available_memory_bytes, memory_gate, Ollama};

/// A gate backed by a local Ollama server.
pub struct OllamaGate {
    client: Ollama,
}

impl OllamaGate {
    pub fn new(client: Ollama) -> Self {
        Self { client }
    }
}

impl ModelGate for OllamaGate {
    fn check(&self, model: &str) -> Result<(), String> {
        let models = self
            .client
            .list_models()
            .map_err(|e| format!("cannot reach Ollama to size {model}: {e}"))?;
        match models.iter().find(|(name, _)| name == model) {
            Some((_, size)) => memory_gate(available_memory_bytes(), *size),
            // Model existence is enforced elsewhere; the gate only judges size.
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigStore, SetError};
    use mote_eventlog::EventLog;
    use mote_testkit::ScriptedServer;
    use std::sync::Arc;

    #[test]
    fn oversized_model_is_refused_naming_memory_and_size() {
        // The scripted server reports a 500 GB model; 1.25x exceeds any real free memory.
        let srv = ScriptedServer::start().unwrap();
        srv.set_models(vec![("huge:500g".to_string(), 500_000_000_000)]);

        let dir = std::env::temp_dir().join(format!("mote-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let gate = Arc::new(OllamaGate::new(Ollama::new(srv.base_url())));
        let mut cs = ConfigStore::load(
            &dir.join("config.toml"),
            EventLog::open_in_memory().unwrap(),
            gate,
        )
        .unwrap();

        let res = cs.set("models.think", toml::Value::String("huge:500g".into()));
        match res {
            Err(SetError::Rejected(reason)) => {
                assert!(reason.contains("500"), "names model size: {reason}");
                assert!(reason.contains("free"), "names free memory: {reason}");
            }
            other => panic!("expected rejection, got {other:?}"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
