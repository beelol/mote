//! Memory graph (docs/rfcs/v0.md section 10). Tables are created here; embeddings, the fused
//! retrieval and forgetting are filled by V0-33 onward. Basic write and text search back the MCP
//! `memory_write` and `memory_search` tools.

use rusqlite::Connection;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default realm in v0.
pub const DEFAULT_REALM: &str = "personal";

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// One memory node returned by a search.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemHit {
    pub id: i64,
    pub kind: String,
    pub text: String,
}

/// Write a memory node in one transaction with no model call. `embedding` is optional until the
/// embedder is wired (V0-33).
pub fn write_node(
    conn: &Connection,
    kind: &str,
    realm: &str,
    text: &str,
    embedding: Option<&[u8]>,
    source_seq: Option<i64>,
) -> rusqlite::Result<i64> {
    let now = now_millis();
    conn.execute(
        "INSERT INTO mem_node(kind, realm, text, embedding, source_seq, created_at, valid_from)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        rusqlite::params![kind, realm, text, embedding, source_seq, now],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Text search over live (not superseded) memory nodes. Replaced by the fused retrieval in V0-34.
pub fn search_like(conn: &Connection, query: &str, limit: usize) -> rusqlite::Result<Vec<MemHit>> {
    let pattern = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT id, kind, text FROM mem_node
         WHERE superseded_by IS NULL AND text LIKE ?1
         ORDER BY created_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit as i64], |r| {
        Ok(MemHit { id: r.get(0)?, kind: r.get(1)?, text: r.get(2)? })
    })?;
    rows.collect()
}

/// Create the memory graph tables if they do not exist.
pub fn create_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mem_node(
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             kind TEXT NOT NULL,
             realm TEXT NOT NULL,
             text TEXT NOT NULL,
             embedding BLOB,
             source_seq INTEGER,
             created_at INTEGER NOT NULL,
             valid_from INTEGER,
             valid_to INTEGER,
             superseded_by INTEGER);
         CREATE TABLE IF NOT EXISTS mem_edge(
             src INTEGER NOT NULL,
             dst INTEGER NOT NULL,
             kind TEXT NOT NULL,
             realm TEXT NOT NULL,
             source_seq INTEGER,
             valid_from INTEGER,
             valid_to INTEGER);
         CREATE INDEX IF NOT EXISTS mem_edge_src ON mem_edge(src);
         CREATE INDEX IF NOT EXISTS mem_edge_dst ON mem_edge(dst);",
    )
}
