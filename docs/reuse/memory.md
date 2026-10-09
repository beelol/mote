# Reuse: memory for mote

Status: proposed on 2026-10-08 from the research in [memory-research.md](../research/memory-research.md). Acceptance criteria MEM-01 to MEM-14 are at the end. The product RFC is [mote](../rfcs/mote.md). The store this document extends is the context store in MOTE-17 to MOTE-19.

## Key points

1. None of the ten projects in the post can be embedded in a Rust daemon. Eight are Python, two are TypeScript, and every one needs its own server, database or LLM-per-write pipeline. All ten are sidecar-only, and a sidecar costs a Python or Node runtime plus a database on the Pi.
2. mote keeps the graph inside its existing SQLite file: nodes, bi-temporal edges, FTS5 for keywords, `sqlite-vec` for vectors. No second database.
3. Embeddings come from `model2vec-rs` with `potion-base-8M` (256 dims, 30 MB, static lookup). A memory write costs one transaction and no model call.
4. Graph walks run in process with `petgraph` over a loaded neighbourhood. Retrieval fuses vector, keyword, graph and recency with reciprocal rank fusion under a per-lane token budget.
5. Facts are edges with four timestamps, as in Graphiti and the Zep paper. Contradictions expire the old edge and never delete it.
6. Extraction of entities and facts happens only where an LLM already runs: the think lane's memory tool, and an idle-time reflect job on the local model. Both emit structured verdicts parsed fail-closed.
7. Every dependency is pinned to an exact crates.io version or a git `tag` locked in `Cargo.lock`. Forks live under the owner's GitHub with an `upstream` remote and are applied with `[patch]`.
8. The benchmark replays LongMemEval and LoCoMo sessions into mote's own event log and scores retrieval by evidence recall, with no judge and no paid turns.

## The memory model for mote

Four kinds of node, one kind of edge, one event-log pointer on everything.

| Node kind | Holds | Time fields | Example |
| --- | --- | --- | --- |
| `entity` | a person, project, tool, place or concept with a canonical key | `first_seen`, `last_seen` | `project:monarch` |
| `event` | one episodic record: an utterance, a tool result, a watcher notice, a card action | `occurred_at` | "owner asked about PR 1141" |
| `procedure` | a how-to: trigger, steps, last outcome | `last_used`, `success_count` | "to check a PR run `gh pr view`" |
| `summary` | a compaction of events in one thread or day | `covers_from`, `covers_to` | "2026-10-08 morning thread" |

Every node carries `source_seq`, the event-log sequence number that produced it, and `thread_id` when it belongs to a thread. Nothing in memory exists without a pointer back to the log.

Edges carry facts and time:

```sql
CREATE TABLE mem_edge (
  id         INTEGER PRIMARY KEY,
  src        INTEGER NOT NULL REFERENCES mem_node(id),
  dst        INTEGER NOT NULL REFERENCES mem_node(id),
  rel        TEXT    NOT NULL,          -- 'mentions', 'prefers', 'works_on', 'lives_in', free text allowed
  fact       TEXT    NOT NULL,          -- one sentence in the owner's words
  valid_from INTEGER NOT NULL,          -- when the fact became true (unix ms)
  valid_to   INTEGER,                   -- when it stopped being true, NULL while current
  created_at INTEGER NOT NULL,          -- when mote learned it
  expired_at INTEGER,                   -- when mote stopped believing it, NULL while believed
  superseded_by INTEGER REFERENCES mem_edge(id),
  source_seq INTEGER NOT NULL,          -- event log row that produced this edge
  confidence REAL    NOT NULL DEFAULT 1.0
);
```

`valid_from` and `valid_to` are the world's timeline. `created_at` and `expired_at` are mote's timeline. "What did mote believe on date D" is `created_at <= D AND (expired_at IS NULL OR expired_at > D)`. "What was true on date D" is `valid_from <= D AND (valid_to IS NULL OR valid_to > D)`.

Episodic edges (`event` mentions `entity`) are written on every event with no LLM. Semantic edges (`entity` to `entity`) are written only by the memory tool or the reflect job.

