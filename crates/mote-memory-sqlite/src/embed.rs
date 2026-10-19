use mote_memory::MemoryError;
use std::fmt;

pub const EMBED_DIM: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EmbeddingModelId(String);

impl EmbeddingModelId {
    pub fn new(value: impl Into<String>) -> Result<Self, MemoryError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(MemoryError::malformed("invalid embedding model id"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EmbeddingModelId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub trait Embedder: Send + Sync {
    fn model_id(&self) -> &EmbeddingModelId;
    fn embed(&self, text: &str) -> Vec<i8>;
}

pub struct HashEmbedder {
    model: EmbeddingModelId,
}

impl HashEmbedder {
    pub fn new() -> Self {
        Self {
            model: EmbeddingModelId::new("hash-v1").expect("static model id is valid"),
        }
    }
}

impl Default for HashEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl Embedder for HashEmbedder {
    fn model_id(&self) -> &EmbeddingModelId {
        &self.model
    }

    fn embed(&self, text: &str) -> Vec<i8> {
        let mut values = vec![0_f32; EMBED_DIM];
        for token in text
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| !token.is_empty())
        {
            let hash = hash64(&token.to_lowercase());
            let index = (hash % EMBED_DIM as u64) as usize;
            values[index] += if hash >> 63 == 0 { 1.0 } else { -1.0 };
        }
        let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
        let norm = if norm == 0.0 { 1.0 } else { norm };
        values
            .into_iter()
            .map(|value| ((value / norm) * 127.0).round().clamp(-127.0, 127.0) as i8)
            .collect()
    }
}

fn hash64(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub(crate) fn checked_embedding(
    embedder: &dyn Embedder,
    text: &str,
) -> Result<Vec<i8>, MemoryError> {
    let embedding = embedder.embed(text);
    if embedding.len() != EMBED_DIM {
        return Err(MemoryError::backend(format!(
            "embedding model {} returned {} dimensions, expected {EMBED_DIM}",
            embedder.model_id(),
            embedding.len()
        )));
    }
    Ok(embedding)
}

pub(crate) fn embedding_bytes(embedding: &[i8]) -> Vec<u8> {
    embedding.iter().map(|value| *value as u8).collect()
}

pub(crate) fn similarity(left: &[i8], right: &[u8]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(left, right)| f64::from(*left) * f64::from(*right as i8))
        .sum()
}
