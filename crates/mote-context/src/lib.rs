//! Context store: profile, working set, packs and the ledger. See docs/rfcs/v0.md section 9.
//! The memory graph (section 10) lives in [`mem`].
//!
//! A pack is an ordered list of parts in the fixed order charter, profile, memory, working set,
//! turn. Each part records `{kind, source_ref, hash, tokens}`. Every model call writes exactly
//! one pack row and one ledger row. Packs are kept 30 days.

pub mod mem;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Packs older than this are pruned by the daily job.
pub const PACK_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// The fixed part order of every pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    Charter,
    Profile,
    Memory,
    WorkingSet,
    Turn,
}

impl PartKind {
    /// The canonical order.
    pub const ORDER: [PartKind; 5] = [
        PartKind::Charter,
        PartKind::Profile,
        PartKind::Memory,
        PartKind::WorkingSet,
        PartKind::Turn,
    ];
}

/// One part of a pack. The text itself is not stored, only its hash and token count.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Part {
    pub kind: PartKind,
    pub source_ref: String,
    pub hash: String,
    pub tokens: usize,
}

/// An assembled pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pack {
    pub lane: String,
    pub parts: Vec<Part>,
}

impl Pack {
    /// Total token estimate across parts.
    pub fn token_estimate(&self) -> usize {
        self.parts.iter().map(|p| p.tokens).sum()
    }
}

fn bpe() -> &'static tiktoken_rs::CoreBPE {
    static B: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();
    B.get_or_init(|| tiktoken_rs::o200k_base().expect("o200k_base ranks are bundled"))
}

/// Token count of text under o200k_base.
pub fn count_tokens(text: &str) -> usize {
    bpe().encode_ordinary(text).len()
}

/// Hex SHA-256 of text.
pub fn hash_text(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    format!("{:x}", h.finalize())
}

fn truncate_to_tokens(text: &str, max: usize) -> String {
    if count_tokens(text) <= max {
        return text.to_string();
    }
    // Shrink by characters until it fits. Cheap and good enough for a budget cap.
    let mut end = text.len();
    while end > 0 {
        end = end.saturating_sub((end / 8).max(1));
        let slice = match text.get(..end) {
            Some(s) => s,
            None => continue,
        };
        if count_tokens(slice) <= max {
            return slice.to_string();
        }
    }
    String::new()
}

/// Builds a pack from part texts in the fixed order, enforcing a token budget.
pub struct PackBuilder {
    lane: String,
    texts: Vec<(PartKind, String, String)>, // (kind, source_ref, text)
}

impl PackBuilder {
    pub fn new(lane: impl Into<String>) -> Self {
        Self { lane: lane.into(), texts: Vec::new() }
    }

    pub fn part(mut self, kind: PartKind, source_ref: impl Into<String>, text: impl Into<String>) -> Self {
        self.texts.push((kind, source_ref.into(), text.into()));
        self
    }

    /// Assemble the pack and return the trimmed part texts alongside it, in [`PartKind::ORDER`].
    /// The texts are what a lane sends to the model; the pack records only hashes and tokens.
    pub fn build_with_texts(self, budget: usize) -> (Pack, Vec<(PartKind, String)>) {
        self.build_inner(budget)
    }

    /// Assemble the pack, trimming trimmable parts so the total stays within `budget` tokens.
    /// Parts always appear in [`PartKind::ORDER`].
    pub fn build(self, budget: usize) -> Pack {
        self.build_inner(budget).0
    }

