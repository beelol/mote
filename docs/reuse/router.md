# Reuse: fail-closed router

Concept 3 in `docs/concepts.md`. Serves MOTE-06, MOTE-07 and MOTE-08 in `docs/rfcs/mote.md`.
Written 2026-10-08. Every number has a source in the last section. Estimates from published
throughput are marked "est." and need a run on the owner's hardware before they count as evidence.

## Key points

1. The router is a trained head on frozen embeddings: `fastembed` with bge-small-en-v1.5 gives a 384-dim vector in 1 to 4 ms on the Mac, and `linfa-logistic` turns it into three probabilities. It trains in seconds from a seed file and later from the ledger, with no Python.
2. The evidence against a tiny generative model as the primary router is strong. Zero-shot, Qwen2.5-0.5B scores 47.9% and Llama-3.2-1B 49.1% on a CLINC150, Banking77 and SNIPS composite, with calibration errors from 0.12 to 0.77. A logistic head on bge-small scores 85.7% on 77-class Banking77 from eight examples per class.
3. Both paths emit the same verdict, `{"route": "talk" | "think" | "work", "reason": "..."}`, and one parser validates it. Anything malformed routes to `think` (MOTE-07).
4. Rules run first and only on structural, machine-shaped signals: a code fence, a diff header, a path with an extension, a shell prompt. Verb lists such as "fix" or "implement" are judgment over natural language and belong to the head, per the repo rule. No rule ever returns `talk`.
5. The head abstains to `think` when the calibrated top probability is under 0.6, and `talk` needs a higher, cost-tuned threshold because a wrong `talk` costs more than a wrong `think`.
6. A judge handles abstained turns on the Mac: the resident talk model with a JSON schema and thinking off, about 100 ms (est.) for a 150-token prompt. Ollama's new `/v1/systemone` decision models (`nimble`, `clef-flash`, 9B, Apache-2.0) return per-option probabilities in one forward pass and are the measured alternative.
7. On the Pi 5 only the head runs. A 0.5B model decodes at 17 to 19 t/s there, so 20 output tokens alone cost about 1 s.
8. Every verdict writes a ledger row with the input hash, probabilities, reason, latency and model. Owner corrections, re-asks and think-lane tool use become labels.

## What the concept needs

- One verdict shape parsed fail-closed, and the parser is the only producer of a route (MOTE-07).
- At most 150 ms added at the median to a `talk` turn (MOTE-08), with the router running beside the talk stream (MOTE-06).
- A rules pre-filter that can only raise `think` or `work` priors from structural signals.
- A calibrated probability per route, a cost-tuned `talk` threshold and an abstain rule.
- A seed set of labelled turns to train before the ledger has any.
- A ledger row per verdict and a label pipeline from corrections and outcomes.
- Training and evaluation commands with per-class precision and recall, a confusion matrix, wrong-talk rate, calibration error and latency.
- The scripted model server for the judge path in tests (MOTE-03).

## Candidates

