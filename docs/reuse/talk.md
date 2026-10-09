# Reuse: instant talk lane

Concept 1 in `docs/concepts.md`. Serves MOTE-04, MOTE-05 and MOTE-06 in `docs/rfcs/mote.md`.
Written 2026-10-08. Every number below has a source in the last section. Numbers from other
hardware need a run on the owner's Mac before they count as evidence.

## Key points

1. Ollama stays the runtime in v0. `mote-localmodel` already speaks `/api/chat`, `/api/ps` and the memory gate, so the talk lane is a thin client over it. Ollama 0.40.2 is current.
2. The Mac talk model is `qwen3:4b`, which Ollama resolves to Qwen3-4B-Instruct-2507: Apache-2.0, 2.5 GB at Q4_K_M, non-thinking by design, IFEval 83.4. Warm 0.6B to 4B models give 20 to 90 ms TTFT on an M4 Max, so 300 ms has headroom.
3. The Pi 5 talk model is `qwen3:1.7b`, falling to `qwen3:0.6b` when TTFT misses. Measured on a Pi 5: `qwen3:0.6b` decodes at 16.6 t/s and processes prompts at 61.8 t/s, so a 200-token prompt costs 3.3 s uncached. Prefix caching is mandatory there.
4. TTFT is won by a resident model (`keep_alive`), a byte-stable system prompt so the KV prefix cache hits, and a context pack under about 1k tokens. Measured on an M3: 160 ms for a repeated prompt against 812 ms cold.
5. Speculative decoding stays off. Ollama has no draft-model support (issue 5800 open since 2024). llama.cpp on Metal measured 0.95x for a 3B target with a 0.5B draft. It only pays on large targets.
6. The memory gate is `fits_simple(mem, estimate(weights, geometry, ctx), 1.25)` then `approve`. Ollama's own rule is `predicted <= 0.8 x free`, which is the same 1.25 factor over weights plus KV cache.
7. In-process runtimes (`llama-cpp-2`, `mistralrs`) are a later step. `candle` is out: 0.6x llama.cpp on Metal and far slower on CPU.
8. Haiku through `claude -p --model haiku` is the fallback, and the card names the lane (MOTE-05).

## What the concept needs

- Streaming chat over loopback with cancel: start a reply, stop it when the router says `think` or `work` (MOTE-06).
- A resident model: load once, keep it loaded, warm the system prompt so TTFT is prompt-eval of the new turn only.
- A memory gate before every load, over weights plus KV cache, not file size alone.
- Thinking off for the talk model, no tool calls, a short stable system prompt.
- A fallback path to Haiku when the gate or the probe fails.
- One ledger row per call: lane, model, pack ids, prompt tokens, output tokens, TTFT, total latency.
- A scripted `/api/chat` server for tests (MOTE-03).
- A bench command, `moted bench talk`, printing p50 and p95 TTFT and decode tokens per second over 20 turns (MOTE-04).

## Candidates

### Models

