# Reuse: event log with cursor replay

What mote reuses for one append-only log that every lane, tool, watcher and window writes to,
and that any client replays from a cursor. Covers MOTE-02 and CRATE-04, CRATE-05, CRATE-07.
Researched 2026-10-08.

## Key points

1. One SQLite table is enough: `events(seq INTEGER PRIMARY KEY AUTOINCREMENT, ts, scope_id, group_id, kind, source, confidence, payload)` with an index on `(scope_id, seq)`. That is Overseer's table with two columns renamed.
2. Append is redact, `INSERT`, then publish on a tokio `broadcast` bus of capacity 4,096. Replay is `SELECT ... WHERE seq > ?1 ORDER BY seq LIMIT 1000`, optionally filtered by scope.
3. Exactly-once to a client is cursor arithmetic, not a protocol. Subscribe to the bus first, replay from the store, drop live events at or below the cursor, send `replayed`, send `resync` on lag. That is Overseer's `server.rs::subscribe`.
4. `AUTOINCREMENT` is required. Pruning deletes old rows, and a plain rowid could then be reused, which would move a client's cursor backwards.
5. PRAGMAs: `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=5000`, `foreign_keys=ON`, `journal_size_limit=67108864`, and a `wal_checkpoint(TRUNCATE)` every five minutes when no replay is running.
6. `rusqlite` 0.40 with `bundled`. Not sqlx, turso, redb, fjall, sled or okaywal. The reasons are in the table.
7. Retention is Overseer's: 5,000 events per scope, a `retention` marker row that is never deleted, and `oldest_retained` so a client can say "earlier output pruned".
8. The daemon knows nothing about clients. Each client owns its cursor and sends it on `events.subscribe`.

## What the concept needs technically

Schema, as `mote-eventlog::migrate` creates it:

```sql
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT);
CREATE TABLE IF NOT EXISTS events(
  seq        INTEGER PRIMARY KEY AUTOINCREMENT,
  ts         INTEGER NOT NULL,
  scope_id   TEXT,            -- a thread in mote, a run in Overseer
  group_id   TEXT,            -- NULL in mote, a task in Overseer
  kind       TEXT NOT NULL,
  source     TEXT NOT NULL,
  confidence TEXT NOT NULL,
  payload    TEXT NOT NULL    -- JSON object, redacted before insert
);
CREATE INDEX IF NOT EXISTS events_scope ON events(scope_id, seq);
```

The Overseer migration renames `run_id` to `scope_id` and `task_id` to `group_id` with `ALTER TABLE ... RENAME COLUMN` once, which SQLite has supported since 3.25.

Connection setup, once per open:

```sql
PRAGMA journal_mode=WAL;
PRAGMA synchronous=NORMAL;
PRAGMA busy_timeout=5000;
PRAGMA foreign_keys=ON;
PRAGMA journal_size_limit=67108864;
PRAGMA wal_autocheckpoint=1000;
```

`synchronous=NORMAL` in WAL mode keeps the file consistent after power loss but may roll back the last commits since the last WAL sync. mote accepts that for events. The ledger and context tables live in the same file and inherit the same guarantee.

Append path. `redact_value(payload)`, then `INSERT` inside the caller's transaction, then `Event { seq: last_insert_rowid(), .. }` goes to `Bus::publish`. A consumer that writes a ledger row and an event in one transaction gets both or neither.

Replay path, from `server.rs`:

1. `let mut live = bus.subscribe()` before touching the store.
2. `oldest = min_seq()`. If `oldest > cursor + 1` and `cursor > 0`, reply `history_truncated: true`.
3. Loop `events_after(cursor, scope, 1000)` until empty, sending each as `{"method":"event","params":e}` and advancing the cursor.
4. Send `{"method":"replayed","params":{"cursor"}}`.
5. Forward live events with `seq > cursor` and the scope filter. On `RecvError::Lagged`, send `{"method":"resync","params":{"cursor"}}` and stop. The client resubscribes from that cursor.