| Name | Repo | Language | License | Last release | Use | Reason | Pin |
|---|---|---|---|---|---|---|---|
| fastembed | https://github.com/Anush008/fastembed-rs | Rust | Apache-2.0 | 7.1.1, 2026-10-07 | embed as crate (default) | Bundles bge-small, MiniLM, nomic, EmbeddingGemma and Qwen3-Embedding through `ort` 2.0.0-rc.13, MiniLM-L6 costs 1.1 to 2.9 ms per short input on an M4 Max | `7.1.1` |
| bge-small-en-v1.5 | https://huggingface.co/BAAI/bge-small-en-v1.5 | ONNX | MIT | 2023 | model (default) | 33M, 384-dim, Banking77 85.7% and MassiveIntent 74.8% with 8-shot logistic regression | file hash |
| linfa-logistic | https://github.com/rust-ml/linfa | Rust | MIT OR Apache-2.0 | 0.8.1, 2025-12-23 | embed as crate (default) | `MultiFittedLogisticRegression` for three classes, pure Rust, trains in seconds | `0.8.1` |
| model2vec-rs | https://github.com/MinishLab/model2vec-rs | Rust | MIT | 0.3.0, 2026-09-19 | embed as crate (Pi alternative) | Static embeddings, about 0.25 ms per sentence, potion-base-8M is 7.5M parameters with MassiveIntent 63.8% at 8-shot, no `ort` | `0.3.0` |
| all-MiniLM-L6-v2 | https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2 | ONNX | Apache-2.0 | 2021 | model (alternative) | 22.7M, Banking77 80.0% at 8-shot, fastest encoder in fastembed | file hash |
| ort | https://github.com/pykeio/ort | Rust | MIT OR Apache-2.0 | 2.0.0-rc.13, 2026-07-28 | embed through fastembed | ONNX Runtime 1.28, prebuilt for aarch64 macOS and aarch64 Linux, CoreML provider on the Mac | `2.0.0-rc.13` |
| tract-onnx | https://github.com/sonos/tract | Rust | MIT OR Apache-2.0 | 0.23.8, 2026-09-21 | keep as alternative | Pure Rust, no native runtime to ship, speed against `ort` on BERT-class models unknown | `0.23.8` |
| Ollama structured outputs | https://github.com/ollama/ollama | Go | MIT | `format` since 0.5.0, 2024-12-04, Ollama 0.40.2 current | sidecar (judge) | JSON schema becomes a llama-server grammar or XGrammar on the MLX runner, `think: false` avoids answers landing in the thinking field | installed version, in evidence |
| Ollama decision models | https://ollama.com/library/nimble and https://ollama.com/library/clef-flash | weights | Apache-2.0 | Ollama 0.35+, 2026 | measure (Mac judge) | `/v1/systemone` scores 2 to 26 options in one forward pass, nimble under 100 ms on an M5 Max, clef-flash 38.8 ms median and Banking77 macro-F1 90.9 zero-shot, 9 to 12 GB of weights | `nimble` |
| Qwen3 0.6B | https://huggingface.co/Qwen/Qwen3-0.6B | weights | Apache-2.0 | 2025-04-29 | keep as alternative judge | 297 t/s decode and 4993 t/s prefill on an M4 Pro, zero-shot intent accuracy under 50% for this size class | `qwen3:0.6b` |
| llama-server grammar | https://github.com/ggml-org/llama.cpp | C++ | MIT | rolling | skip for v0 | `json_schema`, GBNF, `--reasoning off`, pre-sampling `logprobs`, needed only if the judge moves off Ollama | `b` tag in evidence |
| SetFit | https://github.com/huggingface/setfit | Python | Apache-2.0 | 1.2.0, 2026-09-04 | port the idea | Contrastive fine-tuning beats a frozen head only above about 1k examples, 82.6% against 78.0% for model2vec plus logistic and 82.8% for bge-base plus logistic | skip at run time |
| fastText | https://github.com/facebookresearch/fastText | C++ | MIT | archived 2024-03 | skip | Archived upstream, short-text intent accuracy unknown | skip |
| DeBERTa-v3-xsmall, ModernBERT-base | https://huggingface.co | weights | MIT, Apache-2.0 | 2023, 2024 | later, through ort | Fine-tuned encoders reach 93% to 97% on 77 to 150 classes, trained once in Python | HF revision |
| semantic-router | https://github.com/aurelio-labs/semantic-router | Python | MIT | 0.1.16, 2026-07-26 | port the idea | Example utterances per route with a score threshold, hybrid dense and sparse | skip |
| RouteLLM, FrugalGPT, Hybrid LLM | https://github.com/lm-sys/RouteLLM | Python | Apache-2.0 | 2024 | port the idea | Trained routers cut strong-model calls 40% to 85% at near-equal quality | skip |
| vLLM semantic router | https://github.com/vllm-project/semantic-router | Rust and Go | Apache-2.0 | 2025 | port the idea | ModernBERT classifiers in a Rust service, overhead cut from 4.9 s to 50 ms on a GPU | skip |
| Argilla, Label Studio, cleanlab, Snorkel | https://github.com/argilla-io/argilla | Python | Apache-2.0 | 2026 | skip | The window is the labelling tool, one action per card | skip |

### Cost and accuracy comparison

