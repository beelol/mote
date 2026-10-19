mod embed;
mod schema;

pub use embed::{Embedder, EmbeddingModelId, HashEmbedder, EMBED_DIM};
pub use schema::register_sqlite_vec;

use embed::{checked_embedding, embedding_bytes, similarity};
use mote_memory::{
    validate_query, AccountId, BackendHealth, Correction, CorrectionResult, ForgetResult,
    MemoryBackend, MemoryError, MemoryHit, MemoryId, MemoryScope, MemoryWrite, SourceRef,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use schema::database_error;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

const BACKEND_NAME: &str = "sqlite";
const RRF_K: f64 = 60.0;

pub struct SqliteMemory {
    connection: Connection,
    embedder: Box<dyn Embedder>,
    default_account: AccountId,
}

impl SqliteMemory {
    pub fn open(
        path: impl AsRef<Path>,
        embedder: Box<dyn Embedder>,
        default_account: AccountId,
    ) -> Result<Self, MemoryError> {
        register_sqlite_vec();
        let connection = Connection::open(path).map_err(database_error)?;
        Self::from_connection(connection, embedder, default_account)
    }

    pub fn open_in_memory(
        embedder: Box<dyn Embedder>,
        default_account: AccountId,
    ) -> Result<Self, MemoryError> {
        register_sqlite_vec();
        let connection = Connection::open_in_memory().map_err(database_error)?;
        Self::from_connection(connection, embedder, default_account)
    }

    fn from_connection(
        mut connection: Connection,
        embedder: Box<dyn Embedder>,
        default_account: AccountId,
    ) -> Result<Self, MemoryError> {
        schema::initialize(
            &mut connection,
            &default_account,
            embedder.model_id().as_str(),
        )?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(database_error)?;
        Ok(Self {
            connection,
            embedder,
            default_account,
        })
    }

    pub fn default_account(&self) -> &AccountId {
        &self.default_account
    }

    fn source(scope: &MemoryScope, id: &MemoryId, version: u64) -> Result<SourceRef, MemoryError> {
        SourceRef::new(scope.clone(), BACKEND_NAME, id.clone(), version)
    }

    fn numeric_id(id: &MemoryId) -> Result<i64, MemoryError> {
        id.as_str()
            .parse()
            .map_err(|_| MemoryError::malformed("invalid SQLite memory id"))
    }

    fn row_state(
        &self,
        scope: &MemoryScope,
        id: &MemoryId,
    ) -> Result<(i64, u64, bool), MemoryError> {
        Self::row_state_on(&self.connection, scope, id)
    }

    fn row_state_on(
        connection: &Connection,
        scope: &MemoryScope,
        id: &MemoryId,
    ) -> Result<(i64, u64, bool), MemoryError> {
        let numeric = Self::numeric_id(id)?;
        let state = connection
            .query_row(
                "SELECT account_id,realm,version,forgotten FROM mem_node WHERE id=?1",
                [numeric],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, u64>(2)?,
                        row.get::<_, bool>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(database_error)?
            .ok_or_else(|| MemoryError::not_found("memory"))?;
        if state.0 != scope.account_id() || state.1 != scope.realm() {
            return Err(MemoryError::refused("memory scope mismatch"));
        }
        Ok((numeric, state.2, state.3))
    }

    fn make_hit(
        scope: &MemoryScope,
        id: i64,
        version: u64,
        text: String,
        rank: usize,
        score: f64,
    ) -> Result<MemoryHit, MemoryError> {
        let id = MemoryId::new(id.to_string())?;
        let source = Self::source(scope, &id, version)?;
        MemoryHit::new(scope.clone(), id, source, text, rank, score as f32)
    }
}

impl MemoryBackend for SqliteMemory {
    fn write(&mut self, scope: &MemoryScope, write: MemoryWrite) -> Result<MemoryHit, MemoryError> {
        let embedding = checked_embedding(self.embedder.as_ref(), write.text())?;
        let bytes = embedding_bytes(&embedding);
        let now = now_millis();
        let transaction = self.connection.transaction().map_err(database_error)?;
        transaction
            .execute(
                "INSERT INTO mem_node(
                   account_id,realm,kind,text,embedding,embedding_model,source_seq,
                   authoritative_source,version,created_at,valid_from,forgotten)
                 VALUES(?1,?2,?3,?4,?5,?6,NULL,?7,1,?8,?8,0)",
                params![
                    scope.account_id(),
                    scope.realm(),
                    write.kind(),
                    write.text(),
                    bytes,
                    self.embedder.model_id().as_str(),
                    write.authoritative_source(),
                    now
                ],
            )
            .map_err(database_error)?;
        let id = transaction.last_insert_rowid();
        transaction
            .execute(
                "INSERT INTO mem_fts(rowid,text) VALUES(?1,?2)",
                params![id, write.text()],
            )
            .map_err(database_error)?;
        transaction
            .execute(
                "INSERT INTO mem_vec(rowid,embedding) VALUES(?1,vec_int8(?2))",
                params![id, bytes],
            )
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        Self::make_hit(scope, id, 1, write.into_text(), 1, 1.0)
    }

    fn retrieve(
        &mut self,
        scope: &MemoryScope,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryHit>, MemoryError> {
        validate_query(query, limit)?;
        let mut scores: HashMap<i64, f64> = HashMap::new();
        if let Some(query) = fts_query(query) {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT f.rowid FROM mem_fts f
                     JOIN mem_node n ON n.id=f.rowid
                     WHERE mem_fts MATCH ?1 AND n.account_id=?2 AND n.realm=?3 AND n.forgotten=0
                     ORDER BY rank LIMIT 20",
                )
                .map_err(database_error)?;
            let ids = statement
                .query_map(params![query, scope.account_id(), scope.realm()], |row| {
                    row.get::<_, i64>(0)
                })
                .map_err(database_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(database_error)?;
            for (rank, id) in ids.into_iter().enumerate() {
                *scores.entry(id).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
            }
        }

        let query_embedding = checked_embedding(self.embedder.as_ref(), query)?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT id,embedding FROM mem_node
                 WHERE account_id=?1 AND realm=?2 AND forgotten=0
                   AND embedding_model=?3 AND embedding IS NOT NULL",
            )
            .map_err(database_error)?;
        let rows = statement
            .query_map(
                params![
                    scope.account_id(),
                    scope.realm(),
                    self.embedder.model_id().as_str()
                ],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .map_err(database_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_error)?;
        let mut vector_scores = rows
            .into_iter()
            .filter(|(_, embedding)| embedding.len() == EMBED_DIM)
            .map(|(id, embedding)| (id, similarity(&query_embedding, &embedding)))
            .filter(|(_, score)| *score > 0.0)
            .collect::<Vec<_>>();
        vector_scores.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
        });
        vector_scores.truncate(20);
        for (rank, (id, _)) in vector_scores.into_iter().enumerate() {
            *scores.entry(id).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
        }

        let mut scored = Vec::new();
        for (id, score) in scores {
            let row = self
                .connection
                .query_row(
                    "SELECT text,version FROM mem_node
                     WHERE id=?1 AND account_id=?2 AND realm=?3 AND forgotten=0",
                    params![id, scope.account_id(), scope.realm()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
                )
                .optional()
                .map_err(database_error)?;
            if let Some((text, version)) = row {
                scored.push((score, id, version, text));
            }
        }
        scored.sort_by(|left, right| {
            right
                .0
                .partial_cmp(&left.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.1.cmp(&right.1))
        });
        scored.truncate(limit);
        scored
            .into_iter()
            .enumerate()
            .map(|(index, (score, id, version, text))| {
                Self::make_hit(scope, id, version, text, index + 1, score)
            })
            .collect()
    }

    fn source(
        &mut self,
        scope: &MemoryScope,
        id: &MemoryId,
    ) -> Result<Option<SourceRef>, MemoryError> {
        let (_, version, _) = self.row_state(scope, id)?;
        Self::source(scope, id, version).map(Some)
    }

    fn correct(
        &mut self,
        scope: &MemoryScope,
        correction: Correction,
    ) -> Result<CorrectionResult, MemoryError> {
        let embedding = checked_embedding(self.embedder.as_ref(), correction.replacement())?;
        let bytes = embedding_bytes(&embedding);
        let now = now_millis();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database_error)?;
        let (numeric, version, forgotten) =
            Self::row_state_on(&transaction, scope, correction.id())?;
        if version != correction.expected_version() {
            return Err(MemoryError::conflict("stale memory version"));
        }
        if forgotten {
            return Err(MemoryError::not_found("memory was forgotten"));
        }
        let prior = Self::source(scope, correction.id(), version)?;
        let new_version = version + 1;
        transaction
            .execute("DELETE FROM mem_fts WHERE rowid=?1", [numeric])
            .map_err(database_error)?;
        transaction
            .execute("DELETE FROM mem_vec WHERE rowid=?1", [numeric])
            .map_err(database_error)?;
        let updated = transaction
            .execute(
                "UPDATE mem_node SET text=?1,embedding=?2,embedding_model=?3,
                   version=?4,valid_from=?5,valid_to=NULL,superseded_by=NULL
                 WHERE id=?6 AND account_id=?7 AND realm=?8 AND version=?9 AND forgotten=0",
                params![
                    correction.replacement(),
                    bytes,
                    self.embedder.model_id().as_str(),
                    new_version,
                    now,
                    numeric,
                    scope.account_id(),
                    scope.realm(),
                    correction.expected_version()
                ],
            )
            .map_err(database_error)?;
        if updated != 1 {
            return Err(MemoryError::conflict("memory changed during correction"));
        }
        transaction
            .execute(
                "INSERT INTO mem_fts(rowid,text) VALUES(?1,?2)",
                params![numeric, correction.replacement()],
            )
            .map_err(database_error)?;
        transaction
            .execute(
                "INSERT INTO mem_vec(rowid,embedding) VALUES(?1,vec_int8(?2))",
                params![numeric, bytes],
            )
            .map_err(database_error)?;
        transaction
            .execute(
                "INSERT INTO mem_mutation(
                   node_id,account_id,realm,operation,prior_version,current_version,
                   prior_source,created_at)
                 VALUES(?1,?2,?3,'correct',?4,?5,?6,?7)",
                params![
                    numeric,
                    scope.account_id(),
                    scope.realm(),
                    version,
                    new_version,
                    prior.source_id(),
                    now
                ],
            )
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        let current = Self::source(scope, correction.id(), new_version)?;
        Ok(CorrectionResult::Applied { prior, current })
    }

    fn forget(
        &mut self,
        scope: &MemoryScope,
        id: &MemoryId,
        expected_version: u64,
    ) -> Result<ForgetResult, MemoryError> {
        let now = now_millis();
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database_error)?;
        let (numeric, version, forgotten) = Self::row_state_on(&transaction, scope, id)?;
        if version != expected_version {
            return Err(MemoryError::conflict("stale memory version"));
        }
        let prior = Self::source(scope, id, version)?;
        if forgotten {
            return Ok(ForgetResult::AlreadyForgotten { tombstone: prior });
        }
        let new_version = version + 1;
        transaction
            .execute("DELETE FROM mem_fts WHERE rowid=?1", [numeric])
            .map_err(database_error)?;
        transaction
            .execute("DELETE FROM mem_vec WHERE rowid=?1", [numeric])
            .map_err(database_error)?;
        let updated = transaction
            .execute(
                "UPDATE mem_node SET text='',embedding=NULL,embedding_model=NULL,
                   version=?1,valid_to=?2,superseded_by=id,forgotten=1
                 WHERE id=?3 AND account_id=?4 AND realm=?5 AND version=?6 AND forgotten=0",
                params![
                    new_version,
                    now,
                    numeric,
                    scope.account_id(),
                    scope.realm(),
                    expected_version
                ],
            )
            .map_err(database_error)?;
        if updated != 1 {
            return Err(MemoryError::conflict("memory changed during forget"));
        }
        transaction
            .execute(
                "INSERT INTO mem_mutation(
                   node_id,account_id,realm,operation,prior_version,current_version,
                   prior_source,created_at)
                 VALUES(?1,?2,?3,'forget',?4,?5,?6,?7)",
                params![
                    numeric,
                    scope.account_id(),
                    scope.realm(),
                    version,
                    new_version,
                    prior.source_id(),
                    now
                ],
            )
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        let tombstone = Self::source(scope, id, new_version)?;
        Ok(ForgetResult::Forgotten { tombstone })
    }

    fn health(&mut self) -> BackendHealth {
        match self.connection.query_row("SELECT 1", [], |_| Ok(())) {
            Ok(()) => BackendHealth::healthy(BACKEND_NAME),
            Err(error) => BackendHealth::unavailable(BACKEND_NAME, error.to_string()),
        }
    }
}

fn fts_query(query: &str) -> Option<String> {
    let terms = query
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.to_lowercase()))
        .collect::<Vec<_>>();
    (!terms.is_empty()).then(|| terms.join(" OR "))
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