Contradiction. A new edge with the same `src` and `rel` whose `rel` is declared functional (`lives_in`, `prefers`, `works_at`, `current_task`) closes the old edge: `valid_to = new.valid_from`, `expired_at = now`, `superseded_by = new.id`. For free-text facts the think lane's memory tool decides with a verdict `{"action": "add" | "supersede" | "noop", "target": <edge id or null>, "reason": "..."}`. Anything malformed parses as `noop` and shows a card.

Forgetting. Nothing is deleted. A node's rank decays with `exp(-age / half_life)` where `half_life` is 30 days for events and 180 days for entities and procedures, multiplied by `ln(2 + mentions)`. Events older than 180 days in a thread compact into one `summary` node that keeps their `source_seq` range. Compaction runs only with the owner's approval in the window, and the originals stay queryable by sequence.

Working set. Each thread has `mem_working(thread_id, node_id, pinned, added_seq)`. The working set is the first thing in every pack for that thread. It holds the entities and facts the thread touched, capped at 64 rows, evicted by `added_seq` unless pinned.

Retrieval composes four signals under a token budget:

1. Embed the query with model2vec (under 1 ms) and tokenize it for FTS5.
2. Candidates: `vec0` KNN top 40 over the durable partition, FTS5 BM25 top 40, and entity seeds by canonical key.
3. Graph walk (think lane only): load edges within two hops of the seeds that are valid now, at most 200 edges, into a petgraph graph, and run Personalized PageRank seeded by the entities for 20 iterations. petgraph 0.8 ships only plain `algo::page_rank(graph, damping, iterations)` with no seed vector, so the personalized loop is about twenty lines in `mote-memory` over petgraph's graph and neighbour iteration.
4. Fuse with reciprocal rank fusion (`k = 60`) across the three or four lists, then multiply by the decay score and a 1.5x boost for working-set members.
5. Pack greedily by fused score until the lane's budget is full, estimating 4 characters per token, then record `(pack_id, node and edge ids, scores, token estimate)` in the ledger row for the call.

The talk lane skips step 3 and stops at 8 items or 600 tokens with a 40 ms deadline. The think lane runs every step with the budget from MOTE-18 and a 250 ms deadline. A deadline miss returns what is ready and marks the pack `partial`.

Storage lives in the existing store:

```sql
CREATE VIRTUAL TABLE mem_fts USING fts5(name, body, fact, content='', tokenize='unicode61');
CREATE VIRTUAL TABLE mem_vec USING vec0(
  node_id INTEGER PRIMARY KEY,
  tier TEXT PARTITION KEY,              -- 'durable' or 'working'
  embedding int8[256] distance_metric=cosine
);
```

## Candidates table