| Approach | Model | RAM | p50 Mac | p50 Pi 5 | Accuracy anchor | Training | Rust path |
|---|---|---|---|---|---|---|---|
| Rules pre-filter | structural regex | 0 | < 1 ms | < 1 ms | 60.7% alone on 8-class MASSIVE, high precision on structural cues only | none | `regex` |
| Embedding plus logistic head | bge-small-en-v1.5 | about 130 MB | 2 to 4 ms (est. from MiniLM) | 15 to 40 ms (est.) | Banking77 85.7% at 8-shot, 3 routes with 100 labels: unknown, expected higher | seconds | `fastembed` plus `linfa-logistic` |
| Static embedding plus logistic head | potion-base-8M | about 30 MB | 0.3 ms | under 5 ms (est.) | 78.0% average over 14 datasets at 1k examples | seconds | `model2vec-rs` plus `linfa-logistic` |
| Judge: resident talk model, JSON schema | qwen3:4b | shared with talk | 100 to 200 ms (est.) | not run | few-shot accuracy for this size: unknown | prompt only | `mote-localmodel` |
| Judge: decision model | nimble 9B | 9.3 GB | under 100 ms (vendor) | not run | clef-flash Banking77 macro-F1 90.9 | criteria text | raw HTTP to `/v1/systemone` |
| Tiny LLM, JSON schema | qwen3:0.6b | 0.6 GB | 100 ms plus 10 ms HTTP (est.) | over 3 s (est.) | 47.9% to 49.1% zero-shot composite | prompt only | `mote-localmodel` |
| SetFit | MiniLM body plus head | about 100 MB | 2.4 ms | 10 to 30 ms (est.) | 77.9% at 8-shot Banking77, 91.1% on 8-class MASSIVE | minutes in Python | ONNX body through `ort`, head weights in Rust |
| Fine-tuned encoder | DeBERTa-v3-xsmall | about 140 MB | 5 to 20 ms (est.) | unknown | 93% to 97% full data | minutes in Python | `ort` |

The Mac budget is met by every head and by the judges. The Pi budget of 400 ms is met only by the heads.

## Recommendation

```toml
# crates/mote-router/Cargo.toml
[dependencies]
fastembed = { git = "https://github.com/Anush008/fastembed-rs", tag = "v7.1.1" }
linfa = { git = "https://github.com/rust-ml/linfa", tag = "0.8.1" }
linfa-logistic = { git = "https://github.com/rust-ml/linfa", tag = "0.8.1" }
regex = "=1.12.2"
serde = { version = "=1.0.228", features = ["derive"] }
serde_json = "=1.0.151"
mote-localmodel = { path = "../mote-localmodel" }

[dependencies.model2vec-rs]
git = "https://github.com/MinishLab/model2vec-rs"
tag = "v0.3.0"
optional = true
```

Replace each `tag` with the exact `rev` after `git ls-remote --tags` confirms the tag name. The embedding model file is fetched once and pinned by hash.

The judge request for an abstained turn, through `mote_localmodel::ChatRequest`:

```json
{"model":"qwen3:4b","stream":false,"think":false,"keep_alive":"30m",
 "options":{"temperature":0,"num_predict":48},
 "format":{"type":"object","required":["route","reason"],
  "properties":{"route":{"type":"string","enum":["talk","think","work"]},"reason":{"type":"string","maxLength":80}}},
 "messages":[{"role":"system","content":"Classify the owner's turn. talk: small talk or a quick answer from memory. think: needs reasoning, lookup or tools. work: a coding or long task for another machine. Answer with the JSON object only."},
             {"role":"user","content":"fix the flaky probe test in monarch"}]}
```

The parser, the only producer of a route:

```rust
#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Route { Talk, Think, Work }

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict { pub route: Route, pub reason: String }

pub fn parse(text: &str) -> Verdict {
    let mut de = serde_json::Deserializer::from_str(text.trim());
    match Verdict::deserialize(&mut de).and_then(|v| de.end().map(|_| v)) {
        Ok(v) if v.reason.len() <= 80 => v,
        _ => Verdict { route: Route::Think, reason: "parse_error".into() },
    }
}
```