Client rules. Keep `cursor` as the highest seq applied. Ignore any event with `seq <= cursor`. On reconnect send `after: cursor`. On `resync` resubscribe with the same cursor. Overseer's TUI client does exactly this with a `compare_exchange` on first `state`.

Retention. After every append to a scope, when `count % 500 == 0`, run `prune_scope(scope, 5000)`. It deletes the oldest rows past 5,000 except `kind = 'retention'` and inserts one marker with `events_truncated_through_seq`. A client that replays from below the marker sees it first.

Payload limits. Payloads over 256 KiB are truncated to the first 256 KiB of their JSON text with `"truncated": true` added. Text fields inside harness output are already cut at 16 KiB by `mote-harness`.

Checkpoints. A long replay holds a read transaction and blocks checkpoint progress. The checkpoint task runs `PRAGMA wal_checkpoint(TRUNCATE)` every five minutes only when no subscribe loop is inside a batch, so the WAL never grows without bound.

## Candidates

| name | repo | language | license | last release | verdict | reason | pin |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Overseer `store.rs` events, `server.rs` subscribe | ~/projects/overseer | Rust | MIT | a7a5cac0 | port into `mote-eventlog` and `mote-ipc` | Proven by `ac10_replay_cursor_burst_and_retention` with 12,000 appends and a mid-cursor reconnect. | a7a5cac0 |
| rusqlite | github.com/rusqlite/rusqlite | Rust | MIT | 0.40.2, 2026-08-08 | embed | Sync API, one writer, `bundled` ships SQLite 3.53.2, no runtime dependency. Both daemons use it. | 0.40.2 |
| sqlx (sqlite) | github.com/launchbadge/sqlx | Rust | MIT or Apache-2.0 | 0.9.0, 2026-05-21 | skip | Pooled connections contend on SQLite's single writer lock. Default `synchronous=FULL`. One thread per connection. | none |
| turso | github.com/tursodatabase/turso | Rust | MIT | 0.8.2, 2026-10-06 | skip | Pre-1.0. No `synchronous=NORMAL`. Cannot share a file with SQLite processes. Worth a look after 1.0. | none |
| libsql | github.com/tursodatabase/libsql | Rust | MIT | 0.9.30 | skip | New work goes into turso. | none |
| redb | github.com/cberner/redb | Rust | MIT or Apache-2.0 | 4.3.0, 2026-09-15 | skip, port the idea | Good ordered `u64` keys and range deletes, but no SQL for the ledger and context joins, and a second store. | none |
| fjall | github.com/fjall-rs/fjall | Rust | MIT or Apache-2.0 | 3.1.12, 2026-10-03 | skip | LSM tombstones defer space reclaim. Overkill for tens of events per second. | none |
| sled | github.com/spacejam/sled | Rust | MIT or Apache-2.0 | 1.0.0-alpha.124, 2024-10-11 | skip | Beta, on-disk format still changes. | none |
| okaywal | github.com/khonsulabs/okaywal | Rust | MIT or Apache-2.0 | 0.3.1, 2023-11-26 | skip | README says do not use in production. | none |
| rocksdb | github.com/rust-rocksdb/rust-rocksdb | Rust | Apache-2.0 | 0.25.0, 2026-08-16 | skip | Large C++ build for a small log. | none |
| heed (LMDB) | github.com/meilisearch/heed | Rust | MIT | 0.22.1, 2026-04-07 | skip | Fixed map size and mmap crash semantics. | none |
| cqrs-es plus sqlite-es | github.com/serverlesstechnology/cqrs | Rust | Apache-2.0 | 0.5.0, 2026-04-23 | skip | Aggregate-scoped event stores on sqlx. No global seq cursor or pruning. | none |
| tokio `broadcast` | github.com/tokio-rs/tokio | Rust | MIT | 1.53.2 | embed | Live fan-out with `Lagged` as the resync signal. | 1.53 |

## Recommendation