| Name | Repo | Language | License | Decision | Reason | Pinned start |
| --- | --- | --- | --- | --- | --- | --- |
| Mem0 | github.com/mem0ai/mem0 | Python | Apache-2.0 | port the idea | Two LLM calls per write and an external vector store. Keep the extract-then-update verdict shape for the memory tool. | n/a |
| Hindsight | github.com/vectorize-io/hindsight | Python | MIT | port the idea | Needs PostgreSQL, pgvector and an LLM at retain. Keep the four-way recall fusion and the token-bounded pack. | n/a |
| memU | github.com/NevaMind-AI/memU | Python | Apache-2.0 | port the idea | Its `MemoryService` makes no LLM call, which proves the no-LLM write path. SQLite brute-force cosine is its local default. | n/a |
| Cognee | github.com/topoteretes/cognee | Python | Apache-2.0 | skip | LanceDB plus Ladybug plus SQLite, and GLiNER extraction without an LLM is the one idea to revisit later. | n/a |
| Graphiti | github.com/getzep/graphiti | Python | Apache-2.0 | port the idea | Needs Neo4j or FalkorDB and LLM calls per episode. Keep the bi-temporal edge and the invalidation rule. | n/a |
| OpenViking | github.com/volcengine/OpenViking | Python, some Rust | AGPL-3.0 | skip | AGPL on the engine, VLM required, server only. L0/L1/L2 tiers are a good shape for summaries. | n/a |
| Letta and Letta Code | github.com/letta-ai/letta-code | TypeScript | Apache-2.0 | port the idea | Needs the Letta server or cloud. Keep memory blocks as the per-thread working set and the self-edit tool shape. | n/a |
| OpenMemory (LongMemory) | github.com/CaviraOSS/LongMemory | TypeScript | Apache-2.0 | port the idea | Node runtime. Its SQLite schema with immutable content and temporal truth is the closest relative of mote's store. Mem0's OpenMemory MCP was removed from the monorepo on 2026-07-29. | n/a |
| Agent Memory Benchmark | github.com/vectorize-io/agent-memory-benchmark | Python | unknown | port the idea | Needs a Gemini key. Reuse its dataset loaders and retrieval-only scoring. | n/a |
| sqlite-vec | github.com/asg017/sqlite-vec | C | MIT or Apache-2.0 | embed | Vectors inside the existing SQLite file. Brute force is fine to 100k rows. Pre-v1, so pin exactly. | crate `=0.1.9` |
| model2vec-rs | github.com/MinishLab/model2vec-rs | Rust | MIT | embed | Static embeddings, 256 dims, no network at run time with `local-only`. | crate `=0.3.0`, model `potion-base-8M` by content hash |
| petgraph | github.com/petgraph/petgraph | Rust | MIT or Apache-2.0 | embed | In-memory walks and PageRank over a loaded neighbourhood. | crate `=0.8.3` |
| rusqlite bundled | github.com/rusqlite/rusqlite | Rust | MIT | embed (already) | Bundled SQLite ships FTS5, JSON1 and extension loading. | per shared-crates RFC, `0.32` |
| hnsw_rs | github.com/jean-pierreBoth/hnswlib-rs | Rust | MIT or Apache-2.0 | later | Pure Rust ANN for when brute force misses the budget above 100k vectors. | crate `=0.3.4` |
| fastembed-rs | github.com/Anush008/fastembed-rs | Rust | Apache-2.0 | later | ONNX bge-small for quality and a cross-encoder reranker for the think lane. Pulls ONNX Runtime. | crate `=7.1.1` |
| candle | github.com/huggingface/candle | Rust | Apache-2.0 or MIT | later | Pure Rust alternative to ONNX Runtime if the Pi build of `ort` is a problem. | crate `=0.11.0` |
| usearch | github.com/unum-cloud/USearch | C++ | Apache-2.0 | skip | C++ build in the daemon. hnsw_rs covers the need. | n/a |
| tantivy | github.com/quickwit-oss/tantivy | Rust | MIT | skip | FTS5 is already compiled in. Revisit only for ranking features. | n/a |
| LadybugDB (ex Kuzu) | github.com/LadybugDB/ladybug | C++ | MIT | later | The fallback if SQLite tables cannot hold the graph. Rust crate `lbug`. Kuzu itself is archived. | crate `=0.21.2` |
| cozo | github.com/cozodb/cozo | Rust | MPL-2.0 | skip | No upstream commit since 2024-12. | n/a |
| oxigraph | github.com/oxigraph/oxigraph | Rust | Apache-2.0 | skip | RDF and SPARQL do not fit a temporal fact graph without reification. No vectors. | n/a |
| surrealdb embedded | github.com/surrealdb/surrealdb | Rust | BSL 1.1 | skip | Minutes-long build, source-available license, second store. | n/a |
| HelixDB | github.com/HelixDB/helix-db | Rust | Apache-2.0 | skip | Embedded mode documented but not on crates.io at 3.0.0. Revisit. | n/a |
| LanceDB | github.com/lancedb/lancedb | Rust | Apache-2.0 | skip | Arrow and DataFusion in the daemon. Too heavy for the Pi. | n/a |
| sqlite-vss, indradb, sled, redb, fjall | various | various | various | skip | Superseded, inactive, or a second storage engine beside SQLite. | n/a |

## Reuse decisions with Cargo.toml snippets

Workspace pins. Exact versions on crates.io crates, so `Cargo.lock` and `Cargo.toml` agree and a bump is a visible diff.

```toml
# Cargo.toml (workspace root)
[workspace.dependencies]
rusqlite     = { version = "0.32", features = ["bundled"] }   # per shared-crates RFC
sqlite-vec   = "=0.1.9"
model2vec-rs = { version = "=0.3.0", default-features = false, features = ["local-only", "onig"] }
petgraph     = { version = "=0.8.3", default-features = false }
```

