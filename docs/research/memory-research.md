# Memory research notes

Date: 2026-10-08. Companion to `docs/reuse/memory.md`.

These are raw notes. Each entry names the source, the facts taken from it, and measured numbers where they exist. "unknown" marks a fact the research did not find. Repository metadata (language, license, stars, last push, latest release) comes from the GitHub REST API and the crates.io API on 2026-10-08 unless an entry says otherwise. Star counts are approximate and change daily.

## 1. The post

### x.com/RoundtableSpace/status/2107036824925343830

- URL: https://x.com/RoundtableSpace/status/2107036824925343830 (read through the public syndication endpoint `cdn.syndication.twimg.com/tweet-result?id=2107036824925343830`).
- Facts: author @RoundtableSpace, posted 2026-10-05 09:15 UTC. Opening text: "AI memory is getting ridiculous. 10 open-source GitHub projects that stop your agents from waking up with amnesia every session." Item 1 links Mem0 and quotes "66K+ stars". Item 2 names Hindsight with "retain, recall, reflect".
- Numbers: 66K+ stars for Mem0 as of the post.
- unknown: the syndication endpoint returned only the first two items. The remaining eight names come from the owner's brief, not from the fetched text.

## 2. The ten projects

### 2.1 Mem0

- Repo: https://github.com/mem0ai/mem0
- Facts: Python (plus a TypeScript SDK under `mem0-ts`). Apache-2.0. 66,857 stars. Last push 2026-10-08. Latest GitHub release tag `ts-v3.3.1` (2026-09-25). Python package `mem0ai` 2.2.1 per `pyproject.toml`, tag `v2.2.1`. Requires Python 3.10 to <4.0. Declared dependencies include `qdrant-client`, `sqlalchemy`, `chromadb`, `faiss-cpu`. The repo holds a `server/` (FastAPI, Alembic, Docker) and `evaluation/`.
- Architecture (paper, https://arxiv.org/abs/2504.19413, submitted 2025-04-28): two phases per `add`. Extraction: an LLM reads the new message pair plus a rolling summary and recent messages and emits candidate memories. Update: the top semantically similar stored memories are fetched by embedding and an LLM picks ADD, UPDATE, DELETE or NOOP. A graph variant Mem0g stores entities as nodes and relations as labeled directed edges. So every write costs at least two LLM calls plus one embedding call.
- Numbers from the paper abstract: 26% relative improvement over OpenAI's memory feature by LLM-as-a-judge on LoCoMo. Mem0g about 2% above base Mem0. 91% lower p95 latency and more than 90% token savings versus the full-context method. Absolute scores quoted by Zep from the paper: Mem0 66.88%, Mem0 Graph 68.44%, Zep (as run by Mem0) 65.99%.
- Where memories live: an external vector store (Qdrant default in the open-source package, many others supported) plus an optional graph store (Neo4j, Memgraph, Kuzu). Needs a server or an embedded Python process. A Rust daemon can only run it as a sidecar.
- OpenMemory: `git log` on the `openmemory` path shows commit `ea2ee075` on 2026-07-29, "chore: remove OpenMemory from the monorepo (#6530)". See 2.9.
- Mem0's 2026 vendor numbers (https://mem0.ai/blog/state-of-ai-agent-memory-2026 and https://mem0.ai/blog/ai-memory-benchmarks): 92.5 on LoCoMo and 94.4 on LongMemEval at a 200-memory retrieval budget. Vendor reported. Evaluation framework at https://github.com/mem0ai/memory-benchmarks (not inspected).
- unknown: exact LLM call count per `add` in the current hosted product. Mem0's current docs describe single-pass extraction and dedupe, which differs from the 2025 paper.

### 2.2 Hindsight

- Repo: https://github.com/vectorize-io/hindsight
- Facts: Python. MIT. 47,355 stars. Last push 2026-10-08. Latest release v0.10.3 (2026-10-08). `hindsight-api` requires Python >= 3.11. Monorepo with `hindsight-api`, `hindsight-cli`, `hindsight-embed`, `hindsight-clients`, Helm charts, Docker.
- Paper: https://arxiv.org/abs/2512.12818 ("Hindsight is 20/20", submitted 2025-12-14, authors from Vectorize and Virginia Tech). Four logical networks: world facts, agent experiences, synthesized entity summaries, evolving beliefs. Operations retain, recall, reflect. Retain rewrites inputs into structured facts with an LLM. Recall runs several searches at once (semantic vectors, BM25, graph traversal, temporal filter) and fuses them to fit a token budget. Reflect is an LLM reasoning loop over the bank.
- Storage: PostgreSQL with pgvector (ACL 2026 demo abstract, https://aclanthology.org/2026.acl-demo.27/). The Python package starts an embedded PostgreSQL by default (PyPI page https://pypi.org/project/hindsight-api-slim/). Exposes REST and MCP.
- Numbers (paper abstract): with a 20B open model, overall accuracy rises from 39% to 83.6% over a full-context baseline with the same backbone. With a scaled backbone: 91.4% on LongMemEval and 89.61% on LoCoMo, versus 75.78% for the strongest prior open system on LoCoMo. ACL demo: 83.6% LongMemEval and 83.2% LoCoMo with a 20B open model.
- Embeddable in Rust: no. Needs Python 3.11, PostgreSQL and an LLM at retain. Sidecar only.
- unknown: the fusion method in code (sources name both RRF and a cross-encoder). Embedding model default.

### 2.3 memU

- Repo: https://github.com/NevaMind-AI/memU
- Facts: Python >= 3.11. Apache-2.0 (`LICENSE.txt`). 14,517 stars. Last push 2026-10-09. Latest GitHub release v1.5.1 (2026-03-23). A tag `v2.0.0-beta.0` exists. The current `pyproject.toml` names the package `memu-cli` at `0.11.0-beta.3`, so the repo was restructured after v1.5.1.
- Architecture (README, 2026-10-08): the host agent writes skill Markdown from session logs. `commit` embeds each skill name and description and stores it. `retrieve` runs `progressive_retrieve`. The README states that `MemoryService` makes no LLM or chat calls. Storage backends: `inmemory` (brute-force cosine), `sqlite` (brute-force cosine, local default, single writer), `postgres` (pgvector). An embedding provider with an API key is required (`openai` default, also `jina`, `voyage`, `doubao`, `openrouter`).
- Older v1 design (memu.pro docs, https://memu.pro/docs): three layers (resources, memory items, categories) with a file-system metaphor. Non-embedding retrieval by direct file reading and embedding retrieval both existed.
- Numbers: none in the README. Third-party listings claim 92.09% reasoning accuracy and sub-50 ms latency, unverified.
- Embeddable in Rust: no. The write path has no LLM, which is a useful precedent. Sidecar only.
- unknown: whether v2 keeps the L0/L1/L2 layer model named in the v2.0.0-beta.0 release notes.

### 2.4 Cognee

- Repo: https://github.com/topoteretes/cognee
- Facts: Python 3.10 to <3.15. Apache-2.0. 31,778 stars. Last push 2026-10-09. Latest release v1.6.3 (2026-10-07). `pyproject.toml` pins `lancedb>=0.39.0,<0.40.0`, `pylance` 12.0.x, `aiosqlite`, `onnxruntime`, and `ladybug` (0.19.0 on most platforms, 0.17.x on older macOS). Ladybug is the Kuzu successor (see 3.4).
- Architecture (README): operations `remember`, `recall`, `improve`, `forget`. Text ingestion, retrieval and session storage do not need an LLM. LLM-dependent improvement stages skip without a key. A GLiNER extractor handles graph extraction when no LLM key is set. A local embedding model downloads on first use. Default stores from the manifest: SQLite (relational), LanceDB (vectors), Ladybug (graph). Postgres with pgvector can hold all three.
- Numbers (README, BEAM evaluation, LLM judged, 0 to 1): 0.79 at 100K tokens, 0.67 at 10M tokens. The README warns the two settings are not comparable.
- Embeddable in Rust: no. Its default store stack (LanceDB, Ladybug, SQLite) is interesting because every piece has a Rust or C++ embedded form. Sidecar only.
- unknown: the local embedding model name.

### 2.5 Graphiti

- Repo: https://github.com/getzep/graphiti
- Facts: Python >= 3.10. Apache-2.0. 31,572 stars. Last push 2026-10-09. Latest release v0.30.2 (2026-09-08). Drivers in `graphiti_core/driver`: falkordb, kuzu, neo4j, neptune. README: Neo4j 5.26, FalkorDB 1.1.2, Amazon Neptune, Kuzu 0.11.2. Kuzu is deprecated with a `DeprecationWarning` and will be removed. OpenAI is the default LLM and embedder and needs `OPENAI_API_KEY`. Ollama and other OpenAI-compatible servers work but "Graphiti works best with LLM services that support Structured Output". Ingestion runs LLM calls for entity extraction, edge extraction and dedupe, throttled by `SEMAPHORE_LIMIT` (default 10).
- Temporal model (docs, https://help.getzep.com/graphiti and the Zep paper): each entity edge carries `valid_at`, `invalid_at` (when the fact held) and `created_at`, `expired_at` (when the system knew). A contradiction sets the old edge's `invalid_at` to the new edge's `valid_at` and keeps the old edge.
- Paper: https://arxiv.org/html/2501.13956v1 ("Zep: A Temporal Knowledge Graph Architecture for Agent Memory", 2025-01-20). Three subgraphs: episode, semantic entity, community (label propagation). Search: cosine similarity over fact and name embeddings, BM25 (Neo4j Lucene), breadth-first search n hops from seeds. Rerankers: RRF, MMR, episode-mentions, node distance, cross-encoder.
- Numbers (paper): DMR 94.8% (Zep) vs 93.4% (MemGPT) vs 94.4% (full conversation) on gpt-4-turbo. LongMemEval (about 115k-token histories): gpt-4o-mini full-context 55.4% at 31.3 s with 115k tokens, Zep 63.8% at 3.20 s with 1.6k tokens. gpt-4o full-context 60.2% at 28.9 s, Zep 71.2% at 2.58 s. Zep's current research page (https://www.getzep.com/research/) claims LoCoMo 94.7% at 155 ms retrieval with 5,760 context tokens and LongMemEval 90.2% at 162 ms with 4,408 tokens. Vendor reported.
- Embeddable in Rust: no. Needs a graph server (Neo4j or FalkorDB) and LLM calls per episode. Sidecar only, and heavy on a Pi.
- unknown: LLM call count per episode.

### 2.6 OpenViking

- Repo: https://github.com/volcengine/OpenViking
- Facts: Python >= 3.10 plus a Rust workspace (`crates/ragfs`, `crates/ragfs-python`, `crates/ov_cli`). Main project AGPL-3.0. `crates/ov_cli` and `examples` Apache-2.0. `crates/ragfs/Cargo.toml` says Apache-2.0, `rust-version = "1.91.1"`, "Rust implementation of AGFS - Aggregated File System for AI Agents". 39,447 stars. Last push 2026-10-09. Latest release v0.4.23 (2026-10-02).
- Architecture (README): one store for resources, memories and skills, addressed as a virtual filesystem (`viking://resources/...`, `viking://user/{id}/memories/...`) with `ls`, `tree`, `read`, `write`, `grep`. Three tiers per item: L0 abstract (one sentence), L1 overview, L2 full content. Needs a VLM and an embedding model (Volcengine, OpenAI, Codex OAuth, Kimi, GLM, local Ollama). Runs as `openviking-server` or Docker. Hosted edition on VikingDB.
- Numbers (README, vendor reported): LoCoMo accuracy native vs with OpenViking: OpenClaw 24.20% to 82.08%, Hermes 33.38% to 82.86%, Claude Code 57.21% to 80.32%. Input tokens drop 34.3% to 91.0%. Query latency drops 58.45% to 66.10%. tau2-bench retail 70.94% to 77.81%, airline 54.38% to 66.25%.
- Embeddable in Rust: no for the memory engine (Python, AGPL). The `ragfs` crate is an aggregated filesystem layer, not a memory store. Sidecar only.
- unknown: the local vector index used by the self-hosted server.

### 2.7 Letta

- Repo: https://github.com/letta-ai/letta
- Facts: README (2026-10-08) says the current source lives in `letta-ai/letta-code`. The `archive` branch holds the retired V1 API server. Last release 0.16.8 (2026-05-14). Apache-2.0. 25,079 stars. Language field empty.
- Memory model (docs, https://docs-v1.letta.com/guides/agents/architectures/memgpt and https://www.letta.com/blog/memory-blocks): in-context core memory as labeled read-write blocks with a character limit, plus out-of-context recall memory (searchable message history) and archival memory (vector store the agent queries with a tool). State lives in Postgres or SQLite behind a REST server.
- Paper: https://arxiv.org/abs/2310.08560 (MemGPT, 2023-10-12). Virtual context management moves data between main context and external storage through function calls, with interrupts for control flow.
- Benchmark post: https://www.letta.com/blog/benchmarking-ai-agent-memory (2025-08-12). Letta agents on gpt-4o-mini score 74.0% on LoCoMo by storing the conversation as a file and using grep, search, open and close tools. The post contrasts this with Mem0's 68.5%.
- Embeddable in Rust: no. Sidecar only.

### 2.8 Letta Code

- Repo: https://github.com/letta-ai/letta-code
- Facts: TypeScript. Apache-2.0. 3,550 stars. Last push 2026-10-09. Latest release v0.34.8 (2026-10-09). npm `@letta-ai/letta-code`. README: memory blocks, MemFS ("all context (including memory blocks) is tracked via git"), `/sleeptime` dreaming, `/palace` memory view, `letta server` App Server, Letta Cloud default with a `--backend local` option. Code search finds `sqlite` only in test files.
- Embeddable in Rust: no. It is a Node CLI and server. Sidecar only.
- unknown: the local backend's on-disk database.

### 2.9 OpenMemory

Two projects carry this name. The post does not say which one it means.

- mem0 OpenMemory MCP: launched 2025-05-13 (https://mem0.ai/blog/introducing-openmemory-mcp). Lived under `openmemory/` in the mem0 monorepo. Removed on 2026-07-29 (commit `ea2ee075`, "chore: remove OpenMemory from the monorepo (#6530)"). The repo https://github.com/mem0ai/openmemory now holds a different tool: a TypeScript CLI and TUI to port coding sessions between Claude Code, Codex and OpenCode (MIT, 96 stars, last push 2026-07-29, no releases).
- CaviraOSS OpenMemory, renamed LongMemory: https://github.com/CaviraOSS/OpenMemory redirects to `CaviraOSS/LongMemory`. TypeScript. Apache-2.0. 4,524 stars. Last push 2026-09-20. Latest release v1.2.3 (2025-12-12). npm `longmemory`. Uses `better-sqlite3` with a shipped `schema.sql`. README: "Durable local-first storage with SQLite", "Immutable content, provenance, and temporal truth", "token-bounded context", runs as npm library, CLI, HTTP, MCP, dashboard and VS Code extension.
- Embeddable in Rust: neither. The LongMemory schema (SQLite, immutable content, temporal truth) is the closest in spirit to mote's store and is worth a read when the schema is written.
- unknown: LongMemory's embedding model and whether writes need an LLM.

### 2.10 Agent Memory Benchmark (AMB)

- Repo: https://github.com/vectorize-io/agent-memory-benchmark
- Facts: Python >= 3.11. License field empty in the API (unknown). 144 stars. Last push 2026-10-07. Built by the Hindsight team. Leaderboard at https://agentmemorybenchmark.ai. Pipeline: ingest, retrieve, generate (Gemini), judge (Gemini). Needs `GEMINI_API_KEY`. Datasets: LoCoMo, LongMemEval, LifeBench, MemBench, MemSim, PersonaMem, PrecisionMemBench (retrieval-only mode scores the returned belief-ID set against per-case assertions). Retrieval time and ingestion time are recorded separately.
- Announcement: https://hindsight.vectorize.io/blog/2026/03/23/agent-memory-benchmark. Hindsight v0.4.19 single-query mode: LoCoMo 92.0%, LongMemEval 94.6%, LifeBench 71.5%, PersonaMem 86.6%.
- Use for mote: the dataset loaders and the retrieval-only scoring idea. The harness itself needs a paid Gemini key, so mote cannot run it as is.
- unknown: license.

## 3. Rust-native candidates

### 3.1 cozo

- Repo: https://github.com/cozodb/cozo
- Facts: Rust. MPL-2.0. 4,134 stars. Last push 2024-12-04. Latest release v0.7.6 (2023-12-11). crates.io `cozo` 0.7.6 updated 2023-12-11. dbdb.io lists the project as abandoned since 2024. A community fork `cozo-ce` reached 0.7.13-alpha.3 in December 2024. Datalog queries, HNSW vectors, backends memory, SQLite, RocksDB, sled, TiKV.
- Decision input: no upstream activity for 22 months. Skip.

### 3.2 oxigraph

- Repo: https://github.com/oxigraph/oxigraph
- Facts: Rust. Apache-2.0 (dual MIT). 1,982 stars. Last push 2026-10-08. Latest release v0.5.11 (2026-09-02). RDF triple store with SPARQL. No vector search.
- Decision input: the RDF model does not match an entity-fact-with-time graph without extra reification. Skip.

### 3.3 surrealdb (embedded)

- Repo: https://github.com/surrealdb/surrealdb
- Facts: Rust. Business Source License 1.1 (LICENSE file, "SurrealDB 3.0"). 33,115 stars. Latest release v3.3.0 (2026-09-28), crate 3.3.2 (2026-10-08). Embedded through `kv-surrealkv` or `kv-rocksdb` features. Roadmap marks HNSW vector search and full-text indexing available (2026 Q1). docs.rs build of a thin wrapper crate averages about 7.5 minutes.
- unknown: embedded binary size and clean compile time.
- Decision input: heavy build, BSL license, and mote already has SQLite. Skip for v0.

### 3.4 kuzu and LadybugDB

- Repo: https://github.com/kuzudb/kuzu, archived 2025-10-10. README: "We are archiving the KuzuDB project here". Last release v0.11.3 (2025-10-10). C++. MIT.
- Successor: https://github.com/LadybugDB/ladybug. C++. MIT. 1,827 stars. Last push 2026-10-09. Latest release v0.21.2 (2026-10-01). README: "formerly known as Kuzu", embedded property graph, Cypher, native full-text search and vector index. Rust crate `lbug` 0.21.2 (2026-10-01, https://github.com/LadybugDB/ladybug-rust). Cognee depends on the Python package `ladybug`.
- Decision input: a real embedded graph database with vectors and FTS in one C++ library. Keep as the fallback if SQLite tables cannot hold the graph. Not for v0.

### 3.5 HelixDB

- Repo: https://github.com/HelixDB/helix-db
- Facts: Rust. Apache-2.0 (LICENSE file checked). 6,117 stars. Last push 2026-10-08. Latest release v3.5.1 (2026-10-08). crates.io `helix-db` 3.0.0 (2026-08-04) with only a `default` feature. The 2.x crate is an HTTP client plus query DSL. Docs describe a v3 embedded mode (in-process, memory, disk or S3) but say the packages were not published when the page was written. v1 used LMDB through heed. v3 cloud uses an LSM engine on object storage.
- unknown: whether the embedded mode is on crates.io today, and the disk engine behind it.
- Decision input: promising later. Skip for v0.

### 3.6 LanceDB

- Repo: https://github.com/lancedb/lancedb
- Facts: Rust. Apache-2.0. 11,621 stars. Latest release v0.40.0 (2026-10-07). crate `lancedb` 0.40.0. `rust/lancedb/Cargo.toml` (main, 0.41.0-beta.1) depends unconditionally on `arrow*`, nine `datafusion*` crates, and `lance*` crates. Needs `protoc` to build in some setups. Vector, full-text (tantivy) and hybrid search.
- unknown: binary size and compile time on aarch64.
- Decision input: too heavy for a 300 ms talk lane daemon on a Pi. Skip for v0.

### 3.7 sqlite-vec

- Repo: https://github.com/asg017/sqlite-vec
- Facts: C. Repo license Apache-2.0 (API), crate `sqlite-vec` 0.1.9 (2026-03-31) licensed MIT/Apache-2.0. 8,171 stars. Last push 2026-05-18. README: "pre-v1, so expect breaking changes". `vec0` virtual tables store float, int8 and bit vectors. Non-vector data goes in metadata, auxiliary and partition key columns. KNN query shape: `WHERE embedding MATCH ? AND k = 10` (or `LIMIT 10` on SQLite 3.41+). Distance L2 default, `distance_metric=cosine` per column. The Rust page (https://alexgarcia.xyz/sqlite-vec/rust.html) registers the extension with `sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())))` and needs rusqlite's `bundled` feature. The crate's template dev-depends on rusqlite 0.31, but the FFI entry point works with any rusqlite.
- Search mode: brute force. The KNN docs page does not describe an approximate index. The source tree has IVF and DiskANN modules, unreleased.
- Numbers (https://marcobambini.substack.com/p/the-state-of-vector-search-in-sqlite, 2025-09-01, Apple M1 Pro, 100,000 float32 vectors of 384 dims): insert 1179.07 ms, full-scan query 67.84 ms. The rival `sqlite-vector` extension: 56.65 ms full scan, 17.44 ms int8 quantized scan, 3.97 ms quantized preload, recall@20 1.0. Scaled to 10,000 vectors, sqlite-vec's full scan is about 7 ms on that Mac.
- Security note: `vec_npy_each` moved out of the default entry point in v0.1.3 after CVE-2024-46488.
- Decision input: embed. The cheapest vector path that lives inside the existing SQLite file.

### 3.8 sqlite-vss

- Repo: https://github.com/asg017/sqlite-vss. C++ on Faiss. MIT. Last push 2024-05-05. Latest release v0.1.2 (2023-08-06). Superseded by sqlite-vec from the same author. Skip.

### 3.9 usearch

- Repo: https://github.com/unum-cloud/USearch. C++ core with a Rust crate `usearch` 2.26.4 (2026-10-05), Apache-2.0, features `simsimd`, `openmp`, `numkong`. 4,328 stars. HNSW with f32, f16, i8 and binary.
- Decision input: a C++ build in the daemon. Prefer a pure Rust HNSW first.

### 3.10 hnsw_rs

- Repo: https://github.com/jean-pierreBoth/hnswlib-rs. Pure Rust HNSW. Crate `hnsw_rs` 0.3.4 (2026-02-28), repo `Cargo.toml` 0.3.5, license `MIT/Apache-2.0`, edition 2024. 254 stars. Last push 2026-10-08. No GitHub releases.
- Decision input: the ANN index to add when brute force misses the budget.

### 3.11 tantivy

- Repo: https://github.com/quickwit-oss/tantivy. Rust. MIT. 16,193 stars. Crate 0.26.2 (2026-09-08), MSRV 1.86. BM25, "startup time (<10ms)". The maintainers' latency benchmark puts it about 2x Lucene.
- unknown: query latency on a 10k-document index.
- Decision input: SQLite FTS5 already ships in rusqlite `bundled`. Revisit only if FTS5 ranking is not enough.

### 3.12 petgraph

- Repo: https://github.com/petgraph/petgraph. Rust. MIT OR Apache-2.0. Crate 0.8.3 (2025-09-30), MSRV 1.64. In-memory graph structures and algorithms (BFS, Dijkstra). `src/algo/page_rank.rs` at tag `petgraph@v0.8.3` exposes `pub fn page_rank<G, D>(graph: G, damping_factor: D, nb_iter: usize) -> Vec<D>`, which is plain PageRank with no personalization vector.
- Decision input: embed for in-memory walks over a loaded neighbourhood. mote writes the Personalized PageRank loop itself over petgraph's graph.
- model2vec-rs loader signature checked in `src/model.rs` (main, 2026-10-08): `StaticModel::from_pretrained(repo_or_path, token: Option<&str>, normalize: Option<bool>, subfolder: Option<&str>)`, with `encode(&[String]) -> Vec<Vec<f32>>` and `encode_single(&str) -> Vec<f32>`.

### 3.13 indradb

- Repo: https://github.com/indradb/indradb. Rust. MPL-2.0. Crate 5.0.0 (2025-08-16). Last push 2025-08-16. Skip, low activity and a second storage engine.

### 3.14 sled, redb, fjall

- sled: https://github.com/spacejam/sled, 0.34.7 (2021-09-12), 1.0.0-alpha.124. Beta since 2021. Skip.
- redb: https://github.com/cberner/redb, 4.3.0 (2026-09-15). Pure Rust ACID key-value. Not needed beside SQLite. Skip.
- fjall: https://github.com/fjall-rs/fjall, 3.1.12 (2026-10-03). LSM key-value. Not needed beside SQLite. Skip.

### 3.15 fastembed-rs

- Repo: https://github.com/Anush008/fastembed-rs. Rust. Apache-2.0. Crate `fastembed` 7.1.1 (2026-10-07), MSRV 1.88. Runs ONNX models through `ort` with Hugging Face `tokenizers`. Downloads weights from the Hugging Face Hub on first use (`hf-hub` feature). Models include `bge-small-en-v1.5` and `all-MiniLM-L6-v2` (384 dims) and rerankers.
- unknown: latency on a Raspberry Pi 5 for all-MiniLM-L6-v2. A Mac-based Rust benchmark reports about 1 to 2 ms p50 per query for ort fp32.
- Decision input: the quality upgrade path after model2vec, and the reranker path for the think lane.

### 3.16 candle and ort

- candle: https://github.com/huggingface/candle, Apache-2.0 or MIT, crate `candle-core` 0.11.0 (2026-06-26). Pure Rust inference, runs BERT-class embedders. unknown: Pi latency.
- ort: https://github.com/pykeio/ort, Apache-2.0, 2.0.0-rc.13 (2026-07-28). ONNX Runtime bindings, used by fastembed.

### 3.17 model2vec-rs

- Repo: https://github.com/MinishLab/model2vec-rs. Rust. MIT. Crate `model2vec-rs` 0.3.0 (2026-09-19), MSRV 1.88. Features `default`, `hf-hub`, `local-only`, `onig`, `fancy-regex`, `cli`, `wasm`. Loads from a Hub id, a local directory, or bytes. Weights f32, f16 or i8 in safetensors. README benchmark: about 8,000 samples per second in Rust versus 4,650 in Python, single thread, CPU not named.
- Model `minishlab/potion-base-8M` (https://huggingface.co/minishlab/potion-base-8M): `config.json` says `hidden_dim: 256`, `apply_pca: 256`, `normalize: true`, tokenizer `baai/bge-base-en-v1.5`. 7.5M parameters, about 30 MB on disk. Model card: reaches 91.96% of all-MiniLM-L6-v2's MTEB average (51.32). Other models: potion-base-32M, potion-retrieval-32M, potion-multilingual-128M, potion-code-16M-v2.
- Decision input: embed. Static embeddings give a sub-millisecond write path with no neural network at inference.

### 3.18 rusqlite and bundled SQLite

- Repo: https://github.com/rusqlite/rusqlite. Crate 0.40.2 (2026-08-08). The shared-crates RFC pins 0.32 with `bundled`. `libsqlite3-sys/build.rs` compiles the bundled SQLite with `SQLITE_ENABLE_FTS3`, `SQLITE_ENABLE_FTS5`, `SQLITE_ENABLE_JSON1`, `SQLITE_ENABLE_RTREE` and `SQLITE_ENABLE_LOAD_EXTENSION=1`. So FTS5 and extension loading are already available to mote.

## 4. Research behind the libraries

### 4.1 Zep temporal knowledge graph

- https://arxiv.org/html/2501.13956v1. Facts and numbers in 2.5.

### 4.2 MemGPT and Letta tiered memory

- https://arxiv.org/abs/2310.08560. Main context (system instructions, working context, FIFO message queue) and external context (recall storage, archival storage). The model edits memory through function calls. Facts in 2.7.

### 4.3 A-MEM

- https://arxiv.org/abs/2502.12110 (v9, June 2025, Rutgers and AIOS Foundation). Zettelkasten-style notes: each new memory becomes a note with context, keywords and tags. The system links it to similar notes and may rewrite older notes ("memory evolution"). Each step is an LLM call. Code: https://github.com/agiresearch/A-mem.
- Numbers: "superior improvement" over baselines across six foundation models. No absolute numbers in the abstract.

### 4.4 HippoRAG and HippoRAG 2

- https://arxiv.org/abs/2405.14831 (NeurIPS 2024). LLM OpenIE builds a knowledge graph of triples, embedding similarity adds synonym edges, retrieval seeds Personalized PageRank with query entities and ranks passages. Numbers: up to 20% over RAG baselines on multi-hop QA, 10 to 30x cheaper and 6 to 13x faster than iterative retrieval (IRCoT).
- https://arxiv.org/abs/2502.14802 (ICML 2025). Adds passage nodes, query-to-triple matching with an LLM recognition filter, and PPR. Number: 7% improvement on associative memory tasks over a strong embedding model.
- Decision input: PPR over a small subgraph is a cheap graph walk mote can run with petgraph on the think lane.

### 4.5 Mem0 benchmark claims and rebuttals

- Zep rebuttal, 2025-05-06: https://www.getzep.com/blog/lies-damn-lies-statistics-is-mem0-really-sota-in-agent-memory/. Claims Mem0 ran Zep with both speakers as the user, appended timestamps to message text instead of `created_at`, and ran searches sequentially. Zep's corrected LoCoMo J score 75.14% +/- 0.17. Mem0's own scores: base 66.88%, graph 68.44%. Mem0's score for Zep: 65.99%. Full-context baseline on gpt-4o-mini: 72.90%.
- Mem0's counter (GitHub issue on Zep's papers repo, May 2025, per secondary sources): Zep's earlier 84% excluded the adversarial category from the denominator. Corrected figure 58.44% +/- 0.20. Not fetched directly.
- Letta, 2025-08-12: 74.0% with files and grep on gpt-4o-mini (2.7).
- Takeaway: every LoCoMo number above is vendor reported and depends on the judge, the model, and which categories count. mote must run its own harness.

### 4.6 LoCoMo

- Paper: https://arxiv.org/abs/2402.17753 (2024-02-27, ACL 2024). Conversations average about 300 turns and 9K tokens over up to 35 sessions. Tasks: question answering, event summarization, multimodal dialogue generation.
- Repo: https://github.com/snap-research/locomo. Data file `data/locomo10.json` with ten conversations. Evaluation scripts under `scripts/`. License file present, type not stated on the page.
- Own measurement (2026-10-08, parsed `locomo10.json`): 10 conversations. QA by category: 1: 282, 2: 321, 3: 96, 4: 841, 5: 446. Total 1,986. Sessions per conversation: 19, 19, 32, 29, 29, 28, 31, 30, 25, 30. Average 588 turns per conversation in the released set. Category names from the Mem0 paper and Zep's issue: 1 multi-hop, 2 temporal, 3 open-domain, 4 single-hop, 5 adversarial. Vendors score categories 1 to 4 and drop 5.

### 4.7 LongMemEval

- Paper: https://arxiv.org/abs/2410.10813 (ICLR 2025, UCLA, Tencent AI Lab, UCSD). Five abilities: information extraction, multi-session reasoning, temporal reasoning, knowledge updates, abstention. Commercial assistants lose about 30% accuracy over sustained interaction.
- Repo: https://github.com/xiaowu0162/LongMemEval. MIT. 500 questions. `LongMemEval_S`: about 115k tokens of history, about 40 sessions. `LongMemEval_M`: about 500 sessions. Oracle: evidence sessions only. Question types: single-session-user, single-session-assistant, single-session-preference, temporal-reasoning, knowledge-update, multi-session. Abstention questions carry an `_abs` id suffix. Data on Hugging Face `xiaowu0162/longmemeval-cleaned`. Judge script `evaluate_qa.py` with `gpt-4o`.

### 4.8 Other benchmarks seen

- MemoryAgentBench (HUST-AI-HYZ), MemBench (https://github.com/import-myself/Membench), Mem2ActBench, AMA-Bench (ICML 2026), STATE-Bench (Microsoft), DolphinBench (2026-09-21). Names only. Not inspected.
- OSS Insight "agent memory race 2026" (https://ossinsight.io/blog/agent-memory-race-2026): star trend article, not inspected.

## 5. Cargo pinning and update tooling

### 5.1 Cargo book

- Git dependencies: https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html. `git` plus `branch`, `tag` or `rev`. `rev` accepts a commit hash or a named ref such as `refs/pull/493/head`. "Cargo locks the commits of git dependencies in Cargo.lock file at the time of their addition and checks for updates only when you run cargo update command." A `version` key beside `git` is checked against the fetched manifest and used on publish.
- Overrides: https://doc.rust-lang.org/cargo/reference/overriding-dependencies.html. `[patch.crates-io]` replaces a registry crate with a git fork. `[patch."https://github.com/example/baz"]` replaces one git source with another. "Cargo only looks at the patch settings in the Cargo.toml manifest at the root of the workspace."
- cargo update: https://doc.rust-lang.org/cargo/commands/cargo-update.html. `cargo update <spec>` updates only that package conservatively. `--precise` "can be a git revision (such as a SHA hash or tag)".
- cargo vendor: https://doc.rust-lang.org/cargo/commands/cargo-vendor.html. "vendor all crates.io and git dependencies for a project into the specified directory". Prints the `.cargo/config.toml` source replacement. `--versioned-dirs` names every directory with its version. Vendored sources are read-only. Use `[patch]` or a `path` dependency to modify a crate.

### 5.2 Renovate

- https://docs.renovatebot.com/modules/manager/cargo/. The cargo manager extracts from `Cargo.toml`, updates `Cargo.lock`, and supports datasources `crate`, `git-refs`, `git-tags`, `github-tags`, `gitlab-tags`. Lock file maintenance runs cargo as an external command.
- unknown: an explicit statement that `rev` and `tag` keys get rewritten. The datasource list implies `tag` (git-tags) and `rev` (git-refs) support. Confirm with a dry run.

### 5.3 Dependabot

- `cargo/lib/dependabot/cargo/update_checker.rb` in https://github.com/dependabot/dependabot-core (main, fetched 2026-10-08) requires `dependabot/git_commit_checker` and branches on `git_dependency?`. For an unpinned git dependency it takes `head_commit_for_current_branch`. If `pinned_ref_looks_like_version?` it looks up `local_tag_for_latest_version` and moves the pin to that tag's SHA. Git sub-dependencies are not updated ("TODO: Dependabot can't update git sub-dependencies yet").
- Takeaway: a pin of `tag = "v1.2.3"` gets bumped by Dependabot. A raw `rev = "<sha>"` without a version-like tag does not.

## 6. Hardware

- Raspberry Pi 5 (https://www.raspberrypi.com/products/raspberry-pi-5/): Broadcom BCM2712, quad-core 64-bit Arm Cortex-A76 at 2.4 GHz, 512 KB L2 per core, 2 MB shared L3. LPDDR4X-4267 in 1, 2, 4, 8 or 16 GB. microSD (SDR104), PCIe 2.0 x1 for an M.2 SSD through a HAT, two USB 3.0 and two USB 2.0 ports.
- unknown: measured SQLite, sqlite-vec, model2vec and fastembed latencies on a Pi 5. Every Pi number in the reuse doc is a target to measure, not a measurement.

## 7. Secondary sources read for orientation

- https://www.analyticsvidhya.com/?p=257839 ("7 Open-Source Memory GitHub Projects", updated 2026-09-29, lists ten projects including Hindsight and memU).
- https://ibl.ai/blog/openviking-agpl-license-agent-memory-token-costs (OpenViking license and token-cost notes).
- https://signals.aktagon.com/articles/2026/02/benchmarking-ai-agent-memory-is-a-filesystem-all-you-need/ (summary of the Letta post).
- https://www.beri.net/article/zep-vs-mem0-vs-postgres-agent-memory-store-buy-last-2026 (argues for building on a plain store first).
- https://mem0.ai/blog/zep-vs-mem0-which-ai-memory-layer-should-you-choose (Mem0's view of Zep's configurations, 83% to 80.32% on LoCoMo, 71.2% on LongMemEval).
- https://docs.helix-db.com/database/helix-db/start-here/local-development/embedded-database.md (HelixDB v3 embedded mode docs).
- https://surrealdb.com/docs/surrealdb/embedding/rust/ and https://surrealdb.com/roadmap (embedded SurrealDB and HNSW status).
- https://alexgarcia.xyz/sqlite-vec/features/knn.html (KNN query shapes).
- https://memu.pro/docs and https://memu.pro/blog/memu-1-0-0-release (memU v1 layers).
- https://docs.mem0.ai/openmemory and https://mem0.ai/blog/introducing-openmemory-mcp (OpenMemory MCP launch).