    fn build_inner(self, budget: usize) -> (Pack, Vec<(PartKind, String)>) {
        // Index texts by kind, keeping the fixed order.
        let mut ordered: Vec<(PartKind, String, String)> = Vec::new();
        for kind in PartKind::ORDER {
            if let Some((_, src, text)) = self.texts.iter().find(|(k, _, _)| *k == kind) {
                ordered.push((kind, src.clone(), text.clone()));
            }
        }

        // Trim, from least to most important, until within budget.
        let trim_order =
            [PartKind::WorkingSet, PartKind::Memory, PartKind::Profile, PartKind::Charter, PartKind::Turn];
        loop {
            let total: usize = ordered.iter().map(|(_, _, t)| count_tokens(t)).sum();
            if total <= budget {
                break;
            }
            let over = total - budget;
            let mut trimmed = false;
            for tk in trim_order {
                if let Some(entry) = ordered.iter_mut().find(|(k, _, t)| *k == tk && !t.is_empty()) {
                    let current = count_tokens(&entry.2);
                    let target = current.saturating_sub(over).min(current.saturating_sub(1));
                    entry.2 = truncate_to_tokens(&entry.2, target);
                    trimmed = true;
                    break;
                }
            }
            if !trimmed {
                break;
            }
        }

        let texts: Vec<(PartKind, String)> =
            ordered.iter().map(|(k, _, t)| (*k, t.clone())).collect();
        let parts = ordered
            .into_iter()
            .map(|(kind, source_ref, text)| Part {
                kind,
                source_ref,
                hash: hash_text(&text),
                tokens: count_tokens(&text),
            })
            .collect();
        (Pack { lane: self.lane, parts }, texts)
    }
}

fn now_millis() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// One recorded model call: a pack row and a ledger row.
#[derive(Debug, Clone)]
pub struct LedgerEntry {
    pub lane: String,
    pub model: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub latency_ms: i64,
    pub cost_usd: f64,
    pub status: String,
}

/// The context store over one SQLite connection.
pub struct ContextStore {
    conn: Connection,
    clock: Clock,
}

impl ContextStore {
    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        Self::from_conn(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::from_conn(Connection::open_in_memory()?)
    }