`crates/mote-memory/Cargo.toml`:

```toml
[package]
name = "mote-memory"
version = "0.1.0"
edition = "2021"
license = "MIT"

[dependencies]
rusqlite     = { workspace = true }
sqlite-vec   = { workspace = true }
model2vec-rs = { workspace = true }
petgraph     = { workspace = true }
serde        = { version = "1", features = ["derive"] }
serde_json   = "1"

[features]
default = []
ann      = ["dep:hnsw_rs"]          # later, above 100k vectors
rerank   = ["dep:fastembed"]        # later, think lane cross-encoder

[dependencies.hnsw_rs]
version = "=0.3.4"
optional = true

[dependencies.fastembed]
version = "=7.1.1"
optional = true
default-features = false
```

Registering sqlite-vec once per process, before any connection opens:

```rust
use rusqlite::ffi::sqlite3_auto_extension;
use sqlite_vec::sqlite3_vec_init;

pub fn install_vec() {
    // SAFETY: sqlite3_vec_init has the extension entry-point ABI SQLite expects.
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
    }
}
```

The embedding model ships in the data directory, not from the network:

```rust
let model = model2vec_rs::StaticModel::from_pretrained(data_dir.join("models/potion-base-8M"), None, None, None)?;
```

`moted install` copies the model into the data directory and records its SHA-256 in `meta`. Startup refuses a model whose hash does not match.

## Pinning and update policy

1. crates.io crates: exact pins (`=x.y.z`) in `[workspace.dependencies]`. Renovate's cargo manager (datasource `crate`) opens one pull request per bump. Dependabot does the same. Merge only after `cargo test --workspace` and `moted bench memory` are green.
2. git dependencies: only for a fork that carries a local patch. Pin with `tag`, never a bare `branch`, so `Cargo.lock` locks the SHA and Dependabot can still bump a version-like tag. Dependabot's cargo checker bumps a pin only when the ref looks like a version. Renovate's cargo manager lists `git-tags` and `git-refs` datasources. Confirm both with a dry run before relying on them.
3. Forks: `github.com/beelol/<upstream-name>` with remotes `origin` (the fork) and `upstream`. Patches live on branch `mote-patches` rebased onto the upstream tag, and each release of the fork is tagged `v<upstream>-mote.<n>`, for example `v0.1.9-mote.1`.

```toml
# Cargo.toml (workspace root), only while a local patch is needed
[patch.crates-io]
sqlite-vec = { git = "https://github.com/beelol/sqlite-vec", tag = "v0.1.9-mote.1" }
```

4. Rebase flow for a fork:

```sh
git fetch upstream --tags
git checkout mote-patches
git rebase v0.1.10                 # the new upstream tag
cargo test --workspace             # in the mote checkout, with [patch] pointing at ../sqlite-vec by path
git tag v0.1.10-mote.1 && git push origin mote-patches --tags
```

Then in mote: edit the `tag` in `[patch.crates-io]`, run `cargo update -p sqlite-vec`, commit `Cargo.toml` and `Cargo.lock` together.

5. Single-crate bumps: `cargo update -p <crate> --precise <version-or-sha>`. Never run bare `cargo update` in a feature branch.
6. Vendoring: `cargo vendor --versioned-dirs vendor/` is run only to produce an offline build for the Pi. The `vendor/` directory is not committed. It covers git dependencies as well as registry crates.
7. Model weights are pinned by content hash in `meta`, not by Hugging Face revision, and `model2vec-rs` is built with `local-only` so no code path can download.
8. Upstream watching: a weekly cron watcher (MOTE-21) lists new tags on the pinned repos and posts one card. No automatic merge.

## Efficiency targets and what to measure

Targets are for 10,000 memories (nodes) with about 30,000 edges on the owner's Mac. Pi targets apply once DEF-01 is active. Every target is measured by `moted bench memory`, which prints p50 and p95 over 200 operations and writes the evidence record.

