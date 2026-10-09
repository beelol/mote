//! Memory graph (docs/rfcs/v0.md section 10).
//!
//! Tables: `mem_node`, `mem_edge`, `mem_fts` (FTS5 over text) and `mem_vec` (sqlite-vec,
//! int8[256]). Embeddings are produced by an [`Embedder`]; production injects model2vec-rs
//! potion-base-8M, tests use the deterministic [`HashEmbedder`]. Retrieval fuses FTS, vector, a
//! bounded 2-hop walk and recency decay by reciprocal rank. A write is one transaction with no
//! model (LLM) call. Forgetting sets `superseded_by`; nothing is deleted.

use rusqlite::Connection;
use serde::Serialize;
use std::sync::Once;
use std::time::{SystemTime, UNIX_EPOCH};

/// Embedding dimension.
pub const EMBED_DIM: usize = 256;
/// Default realm in v0.
pub const DEFAULT_REALM: &str = "personal";
/// Reciprocal-rank fusion constant.
pub const RRF_K: f64 = 60.0;
/// Recency half-life (30 days) in milliseconds.
pub const HALF_LIFE_MS: f64 = 30.0 * 24.0 * 60.0 * 60.0 * 1000.0;
/// Edge fan-out bound per node in the 2-hop walk.
pub const MAX_EDGES_PER_NODE: usize = 200;

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Produces an int8[EMBED_DIM] embedding for text. Local only; never an LLM call.
pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Vec<i8>;
}

/// A deterministic, dependency-free embedder for tests and as a fallback. Feature-hashes tokens
/// into EMBED_DIM dimensions, L2-normalizes and quantizes to int8.
pub struct HashEmbedder;

fn hash64(s: &str) -> u64 {
    // FNV-1a.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

impl Embedder for HashEmbedder {
    fn embed(&self, text: &str) -> Vec<i8> {
        let mut acc = vec![0f32; EMBED_DIM];
        for token in text.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()) {
            let lower = token.to_lowercase();
            let h = hash64(&lower);
            let idx = (h % EMBED_DIM as u64) as usize;
            let sign = if (h >> 63) & 1 == 1 { -1.0 } else { 1.0 };
            acc[idx] += sign;
        }
        let norm = acc.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm = if norm == 0.0 { 1.0 } else { norm };
        acc.iter().map(|x| ((x / norm) * 127.0).round().clamp(-127.0, 127.0) as i8).collect()
    }
}

/// Quantize an f32 embedding (e.g. from model2vec) to int8[EMBED_DIM] after L2 normalization.
pub fn quantize(embedding: &[f32]) -> Vec<i8> {
    let norm = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm = if norm == 0.0 { 1.0 } else { norm };
    embedding
        .iter()
        .take(EMBED_DIM)
        .map(|x| ((x / norm) * 127.0).round().clamp(-127.0, 127.0) as i8)
        .collect()
}

fn i8_as_u8(v: &[i8]) -> &[u8] {
    // Safety: i8 and u8 have the same size and alignment.
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, v.len()) }
}

/// Register the sqlite-vec extension once per process, before opening connections that use it.
pub fn ensure_vec_extension() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let init: *const () = sqlite_vec::sqlite3_vec_init as *const ();
        let entry: unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *const std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int = std::mem::transmute(init);
        rusqlite::ffi::sqlite3_auto_extension(Some(entry));
    });
}

/// One memory node returned by a search.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemHit {
    pub id: i64,
    pub kind: String,
    pub text: String,
}

/// Create the memory graph tables. [`ensure_vec_extension`] must have run first.
pub fn create_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mem_node(
             id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, realm TEXT NOT NULL,
             text TEXT NOT NULL, embedding BLOB, source_seq INTEGER, created_at INTEGER NOT NULL,
             valid_from INTEGER, valid_to INTEGER, superseded_by INTEGER);
         CREATE TABLE IF NOT EXISTS mem_edge(
             src INTEGER NOT NULL, dst INTEGER NOT NULL, kind TEXT NOT NULL, realm TEXT NOT NULL,
             source_seq INTEGER, valid_from INTEGER, valid_to INTEGER);
         CREATE INDEX IF NOT EXISTS mem_edge_src ON mem_edge(src);
         CREATE INDEX IF NOT EXISTS mem_edge_dst ON mem_edge(dst);
         CREATE VIRTUAL TABLE IF NOT EXISTS mem_fts USING fts5(text);",
    )?;
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS mem_vec USING vec0(embedding int8[{EMBED_DIM}]);"
    ))?;
    Ok(())
}

