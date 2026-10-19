use mote_memory::{AccountId, MemoryBackend, MemoryScope};
use mote_memory_sqlite::{Embedder, EmbeddingModelId, HashEmbedder, SqliteMemory, EMBED_DIM};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn temp_db(label: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "mote-memory-migration-{label}-{}-{}.db",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

struct ConstantEmbedder {
    model: EmbeddingModelId,
}

struct WrongDimensionEmbedder {
    model: EmbeddingModelId,
}

impl Embedder for WrongDimensionEmbedder {
    fn model_id(&self) -> &EmbeddingModelId {
        &self.model
    }

    fn embed(&self, _text: &str) -> Vec<i8> {
        vec![1; EMBED_DIM - 1]
    }
}

impl ConstantEmbedder {
    fn new(model: &str) -> Self {
        Self {
            model: EmbeddingModelId::new(model).unwrap(),
        }
    }
}

impl Embedder for ConstantEmbedder {
    fn model_id(&self) -> &EmbeddingModelId {
        &self.model
    }

    fn embed(&self, _text: &str) -> Vec<i8> {
        vec![1; EMBED_DIM]
    }
}

fn create_legacy_db(path: &PathBuf, text: &str) {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE mem_node(
               id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, realm TEXT NOT NULL,
               text TEXT NOT NULL, embedding BLOB, source_seq INTEGER, created_at INTEGER NOT NULL,
               valid_from INTEGER, valid_to INTEGER, superseded_by INTEGER);
             CREATE TABLE mem_edge(
               src INTEGER NOT NULL, dst INTEGER NOT NULL, kind TEXT NOT NULL, realm TEXT NOT NULL,
               source_seq INTEGER, valid_from INTEGER, valid_to INTEGER);
             CREATE VIRTUAL TABLE mem_fts USING fts5(text);",
        )
        .unwrap();
    let embedding = vec![1_u8; EMBED_DIM];
    connection
        .execute(
            "INSERT INTO mem_node(kind,realm,text,embedding,created_at,valid_from)
             VALUES('fact','personal',?1,?2,1,1)",
            rusqlite::params![text, embedding],
        )
        .unwrap();
    connection
        .execute("INSERT INTO mem_fts(rowid,text) VALUES(1,?1)", [text])
        .unwrap();
}

#[test]
fn legacy_vectors_are_labelled_hash_and_excluded_from_potion_queries() {
    let path = temp_db("legacy");
    create_legacy_db(&path, "opaque legacy wording");
    let mut memory = SqliteMemory::open(
        &path,
        Box::new(ConstantEmbedder::new("potion-base-8m")),
        AccountId::new("account-a").unwrap(),
    )
    .unwrap();
    let scope = MemoryScope::new("account-a", "personal").unwrap();
    assert!(memory
        .retrieve(&scope, "semantic-only", 5)
        .unwrap()
        .is_empty());
    drop(memory);

    let connection = Connection::open(&path).unwrap();
    let model: String = connection
        .query_row(
            "SELECT embedding_model FROM mem_node WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let vectors: i64 = connection
        .query_row("SELECT count(*) FROM mem_vec", [], |row| row.get(0))
        .unwrap();
    assert_eq!(model, "hash-v1");
    assert_eq!(vectors, 0);
    drop(connection);
    std::fs::remove_file(path).ok();
}

#[test]
fn current_model_vectors_survive_restart_and_remain_searchable() {
    let path = temp_db("restart");
    let scope = MemoryScope::new("account-a", "personal").unwrap();
    {
        let mut memory = SqliteMemory::open(
            &path,
            Box::new(ConstantEmbedder::new("potion-base-8m")),
            AccountId::new("account-a").unwrap(),
        )
        .unwrap();
        memory
            .write(
                &scope,
                mote_memory::MemoryWrite::new("opaque stored wording").unwrap(),
            )
            .unwrap();
    }
    let mut reopened = SqliteMemory::open(
        &path,
        Box::new(ConstantEmbedder::new("potion-base-8m")),
        AccountId::new("account-a").unwrap(),
    )
    .unwrap();
    let hits = reopened.retrieve(&scope, "different query", 5).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].text(), "opaque stored wording");
    drop(reopened);
    let connection = Connection::open(&path).unwrap();
    let model: String = connection
        .query_row("SELECT embedding_model FROM mem_node", [], |row| row.get(0))
        .unwrap();
    let vectors: i64 = connection
        .query_row("SELECT count(*) FROM mem_vec", [], |row| row.get(0))
        .unwrap();
    assert_eq!(model, "potion-base-8m");
    assert_eq!(vectors, 1);
    drop(connection);
    std::fs::remove_file(path).ok();
}

#[test]
fn account_and_realm_isolation_survives_restart() {
    let path = temp_db("scope-restart");
    let scopes = [
        MemoryScope::new("account-a", "personal").unwrap(),
        MemoryScope::new("account-a", "work").unwrap(),
        MemoryScope::new("account-b", "personal").unwrap(),
    ];
    let mut ids = Vec::new();
    {
        let mut memory = SqliteMemory::open(
            &path,
            Box::new(HashEmbedder::new()),
            AccountId::new("account-a").unwrap(),
        )
        .unwrap();
        for scope in &scopes {
            ids.push(
                memory
                    .write(
                        scope,
                        mote_memory::MemoryWrite::new("shared scoped phrase").unwrap(),
                    )
                    .unwrap()
                    .id()
                    .clone(),
            );
        }
    }
    let mut reopened = SqliteMemory::open(
        &path,
        Box::new(HashEmbedder::new()),
        AccountId::new("account-a").unwrap(),
    )
    .unwrap();
    for (scope, expected_id) in scopes.iter().zip(ids) {
        let hits = reopened.retrieve(scope, "shared scoped phrase", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id(), &expected_id);
        assert_eq!(hits[0].scope(), scope);
    }
    drop(reopened);
    std::fs::remove_file(path).ok();
}

#[test]
fn hash_embedder_has_an_explicit_stable_model_id() {
    assert_eq!(HashEmbedder::new().model_id().as_str(), "hash-v1");
}

#[test]
fn invalid_embedding_dimensions_are_refused_before_write() {
    let path = temp_db("dimension");
    let scope = MemoryScope::new("account-a", "personal").unwrap();
    let mut memory = SqliteMemory::open(
        &path,
        Box::new(WrongDimensionEmbedder {
            model: EmbeddingModelId::new("broken-v1").unwrap(),
        }),
        AccountId::new("account-a").unwrap(),
    )
    .unwrap();
    assert!(memory
        .write(
            &scope,
            mote_memory::MemoryWrite::new("dimension mismatch").unwrap()
        )
        .is_err());
    drop(memory);
    let connection = Connection::open(&path).unwrap();
    let count: i64 = connection
        .query_row("SELECT count(*) FROM mem_node", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
    drop(connection);
    std::fs::remove_file(path).ok();
}
