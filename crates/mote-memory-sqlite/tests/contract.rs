use mote_memory::{
    run_contract_suite, AccountId, Correction, MemoryBackend, MemoryScope, MemoryWrite,
};
use mote_memory_sqlite::{HashEmbedder, SqliteMemory};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};

fn temp_db(label: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "mote-memory-sqlite-{label}-{}-{}.db",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn sqlite_satisfies_common_contract() {
    run_contract_suite(|| {
        Box::new(
            SqliteMemory::open_in_memory(
                Box::new(HashEmbedder::new()),
                AccountId::new("account-a").unwrap(),
            )
            .unwrap(),
        )
    });
}

#[test]
fn write_rolls_back_node_text_and_vector_when_indexing_fails() {
    let path = temp_db("rollback");
    drop(
        SqliteMemory::open(
            &path,
            Box::new(HashEmbedder::new()),
            AccountId::new("account-a").unwrap(),
        )
        .unwrap(),
    );
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_after_node AFTER INSERT ON mem_node
             BEGIN SELECT RAISE(ABORT, 'injected index failure'); END;",
        )
        .unwrap();
    drop(connection);

    let mut memory = SqliteMemory::open(
        &path,
        Box::new(HashEmbedder::new()),
        AccountId::new("account-a").unwrap(),
    )
    .unwrap();
    let scope = MemoryScope::new("account-a", "personal").unwrap();
    assert!(memory
        .write(&scope, MemoryWrite::new("must roll back").unwrap())
        .is_err());
    drop(memory);

    let connection = Connection::open(&path).unwrap();
    let nodes: i64 = connection
        .query_row("SELECT count(*) FROM mem_node", [], |row| row.get(0))
        .unwrap();
    let text_rows: i64 = connection
        .query_row("SELECT count(*) FROM mem_fts", [], |row| row.get(0))
        .unwrap();
    let vector_rows: i64 = connection
        .query_row("SELECT count(*) FROM mem_vec", [], |row| row.get(0))
        .unwrap();
    assert_eq!((nodes, text_rows, vector_rows), (0, 0, 0));
    drop(connection);
    std::fs::remove_file(path).ok();
}

#[test]
fn source_attribution_is_stored_with_the_scoped_record() {
    let path = temp_db("source");
    let scope = MemoryScope::new("account-a", "work").unwrap();
    let mut memory = SqliteMemory::open(
        &path,
        Box::new(HashEmbedder::new()),
        AccountId::new("account-a").unwrap(),
    )
    .unwrap();
    let hit = memory
        .write(
            &scope,
            MemoryWrite::new("sourced fact")
                .unwrap()
                .with_authoritative_source("event:42")
                .unwrap(),
        )
        .unwrap();
    drop(memory);
    let connection = Connection::open(&path).unwrap();
    let stored: (String, String, String) = connection
        .query_row(
            "SELECT account_id, realm, authoritative_source FROM mem_node WHERE id=?1",
            [hit.id().as_str().parse::<i64>().unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        stored,
        ("account-a".into(), "work".into(), "event:42".into())
    );
    drop(connection);
    std::fs::remove_file(path).ok();
}

#[test]
fn correction_and_forget_keep_pointers_but_remove_searchable_text() {
    let path = temp_db("audit");
    let scope = MemoryScope::new("account-a", "personal").unwrap();
    let mut memory = SqliteMemory::open(
        &path,
        Box::new(HashEmbedder::new()),
        AccountId::new("account-a").unwrap(),
    )
    .unwrap();
    let hit = memory
        .write(&scope, MemoryWrite::new("old private phrase").unwrap())
        .unwrap();
    let corrected = memory
        .correct(
            &scope,
            Correction::new(hit.id().clone(), 1, "new retained phrase").unwrap(),
        )
        .unwrap();
    memory
        .forget(&scope, hit.id(), corrected.current().version())
        .unwrap();
    drop(memory);

    let connection = Connection::open(&path).unwrap();
    let row: (String, Option<Vec<u8>>, i64) = connection
        .query_row(
            "SELECT text,embedding,forgotten FROM mem_node WHERE id=?1",
            [hit.id().as_str().parse::<i64>().unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let operations: Vec<(String, String)> = connection
        .prepare("SELECT operation,prior_source FROM mem_mutation ORDER BY id")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(row, (String::new(), None, 1));
    assert_eq!(operations.len(), 2);
    assert_eq!(operations[0].0, "correct");
    assert_eq!(operations[1].0, "forget");
    assert!(operations
        .iter()
        .all(|(_, source)| source.starts_with("memory:sqlite:")));
    let fts_rows: i64 = connection
        .query_row("SELECT count(*) FROM mem_fts", [], |row| row.get(0))
        .unwrap();
    let vector_rows: i64 = connection
        .query_row("SELECT count(*) FROM mem_vec", [], |row| row.get(0))
        .unwrap();
    assert_eq!((fts_rows, vector_rows), (0, 0));
    drop(connection);
    std::fs::remove_file(path).ok();
}

#[test]
fn competing_corrections_have_exactly_one_winner() {
    let path = temp_db("correction-race");
    let scope = MemoryScope::new("account-a", "personal").unwrap();
    let id = {
        let mut memory = SqliteMemory::open(
            &path,
            Box::new(HashEmbedder::new()),
            AccountId::new("account-a").unwrap(),
        )
        .unwrap();
        memory
            .write(&scope, MemoryWrite::new("original value").unwrap())
            .unwrap()
            .id()
            .clone()
    };
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for replacement in ["first replacement", "second replacement"] {
        let mut memory = SqliteMemory::open(
            &path,
            Box::new(HashEmbedder::new()),
            AccountId::new("account-a").unwrap(),
        )
        .unwrap();
        let scope = scope.clone();
        let id = id.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            memory.correct(&scope, Correction::new(id, 1, replacement).unwrap())
        }));
    }
    barrier.wait();
    let results = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(mote_memory::MemoryError::Conflict { .. })))
            .count(),
        1
    );
    std::fs::remove_file(path).ok();
}

#[test]
fn competing_correction_and_forget_have_exactly_one_winner() {
    let path = temp_db("mutation-race");
    let scope = MemoryScope::new("account-a", "personal").unwrap();
    let id = {
        let mut memory = SqliteMemory::open(
            &path,
            Box::new(HashEmbedder::new()),
            AccountId::new("account-a").unwrap(),
        )
        .unwrap();
        memory
            .write(&scope, MemoryWrite::new("original value").unwrap())
            .unwrap()
            .id()
            .clone()
    };
    let barrier = Arc::new(Barrier::new(3));
    let correction = {
        let mut memory = SqliteMemory::open(
            &path,
            Box::new(HashEmbedder::new()),
            AccountId::new("account-a").unwrap(),
        )
        .unwrap();
        let scope = scope.clone();
        let id = id.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            memory
                .correct(&scope, Correction::new(id, 1, "replacement value").unwrap())
                .map(|_| ())
        })
    };
    let forgetting = {
        let mut memory = SqliteMemory::open(
            &path,
            Box::new(HashEmbedder::new()),
            AccountId::new("account-a").unwrap(),
        )
        .unwrap();
        let scope = scope.clone();
        let id = id.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            memory.forget(&scope, &id, 1).map(|_| ())
        })
    };
    barrier.wait();
    let results = [correction.join().unwrap(), forgetting.join().unwrap()];
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(mote_memory::MemoryError::Conflict { .. })))
            .count(),
        1
    );
    std::fs::remove_file(path).ok();
}