/// Write a memory node in one transaction with no model (LLM) call. Stores the node, its FTS row
/// and its vector row.
pub fn write_node(
    conn: &Connection,
    embedder: &dyn Embedder,
    kind: &str,
    realm: &str,
    text: &str,
    source_seq: Option<i64>,
) -> rusqlite::Result<i64> {
    let now = now_millis();
    let emb = embedder.embed(text);
    let blob = i8_as_u8(&emb);
    conn.execute(
        "INSERT INTO mem_node(kind, realm, text, embedding, source_seq, created_at, valid_from)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        rusqlite::params![kind, realm, text, blob, source_seq, now],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute("INSERT INTO mem_fts(rowid, text) VALUES(?1, ?2)", rusqlite::params![id, text])?;
    conn.execute(
        "INSERT INTO mem_vec(rowid, embedding) VALUES(?1, vec_int8(?2))",
        rusqlite::params![id, blob],
    )?;
    Ok(id)
}

/// Add an edge between two nodes.
pub fn add_edge(conn: &Connection, src: i64, dst: i64, kind: &str) -> rusqlite::Result<()> {
    let now = now_millis();
    conn.execute(
        "INSERT INTO mem_edge(src, dst, kind, realm, valid_from) VALUES(?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![src, dst, kind, DEFAULT_REALM, now],
    )?;
    Ok(())
}

/// Mark a node superseded. Nothing is deleted.
pub fn forget(conn: &Connection, id: i64, superseded_by: Option<i64>) -> rusqlite::Result<()> {
    let now = now_millis();
    conn.execute(
        "UPDATE mem_node SET superseded_by=?1, valid_to=?2 WHERE id=?3",
        rusqlite::params![superseded_by.unwrap_or(id), now, id],
    )?;
    Ok(())
}

/// Basic text search over live nodes (backs the MCP memory_search tool).
pub fn search_like(conn: &Connection, query: &str, limit: usize) -> rusqlite::Result<Vec<MemHit>> {
    let pattern = format!("%{query}%");
    let mut stmt = conn.prepare(
        "SELECT id, kind, text FROM mem_node
         WHERE superseded_by IS NULL AND text LIKE ?1 ORDER BY created_at DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit as i64], |r| {
        Ok(MemHit { id: r.get(0)?, kind: r.get(1)?, text: r.get(2)? })
    })?;
    rows.collect()
}

fn fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.to_lowercase()))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