```toml
# crates/mote-eventlog/Cargo.toml
[dependencies]
anyhow = "1"
rusqlite = { version = "0.40.2", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1.53", features = ["sync"] }
mote-redact = { path = "../mote-redact" }

[features]
overseer-wire = []   # serde renames scope_id to run_id and group_id to task_id
```

Checks against a live store:

```bash
sqlite3 "$HOME/Library/Application Support/mote/mote.sqlite" \
  'PRAGMA journal_mode; PRAGMA synchronous; SELECT COUNT(*), MIN(seq), MAX(seq) FROM events;'
ls -la "$HOME/Library/Application Support/mote/"mote.sqlite*      # the -wal file size
moted ctl events.subscribe '{"after":0,"scope_id":"th-xxxx"}' | head -20
```

## Efficiency targets and what to measure

- Append: p50 under 1 ms, 10,000 single-row autocommit appends per second. Published numbers for WAL plus `NORMAL` are 33,000 to 44,000 per second on laptop SSDs.
- Replay: 1,000 rows under 20 ms from a warm cache.
- Bus: zero `Lagged` events in normal use. Count them as `resync_sent` in a counter the window can show.
- WAL: the `-wal` file stays under 64 MB after 100,000 appends with the checkpoint task running.
- Size: about 60 MB per 100,000 events at a 500 byte average payload. Measure with `moted bench eventlog`.
- Prune: 5,000-row prune under 50 ms.

## Open decisions with defaults

1. `synchronous`. Default: `NORMAL`. `FULL` costs one extra sync per commit and the events are re-derivable from the harness segment files for the think and work lanes.
2. One file or two. Default: one `mote.sqlite` with events, context and ledger, behind one writer connection and a mutex. Cross-table transactions matter more than file separation.
3. Retention. Default: count-based, 5,000 per scope, with markers. Time-based pruning of closed threads is a later decision.
4. Payload format. Default: JSON text, as Overseer. SQLite 3.45+ JSONB saves space but complicates `sqlite3` inspection.
5. Bus capacity. Default: 4,096.
6. Client cursor storage. Default: the client owns it. The Tauri window keeps it in its own settings file. The daemon stores cursors only for its own followers, such as the Overseer mirror, under `meta`.
7. Global or per-scope seq. Default: one global seq with a scope filter.
8. Checkpoint policy. Default: `TRUNCATE` every five minutes when no replay batch is running, plus `journal_size_limit` 64 MB.
9. Prune trigger. Default: every 500th append to a scope, inline. A background sweeper is not needed at mote's rates.
10. Event schema versioning. Default: a `meta` key `schema_version` and additive migrations only, as Overseer does.

## Acceptance criteria

- **LOG-01** `migrate` creates the schema above and renames Overseer's `run_id` and `task_id` columns once when they exist. **Verify:** `cargo test -p mote-eventlog migrate` opens a copy of Overseer's schema and reads `pragma_table_info`.
- **LOG-02** `append` redacts the payload and returns a seq strictly greater than every earlier seq, including after a prune. **Verify:** test prunes to 10, appends, and asserts `seq > old max`.
- **LOG-03** Replay after any cursor returns every retained event once, in order, with no gap, after a 12,000 append burst. **Verify:** `cargo test -p mote-eventlog replay_burst`, ported from Overseer's ac10 (CRATE-04).
- **LOG-04** A subscriber on a capacity-1 bus sees `Lagged`, resumes with `after(cursor)`, and ends with every seq once. **Verify:** `cargo test -p mote-eventlog lag` (CRATE-05).
- **LOG-05** `subscribe` sends replay, `replayed`, then live events, delivers each seq exactly once when the connection is cut at 100 random points, and sends `resync` on lag. **Verify:** `cargo test -p mote-ipc subscribe_cuts` with a seeded RNG.
- **LOG-06** The connection applies the six PRAGMAs. **Verify:** a test reads `PRAGMA journal_mode`, `synchronous`, `busy_timeout`, `foreign_keys`, `journal_size_limit`, `wal_autocheckpoint`.
- **LOG-07** `prune_scope` keeps 5,000 rows, never deletes `retention` markers, writes `events_truncated_through_seq`, and `oldest_retained` reports the new minimum. **Verify:** unit test over 12,000 appends.
- **LOG-08** The WAL file is under 64 MB after 100,000 appends with the checkpoint task running. **Verify:** test checks `mote.sqlite-wal` size.
- **LOG-09** Append p50 under 1 ms and 10,000 per second, replay of 1,000 under 20 ms. **Verify:** `moted bench eventlog` output in the evidence record.
- **LOG-10** A window closed mid-turn and reopened shows every event once. **Verify:** live check with the Tauri window (MOTE-02).
- **LOG-11** A process killed with SIGKILL mid-append leaves a database that passes `PRAGMA integrity_check` and holds every committed seq. **Verify:** test spawns a child that appends in a loop, kills it, reopens.
- **LOG-12** A payload over 256 KiB is stored truncated with `truncated: true`. **Verify:** unit test with a 300 KiB payload.