The head builds its verdict through the same type, with `reason` set to the probabilities, for example `head talk=0.91 think=0.07 work=0.02`.

Commands: `moted router seed` loads the owner's seed file, `moted router label` derives labels from the ledger, `moted router train` fits the head, the temperature and the thresholds, `moted router eval` prints the report, `moted bench router` prints latency.

## Efficiency targets and what to measure

| Metric | Mac | Pi 5 | How measured |
|---|---|---|---|
| Added latency p50, head | <= 20 ms | <= 100 ms | `moted bench router`, 50 turns |
| Added latency p50, with judge on abstain | <= 150 ms | n/a | same, abstain forced |
| Added latency p95 | <= 300 ms | <= 400 ms | same |
| Macro F1 on held-out | >= 0.90 | same | `moted router eval` |
| Wrong `talk` rate | <= 2% | same | confusion matrix in the eval report |
| Expected calibration error | <= 0.08 | same | eval report, after temperature scaling |
| Abstain rate | <= 15% | same | eval report |
| RAM | <= 200 MB for the head | <= 200 MB | process RSS |

Added latency is the time from turn arrival to a parsed verdict, measured while the talk stream runs, because the talk card only needs the verdict before its second chunk. The ledger's `prompt_eval_duration` and `eval_duration` split the judge's cost.

## Open decisions with defaults

1. Head. Default: `fastembed` with bge-small-en-v1.5 and `linfa-logistic`. `model2vec-rs` with potion-base-8M when the Pi misses its latency target.
2. Judge on the Mac. Default: the resident talk model with the JSON schema above. `nimble` through `/v1/systemone` when the judge's accuracy on the labelled set is under 0.90 macro F1, after the owner approves the 9.3 GB pull.
3. Abstain threshold. Default: calibrated top probability under 0.6 routes to `think` on the Pi and to the judge on the Mac.
4. `talk` threshold. Default: tuned with Elkan's cost formula on the labelled set with a wrong `talk` costing five times a wrong `think`, starting at 0.8.
5. Rules. Default: a fenced code block, a unified diff header, a path with a slash and an extension, or a leading shell prompt raises the `work` logit by a fixed amount. No verb lists. No rule produces `talk`.
6. Calibration. Default: temperature scaling on the head's logits. Conformal sets once 500 labels exist, with any set holding more than one route routed to `think`.
7. Seed set. Default: 30 owner-written turns per route before any ledger data.
8. Labels. Default: an owner action on the card, a re-ask or rephrase within 60 s after a `talk` reply marks `talk` wrong, think-lane tool use marks `think` right, a think-lane answer with no tool call under 100 tokens marks `talk` as acceptable, an accepted work hand-off marks `work` right.
9. Retraining. Default: manual `moted router train`, never automatic.
10. Judge prompt. Default: six byte-stable few-shot examples, two per route, so the prefix cache hits.

## Acceptance criteria

- **ROUTE-01** The parser accepts only the exact verdict object, and unknown fields, unknown routes, trailing text, truncation, prose and think blocks all map to `think` with reason `parse_error`. **Verify:** `cargo test -p mote-router parse::` with one case per failure class.
- **ROUTE-02** The rules pre-filter reads only structural signals, can only raise the `think` or `work` logit, and a property test shows it never yields `talk` on random input. **Verify:** `cargo test -p mote-router rules::`.
- **ROUTE-03** The head returns three probabilities that sum to 1 within 0.001, abstains under the threshold, and builds its verdict through the parser type. **Verify:** `cargo test -p mote-router head::`.
- **ROUTE-04** The judge path sends `think: false` and the schema, and returns a parsed verdict within 150 ms p50 on the Mac. **Verify:** `moted bench router --judge` output in the evidence file.
- **ROUTE-05** Every verdict writes a ledger row with input hash, route, probabilities, reason, model, latency split and an empty label. **Verify:** `SELECT` after ten live turns.
- **ROUTE-06** `moted router label` fills labels from owner actions, re-asks, think-lane tool use and work hand-offs in the stated priority. **Verify:** a fixture ledger with one case per rule.
- **ROUTE-07** `moted router train` fits the head, the temperature and both thresholds from the seed file plus the ledger, and prints per-class precision and recall, the confusion matrix and the calibration error. **Verify:** run on the fixture ledger.
- **ROUTE-08** On the held-out set macro F1 is at least 0.90 and the wrong `talk` rate is at most 2%. **Verify:** `moted router eval` report in the evidence file.
- **ROUTE-09** The router runs beside the talk stream and its verdict arrives before the talk's second chunk at the median. **Verify:** scripted test with event order over 20 turns.
- **ROUTE-10** The Pi profile runs the head only and never starts a judge when total memory is under 16 GiB. **Verify:** memory fixture test.
- **ROUTE-11** The head's p50 latency is at most 20 ms on the Mac. **Verify:** `moted bench router --head` output in the evidence file.