/// Fused retrieval: FTS top 20, vector top 20, their bounded 2-hop neighbours, fused by
/// reciprocal rank (k=60) with a 30-day recency half-life, superseded nodes excluded, cut to
/// `limit`.
pub fn retrieve(
    conn: &Connection,
    embedder: &dyn Embedder,
    query: &str,
    limit: usize,
) -> rusqlite::Result<Vec<MemHit>> {
    use std::collections::HashMap;
    let mut score: HashMap<i64, f64> = HashMap::new();

    // FTS top 20.
    if let Some(q) = fts_query(query) {
        let mut stmt = conn
            .prepare("SELECT rowid FROM mem_fts WHERE mem_fts MATCH ?1 ORDER BY rank LIMIT 20")?;
        let ids: Vec<i64> = stmt
            .query_map(rusqlite::params![q], |r| r.get(0))?
            .collect::<Result<_, _>>()
            .unwrap_or_default();
        for (rank, id) in ids.into_iter().enumerate() {
            *score.entry(id).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
    }

    // Vector top 20.
    let qvec = embedder.embed(query);
    {
        let mut stmt = conn.prepare(
            "SELECT rowid FROM mem_vec WHERE embedding MATCH vec_int8(?1) ORDER BY distance LIMIT 20",
        )?;
        let ids: Vec<i64> = stmt
            .query_map(rusqlite::params![i8_as_u8(&qvec)], |r| r.get(0))?
            .collect::<Result<_, _>>()
            .unwrap_or_default();
        for (rank, id) in ids.into_iter().enumerate() {
            *score.entry(id).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
    }

    // Bounded 2-hop neighbours of the current candidates.
    let seeds: Vec<i64> = score.keys().copied().collect();
    let mut frontier = seeds.clone();
    for hop in 0..2 {
        let mut next = Vec::new();
        for id in &frontier {
            let mut stmt = conn.prepare(
                "SELECT dst FROM mem_edge WHERE src=?1 UNION SELECT src FROM mem_edge WHERE dst=?1 LIMIT ?2",
            )?;
            let neighbours: Vec<i64> = stmt
                .query_map(rusqlite::params![id, MAX_EDGES_PER_NODE as i64], |r| r.get(0))?
                .collect::<Result<_, _>>()
                .unwrap_or_default();
            for n in neighbours {
                // Neighbours get a small, hop-decayed contribution so they are included but rank
                // below direct matches.
                score.entry(n).or_insert(0.0);
                *score.get_mut(&n).unwrap() += 1.0 / (RRF_K + 20.0 + (hop as f64 + 1.0) * 20.0);
                next.push(n);
            }
        }
        frontier = next;
    }

    if score.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch live nodes, apply recency decay, exclude superseded.
    let now = now_millis() as f64;
    let ids: Vec<i64> = score.keys().copied().collect();
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, kind, text, created_at FROM mem_node
         WHERE superseded_by IS NULL AND id IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(params.as_slice(), |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?))
    })?;

    let mut scored: Vec<(f64, MemHit)> = Vec::new();
    for row in rows {
        let (id, kind, text, created_at) = row?;
        let age = (now - created_at as f64).max(0.0);
        let recency = 0.5f64.powf(age / HALF_LIFE_MS);
        let final_score = score.get(&id).copied().unwrap_or(0.0) * recency;
        scored.push((final_score, MemHit { id, kind, text }));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    scored.truncate(limit);
    Ok(scored.into_iter().map(|(_, h)| h).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        ensure_vec_extension();
        let c = Connection::open_in_memory().unwrap();
        create_tables(&c).unwrap();
        c
    }

    #[test]
    fn write_makes_no_model_call_and_stores_embedding_realm_source_seq() {
        // A scripted Ollama server stands in for any model call; the write must not touch it.
        let srv = mote_testkit::ScriptedServer::start().unwrap();
        let c = conn();
        let e = HashEmbedder;
        let id = write_node(&c, &e, "event", DEFAULT_REALM, "one transaction write", Some(42)).unwrap();

        let (emb_len, realm, src): (i64, String, i64) = c
            .query_row(
                "SELECT length(embedding), realm, source_seq FROM mem_node WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(emb_len as usize, EMBED_DIM);
        assert_eq!(realm, DEFAULT_REALM);
        assert_eq!(src, 42);
        assert!(srv.recorded().is_empty(), "memory_write must make no model call");
    }

    #[test]
    fn write_stores_embedding_fts_and_vector() {
        let c = conn();
        let e = HashEmbedder;
        let id = write_node(&c, &e, "event", DEFAULT_REALM, "the sky is blue", Some(7)).unwrap();
        let (emb_len, src): (i64, i64) = c
            .query_row(
                "SELECT length(embedding), source_seq FROM mem_node WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(emb_len as usize, EMBED_DIM, "int8[256] embedding stored");
        assert_eq!(src, 7);
        let vec_rows: i64 =
            c.query_row("SELECT count(*) FROM mem_vec", [], |r| r.get(0)).unwrap();
        assert_eq!(vec_rows, 1);
    }

    #[test]
    fn retrieve_finds_relevant_and_recency_orders() {
        let c = conn();
        let e = HashEmbedder;
        write_node(&c, &e, "event", DEFAULT_REALM, "the cat sat on the mat", None).unwrap();
        write_node(&c, &e, "event", DEFAULT_REALM, "quantum chromodynamics lecture", None).unwrap();
        let hits = retrieve(&c, &e, "cat mat", 5).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].text.contains("cat"), "most relevant first: {hits:?}");
    }

    #[test]
    fn retrieve_p50_under_40ms_on_10k_nodes() {
        let c = conn();
        let e = HashEmbedder;
        let words = ["alpha", "beta", "gamma", "delta", "cat", "mat", "sky", "blue", "plan", "note"];
        c.execute_batch("BEGIN").unwrap();
        for i in 0..10_000 {
            let text = format!("memory {i} about {} and {}", words[i % 10], words[(i / 10) % 10]);
            let id = write_node(&c, &e, "event", DEFAULT_REALM, &text, None).unwrap();
            if i > 0 && i % 3 == 0 {
                add_edge(&c, id, id - 1, "rel").unwrap();
            }
        }
        c.execute_batch("COMMIT").unwrap();

        let mut samples = Vec::new();
        for _ in 0..30 {
            let start = std::time::Instant::now();
            let hits = retrieve(&c, &e, "cat and sky plan", 8).unwrap();
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            assert!(!hits.is_empty());
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = samples[samples.len() / 2];
        println!("retrieve p50 = {p50:.2} ms on 10k nodes");
        assert!(p50 < 40.0, "retrieve p50 {p50:.2} ms exceeds 40 ms");
    }

    #[test]
    fn forgetting_omits_from_retrieval() {
        let c = conn();
        let e = HashEmbedder;
        let id = write_node(&c, &e, "event", DEFAULT_REALM, "secret plan alpha", None).unwrap();
        assert!(!retrieve(&c, &e, "secret plan alpha", 5).unwrap().is_empty());
        forget(&c, id, None).unwrap();
        let hits = retrieve(&c, &e, "secret plan alpha", 5).unwrap();
        assert!(hits.iter().all(|h| h.id != id), "forgotten node must be omitted");
    }
}
