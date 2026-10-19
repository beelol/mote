use crate::embed::EMBED_DIM;
use mote_memory::{AccountId, MemoryError};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::sync::Once;

pub fn register_sqlite_vec() {
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

pub(crate) fn initialize(
    connection: &mut Connection,
    default_account: &AccountId,
    active_model: &str,
) -> Result<(), MemoryError> {
    let transaction = connection.transaction().map_err(database_error)?;
    migrate_nodes(&transaction, default_account)?;
    migrate_edges(&transaction, default_account)?;
    create_indexes(&transaction)?;
    rebuild_indexes(&transaction, active_model)?;
    transaction.commit().map_err(database_error)
}

fn table_exists(transaction: &Transaction<'_>, table: &str) -> Result<bool, MemoryError> {
    transaction
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type IN ('table','view') AND name=?1",
            [table],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(database_error)
}

fn columns(transaction: &Transaction<'_>, table: &str) -> Result<Vec<String>, MemoryError> {
    let mut statement = transaction
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(database_error)?;
    let columns = statement
        .query_map([], |row| row.get(1))
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    Ok(columns)
}

fn create_nodes(transaction: &Transaction<'_>) -> Result<(), MemoryError> {
    transaction
        .execute_batch(
            "CREATE TABLE mem_node(
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               account_id TEXT NOT NULL,
               realm TEXT NOT NULL,
               kind TEXT NOT NULL,
               text TEXT NOT NULL,
               embedding BLOB,
               embedding_model TEXT,
               source_seq INTEGER,
               authoritative_source TEXT,
               version INTEGER NOT NULL DEFAULT 1,
               created_at INTEGER NOT NULL,
               valid_from INTEGER,
               valid_to INTEGER,
               superseded_by INTEGER,
               forgotten INTEGER NOT NULL DEFAULT 0
             );",
        )
        .map_err(database_error)
}

fn migrate_nodes(
    transaction: &Transaction<'_>,
    default_account: &AccountId,
) -> Result<(), MemoryError> {
    if !table_exists(transaction, "mem_node")? {
        return create_nodes(transaction);
    }
    let existing = columns(transaction, "mem_node")?;
    let required = [
        "account_id",
        "embedding_model",
        "authoritative_source",
        "version",
        "forgotten",
    ];
    if required
        .iter()
        .all(|required| existing.iter().any(|column| column == required))
    {
        return Ok(());
    }

    transaction
        .execute_batch("ALTER TABLE mem_node RENAME TO mem_node_legacy;")
        .map_err(database_error)?;
    create_nodes(transaction)?;
    let has = |name: &str| existing.iter().any(|column| column == name);
    let account = if has("account_id") {
        "account_id"
    } else {
        "?1"
    };
    let model = if has("embedding_model") {
        "embedding_model"
    } else {
        "CASE WHEN embedding IS NULL THEN NULL ELSE 'hash-v1' END"
    };
    let authoritative = if has("authoritative_source") {
        "authoritative_source"
    } else {
        "NULL"
    };
    let version = if has("version") { "version" } else { "1" };
    let forgotten = if has("forgotten") {
        "forgotten"
    } else {
        "CASE WHEN superseded_by IS NULL THEN 0 ELSE 1 END"
    };
    let sql = format!(
        "INSERT INTO mem_node(
           id,account_id,realm,kind,text,embedding,embedding_model,source_seq,
           authoritative_source,version,created_at,valid_from,valid_to,superseded_by,forgotten)
         SELECT id,{account},realm,kind,text,embedding,{model},source_seq,
                {authoritative},{version},created_at,valid_from,valid_to,superseded_by,{forgotten}
         FROM mem_node_legacy"
    );
    transaction
        .execute(&sql, [default_account.as_str()])
        .map_err(database_error)?;
    transaction
        .execute_batch("DROP TABLE mem_node_legacy;")
        .map_err(database_error)
}