## Sources

- Zero-shot intent accuracy of small LLMs: https://arxiv.org/html/2607.27421
- SetFit, rules and encoder baselines on MASSIVE and Banking77: https://arxiv.org/html/2602.18922v2 and https://arxiv.org/abs/2209.11055
- MTEB classification protocol and results: https://github.com/embeddings-benchmark/mteb/blob/main/mteb/abstasks/classification.py and https://github.com/embeddings-benchmark/results
- bge-small: https://huggingface.co/BAAI/bge-small-en-v1.5
- MiniLM: https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2
- model2vec results and crate: https://github.com/MinishLab/model2vec/blob/main/results/README.md and https://crates.io/crates/model2vec-rs
- Rust embedding latency bench: https://github.com/jerrythomas/rust-embedding-bench/blob/main/REPORT.md
- fastembed-rs: https://crates.io/crates/fastembed
- ort: https://crates.io/crates/ort and https://ort.pyke.io/perf/execution-providers
- tract: https://crates.io/crates/tract-onnx
- linfa-logistic: https://docs.rs/linfa-logistic
- Small-model throughput on Apple Silicon: https://arxiv.org/html/2607.00501
- Pi 5 small-model decode: https://www.dfrobot.com/blog-14068.html and https://stratospherelinuxips.readthedocs.io/en/develop/immune/research_rpi_llm_performance.html
- Ollama structured outputs: https://github.com/ollama/ollama/releases/tag/v0.5.0, https://ollama.com/blog/structured-outputs, https://github.com/ollama/ollama/blob/main/llm/gbnf.go
- Ollama thinking field issue: https://github.com/ollama/ollama-python/issues/597
- Ollama decision endpoint: https://github.com/ollama/ollama/blob/main/docs/capabilities/decision.mdx, https://ollama.com/library/nimble, https://ollama.com/library/clef-flash
- llama-server grammar and logprobs: https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md and https://github.com/ggml-org/llama.cpp/pull/10783
- SetFit: https://github.com/huggingface/setfit
- fastText archive: https://github.com/facebookresearch/fastText
- semantic-router: https://github.com/aurelio-labs/semantic-router
- RouteLLM: https://arxiv.org/abs/2406.18665, FrugalGPT: https://arxiv.org/abs/2305.05176, Hybrid LLM: https://arxiv.org/abs/2404.14618
- vLLM semantic router: https://github.com/vllm-project/semantic-router
- Calibration: https://arxiv.org/abs/1706.04599, https://arxiv.org/pdf/2609.19072, https://arxiv.org/abs/2305.14975
- Prototype distances as logits: https://ar5iv.labs.arxiv.org/html/1703.05175
- Conformal abstention: https://arxiv.org/abs/2107.07511
- Cost-sensitive thresholds: https://cseweb.ucsd.edu/~elkan/rescale.pdf and https://scikit-learn.org/stable/auto_examples/model_selection/plot_cost_sensitive_learning.html
- Fail-closed serde: https://serde.rs/container-attrs and https://doc.servo.org/serde_json/error/enum.ErrorCode.html
- Implicit feedback labels: https://arxiv.org/abs/1911.02557
- Labelling tools: https://github.com/argilla-io/argilla, https://github.com/HumanSignal/label-studio, https://github.com/cleanlab/cleanlab, https://github.com/snorkel-team/snorkel
- Repo rule on regex over natural language: `AGENTS.md` in this repository.