## Sources

- SQLite synchronous: https://sqlite.org/pragma.html#pragma_synchronous
- SQLite WAL and large WAL files: https://sqlite.org/wal.html#avoiding_excessively_large_wal_files
- SQLite wal_checkpoint: https://sqlite.org/pragma.html#pragma_wal_checkpoint
- SQLite wal_autocheckpoint: https://sqlite.org/pragma.html#pragma_wal_autocheckpoint
- SQLite AUTOINCREMENT: https://sqlite.org/autoinc.html
- SQLite auto_vacuum: https://sqlite.org/pragma.html#pragma_auto_vacuum
- SQLite VACUUM: https://sqlite.org/lang_vacuum.html
- SQLite mmap: https://sqlite.org/mmap.html
- SQLite insert throughput FAQ: https://sqlite.org/faq.html#q19
- SQLite durability write-up: https://www.agwa.name/blog/post/sqlite_durability
- WAL plus NORMAL measurements: https://travishorn.com/a-hands-on-exploration-of-sqlite-for-production/
- 100k TPS over a billion rows: https://andersmurphy.com/2025/12/02/100000-tps-over-a-billion-rows-the-unreasonable-effectiveness-of-sqlite.html
- rusqlite: https://github.com/rusqlite/rusqlite
- sqlx SQLite: https://docs.rs/sqlx/latest/sqlx/sqlite/index.html
- sqlx write transactions footgun: https://emschwartz.me/psa-write-transactions-are-a-footgun-with-sqlx-and-sqlite
- turso: https://github.com/tursodatabase/turso and https://raw.githubusercontent.com/tursodatabase/turso/main/COMPAT.md
- libsql: https://github.com/tursodatabase/libsql
- redb: https://github.com/cberner/redb
- fjall: https://github.com/fjall-rs/fjall
- sled: https://github.com/spacejam/sled
- okaywal: https://github.com/khonsulabs/okaywal
- rocksdb: https://github.com/rust-rocksdb/rust-rocksdb
- heed: https://github.com/meilisearch/heed
- cqrs-es: https://crates.io/crates/cqrs-es and https://crates.io/crates/sqlite-es
- tokio broadcast: https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html
- Idempotent consumer: https://microservices.io/patterns/communication-style/idempotent-consumer.html
- Idempotent receiver: https://martinfowler.com/articles/patterns-of-distributed-systems/idempotent-receiver.html
- Low-water mark: https://martinfowler.com/articles/patterns-of-distributed-systems/low-watermark.html
- NATS JetStream consumers: https://docs.nats.io/nats-concepts/jetstream/consumers
- Kafka consumer offsets: https://kafka.apache.org/43/javadoc/org/apache/kafka/clients/consumer/KafkaConsumer.html
- You cannot have exactly-once delivery: https://bravenewgeek.com/you-cannot-have-exactly-once-delivery/
- Overseer `daemon/src/store.rs`, `server.rs`, `tui/src/client.rs`, `daemon/tests/protocol.rs` at `a7a5cac0` (local, read-only)
