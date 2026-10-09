//! Memory graph (docs/rfcs/v0.md section 10). Tables are created here; retrieval, embeddings
//! and forgetting are filled by V0-33 onward.

use rusqlite::Connection;

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
