# Handoff: Matrix compiler latency

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `0b7c0a6`.
Commit: `perf(matrix): benchmark compiler p95`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

`moted bench compiler` now benchmarks the production Matrix compiler with a fixed in-memory
fixture. It performs 200 untimed warmups and 2,000 interleaved samples. It reports nearest-rank p95
for parsing, resolve and check, v0 lowering and production end-to-end compilation. Debug builds
refuse to run.

The JSON report includes the Git revision, release profile, Rust and Cargo versions, operating
system, architecture, hardware name, source and combined fixture hashes, manifest version, sample
counts, percentile method, nanosecond and millisecond results and target state. The report appends
to `bench.jsonl`. The compiler command initializes no database, daemon, Ollama client or live user
configuration.

The Apple M5 Max release run passed. Production end-to-end p95 was 39,458 ns against a strict
2,000,000 ns limit. UI-46 remains partial because no real Raspberry Pi 5 run exists. Raspberry Pi 5
must be identified by its hardware model before the 10,000,000 ns budget applies.

The fixed source hash is `358e1bbbdb30b38a134a6b43152b3b90bddc463ac493920eadcbee8b44f10ea3`.
The combined fixture hash is `0cacb63fcbfda9cf8f199ccf26516f1e89c505fd8ba435b21109c121ae5305ee`.
The byte and token measurement tools and output were not changed.