| Measure | Mac target | Pi 5 target | How |
| --- | --- | --- | --- |
| Episodic write (embed + 3 inserts, one transaction) | p50 <= 5 ms, p95 <= 15 ms | p50 <= 20 ms | `bench memory write` |
| Model calls per episodic write | 0 | 0 | ledger `SELECT COUNT(*)` during the bench |
| Talk lane retrieval (vec + FTS, 8 items) | p50 <= 40 ms, p95 <= 80 ms | p95 <= 120 ms | `bench memory retrieve --lane talk` |
| Think lane retrieval (full composition) | p95 <= 250 ms | p95 <= 600 ms | `bench memory retrieve --lane think` |
| Vector scan alone (10k x int8[256]) | p50 <= 10 ms | p50 <= 40 ms | `bench memory vec` |
| Database size per 10,000 memories | <= 50 MB | same | `PRAGMA page_count * page_size` after `VACUUM` |
| Resident memory added by mote-memory | <= 100 MB | <= 100 MB | RSS delta after warm-up |
| Embedding model load | <= 200 ms | <= 1 s | time to first `encode` |

Size estimate behind the 50 MB target: 10,000 nodes at 300 bytes of text (3 MB), FTS5 index at about 1.5x the text (4.5 MB), 10,000 int8 vectors of 256 dims (2.6 MB), 30,000 edges at 200 bytes (6 MB), indexes and overhead (about 10 MB). About 26 MB expected. Float32 vectors would add 7.6 MB, so int8 is the default.

Reference point for the vector scan: sqlite-vec scanned 100,000 float32 vectors of 384 dims in 67.84 ms on an M1 Pro (2025-09-01 measurement), so 10,000 int8 vectors of 256 dims should land well under 10 ms.

Also measured on every bench run: recall@5 and recall@10 against the benchmark set below, tokens per pack, and the share of packs marked `partial`.

## Benchmark plan

Goal: score retrieval against mote's own event log with no network and no paid turns.

1. Fixtures. `moted bench membench import --set longmemeval-s` converts the 500 LongMemEval_S questions (MIT) into event-log replays: each session becomes `event` rows at the session timestamp, each question becomes a query at the question date. `--set locomo` does the same for the ten LoCoMo conversations (1,986 questions, categories 1 to 4 scored, category 5 adversarial reported separately). Fixtures stay outside the repository under the data directory because of size and license checks.
2. Ingest. Replay through the real write path with the scripted model server from MOTE-03 standing in for any model. The reflect job runs with the scripted server so the run is deterministic.
3. Retrieval score, no judge. LongMemEval carries `answer_session_ids` and LoCoMo carries evidence dialog ids. Score recall@5, recall@10 and MRR of evidence sessions in the pack. This is the number that gates changes to retrieval.
4. Optional answer score, local only. A local Ollama model reads the pack and the question and emits `{"answer": "...", "confidence": 0.0}`. A second local call judges against the gold answer with `{"correct": true | false}` parsed fail-closed. Report accuracy per question type, and mark it as a local-judge number that is not comparable to vendor tables.
5. Owner set. Twenty hand-written questions over the owner's real event log, kept private in the data directory, run with the same commands.
6. Configurations. `fts`, `vec`, `fused`, `fused+graph`. One table per run in `docs/verification/MEM-11.md` with recall, latency, tokens and the commit.

## Open decisions with defaults

1. Embedding model. Default `potion-base-8M` (English, 256 dims). Switch to `potion-multilingual-128M` if the owner needs other languages. Add `bge-small` through fastembed only if recall@10 on the benchmark trails by more than five points.
2. Vector type. Default `int8[256]` in vec0. Float32 only inside the bench for comparison.
3. ANN. Default none. Enable the `ann` feature when vectors pass 100,000 or the Pi misses the talk target.
4. Extraction. Default: the think lane's memory tool for explicit writes, and a reflect job on the local Ollama model once per hour over new events, capped at 200 events per run. No extraction on the talk path.
5. Graph store. Default SQLite tables plus petgraph. Move to LadybugDB only if three-hop queries become common.
6. Compaction. Default 180 days, owner-approved, originals retained.
7. Fork owner. Default `github.com/beelol`. Change if the owner names another organization.
8. Vendoring. Default not committed. Produced on demand for offline Pi builds.

## Acceptance criteria