| Name | Repo | Language | License | Last release | Use | Reason | Pin |
|---|---|---|---|---|---|---|---|
| Qwen3-4B-Instruct-2507 | https://huggingface.co/Qwen/Qwen3-4B-Instruct-2507 | weights | Apache-2.0 | 2025-08-06 | embed through Ollama (Mac default) | 2.5 GB Q4_K_M, 256K context, non-thinking only, IFEval 83.4, Arena-Hard v2 43.4, MLX 4-bit decodes 145.8 t/s on an M4 Max | `qwen3:4b` |
| Qwen3 0.6B / 1.7B / 8B | https://huggingface.co/Qwen/Qwen3-8B | weights | Apache-2.0 | 2025-04-29 | embed through Ollama (Pi and upgrade) | 0.52 / 1.4 / 5.2 GB, thinking toggle, 8B IFEval 89.4 and 90 t/s MLX on an M4 Max, 1.7B and 8B Pi 5 speed unknown | `qwen3:1.7b`, `qwen3:0.6b`, `qwen3:8b` |
| Qwen3.5 0.8B / 2B / 4B / 9B | https://huggingface.co/Qwen/Qwen3.5-4B | weights | Apache-2.0 | 2026-02 | keep as alternative | 4B IFEval 89.8, but thinking is on by default, Ollama runs it on MLX with parallel forced to 1, and TTFT measured 433 ms cached on an M1 Max through Ollama | `qwen3.5:4b-q4_K_M` |
| Gemma 4 E2B / E4B | https://huggingface.co/google/gemma-4-E4B-it | weights | Apache-2.0 | 2026-04-02 | keep as alternative | License now open, audio and image input, E4B MMLU-Pro 69.4, 80.5 t/s and 62 ms TTFT on an M4 Max with llama.cpp, but 4.6 / 6.6 GB and 7.6 t/s with 7.8 s TTFT on a Pi 5 through LiteRT | `gemma4:e4b-it-q4_K_M` |
| Gemma 3n E2B / E4B | https://huggingface.co/google/gemma-3n-E4B-it | weights | Gemma Terms of Use | 2025-06-26 | skip | 5.6 / 7.5 GB in Ollama, gated license, about 6 / 2.5 t/s on a Pi 5, superseded by Gemma 4 | skip |
| Gemma 3 1B / 4B | https://huggingface.co/google/gemma-3-4b-it | weights | Gemma Terms of Use | 2025-03-12 | skip | 1B decodes 10 to 11.5 t/s on a Pi 5, gated license, no win over Qwen3 | skip |
| Llama 3.2 1B / 3B | https://huggingface.co/meta-llama/Llama-3.2-3B-Instruct | weights | Llama 3.2 Community | 2024-09-25 | keep as Pi reference | 1B 11.2 t/s and 3B 4.7 t/s on a Pi 5, IFEval 59.5 / 77.4, the most benchmarked Pi models | `llama3.2:1b` |
| Phi-4-mini 3.8B | https://huggingface.co/microsoft/Phi-4-mini-instruct | weights | MIT | 2025-02 | keep as alternative | 2.5 GB, 128K, MMLU 67.3, no thinking toggle to manage | `phi4-mini:3.8b` |
| SmolLM3 3B | https://huggingface.co/HuggingFaceTB/SmolLM3-3B | weights | Apache-2.0 | 2025-07-08 | keep as alternative | IFEval 76.7 no-think, no official Ollama tag, 5.2 t/s on a Pi 5 through a community tag | `hf.co/unsloth/SmolLM3-3B-GGUF:Q4_K_M` |
| LFM2 1.2B | https://huggingface.co/LiquidAI/LFM2-1.2B | weights | LFM Open License v1.0 | 2025 | measure on the Pi | 0.73 GB, IFEval 74.9, vendor claims 2x CPU decode against Qwen3, license is not Apache | `hf.co/LiquidAI/LFM2-1.2B-GGUF` |
| Granite 4.0 H-Micro 3B | https://huggingface.co/ibm-granite/granite-4.0-h-micro | weights | Apache-2.0 | 2025-10-02 | measure on the Pi | 1.9 GB, hybrid Mamba2 layers, IFEval 84.3, 1B variant decodes 141.9 t/s on an M4 Max, Pi 5 speed unknown | `granite4:micro-h` |
| Ministral 3 3B | https://huggingface.co/mistralai/Ministral-3-3B-Instruct-2512 | weights | Apache-2.0 | 2025-12-02 | skip for v0 | 3.0 GB with a vision tower, no published Pi or Mac speed | `ministral-3:3b` |

### Runtimes and crates

