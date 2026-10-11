use mote_localmodel::Ollama;
use serde::Deserialize;
use std::path::{Path, PathBuf};

pub(crate) const COMPILER_CONTRACT_VERSION: u64 = 1;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SurfaceAttestation {
    model: String,
    model_digest: String,
    corpus_hash: String,
    compiler_contract_version: u64,
    first_try_valid: u64,
    sample_count: u64,
}

pub(crate) struct SurfaceActivationStore {
    path: PathBuf,
}

impl SurfaceActivationStore {
    pub(crate) fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("surface-activation.json"),
        }
    }

    pub(crate) fn allows(&self, client: &Ollama) -> bool {
        self.allows_for_corpus(client, &super::bench::current_surface_corpus_hash())
    }

    fn allows_for_corpus(&self, client: &Ollama, corpus_hash: &str) -> bool {
        let Ok(bytes) = std::fs::read(&self.path) else {
            return false;
        };
        let Ok(attestation) = serde_json::from_slice::<SurfaceAttestation>(&bytes) else {
            return false;
        };
        if attestation.model != super::inferred_ui::SURFACE_MODEL
            || attestation.model_digest.is_empty()
            || attestation.corpus_hash != corpus_hash
            || attestation.compiler_contract_version != COMPILER_CONTRACT_VERSION
            || attestation.sample_count != 200
            || !(190..=200).contains(&attestation.first_try_valid)
        {
            return false;
        }
        matches!(client.model_digest(&attestation.model), Ok(Some(digest)) if digest == attestation.model_digest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mote_testkit::ScriptedServer;
    use serde_json::json;

    fn data_dir(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "mote-surface-gate-{label}-{}-{}",
            std::process::id(),
            crate::now_ms()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn attestation() -> serde_json::Value {
        json!({
            "model": "qwen3:4b",
            "model_digest": "sha256:good",
            "corpus_hash": crate::bench::current_surface_corpus_hash(),
            "compiler_contract_version": COMPILER_CONTRACT_VERSION,
            "first_try_valid": 190,
            "sample_count": 200
        })
    }

    #[test]
    fn inferred_activation_is_disabled_without_a_valid_current_attestation() {
        let dir = data_dir("missing");
        let server = ScriptedServer::start().unwrap();
        server.set_models(vec![("qwen3:4b".into(), 1)]);
        server.set_model_digest("qwen3:4b", "sha256:good");
        let client = Ollama::new(server.base_url());
        assert!(!SurfaceActivationStore::new(&dir).allows(&client));
        std::fs::write(dir.join("surface-activation.json"), "not json").unwrap();
        assert!(!SurfaceActivationStore::new(&dir).allows(&client));
    }

    #[test]
    fn inferred_activation_accepts_only_the_exact_live_identity_and_passing_score() {
        let dir = data_dir("exact");
        let server = ScriptedServer::start().unwrap();
        server.set_models(vec![("qwen3:4b".into(), 1)]);
        server.set_model_digest("qwen3:4b", "sha256:good");
        let client = Ollama::new(server.base_url());
        for (field, value) in [
            ("model", json!("qwen3:0.6b")),
            ("model_digest", json!("sha256:old")),
            ("corpus_hash", json!("stale")),
            ("compiler_contract_version", json!(0)),
            ("first_try_valid", json!(189)),
            ("sample_count", json!(199)),
        ] {
            let mut stale = attestation();
            stale[field] = value;
            std::fs::write(dir.join("surface-activation.json"), stale.to_string()).unwrap();
            assert!(
                !SurfaceActivationStore::new(&dir).allows(&client),
                "accepted stale {field}"
            );
        }
        std::fs::write(
            dir.join("surface-activation.json"),
            attestation().to_string(),
        )
        .unwrap();
        assert!(SurfaceActivationStore::new(&dir).allows(&client));
        assert!(
            !SurfaceActivationStore::new(&dir).allows_for_corpus(&client, "changed-corpus-hash")
        );
        server.set_model_digest("qwen3:4b", "sha256:changed");
        assert!(!SurfaceActivationStore::new(&dir).allows(&client));
    }
}