fn create_edges(transaction: &Transaction<'_>) -> Result<(), MemoryError> {
    transaction
        .execute_batch(
            "CREATE TABLE mem_edge(
               src INTEGER NOT NULL,
               dst INTEGER NOT NULL,
               kind TEXT NOT NULL,
               account_id TEXT NOT NULL,
               realm TEXT NOT NULL,
               source_seq INTEGER,
               valid_from INTEGER,
               valid_to INTEGER
             );",
        )
        .map_err(database_error)
}

fn migrate_edges(
    transaction: &Transaction<'_>,
    default_account: &AccountId,
) -> Result<(), MemoryError> {
    if !table_exists(transaction, "mem_edge")? {
        return create_edges(transaction);
    }
    let existing = columns(transaction, "mem_edge")?;
    if existing.iter().any(|column| column == "account_id") {
        return Ok(());
    }
    transaction
        .execute_batch("ALTER TABLE mem_edge RENAME TO mem_edge_legacy;")
        .map_err(database_error)?;
    create_edges(transaction)?;
    transaction
        .execute(
            "INSERT INTO mem_edge(src,dst,kind,account_id,realm,source_seq,valid_from,valid_to)
             SELECT src,dst,kind,?1,realm,source_seq,valid_from,valid_to FROM mem_edge_legacy",
            [default_account.as_str()],
        )
        .map_err(database_error)?;
    transaction
        .execute_batch("DROP TABLE mem_edge_legacy;")
        .map_err(database_error)
}

fn create_indexes(transaction: &Transaction<'_>) -> Result<(), MemoryError> {
    transaction
        .execute_batch(&format!(
            "CREATE INDEX IF NOT EXISTS mem_node_scope
               ON mem_node(account_id,realm,forgotten,embedding_model);
             CREATE INDEX IF NOT EXISTS mem_edge_src
               ON mem_edge(account_id,realm,src);
             CREATE INDEX IF NOT EXISTS mem_edge_dst
               ON mem_edge(account_id,realm,dst);
             CREATE TABLE IF NOT EXISTS mem_mutation(
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               node_id INTEGER NOT NULL,
               account_id TEXT NOT NULL,
               realm TEXT NOT NULL,
               operation TEXT NOT NULL,
               prior_version INTEGER NOT NULL,
               current_version INTEGER NOT NULL,
               prior_source TEXT NOT NULL,
               created_at INTEGER NOT NULL
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS mem_fts USING fts5(text);
             CREATE VIRTUAL TABLE IF NOT EXISTS mem_vec
               USING vec0(embedding int8[{EMBED_DIM}]);"
        ))
        .map_err(database_error)
}

fn rebuild_indexes(transaction: &Transaction<'_>, active_model: &str) -> Result<(), MemoryError> {
    transaction
        .execute("DELETE FROM mem_fts", [])
        .map_err(database_error)?;
    transaction
        .execute(
            "INSERT INTO mem_fts(rowid,text)
             SELECT id,text FROM mem_node WHERE forgotten=0",
            [],
        )
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM mem_vec", [])
        .map_err(database_error)?;
    let mut statement = transaction
        .prepare(
            "SELECT id,embedding FROM mem_node
             WHERE forgotten=0 AND embedding IS NOT NULL AND embedding_model=?1",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([active_model], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    drop(statement);
    for (id, embedding) in rows {
        if embedding.len() != EMBED_DIM {
            return Err(MemoryError::backend(format!(
                "stored embedding {id} has {} dimensions, expected {EMBED_DIM}",
                embedding.len()
            )));
        }
        transaction
            .execute(
                "INSERT INTO mem_vec(rowid,embedding) VALUES(?1,vec_int8(?2))",
                params![id, embedding],
            )
            .map_err(database_error)?;
    }
    Ok(())
}

pub(crate) fn database_error(error: rusqlite::Error) -> MemoryError {
    MemoryError::backend(format!("sqlite memory: {error}"))
}