| Name | Repo | Language | License | Last release | Use | Reason | Pin |
|---|---|---|---|---|---|---|---|
| Ollama | https://github.com/ollama/ollama | Go | MIT | v0.40.2, 2026-10-08 | sidecar (Mac v0) | The contract of `mote-localmodel`, `keep_alive`, JSON schema `format`, `think`, `logprobs`, spawns upstream llama-server so `--cache-prompt` applies, some new architectures run on MLX on Apple Silicon | the installed version, in evidence |
| llama.cpp `llama-server` | https://github.com/ggml-org/llama.cpp | C++ | MIT | b11514, 2026-10-08 | sidecar (Pi) | NEON CPU builds, `--cache-prompt` on by default, `--cache-reuse`, slot save and restore, `--spec-draft-model`, GBNF and JSON schema, OpenAI-compatible API, `--fit` memory placement | a `b` tag recorded in evidence |
| llama-cpp-2 | https://github.com/utilityai/llama-cpp-rs | Rust | MIT OR Apache-2.0 | 0.1.159, 2026-10-07 | embed later | In-process llama.cpp, `metal` feature, KV state save and load, `llguidance` feature for schemas | `0.1.159` |
| mistral.rs | https://github.com/EricLBuehler/mistral.rs | Rust | MIT | git v0.9.4, 2026-09-24 (crates.io 0.8.1 lags and its docs build failed) | embed later | In-process, Metal, ISQ, prefix cache, in-model speculation, Pi support unknown | git tag `v0.9.4` |
| candle | https://github.com/huggingface/candle | Rust | MIT OR Apache-2.0 | `candle-core` 0.11.0, 2026-06-26 | skip | `quantized-qwen3` example exists, but 31.8 against 51.3 t/s for llama.cpp on Metal, and an open CPU issue at 8.5x slower | skip |
| ollama-rs | https://github.com/pepperoni21/ollama-rs | Rust | MIT | 0.3.6, 2026-07-24 | skip | `mote-localmodel` carries its own loopback-only client with the memory guard | skip |
| sysinfo | https://github.com/GuillaumeGomez/sysinfo | Rust | MIT | 0.39.6, 2026-07-09 | skip | `mote-localmodel::memory` already reads `host_statistics64` and `/proc/meminfo` within 5% of the system tools | skip |

### Measured speed

| Model, quant | Hardware | Runtime | Prompt t/s | Decode t/s | TTFT | Source |
|---|---|---|---|---|---|---|
| LLaMA 7B Q4_0 | M1 Pro | llama.cpp Metal | 266 | 36.4 | | llama.cpp 4167 |
| LLaMA 7B Q4_0 | M3 Max | llama.cpp Metal | 760 | 66.3 | | llama.cpp 4167 |
| LLaMA 7B Q4_0 | M4 Max | llama.cpp Metal | 886 | 83.1 | | llama.cpp 4167 |
| LLaMA 7B Q4_0 | M2 Ultra | llama.cpp Metal | 1238 | 94.3 | | llama.cpp 4167 |
| Qwen3 4B 4-bit | M4 Max | MLX | 1495 | 145.8 | | john-rocky |
| Qwen3 8B 4-bit | M4 Max | MLX | 825 | 90.0 | | john-rocky |
| Qwen3.5 2B Q4_K_M | M4 Max | llama.cpp | | 150 | 29 ms | john-rocky |
| Gemma 4 E4B Q4_K_M | M4 Max | llama.cpp | | 80.5 | 62 ms | john-rocky |
| Qwen3.5 4B 4-bit | M1 Max | Ollama 0.24 | | 45.4 | 519 ms cold, 433 ms cached | here.now |
| qwen2.5:7b Q4_K_M | M3 | Ollama | | | 812 ms cold, 160 ms repeated | konjoai, vendor |
| qwen3:0.6b Q4_K_M | Pi 5 8 GB | Ollama | 61.8 | 16.6 | 155 ms warm load | Adafruit |
| gemma3:1b Q4_K_M | Pi 5 8 GB | Ollama | 27.7 | 10.0 | | DigiKey |
| llama3.2:1b Q4_K_M | Pi 5 | Ollama | | 11.2 | | Stratosphere |
| llama3.2:3b Q4_K_M | Pi 5 8 GB | llama.cpp | | 4.6 | | Geerling |
| Qwen3 1.7B / 4B | Pi 5 | | unknown | unknown | | |

The owner's Mac sits in the M-series class above, so a 4B model decodes far above the 30 t/s a spoken or read reply needs. On the Pi 5 prompt processing sets TTFT: at 25 to 60 t/s a 200-token system prompt costs 3 to 8 s uncached, so the Pi path depends on a cached prefix and a context pack under 60 new tokens per turn.

## Recommendation

v0 on the Mac: Ollama, `qwen3:4b`, thinking off, resident. The talk lane calls `mote-localmodel`
directly. No new crate is needed.

```toml
# crates/moted/Cargo.toml (talk lane dependencies)
[dependencies]
mote-localmodel = { path = "../mote-localmodel" }
serde_json = "=1.0.151"
```

Pull and preload the model. The pull needs the owner's approval, per the README status list. An empty `/api/generate` with `keep_alive` loads without generating.