Each criterion is one sentence plus a Verify clause. The checkboxes belong in the root README under the Context engine feature once the owner approves this document.

- **MEM-01** The context store holds `mem_node`, `mem_edge`, `mem_fts`, `mem_vec`, `mem_working` and `mem_pack` tables inside the daemon's SQLite file, and every node and edge carries a `source_seq` that resolves to an event-log row. **Verify:** `cargo test -p mote-memory schema::` and a `SELECT` joining a node to its event-log row after a live turn.
- **MEM-02** An episodic write embeds locally and completes in one transaction with no model call. **Verify:** `moted bench memory write` shows p50 <= 5 ms on the Mac and the ledger shows zero model rows during the bench.
- **MEM-03** Edges carry `valid_from`, `valid_to`, `created_at` and `expired_at`, a superseding fact closes the old edge without deleting it, and both "true on date D" and "believed on date D" queries answer correctly. **Verify:** `cargo test -p mote-memory temporal::` with two conflicting facts queried at three dates.
- **MEM-04** The memory tool and the reflect job emit a JSON verdict with `action` in `add`, `supersede` or `noop`, and any malformed verdict becomes `noop` with a visible card. **Verify:** unit tests feed empty, truncated, prose and unknown-action outputs.
- **MEM-05** Retrieval fuses vector, keyword, graph walk and recency with reciprocal rank fusion inside the lane's token budget, and the pack is recorded against the ledger row of the call. **Verify:** `cargo test -p mote-memory retrieval::` and a ledger `SELECT` showing pack ids and token estimates after a live turn.
- **MEM-06** Talk lane retrieval returns within 40 ms at p50 and 80 ms at p95 with 10,000 memories on the Mac. **Verify:** `moted bench memory retrieve --lane talk` output in the evidence record.
- **MEM-07** Think lane retrieval with the graph walk returns within 250 ms at p95 with 10,000 memories on the Mac, and a deadline miss returns a pack marked `partial`. **Verify:** `moted bench memory retrieve --lane think` plus a test with an injected slow walk.
- **MEM-08** The database grows by no more than 50 MB per 10,000 memories with int8 vectors. **Verify:** `moted bench memory size` prints `page_count * page_size` after `VACUUM`.
- **MEM-09** Every node and edge change is an event, and the window can revert any change to the prior state. **Verify:** write, inspect, revert, inspect, with the event log showing both rows.
- **MEM-10** Every crate used by `mote-memory` is pinned to an exact version or a git tag, and the model weights are pinned by content hash. **Verify:** `grep -E '"=|tag =' Cargo.toml` lists every memory dependency and startup refuses a model with a wrong hash.
- **MEM-11** `moted bench membench` imports LongMemEval_S and LoCoMo into the event log and prints recall@5, recall@10, MRR, latency and tokens per configuration with no network. **Verify:** run with networking disabled and record the table in `docs/verification/MEM-11.md`.
- **MEM-12** Decayed or expired memories drop out of packs but remain queryable by sequence, and compaction into a summary runs only after owner approval. **Verify:** unit test for decay ordering plus a window check that compaction waits for the approval card.
- **MEM-13** The working set per thread caps at 64 rows, evicts by age unless pinned, and leads every pack for that thread. **Verify:** `cargo test -p mote-memory working::` and a "what did you know" card showing the working set first.
- **MEM-14** The same bench meets the Pi targets on a Raspberry Pi 5. **Verify:** `moted bench memory` output on the Pi in the evidence record, after DEF-01 is active.

## Sources

Every source, URL, number and unknown is in [memory-research.md](../research/memory-research.md). The decisions above rest mainly on: the Zep paper (arXiv 2501.13956) for the bi-temporal edge, the Hindsight paper (arXiv 2512.12818) for four-way recall fusion, HippoRAG (arXiv 2405.14831) for Personalized PageRank over a small graph, the MemGPT paper (arXiv 2310.08560) for the working-set tier, memU's README for the no-LLM write path, the sqlite-vec docs and the 2025-09-01 SQLite vector benchmark for the vector path, the model2vec-rs README and the potion-base-8M model card for embeddings, the Cargo book for pinning, and the dependabot-core and Renovate sources for update tooling.
