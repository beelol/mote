//! The production embedder: model2vec-rs with potion-base-8M, quantized to int8[256]. See
//! docs/rfcs/v0.md section 10. Loading downloads the ~30 MB model once and caches it. If it
//! cannot load, the daemon falls back to the deterministic local embedder.

use mote_context::mem::{quantize, Embedder};
use std::sync::Mutex;

/// The pinned embedding model.
pub const MODEL: &str = "minishlab/potion-base-8M";

/// An embedder backed by model2vec-rs potion-base-8M.
pub struct PotionEmbedder {
    model: Mutex<model2vec_rs::model::StaticModel>,
}

impl PotionEmbedder {
    /// Load potion-base-8M (downloading and caching on first use).
    pub fn try_new() -> Result<Self, String> {
        let model = model2vec_rs::model::StaticModel::from_pretrained(MODEL, None, None, None)
            .map_err(|e| format!("load {MODEL}: {e}"))?;
        Ok(Self { model: Mutex::new(model) })
    }
}

impl Embedder for PotionEmbedder {
    fn embed(&self, text: &str) -> Vec<i8> {
        let out = self.model.lock().unwrap().encode(&[text.to_string()]);
        match out.into_iter().next() {
            Some(v) => quantize(&v),
            None => vec![0i8; mote_context::mem::EMBED_DIM],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Live: verify the contract's embedder loads and returns int8[256]. Skipped unless MOTE_LIVE.
    #[test]
    fn live_potion_embedder_dim() {
        if std::env::var("MOTE_LIVE").is_err() {
            return;
        }
        let e = PotionEmbedder::try_new().expect("load potion-base-8M");
        let v = e.embed("the sky is blue");
        assert_eq!(v.len(), mote_context::mem::EMBED_DIM);
    }
}