```sh
ollama pull qwen3:4b
curl -s http://127.0.0.1:11434/api/generate -d '{"model":"qwen3:4b","keep_alive":-1}'
```

The chat request the lane sends through `mote_localmodel::ChatRequest`:

```json
{"model":"qwen3:4b","stream":true,"think":false,"keep_alive":"30m",
 "options":{"num_ctx":8192,"temperature":0.7,"num_predict":256},
 "messages":[{"role":"system","content":"<byte-stable talk prompt>"},{"role":"user","content":"morning"}]}
```

The final chunk carries `prompt_eval_count`, `prompt_eval_duration`, `eval_count`, `eval_duration` and `load_duration`, which the ledger row stores.

Pi profile: `llama-server` as a systemd service on the same port and the OpenAI-compatible route. The slot save path keeps the system prompt's KV cache across restarts.

```sh
llama-server -m qwen3-1.7b-q4_k_m.gguf -c 4096 -t 4 --host 127.0.0.1 --port 11434 \
  --cache-reuse 256 --slot-save-path /var/lib/mote/slots --reasoning-format none
```

Later, in-process on the Pi if the sidecar misses its targets:

```toml
[dependencies]
llama-cpp-2 = { git = "https://github.com/utilityai/llama-cpp-rs", tag = "0.1.159" }
# Mac only: features = ["metal"]
```

Replace `tag` with the exact `rev` after `git ls-remote --tags` confirms the tag name.

## Efficiency targets and what to measure

| Metric | Mac target | Pi 5 target | How measured |
|---|---|---|---|
| TTFT p50 | <= 300 ms | <= 1000 ms | `moted bench talk`, 20 turns, warm model |
| TTFT p95 | <= 600 ms | <= 2000 ms | same run |
| Decode | >= 30 t/s | >= 8 t/s | `eval_count` over `eval_duration` from the final chunk |
| Prompt eval on turn 2 | new tokens only | same | `prompt_eval_count` turn 2 against turn 1 |
| Resident size | <= available / 1.25 at load | same | `ollama ps` size against `memory()` |
| Cold load | <= 3 s for 4B Q4 | <= 10 s for 1.7B Q4 | `load_duration` in the first chunk |
| Idle CPU with the model resident | about 0% | about 0% | `top` over 60 s |

Every talk call writes TTFT, total latency, prompt tokens and output tokens to the ledger, so the bench reads the same rows the window shows.

## Open decisions with defaults

1. Mac talk model. Default: `qwen3:4b`. `qwen3:8b` when the 4B TTFT p95 stays under 400 ms. Gemma 4 E4B when voice needs audio input in the same model.
2. Pi talk model. Default: `qwen3:1.7b` Q4_K_M, fall to `qwen3:0.6b` when TTFT p50 misses 1000 ms. LFM2 1.2B and Granite 4.0 H-Micro get one measurement each.
3. Runtime. Default: Ollama on the Mac, `llama-server` on the Pi, in-process only after measurement.
4. `keep_alive`. Default: `30m` on the Mac, `-1` on the Pi. `OLLAMA_KEEP_ALIVE` stays at its 5m default so other users of Ollama are unaffected.
5. Talk context length. Default: 8192 on the Mac through a derived tag, 4096 on the Pi.
6. Speculative decoding. Default: off.
7. Prefix cache. Default: rely on the runtime. The system prompt is byte-stable per thread and the context pack appends after it.
8. Quantization. Default: Q4_K_M.
9. Sampling. Default: temperature 0.7, `num_predict` 256.
10. Ollama's MLX runner. Default: leave it on for architectures Ollama routes there, and record the runner name from `ollama ps` in every bench.

## Acceptance criteria