    fn from_conn(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS profile(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS working_set(
                 thread_id TEXT NOT NULL, seq INTEGER NOT NULL, text TEXT NOT NULL,
                 PRIMARY KEY(thread_id, seq));
             CREATE TABLE IF NOT EXISTS packs(
                 id INTEGER PRIMARY KEY AUTOINCREMENT, lane TEXT NOT NULL, parts TEXT NOT NULL,
                 token_estimate INTEGER NOT NULL, created_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS ledger(
                 id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, lane TEXT NOT NULL,
                 model TEXT NOT NULL, pack_id INTEGER NOT NULL, prompt_tokens INTEGER NOT NULL,
                 completion_tokens INTEGER NOT NULL, latency_ms INTEGER NOT NULL,
                 cost_usd REAL NOT NULL, status TEXT NOT NULL);",
        )?;
        mem::create_tables(&conn)?;
        Ok(Self { conn, clock: Arc::new(now_millis) })
    }

    /// Replace the clock, for tests.
    pub fn set_clock(&mut self, clock: Clock) {
        self.clock = clock;
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }

    /// Borrow the underlying connection (used by the memory graph).
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn set_profile(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO profile(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }

    pub fn get_profile(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM profile WHERE key=?1", [key], |r| r.get(0))
            .ok()
            .map(Some)
            .map(Ok)
            .unwrap_or(Ok(None))
    }

    pub fn append_working_set(&self, thread_id: &str, seq: i64, text: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO working_set(thread_id, seq, text) VALUES(?1, ?2, ?3)",
            rusqlite::params![thread_id, seq, text],
        )?;
        Ok(())
    }

    pub fn working_set_text(&self, thread_id: &str) -> rusqlite::Result<String> {
        let mut stmt = self
            .conn
            .prepare("SELECT text FROM working_set WHERE thread_id=?1 ORDER BY seq ASC")?;
        let rows = stmt.query_map([thread_id], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out.join("\n"))
    }

    /// Record a model call: one pack row and one ledger row. Returns (pack_id, ledger_id).
    pub fn record(&self, pack: &Pack, entry: &LedgerEntry) -> rusqlite::Result<(i64, i64)> {
        let now = self.now();
        let parts_json = serde_json::to_string(&pack.parts).unwrap();
        self.conn.execute(
            "INSERT INTO packs(lane, parts, token_estimate, created_at) VALUES(?1, ?2, ?3, ?4)",
            rusqlite::params![pack.lane, parts_json, pack.token_estimate() as i64, now],
        )?;
        let pack_id = self.conn.last_insert_rowid();
        self.conn.execute(
            "INSERT INTO ledger(ts, lane, model, pack_id, prompt_tokens, completion_tokens,
                 latency_ms, cost_usd, status)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                now,
                entry.lane,
                entry.model,
                pack_id,
                entry.prompt_tokens,
                entry.completion_tokens,
                entry.latency_ms,
                entry.cost_usd,
                entry.status
            ],
        )?;
        let ledger_id = self.conn.last_insert_rowid();
        Ok((pack_id, ledger_id))
    }

    pub fn count_packs(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM packs", [], |r| r.get(0))
    }

    pub fn count_ledger(&self) -> rusqlite::Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM ledger", [], |r| r.get(0))
    }

    /// Write a memory node (one transaction, no model call).
    pub fn memory_write(
        &self,
        kind: &str,
        text: &str,
        source_seq: Option<i64>,
    ) -> rusqlite::Result<i64> {
        mem::write_node(&self.conn, kind, mem::DEFAULT_REALM, text, None, source_seq)
    }

    /// Text search over memory nodes.
    pub fn memory_search(&self, query: &str, limit: usize) -> rusqlite::Result<Vec<mem::MemHit>> {
        mem::search_like(&self.conn, query, limit)
    }

    /// Fetch a pack's parts by id.
    pub fn pack_parts(&self, pack_id: i64) -> rusqlite::Result<Vec<Part>> {
        let parts_json: String =
            self.conn.query_row("SELECT parts FROM packs WHERE id=?1", [pack_id], |r| r.get(0))?;
        Ok(serde_json::from_str(&parts_json).unwrap_or_default())
    }

    /// Delete packs older than 30 days. Returns the number deleted.
    pub fn prune_packs(&self) -> rusqlite::Result<usize> {
        let cutoff = self.now() - PACK_TTL_MS;
        let n = self
            .conn
            .execute("DELETE FROM packs WHERE created_at < ?1", [cutoff])?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn sample_pack() -> Pack {
        PackBuilder::new("talk")
            .part(PartKind::Charter, "house.md", "be kind")
            .part(PartKind::Profile, "profile", "name: owner")
            .part(PartKind::Memory, "mem", "")
            .part(PartKind::WorkingSet, "ws", "earlier message")
            .part(PartKind::Turn, "turn", "hello")
            .build(600)
    }

    fn entry() -> LedgerEntry {
        LedgerEntry {
            lane: "talk".into(),
            model: "qwen3:4b".into(),
            prompt_tokens: 10,
            completion_tokens: 5,
            latency_ms: 100,
            cost_usd: 0.0,
            status: "ok".into(),
        }
    }

    #[test]
    fn three_turns_write_three_packs_and_three_ledger_rows() {
        let store = ContextStore::open_in_memory().unwrap();
        for _ in 0..3 {
            let pack = sample_pack();
            let (pack_id, _) = store.record(&pack, &entry()).unwrap();
            let parts = store.pack_parts(pack_id).unwrap();
            // Fixed order and hashed parts.
            let kinds: Vec<PartKind> = parts.iter().map(|p| p.kind).collect();
            assert_eq!(kinds, PartKind::ORDER.to_vec());
            assert!(parts.iter().all(|p| p.hash.len() == 64));
        }
        assert_eq!(store.count_packs().unwrap(), 3);
        assert_eq!(store.count_ledger().unwrap(), 3);
    }

    #[test]
    fn prune_removes_packs_older_than_30_days() {
        let mut store = ContextStore::open_in_memory().unwrap();
        let clock = Arc::new(Mutex::new(1_000_000_000_000i64));
        let c = clock.clone();
        store.set_clock(Arc::new(move || *c.lock().unwrap()));

        // One old pack.
        store.record(&sample_pack(), &entry()).unwrap();
        // Advance 31 days and add a fresh pack.
        *clock.lock().unwrap() += 31 * 24 * 60 * 60 * 1000;
        store.record(&sample_pack(), &entry()).unwrap();

        let deleted = store.prune_packs().unwrap();
        assert_eq!(deleted, 1, "only the 31-day-old pack is pruned");
        assert_eq!(store.count_packs().unwrap(), 1);
    }

    #[test]
    fn pack_stays_within_budget() {
        let big = "word ".repeat(2000);
        let pack = PackBuilder::new("talk")
            .part(PartKind::Charter, "h", "charter")
            .part(PartKind::Turn, "t", "hi")
            .part(PartKind::WorkingSet, "w", &big)
            .build(600);
        assert!(pack.token_estimate() <= 600, "got {}", pack.token_estimate());
    }
}
