# Matrix compiler benchmark plan

## Goal

Add a deterministic compiler latency benchmark separate from source byte and token measurements.
Verify the M-series Mac portion of UI-46. Keep Pi 5 open until the same benchmark runs on real Pi 5
hardware.

## Task 1: Add the fixed compiler fixture and report

**Files:**
- Modify: `moted/src/bench.rs`
- Modify: `moted/src/main.rs`

**Fixed input:**
- Source: `matrix/examples/mote-dashboard.matrix`
- Source SHA-256: `358e1bbbdb30b38a134a6b43152b3b90bddc463ac493920eadcbee8b44f10ea3`
- Nine fixed config defaults
- Five deterministic pipeline stage rows
- Ready host progress `0.5` and list values `first`, `second`

No database, daemon, Ollama, network, clock or user configuration may enter the fixture.

- [ ] **Step 1: Pin fixture and percentile tests**

Assert the source and combined fixture hashes, successful resolve/lower/consumer validation,
populated collection and interactive controls. Pin nearest-rank p95 and strict budgets.

- [ ] **Step 2: Measure four phases**

Add `moted bench compiler` with 200 untimed warmups and 2,000 interleaved samples. Use
`std::hint::black_box`. Record parse, resolve/check, v0 lowering and production end-to-end compile
separately.

- [ ] **Step 3: Emit complete provenance**

Print and append one JSON line with revision, release profile, rustc/cargo versions, hardware,
source and combined hashes, manifest version, warmup/sample counts, nearest-rank method, per-phase
p95 nanoseconds/milliseconds and pass state.

Refuse debug builds. On Apple Silicon require end-to-end p95 strictly below 2.0 ms. Apply the 10.0
ms Pi budget only when Linux arm64 hardware identifies itself as Raspberry Pi 5. Never infer Pi 5
from architecture alone. Unsupported hardware may report but cannot pass UI-46.

## Task 2: Record honest UI-46 evidence

**Files:**
- Create: `docs/verification/UI-46.md`
- Replace: `docs/HANDOFF.md`

Run the release benchmark on the current Mac and record exact output and environment. Keep UI-46
partial because the Pi 5 measurement is absent. State that `matrix/measurement.md` byte/token data
is not latency evidence and leave its tools/output unchanged.

## Verification

Run:

```text
cargo test -p moted compiler_bench
cargo run --release -p moted -- bench compiler
cargo test --workspace
cargo clippy --workspace -- -D warnings
git diff --check
```

Commit: `perf(matrix): benchmark compiler p95`