- **TALK-01** `chat_stream` yields the first chunk and every later chunk in order and reports `eval_count`, `prompt_eval_count` and `load_duration` from the final chunk. **Verify:** `cargo test -p mote-localmodel chat::` against the scripted server.
- **TALK-02** The talk lane refuses to load a model that fails `fits_simple` over `estimate(weights, geometry, ctx)` or fails `approve`, records the refusal in the event log, and takes the Haiku path. **Verify:** memory fixture test for an 8 GiB machine plus the event log row.
- **TALK-03** After a turn the model stays resident with `expires_at` in the future. **Verify:** `ollama ps` after a live turn, output in the evidence file.
- **TALK-04** Turn 2 in a thread reports `prompt_eval_count` no greater than the new tokens plus 16. **Verify:** ledger `SELECT` over two live turns in one thread.
- **TALK-05** A `think` or `work` verdict cancels the stream within one chunk and the card keeps only the acknowledgement. **Verify:** scripted slow-router test and the event order.
- **TALK-06** The request carries `think: false` and no think text reaches a card. **Verify:** scripted and live turn, card text contains no think block.
- **TALK-07** `moted bench talk` prints p50 and p95 TTFT, decode t/s and the runner name, and exits non-zero when p50 exceeds the configured target. **Verify:** run against the scripted server with a delay injected.
- **TALK-08** The hardware profile picks the Pi model and runtime when total memory is under 16 GiB. **Verify:** memory fixture for an 8 GiB Pi.
- **TALK-09** Every talk call, including the Haiku fallback, writes a ledger row with TTFT and total latency. **Verify:** `SELECT` after a live turn with Ollama stopped.

## Sources

- Qwen3 and the 2507 update: https://qwenlm.github.io/blog/qwen3/, https://huggingface.co/Qwen/Qwen3-4B-Instruct-2507, https://huggingface.co/Qwen/Qwen3-8B
- Qwen3.5: https://huggingface.co/Qwen/Qwen3.5-4B and https://ollama.com/library/qwen3.5/tags
- Gemma 4: https://huggingface.co/google/gemma-4-E4B-it and https://ollama.com/library/gemma4/tags
- Gemma 3n and Gemma 3: https://huggingface.co/google/gemma-3n-E4B-it, https://huggingface.co/google/gemma-3-4b-it
- Llama 3.2: https://huggingface.co/meta-llama/Llama-3.2-3B-Instruct
- Phi-4-mini: https://huggingface.co/microsoft/Phi-4-mini-instruct
- SmolLM3: https://huggingface.co/HuggingFaceTB/SmolLM3-3B
- LFM2: https://huggingface.co/LiquidAI/LFM2-1.2B
- Granite 4.0: https://huggingface.co/ibm-granite/granite-4.0-h-micro
- Ollama releases, API, FAQ: https://github.com/ollama/ollama/releases, https://github.com/ollama/ollama/blob/main/docs/api.md, https://docs.ollama.com/faq, https://docs.ollama.com/capabilities/thinking
- Ollama scheduler and memory rule: https://raw.githubusercontent.com/ollama/ollama/main/server/sched.go and https://raw.githubusercontent.com/ollama/ollama/main/llm/llama_server.go
- Ollama speculative decoding issue: https://github.com/ollama/ollama/issues/5800
- llama.cpp Apple Silicon table: https://github.com/ggml-org/llama.cpp/discussions/4167
- llama-server README: https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md
- llama.cpp speculative decoding PR: https://github.com/ggml-org/llama.cpp/pull/10455 and the Metal measurement https://dev.to/cyprus09/is-speculative-decodings-speedup-a-hardware-problem-or-a-model-problem-7k6
- Apple Silicon small-model bench: https://github.com/john-rocky/apple-silicon-llm-bench
- Qwen3.5-4B on an M1 Max: https://bright-lotus-8q5y.here.now/
- Cold against warm load: https://beginnersinai.org/ollama-vs-lm-studio/
- Repeated prompt TTFT (vendor): https://github.com/konjoai/squish
- Pi 5 numbers: https://learn.adafruit.com/local-llms-on-raspberry-pi/qwen3, https://www.digikey.com/en/maker/tutorials/2025/local-llms-on-raspberry-pi, https://stratospherelinuxips.readthedocs.io/en/develop/immune/research_rpi_llm_performance.html, https://github.com/geerlingguy/ai-benchmarks
- llama-cpp-2: https://crates.io/crates/llama-cpp-2
- mistral.rs: https://github.com/EricLBuehler/mistral.rs/releases and https://crates.io/crates/mistralrs
- candle speed issues: https://github.com/huggingface/candle/issues/1939
- ollama-rs: https://crates.io/crates/ollama-rs
- sysinfo: https://crates.io/crates/sysinfo
- `mote-localmodel` API: `docs/rfcs/shared-crates.md`, section `mote-localmodel`, decision 10.
