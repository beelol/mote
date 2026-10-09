# UI language research notes

Date: 2026-10-08. Companion to `docs/rfcs/ui-language.md`.

These are raw notes. Each entry names the source, the facts taken from it, and measured numbers where they exist. "unknown" marks a fact the research did not find.

## 1. Own measurements: token cost per wire format

### Method

- Three fixtures with identical content: a card (title, two lines, two buttons with actions), a table (title, four typed columns, five rows, single selection, one action button), and a form (three fields with validation, submit and cancel actions).
- The JSON fixtures predate two final tree spellings in the RFC: they use `"select":"single"` where the RFC uses `"select":"one"`, and `"kind":"text","required":true` where the RFC uses `"kind":"str"` with required as the default. Each difference is one token or less per occurrence and does not change any ranking.
- Twelve encodings of the same content. JSON variants were generated from one canonical Python dict. YAML was generated with PyYAML `safe_dump`. TOON was generated twice: by hand per the spec, and by the official `@toon-format/toon` encoder (npm, `encode(json)`). S-expression, indentation DSL, Slint-like, Mote UI, and HTML were written by hand with the same content.
- Tokenizers: `o200k_base` (OpenAI GPT-4o, GPT-4.1, GPT-5 family) and `cl100k_base` (GPT-4, GPT-3.5) via `tiktoken 0.x`, and the public `Qwen/Qwen2.5-7B-Instruct` `tokenizer.json` via the `tokenizers` crate bindings as a proxy for a local model.
- Baseline is compact JSON of the lean tree (`{"type":...,"children":[...]}`, no whitespace).
- Fixture files live in the session scratchpad under `fixtures/formats/<format>/<fixture>.<ext>`. The RFC reproduces the card, table, and form in the language and the tree.

### Results per fixture (tokens)

| format | fixture | chars | o200k | cl100k | qwen2.5 | o200k vs JSON lean compact |
|---|---|---|---|---|---|---|
| json-verbose-pretty (type/props/children, 2-space) | card | 934 | 249 | 250 | 262 | +108% |
| json-verbose-pretty | table | 1470 | 443 | 446 | 486 | +80% |
| json-verbose-pretty | form | 1308 | 329 | 329 | 329 | +125% |
| json-lean-pretty (2-space) | card | 748 | 209 | 210 | 222 | +74% |
| json-lean-pretty | table | 1294 | 429 | 432 | 472 | +74% |
| json-lean-pretty | form | 1065 | 282 | 282 | 282 | +93% |
| json-lean-compact | card | 403 | 120 | 119 | 131 | +0% |
| json-lean-compact | table | 816 | 246 | 246 | 286 | +0% |
| json-lean-compact | form | 571 | 146 | 143 | 143 | +0% |
| json-flat-map-compact (root + elements by id, json-render/A2UI style) | card | 485 | 160 | 160 | 172 | +33% |
| json-flat-map-compact | table | 858 | 262 | 263 | 303 | +7% |
| json-flat-map-compact | form | 663 | 193 | 191 | 191 | +32% |
| json-hiccup-compact (JsonML arrays) | card | 345 | 113 | 114 | 126 | -6% |
| json-hiccup-compact | table | 793 | 242 | 243 | 283 | -2% |
| json-hiccup-compact | form | 508 | 139 | 138 | 138 | -5% |
| yaml-lean | card | 404 | 132 | 133 | 145 | +10% |
| yaml-lean | table | 764 | 286 | 287 | 327 | +16% |
| yaml-lean | form | 574 | 180 | 180 | 180 | +23% |
| toon (hand-written) | card | 470 | 139 | 140 | 152 | +16% |
| toon | table | 566 | 215 | 216 | 256 | -13% |
| toon | form | 661 | 184 | 184 | 184 | +26% |
| toon-official (encoder output) | card | 476 | 142 | 143 | 155 | +18% |
| toon-official | table | 603 | 226 | 226 | 266 | -8% |
| toon-official | form | 661 | 184 | 184 | 184 | +26% |
| sexpr | card | 243 | 87 | 87 | 99 | -28% |
| sexpr | table | 487 | 187 | 190 | 234 | -24% |
| sexpr | form | 304 | 84 | 84 | 84 | -42% |
| slint-like | card | 352 | 109 | 110 | 122 | -9% |
| slint-like | table | 888 | 293 | 296 | 340 | +19% |
| slint-like | form | 490 | 135 | 133 | 133 | -8% |
| indent-dsl (no braces) | card | 235 | 82 | 83 | 95 | -32% |
| indent-dsl | table | 454 | 177 | 180 | 224 | -28% |
| indent-dsl | form | 237 | 67 | 66 | 66 | -54% |
| mote-ui (braces) | card | 261 | 86 | 87 | 99 | -28% |
| mote-ui | table | 492 | 181 | 184 | 228 | -26% |
| mote-ui | form | 265 | 71 | 70 | 70 | -51% |
| html (semantic, data attributes) | card | 340 | 119 | 120 | 132 | -1% |
| html | table | 819 | 328 | 328 | 372 | +33% |
| html | form | 506 | 141 | 138 | 138 | -3% |

### Totals (card + table + form)

| format | o200k | cl100k | qwen2.5 | o200k vs JSON lean compact |
|---|---|---|---|---|
| json-verbose-pretty | 1021 | 1025 | 1077 | +99% |
| json-lean-pretty | 920 | 924 | 976 | +80% |
| json-lean-compact | 512 | 508 | 560 | +0% |
| json-flat-map-compact | 615 | 614 | 666 | +20% |
| json-hiccup-compact | 494 | 495 | 547 | -4% |
| yaml-lean | 598 | 600 | 652 | +17% |
| toon (hand) | 538 | 540 | 592 | +5% |
| toon-official | 552 | 553 | 605 | +8% |
| sexpr | 358 | 361 | 417 | -30% |
| slint-like | 537 | 539 | 595 | +5% |
| indent-dsl | 326 | 329 | 385 | -36% |
| mote-ui | 338 | 341 | 397 | -34% |
| html | 588 | 586 | 642 | +15% |

### Variants

Short JSON keys (`type` to `t`, `children` to `c`, `label` to `l`, `action` to `a`, `text` to `x`), o200k: card 120 to 120, table 246 to 246, form 146 to 146. Saving 0%. Single common words are already one token.

Whitespace in JSON, o200k, card / table / form. Compact: 120 / 246 / 146. Newline per value with no indent: 141 / 309 / 185 (+18% to +27%). One-space indent: 186 / 377 / 252 (+55% to +73%).

### Observations

- Whitespace is the biggest cost in JSON. Pretty JSON is +80% over compact. Every published "JSON is expensive" number that uses pretty JSON as the baseline overstates the gap to other formats.
- Key shortening gains nothing. Full words stay.
- Hiccup arrays beat object JSON by 4%. The saving is too small to pay for a tuple-shaped schema that strict structured outputs cannot express.
- The flat id map (A2UI, json-render) costs +20% over the nested tree because every node repeats an id and parents list child ids.
- TOON wins only on the table (-8% to -13%) and loses on nested nodes (+16% to +26%). The official encoder output is 3% worse than the hand-written version because it quotes strings with commas and expands a one-row table.
- The brace language costs 4% more than the same language without braces (338 vs 326). Braces buy explicit close markers for partial parsing.
- The language is 34% cheaper than compact JSON and 63% cheaper than pretty JSON. Most of the saving comes from inferred keys (`field title str` instead of `"type":"field","name":"title","kind":"str"`), not from punctuation.
- Qwen2.5 tokenizes all formats about 8% to 17% higher than o200k, with the same ordering.

## 2. Wire formats and constrained decoding (published)

### TOON

- Spec and encoder: https://toonformat.dev/ , https://cdn.jsdelivr.net/npm/@toon-format/toon@2.3.0/README.md , https://raw.githubusercontent.com/toon-format/spec/main/SPEC.md
- Tabular header `key[N]{f1,f2}:` then one delimited row per item. `[N]` declares row count so truncation is detectable. Delimiters comma, tab, pipe. Indentation spaces only. Strings quoted only when ambiguous.
- Strict mode errors on row count mismatch, cell count mismatch, bad indentation, duplicate keys. Non-strict mode recovers from five conditions.
- Streaming: `encodeLines()`, `decodeFromLines()`, `decodeStream()` exist in the reference library.
- The spec says TOON is "designed primarily for LLM prompts" and gives no rules for model-generated output.
- Official benchmark (README 2.3.0): 209 questions x 6 formats x 4 models, o200k_base, baseline 2-space pretty JSON. TOON 76.4% accuracy at 2,759 tokens, JSON 75.0% at 4,587, compact JSON 73.7% at 3,104, YAML 74.5% at 3,749, XML 72.1% at 5,221. TOON is 39.9% fewer tokens than pretty JSON. Mixed-structure track: TOON +14.7% tokens vs compact JSON. Nested config +11.1% vs compact JSON. Event logs +19.9% vs compact JSON. Truncated-array validation dataset: TOON 0.0% vs CSV and XML 100%.
- README caveat: local or quantized deployments (Ollama named) may process compact JSON faster despite lower token count.
- improvingagents.com TOON benchmark: https://www.improvingagents.com/blog/toon-benchmarks . GPT-4.1-nano, 1,000 flat records: TOON 47.5% at 21,518 tokens, JSON 52.3% at 66,396, Markdown-KV 60.7% at 52,104, CSV 44.3% at 19,524. Nested, GPT-5 nano: TOON 43.1% at 45,436, YAML 62.1% at 42,477, JSON 50.3% at 57,933. Authors "failed to find circumstances where TOON was the best-performing format."
- arXiv 2601.12014 "Are LLMs Ready for TOON?": https://arxiv.org/html/2601.12014v1 . 8 open models generating StructEval outputs. TOON 217.94 generated tokens vs JSON 296.15 (-26.4%). Syntax score TOON 0.484 vs JSON 0.802. Gemma 3 4B scored 0.045 under TOON vs 0.779 under JSON. Llama 3.3 70B TOON 0.819 vs JSON 0.808.
- arXiv 2603.03306 generation benchmark: https://arxiv.org/html/2603.03306v1 . 21 models x 4 cases x 10 runs. Users case: plain JSON 94.8% at 1,078 tokens, constrained JSON 92.9% one-shot then 100% at 556 tokens, TOON 90.5% at 840. Invoice case: TOON 0.0% one-shot, 52.4% final, 3,626 tokens vs plain JSON 90.0% at 1,723. Author advises against TOON for deep hierarchies.
- arXiv 2605.29676 "Notation Matters": https://arxiv.org/html/2605.29676v1 . TOON cuts tool schemas up to 23% and tool results up to 32% but gives no net saving on tool calls because of parse failures. BFCL accuracy deltas: Mistral-Small-24B -36.1 pp, Qwen3-32B no-think -30.2 pp. Authors advise against TOON as a default for multi-turn agents.

### JSON vs YAML vs XML vs CSV vs Markdown

- improvingagents flat table, GPT-4.1-nano, accuracy at tokens: https://www.improvingagents.com/blog/best-input-data-format-for-llms . Markdown-KV 60.7% at 52,104. XML 56.0% at 76,114. INI 55.7% at 48,100. YAML 54.7% at 55,395. HTML 53.6% at 75,204. JSON 52.3% at 66,396. Markdown table 51.9% at 25,140. Natural language 49.6% at 43,411. JSONL 45.0% at 54,407. CSV 44.3% at 19,524. Pipe 41.1% at 43,098.
- improvingagents nested (6 to 7 levels): https://www.improvingagents.com/blog/best-nested-data-format/ . GPT-5 Nano: YAML 62.1% at 42,477, Markdown 54.3% at 38,357, JSON 50.3% at 57,933, XML 44.4% at 68,804. Gemini 2.5 Flash Lite: YAML 51.9%, Markdown 48.2%, JSON 43.1%, XML 33.8%. Llama 3.2 3B: JSON 52.7% best. Markdown used the fewest tokens for every model.
- Microsoft tabular representation (arXiv 2310.10358): https://arxiv.org/html/2310.10358v1 . text-davinci-003 fact-finding pass@1: DFLoader 79.79, JSON 77.93, TSV 75.80, CSV 75.78, HTML 71.40, Markdown 67.32. Transformation F1: JSON 94.89, Markdown 36.11. HTML verbosity halved rows that fit a 4,097-token window.
- Matt Rickard, one object, OpenAI tokenizer: https://blog.matt-rickard.com/p/a-token-efficient-language-for-llms . Pretty JSON 162, XML 201, TOML 91, YAML 85, INI 84, HCL 79, minified JSON 64, custom "Tok-son" 61. LLaMA tokenizer: JSON 149, YAML 98, minified JSON 84. Each repeated space costs about one token.
- Tam et al. appendix: YAML cheapest for several GPT-3.5 tasks. Plain text cheapest for Claude 3 Haiku. Qualitative only. https://arxiv.org/html/2408.02442v1

### Format restriction and reasoning

- Tam et al. 2024 "Let Me Speak Freely" (arXiv 2408.02442): https://arxiv.org/html/2408.02442v1 . GSM8K text vs JSON-schema: GPT-3.5 76.60 vs 49.25. Claude 3 Haiku 86.51 vs 23.44. LLaMA 3 8B 74.73 vs 48.90. Gemini 1.5 Flash 89.33 vs 89.21. Last Letter LLaMA 3 8B 70.07 vs 28.00. For GPT-3.5 JSON mode, 100% of responses put "answer" before "reason". Classification went the other way: Gemini DDXPlus 41.59 text vs 60.36 JSON.
- dottxt "Say What You Mean": https://blog.dottxt.ai/say-what-you-mean.html . Llama-3-8B-Instruct re-run: GSM8K 0.77 unstructured vs 0.78 structured. Last Letter 0.73 vs 0.77. Shuffle 0.41 vs 0.44. Critique: different prompts per condition, JSON-mode prompt never mentions a schema, the Claude-based answer parser scored below a regex.
- Dylan Castillo replication, GPT-4o-mini: https://dylancastillo.co/posts/say-what-you-mean-sometimes.html . GSM8K NL 94.31 vs JSON-schema 93.48. Shuffled Objects NL 95.12 vs FRI 79.67 vs JSON-schema 89.84. Conclusion: depends on task, model, and prompts.
- JSONSchemaBench (arXiv 2501.10868): https://arxiv.org/html/2501.10868v2 . Llama-3.1-8B-Instruct GSM8K LM-only 80.1% vs Guidance 83.8%. Last Letters 50.7 vs 54.0. Shuffle 52.6 vs 55.9. All constrained engines scored above unconstrained.
- "Capacity, Not Format" (arXiv 2606.09410): https://arxiv.org/abs/2606.09410 . Haiku -36.2 pp under simultaneous JSON (mostly truncation). GPT-4o-mini -28.0 pp. Opus 4.7 AIME 96.2% to 91.0%. Sonnet MATH-Hard 88.7% JSON vs 89.3% CoT. Deferred formatting (reason first, format after) recovers to 80% to 87%.

### Constrained decoding tools

- Outlines: https://github.com/dottxt-ai/outlines . JSON Schema, regex, CFG, choice, function signatures. Backends: transformers, llama.cpp, vLLM, Ollama, OpenAI, Gemini. outlines-core in Rust: https://dottxt.ai/announcements/hugging-face-outlines-outlines-core-0-1-0-structured-generation-in-rust-and-python . JSONSchemaBench llama.cpp backend medians: grammar compile 3.48 s, TTFT 3.65 s, TPOT 30.33 ms vs LM-only TTFT 0.10 s, TPOT 15.40 ms. Recursive JSON schema not supported per https://github.com/noamgat/lm-format-enforcer .
- llguidance: https://github.com/guidance-ai/llguidance . Lark-like CFG with embedded JSON Schema and regex. Mask computation about 50 µs single-core per token for a 128k tokenizer. Full mask for a typical JSON schema about 1.5 ms. Under 1% of masks exceed 1 ms. Integrations: Guidance, OpenAI Structured Output, llama.cpp, Chromium, SGLang, vLLM, TensorRT-LLM, mistral.rs, onnxruntime-genai. llama.cpp doc: https://github.com/ggml-org/llama.cpp/blob/master/docs/llguidance.md . JSON Schema Bench with llama3 tokenizer avg 50 µs, p99 0.5 ms, p100 20 ms. Build with `-DLLAMA_LLGUIDANCE=ON`. Grammars starting `%llguidance` route to it. `gbnf_to_lark.py` converts GBNF. JSONSchemaBench: Guidance TPOT 6.37 ms vs LM-only 15.40 ms through fast-forwarding of forced tokens.
- XGrammar: https://arxiv.org/html/2411.15100v3 and https://blog.mlc.ai/2024/11/22/achieving-efficient-flexible-portable-structured-generation-with-xgrammar . EBNF, JSON Schema, regex. Per-token mask under 40 µs for JSON Schema and CFG(JSON), under 200 µs for XML and Python DSL grammars. MLC-LLM TPOT with/without, Llama-3.1-8B: batch 1 6.2 vs 6.3 ms. Integrations: MLC-LLM, SGLang (default), vLLM, WebLLM, TensorRT-LLM, HF transformers. XGrammar-2 (arXiv 2601.04426): https://arxiv.org/html/2601.04426v4 . EBNF compile for 100 tool tags 15,483.5 ms vs TagDispatch 2,002.1 ms. Gap to unconstrained "no more than 6%". Reports llguidance at about 250 µs/token on OpenAI Harmony format.
- lm-format-enforcer: https://github.com/noamgat/lm-format-enforcer . JSON Schema, schemaless JSON, regex. No CFG. Model controls whitespace and field order. Per-token overhead unknown.
- vLLM: https://docs.vllm.ai/en/stable/features/structured_outputs/ . Backends xgrammar and guidance. Constraint types choice, regex, json, grammar (EBNF), structural_tag.
- SGLang: https://docs.sglang.io/advanced_features/structured_outputs.html . XGrammar default, Outlines, llguidance. One constraint per request.
- llama.cpp GBNF: https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md . `name ::= sequence`, `root` start rule, alternation, grouping, repetition, ranges. Passed via `grammar` on llama-server or `--grammar-file`. `json_schema` converts to GBNF with limits: `prefixItems` broken, nested `$ref` broken, `patternProperties` unsupported, unsupported keywords skipped silently. `x? x? x?` patterns are extremely slow, use `x{0,N}`. Apple Silicon grammar-specific timing: unknown.
- Ollama: https://ollama.com/blog/structured-outputs (2024-12-06). `format` accepts a JSON schema only. "Additional format support beyond JSON schema" listed as future work. Page advises "return as JSON" in the prompt and temperature 0. GBNF PR #2404 opened 2024-02-08, closed unmerged 2024-12-05 in favor of JSON schema: https://github.com/ollama/ollama/pull/2404 . A non-JSON DSL grammar needs llama-server directly.
- Apple Silicon structured-output benchmark (blog, 2026-06-24, methodology partly disclosed): https://contracollective.com/blog/structured-outputs-apple-silicon-outlines-llama-cpp-grammars-mlx-2026 . 128 GB M5 Max, MLX 0.21, llama.cpp b4900, Outlines 0.1.4, 5,000 tool calls. Decode tok/s unconstrained / Outlines MLX / llama.cpp GBNF / MLX logit bias: Llama 3.3 70B Q4 42.1 / 38.4 / 31.6 / 41.2. Qwen3 32B Q5 68.2 / 62.8 / 49.4 / 67.1. Gemma 4 27B Q6 71.4 / 65.8 / 51.2 / 69.6. Throughput tax: Outlines 8% to 9%, GBNF 25% to 28%, logit bias 2% to 3%. First-token overhead: Outlines 14 ms, GBNF 38 ms, logit bias 4 ms. Outlines FSM build 60 to 200 ms per schema. Validity: Outlines 100% parse / 100% validate, GBNF 99.8% / 99.2%, logit bias 99.4% / 97.6%. Unconstrained validate: Llama 88.4%, Qwen 91.6%. 50-step loop survival on Llama: unconstrained 0.2%, Outlines 100%, GBNF 67.0%, logit bias 30.4%. Not independently verified.
- Outlines on mlx-lm: LM Studio describes wrapping Outlines' JSON logits processor into `mlx_lm` `generate_step` custom logits processors: https://lmstudio.ai/blog/lmstudio-v0.3.4 . `outlinesmlx` adapter on PyPI tracks Outlines 0.0.27: https://pypi.org/project/outlinesmlx/0.0.27 . vllm-mlx has a constrained-decoding module: https://vllm-mlx.is-a.dev/reference/api/vllm_mlx/constrained/ . No official mlx-lm grammar support found: unknown.
- Apple Foundation Models guided generation: https://developer.apple.com/videos/play/wwdc2025/286 and https://arxiv.org/pdf/2507.13575 . `@Generable` and `@Guide` macros. The OS daemon uses constrained decoding and speculative decoding. WWDC presenters state constrained decoding guarantees structural correctness. Declaration order is generation order. `PartiallyGenerated` types expose each property as optional for streaming. Third-party guide reports a 4K token on-device budget: https://www.atelier-socle.com/en/articles/foundation-models-api-guide . Supported hardware includes Apple Silicon Macs.

### Hosted structured outputs

- OpenAI Structured Outputs: https://openai.com/index/introducing-structured-outputs-in-the-api/ . gpt-4o-2024-08-06 "scores a perfect 100% on our evals of complex JSON schema following". gpt-4-0613 under 40%. Guide: https://developers.openai.com/api/docs/guides/structured-outputs . Supported: string, number, boolean, integer, object, array, enum, anyOf, `$defs`/`$ref`, recursion via `#`, string pattern and format, numeric bounds, array minItems/maxItems. Limits: 5,000 properties, 10 nesting levels, 120,000 chars of names and enum strings, 1,000 enum values. Root must be an object. All fields required. `additionalProperties: false`. Unsupported: allOf, not, if/then/else, dependentRequired. First request with a new schema adds latency.
- OpenAI custom tools with grammars (GPT-5 family): https://developers.openai.com/api/docs/guides/function-calling . Free-form tool input constrained with `syntax: "lark"` or `"regex"`, implemented with llguidance. Unsupported Lark: lookarounds, lazy modifiers, terminal priorities, templates, `%declare`, imports other than `%import common`. Grammar size limit reported at 20,000 characters: https://community.openai.com/t/lark-grammar-length-limit-is-too-low/1356794 . GPT-5.2 guide confirms CFG support for custom tools: https://developers.openai.com/api/docs/guides/latest-model/gpt-5.2 . No published adherence number.
- Anthropic structured outputs: https://platform.claude.com/docs/en/build-with-claude/structured-outputs.md . JSON outputs via `output_config.format` with `type: "json_schema"` and strict tool use via `strict: true`. GA on Claude API, AWS, Google Cloud, Microsoft Foundry. Supported: object, array, string, integer, number, boolean, null, enum, const, anyOf, allOf (not with `$ref`), internal `$ref`/`$defs`, required, `additionalProperties: false` (required), string formats, array minItems 0 or 1, simple regex. Unsupported (400): recursive schemas, numeric constraints, minLength/maxLength, other array constraints, regex backreferences and lookaround. Limits: 20 strict tools per request, 24 optional parameters, 16 union-typed parameters, 180 s compile timeout. Compiled grammar cached 24 h. Changing the format invalidates the prompt cache. A system prompt is injected. Invalid outputs produce `stop_reason: "refusal"` or `max_tokens` truncation.
- Anthropic streaming: https://platform.claude.com/docs/en/build-with-claude/streaming . Tool input streams as `input_json_delta` events with `partial_json` fragments. "Current models only support emitting one complete key and value property from input at a time," so the API buffers per property. Fine-grained tool streaming: https://platform.claude.com/docs/en/agents-and-tools/tool-use/fine-grained-tool-streaming.md . `eager_input_streaming: true` per tool removes buffering and validation. "You might receive partial or invalid JSON." JSON outputs via `output_config.format` arrive in a text block and stream as text deltas.

### Partial JSON parsing and repair

- Vercel AI SDK `parsePartialJson`: https://github.com/vercel/ai/blob/main/packages/ai/src/util/parse-partial-json.ts . Tries `JSON.parse`, then `fixJson`, then parse again. States: successful-parse, repaired-parse, failed-parse, undefined-input. `fixJson`: https://github.com/vercel/ai/blob/main/packages/ai/src/util/fix-json.ts . Single linear scan with a state stack and `lastValidIndex`. Truncated strings kept and closed. Arrays and objects closed from the stack. Partial literals completed (`tru` to `true`). Trailing comma dropped. Trailing `.`, `e`, `-` cut.
- best-effort-json-parser (TypeScript): https://github.com/beenotung/best-effort-json-parser . `parse('[1, 2, {"a": "apple')` returns `[1, 2, { a: 'apple' }]`.
- jiter (pydantic, Rust): https://simonwillison.net/2024/Sep/22/jiter/ . `partial_mode` off raises, on drops the trailing unfinished string, trailing-strings keeps it.
- partial-json-parser (Python): https://github.com/promplate/partial-json-parser . `Allow` flags per value kind. `loads('{"key": "v', STR | OBJ)` gives `{'key': 'v'}`.
- json_repair (Python): https://github.com/mangiucugna/json_repair . Fixes missing quotes, commas, brackets, comments, stray prose, truncated values. `stream_stable=True` keeps partial output stable while streaming.
- jsonrepair (npm): https://www.npmjs.com/package/jsonrepair . Repairs truncation, missing commas, trailing commas, comments, code fences. Throws when unsalvageable.
- YAML partial parsing for LLM streams: no LLM-oriented partial YAML parser found. rust-yaml has a `StreamingYamlParser` with no documented partial-value semantics: https://docs.rs/rust-yaml/latest/rust_yaml/streaming_enhanced/struct.StreamingYamlParser.html . Partial-value behavior unknown.
- S-expression incremental parsers for LLM streams: none found. Unknown.

### S-expressions, indentation DSLs, markup

- S-expression output token cost or accuracy vs JSON: no paper or benchmark found. Unknown. Token Sugar (arXiv 2512.08266): shorthand code syntax cut up to 15.1% of source tokens and up to 11.2% of generated tokens with Pass@1 nearly unchanged. https://www.arxiv.org/pdf/2512.08266 . "Tokenmaxxing" (arXiv 2607.22807): accuracy mostly language-independent across output languages. https://arxiv.org/pdf/2607.22807
- Pug vs HTML (Hackernoon, 5 samples, cl100k_base): https://hackernoon.com/we-measured-the-llm-token-cost-of-4-markup-formats-tsx-costs-94percent-more-than-pug . Relative to Pug 1.000: HTML 1.608, JSX 1.879, TSX 1.944. 15 closing tags cost 48 tokens (31% of the sample). Author notes Claude tokenizers run about 30% higher and that Pug is rarer in training data. Haml and Slim counts unknown.
- "Hidden Cost of Readability" (arXiv 2508.13666): https://arxiv.org/abs/2508.13666 . Removing indentation and newlines from code cut input tokens 24.5% on average across 10 LLMs with no accuracy loss. Removing formatting alone gave negligible output savings.
- Cloudflare "Markdown for Agents": https://blog.cloudflare.com/markdown-for-agents/ . One blog post 16,180 HTML tokens vs 3,150 Markdown tokens (-80%). Tokenizer unspecified.
- Ubuntu docs, cl100k_base: https://discourse.ubuntu.com/t/llms-txt-for-documentation/79900 . Markdown 68,855, stripped HTML 65,023, raw HTML 482,860.
- Skyvern, DOM element data: https://www.skyvern.com/blog/html-vs-json-llm-tokens-cost-reduction-success-rate/ . One element 31 tokens as HTML vs 70 as JSON. HTML 20% to 27% fewer tokens than JSON across tasks. Production A/B (about 1,100 tasks): HTML 63.8% success at $1.08 p50 vs JSON 59.9% at $1.22.
- Three-way HTML vs Markdown vs JSON for identical UI content: unknown. Section 1 of this file fills that gap for the three fixtures.
- JsonML: https://en.wikipedia.org/wiki/JsonML . Element is `[tag, attributes?, children...]`. Simon Willison (2009) notes many common cases are larger than the XML they replace: https://simonwillison.net/2009/Feb/10/jsonml/ . No LLM-specific evidence found.

### Generative UI research papers

- "Generative UI: LLMs are Effective UI Generators" (arXiv 2604.09577, submitted 2026-02-24, Leviathan et al.): https://arxiv.org/abs/2604.09577 . Claims a modern LLM can robustly generate custom UIs for nearly any prompt with proper prompting and tools. Human preference over Markdown output is "overwhelming" when speed is ignored. Against expert-crafted UIs the outputs are worse overall but "at least comparable in 50% of cases." Releases the PAGEN dataset. Emit representation not stated in the abstract: unknown.
- "Generative Interfaces for Language Models" (arXiv 2508.19227, ACL 2026 Findings, Chen et al.): https://arxiv.org/abs/2508.19227 . Uses "structured interface-specific representations" and iterative refinement. Generative interfaces beat chat with up to 72% improvement in human preference. Code at SALT-NLP/GenUI. Format not named in the abstract.
- Thesys "Why generating code for generative UI is a bad idea" (2025-07-10, updated 2026-06-05): https://www.thesys.dev/blogs/generating-code-for-generative-ui-is-a-bad-idea . Body text was not retrievable by fetch. Argument content unknown.

## 3. Declarative UI languages and architectures

### Slint

- Syntax: components `export component Name inherits Base { ... }`, properties `name: value;`, nested elements by braces. https://docs.slint.dev/latest/docs/slint/guide/language/coding/animation/
- Bindings: an expression assigned to a property is a binding. The runtime records dependencies and marks dependents dirty. Evaluation is lazy. An imperative assignment breaks the binding permanently. https://docs.slint.dev/latest/docs/slint/guide/language/concepts/reactivity/
- Two-way binding `a <=> b`. Typing into a `TextInput` counts as an assignment and breaks a one-way binding. Same URL.
- Purity: side effects are rejected in binding expressions, pure functions, and pure callbacks. Same URL.
- Callbacks: `callback hello(int, int) -> int;`, handled with `hello => { ... }`. https://docs.slint.dev/latest/docs/slint/guide/language/coding/functions-and-callbacks/
- States: `states [ active when cond : { el.prop: value; } ]`. https://docs.slint.dev/latest/docs/slint/guide/language/coding/states/
- Layout: `HorizontalLayout`, `VerticalLayout`, `GridLayout`. Children stretch by default. `alignment` takes stretch, start, end, center, space-between, space-around. `min-*`, `max-*`, `preferred-*`, `horizontal-stretch`, `vertical-stretch`, `spacing`, `padding`. `GridLayout` row and col must be compile-time constants. https://docs.slint.dev/latest/docs/slint/guide/language/coding/positioning-and-layouts/
- Repetition: `for data in root.model : Element { ... }`, `if cond : Element {}`. https://docs.slint.dev/latest/docs/slint/guide/language/coding/repetition-and-data-models/
- ListView instantiates only visible rows. ScrollView instantiates all children. https://docs.slint.dev/latest/docs/slint/reference/std-widgets/views/listview/
- Rust model API: `slint::ModelRc<T>`, `Model` trait (`row_count`, `row_data`, `model_tracker`), `VecModel<T>`. https://docs.rs/slint/latest/slint/trait.Model.html
- Runtime loading from a string: yes. `slint-interpreter` `Compiler::build_from_source`, diagnostics, `ComponentDefinition`, `ComponentInstance` with `set_property`, `get_property`, `set_callback`, dynamic `Value`. https://docs.slint.dev/latest/docs/rust/slint_interpreter/
- Focus and keyboard: `FocusScope`, `key-pressed`, `focus-on-tab-navigation`, `KeyBinding`. https://docs.slint.dev/latest/docs/slint/reference/keyboard-input/focusscope/
- Std widgets: Button, CheckBox, ComboBox, ProgressIndicator, Slider, SpinBox, Switch, LineEdit, ListView, ScrollView, StandardListView, StandardTableView, TabWidget, TextEdit, GridBox, GroupBox, DatePickerPopup, TimePickerPopup. https://docs.slint.dev/latest/docs/slint/reference/std-widgets/overview/ Styles: fluent, material, cupertino, cosmic (each with light and dark), qt, native. Style fixed at compile time or via `ComponentCompiler::set_style()`. https://docs.slint.dev/latest/docs/slint/reference/std-widgets/style/
- Accessibility: not found in fetched docs. Unknown from this pass. Loading, empty, and error states: none built in.
- Backends: qt, winit, linuxkms. Renderers: software (no GPU, partial and line-by-line rendering, `no_std`, limited to western scripts, no drop shadow or rotation), FemtoVG (OpenGL), Skia, Vello (experimental). https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers/
- LinuxKMS: no X11 or Wayland. DRM/KMS. `SLINT_BACKEND=linuxkms-software` uses DRM dumb buffers with a legacy `/dev/fbX` fallback (`SLINT_BACKEND_LINUXFB=1`). Input via libinput and libudev. libseat avoids root. The page labels the backend "still experimental". The software renderer does not draw a mouse cursor. https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_linuxkms/
- Minimum hardware: no official minimum published. Embedded Linux needs OpenGL ES 2.0 or Vulkan for GPU renderers. The 2022 MCU port ran on an RP2040 with 264 KB RAM driving a 320x240 display. https://slint.dev/blog/porting-slint-to-microcontrollers
- License: GPL-3.0-only, or Royalty-free 2.0 (proprietary desktop, mobile, web at no cost, embedded excluded), or commercial. https://cdn.jsdelivr.net/gh/slint-ui/slint@1.16.0/LICENSE.md

### QML

- Object declarations with braces. JavaScript expression bindings with automatic dependency tracking. Assigning a static value removes the binding. https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
- Signals `signal activated(x: real)`, handlers `onActivated`, automatic `on<Prop>Changed`. https://doc.qt.io/qt-6/qtqml-syntax-signals.html
- QML "extends a standards-compliant JavaScript engine". https://doc.qt.io/qt-6/qtqml-javascript-topic.html
- Anchors: seven anchor lines plus `anchors.fill` and margins. Layouts: `RowLayout`, `ColumnLayout`, `GridLayout` with `Layout.*` attached properties. https://doc.qt.io/qt-6/qtquicklayouts-overview.html
- Runtime from string: `Qt.createQmlObject` exists and Qt warns it is "extremely slow". https://doc.qt.io/qt-6/qml-qtqml-qt.html
- License: commercial, LGPLv3, or GPLv3. https://doc.qt.io/qt-6/licensing.html

### SwiftUI

- Result builders (`@ViewBuilder`), views as structs that may be reinitialized at any time. https://developer.apple.com/documentation/swiftui/declaring-a-custom-view
- `@State` owner, `Binding` borrower, `@Observable`, environment. https://developer.apple.com/documentation/swiftui/state
- Lists carry scrolling, selection, edit mode, refresh. https://developer.apple.com/documentation/swiftui/list . Accessibility labels on standard controls. https://developer.apple.com/documentation/swiftui/accessibility-fundamentals . Dark mode via `colorScheme`. https://developer.apple.com/documentation/swiftui/colorscheme
- License: proprietary (Xcode SDK agreement).

### Jetpack Compose

- Composable functions emit UI and return nothing. Recomposition re-runs changed functions. Side effects forbidden in composition. https://developer.android.com/develop/ui/compose/mental-model
- `remember { mutableStateOf }`, `rememberSaveable`. https://developer.android.com/develop/ui/compose/state
- Unidirectional data flow: state flows down, events flow up. Sealed `UiState` (SignedOut, InProgress, Error, SignedIn). https://developer.android.com/develop/ui/compose/architecture
- Modifier chains are ordered. No margin, only padding. https://developer.android.com/develop/ui/compose/modifiers
- MaterialTheme, dark theme, dynamic color. https://developer.android.com/develop/ui/compose/designsystems/material3 . Semantics tree for accessibility. https://developer.android.com/develop/ui/compose/accessibility
- License: Apache 2.0.

### Flutter, GenUI, A2UI (from the DSL pass)

- Widget tree of immutable widgets. `StatelessWidget`, `StatefulWidget` with `State`, `setState`. https://docs.flutter.dev/ui
- Accessibility: screen readers, 4.5:1 contrast, 48x48 tap targets, text scaling. https://docs.flutter.dev/ui/accessibility-and-internationalization/accessibility
- GenUI SDK: "an orchestration layer" using "a JSON-based format to compose a UI from your existing widget catalog". Alpha. https://docs.flutter.dev/ai/genui . `Catalog` of `CatalogItem` with name, schema, builder. Observable `DataModel`. Messages `createSurface`, `surfaceUpdate`, `dataModelUpdate`, `deleteSurface`. https://docs.flutter.dev/ai/genui/components . Example `{ "Text": { "text": "Welcome to GenUI", "hint": "h1" } }`. https://github.com/flutter/genui/blob/main/packages/genui/README.md . BSD-3, "highly experimental", uses A2UI v0.9.
- A2UI: Apache 2.0, "declarative data format, not executable code". https://github.com/google/A2UI . JSON Lines messages with `"version": "v0.9"` and one top-level key: `createSurface`, `updateComponents`, `updateDataModel`, `deleteSurface`. https://a2ui.org/reference/messages/ . Flat component list with `id`, `component`, child id refs. Values literal or `{ "path": "/user/name" }` JSON Pointer bindings. https://a2ui.org/concepts/data-binding/ . Roadmap: Lit, Angular, React, Flutter stable. SwiftUI and Compose planned. https://a2ui.org/roadmap/

### Elm and elm-ui

- Model, Update, View. `Msg` is a closed sum type. `update` is pure. https://guide.elm-lang.org/architecture/
- elm-ui: "a complete alternative to HTML and CSS". `row`, `column`, `el`, `wrappedRow`. `width fill`, `fillPortion n`, `shrink`, `px`. No margins. `padding` and `spacing`. `above`, `below`, `inFront`. `clip`, `scrollbars`. https://github.com/mdgriffith/elm-ui and https://raw.githubusercontent.com/mdgriffith/elm-ui/master/src/Element.elm
- No built-in accessibility, dark mode, or focus found. Renders to DOM. BSD-3.

### HTMX, _hyperscript, Datastar

- HTMX: `hx-get`, `hx-post`, `hx-trigger`, `hx-target`, `hx-swap` (innerHTML default, outerHTML, beforebegin, afterend, delete, none). Server replies with HTML. https://htmx.org/docs/ . About 14k min.gz. 0BSD. https://github.com/bigskysoftware/htmx
- Hypermedia-driven applications: state rendered into markup by the server. https://htmx.org/essays/hypermedia-driven-applications/
- _hyperscript: `on click add .clicked`, `on first click`, `queue last`, filters, `send foo to the next <output/>`. https://hyperscript.org/docs/events . 0BSD.
- Datastar: `data-signals`, `data-bind`, `data-text`, `data-computed`, `data-show`, `data-on:click="@post('/endpoint')"`. Backend patches signals (JSON Merge Patch, RFC 7396) and elements via SSE events `datastar-patch-signals` and `datastar-patch-elements`, morphed by element id. 11.89 KiB. MIT. https://github.com/starfederation/datastar and https://data-star.dev/guide/reactive_signals

### Typst

- Markup first, `#` enters code mode, content blocks in `[...]`, set rules as defaults. https://github.com/typst/typst
- Set and show rules: `#set text(red) if critical`, `#show heading: it => ...`, selectors. https://typst.app/docs/reference/styling/
- Layout: `grid` (columns as `auto`, fixed length, or `fr`), `stack`, `box`, `block`, `align`, `pad`, `place`, `columns`, `page`, `layout`, `measure`. https://typst.app/docs/reference/layout/
- No runtime state or events. Renders to PDF, PNG, SVG. HTML export experimental. Apache-2.0.

### egui

- Immediate mode. Whole UI rebuilt every frame. "No callbacks to manage, and GUI state can't drift out of sync with app state." Widget ids generated automatically. https://github.com/emilk/egui
- Accessibility: optional AccessKit. eframe enables it by default where an adapter exists. `Context::enable_accesskit()` for custom integrations. Built-in widgets register role and name. `egui_kittest` queries the same roles and labels. https://raw.githubusercontent.com/emilk/egui/main/docs/accessibility.md and https://docs.rs/egui_kittest
- Keyboard focus: `Memory::request_focus`, `move_focus`, `interested_in_focus`. https://docs.rs/egui/latest/egui/struct.Memory.html
- Render: triangle mesh painted by an integration (eframe, egui_glow, egui-wgpu, egui-winit). `egui_software_backend` rasterizes on the CPU into a byte buffer. https://github.com/DGriffin91/egui_software_backend
- DRM/KMS or linuxfb backend for egui: none found. winit has an open request for a KMS/DRM/GBM backend: https://github.com/rust-windowing/winit/issues/1865 . Unknown.
- License: MIT OR Apache-2.0.

### Ratatui

- Layout constraints: `Length(n)`, `Percentage(n)`, `Ratio(a, b)`, `Min(n)`, `Max(n)`, `Fill(weight)`, applied in order. `Flex` (Legacy, Start, End, Center, SpaceBetween, SpaceAround). `.spacing(n)`. https://ratatui.rs/concepts/layout/
- Immediate mode. `draw` fills a `Buffer` of `Cell`s, then `flush()` diffs against the previous buffer. https://ratatui.rs/concepts/rendering/under-the-hood/
- Widgets: Block, BarChart, Calendar, Canvas, Chart, Clear, Gauge, LineGauge, List, Paragraph, Scrollbar, Sparkline, Table, Tabs. https://ratatui.rs/concepts/widgets/
- License: MIT.

### Cross-cutting facts from the DSL pass

- Closed vocabularies validate. Slint purity rules, Elm `Msg`, and A2UI catalogs are enumerable. QML JavaScript, hyperscript, and Datastar expressions are not.
- Sizing grammars stay under ten words in every system: elm-ui (`fill`, `fillPortion`, `shrink`, `px`), Ratatui (six constraints), Typst (`auto`, `fr`, fixed), Slint (`min`, `max`, `preferred`, stretch).
- Compose's sealed `UiState` is the only system that documents loading and error as first-class states. None of the others ship loading, empty, or error affordances for free.
- Slint `linuxkms-software` is the only documented first-party path to a Raspberry Pi with no compositor among the DSL systems. It is labeled experimental and the Royalty-free license excludes embedded use.

## 4. Generative UI protocols and SDKs

Several official pages returned 404 or 403 during the pass (a2ui.org/specification/v0.9/, npmjs.com, the OpenAI DevDay post, adaptivecards.microsoft.com/explorer). Alternates are cited.

### Google A2UI

- Maker: Google with CopilotKit and community contributions. Apache 2.0. 16.6k stars at fetch time. https://a2ui.org/ and https://github.com/google/A2UI
- Versions: v0.8 legacy (Q4 2025), v0.9 previous stable, v0.9.1 current production, v1.0 release candidate (called v0.10 in draft). v1.0 finalization and a renderer certification program scheduled for Q4 2026. https://a2ui.org/roadmap/
- Wire format: JSON Lines. Each line is one JSON object with a `version` field and exactly one envelope key. The site calls this "streaming-friendly, easy for LLMs to generate incrementally, and resilient to errors." https://a2ui.org/concepts/data-flow/
- v0.9 agent-to-client messages: `createSurface` (surfaceId, catalogId, theme, sendDataModel), `updateComponents` (surfaceId, components[]), `updateDataModel` (surfaceId, path, value), `deleteSurface`. Client-to-agent: `action` (name, surfaceId, sourceComponentId, timestamp, context) and `error`. https://a2ui.org/specification/v0.9-a2ui/
- v0.8 used `surfaceUpdate`, `dataModelUpdate`, `beginRendering`, `userAction`, wrapped the component type as an object key, and used `explicitList` and `literalString`. Three incompatible envelope syntaxes exist across v0.8, v0.9, v1.0. https://a2ui.org/concepts/data-binding/
- v1.0 adds `callRendererFunction`, `agentFunctionResponse`, `callAgentFunction`, `rendererFunctionResponse`, `actionResponse`. `theme` becomes `surfaceProperties`. https://a2ui.org/specification/v1.0-a2ui/
- Nesting: flat adjacency list. Each component is `{id, component, ...props}`. Parents reference children by id. Template children use `{"path": "/items", "componentId": "item-template"}`. https://a2ui.org/concepts/components
- Data binding: one JSON object per surface. RFC 6901 JSON Pointer paths. Prop value is a literal or `{"path": "/user/name"}`. Inputs write back (two-way). https://a2ui.org/concepts/data-binding/
- Events: a Button declares `action: {event: {name, context: {key: {path}}}}` or a local `functionCall`. The renderer resolves context paths and sends `action`. Buttons can declare `checks` that disable the button. https://a2ui.org/concepts/actions/
- Validation: basic catalog is JSON Schema draft 2020-12 with `unevaluatedProperties: false`. Renderers send `error` with `VALIDATION_FAILED`, `UNALLOWED_PARENT`, or `UNALLOWED_CHILD` plus `path` and `message` so the model can self-correct. https://github.com/a2ui-project/a2ui/blob/main/specification/v0_9/catalogs/basic/catalog.json
- Catalog: 18 basic components: Text, Image, Icon, Video, AudioPlayer, Row, Column, List, Card, Tabs, Modal, Divider, Button, TextField, CheckBox, ChoicePicker, Slider, DateTimeInput. Gemini Enterprise ships a Material catalog. https://docs.cloud.google.com/gemini/enterprise/docs/a2ui-agents/a2ui-component-gallery-reference
- Security claim: "Declarative data, not executable code." https://a2ui.org/
- Renderers: Web Core Lib, Lit, Angular, React, Flutter (GenUI SDK), Lynx. SwiftUI and Compose planned Q3 2026. Rust: unknown. MIME type `application/a2ui+json`. https://a2ui.org/roadmap/
- Example (v0.9 updateComponents, condensed): `{"version":"v0.9","updateComponents":{"surfaceId":"order_status","components":[{"id":"root","component":"Column","children":["title","status"]},{"id":"title","component":"Text","text":"Your order"},{"id":"status","component":"Text","text":{"path":"/order/status"}}]}}`
- Example (client action): `{"version":"v0.9","action":{"name":"submit_reservation","surfaceId":"booking-surface","sourceComponentId":"submit-btn","timestamp":"2026-02-25T10:40:00Z","context":{"time":"7:00 PM","size":4}}}`

### Vercel json-render

- Vercel Labs. Apache 2.0. Repo created 2026-01-14. 18.6k stars. `@json-render/core` 0.21.0 depends on zod ^4.3.6. Guillermo Rauch called it an early research prototype. https://github.com/vercel-labs/json-render , https://thenewstack.io/vercels-json-render-a-step-toward-generative-ui/ , https://www.infoq.com/news/2026/03/vercel-json-render
- Spec shape: flat map. `root` names the starting element id. `elements` maps id to `{type, props, children: [ids]}`. Named `slots` for non-default children. https://json-render.dev/docs
- Catalog: `defineCatalog(schema, {components, actions, functions})`. Each component has zod `props`, optional `slots`, a `description`. `catalog.prompt()` generates the system prompt. The docs call the catalog "your guardrail." https://json-render.dev/docs/catalog
- Streaming: SpecStream is JSONL where each line is an RFC 6902 JSON Patch op with RFC 6901 paths. `createSpecStreamCompiler().push(chunk)` returns `{result, newPatches}`. Behavior on a truncated line is not documented. https://json-render.dev/docs/streaming
- Validation: zod props per catalog. Unknown component type behavior at render time: unknown. https://json-render.dev/docs/validation
- Events: components `emit("press")`. The model names an action from the catalog. The app supplies handlers `(params, setState, state)`. A built-in `setState` action writes to a `statePath`. https://json-render.dev/docs/actions
- State: `{"$state": "/path"}` reads, `{"$bindState": ...}` two-way, `$cond/$then/$else`, `$template`, `$computed`, `visible`. State adapters for redux, zustand, jotai, xstate. https://github.com/vercel-labs/json-render
- Renderers: React, Vue, Svelte, Solid, React Native, Ink, Remotion, React PDF, React Email, image, React Three Fiber, Next.js, TanStack Start. `@json-render/shadcn` ships 36 components. Flutter third-party. Rust: unknown.
- LangChain adopted json-render as its declarative generative UI format, with A2UI as the alternative. https://docs.langchain.com/oss/python/langchain/frontend/generative-ui
- Example (spec): `{"root":"card-1","elements":{"card-1":{"type":"Card","props":{"title":"Hello"},"children":["button-1"]},"button-1":{"type":"Button","props":{"label":"Click me"},"children":[]}}}`
- Example (SpecStream line): `{"op":"add","path":"/elements/root","value":{"type":"Card","props":{"title":"Dashboard"},"children":["metric-1","metric-2"]}}`

### Vercel AI SDK generative UI

- RSC path: `streamUI` from `@ai-sdk/rsc` takes tools with `inputSchema` and a `generate` function returning ReactNode or a generator. "AI SDK RSC is currently experimental. We recommend using AI SDK UI for production." https://ai-sdk.dev/docs/reference/ai-sdk-rsc/stream-ui and https://ai-sdk.dev/docs/ai-sdk-rsc/overview
- Why Vercel moved away: streams cannot be aborted from server actions, components remount on `.done()` and flicker, many suspense boundaries can crash, `createStreamableUI` can cause quadratic data transfer, closed RSC streams cause update issues, `streamUI` does not support parallel or multi-step tool calls. https://ai-sdk.dev/docs/ai-sdk-rsc/migrating-to-ui
- Current path: server runs `streamText` with tools. Client `useChat` renders `message.parts`. Tool parts typed `tool-<name>`. States: `input-streaming`, `input-available`, `output-available`, `output-error`, `approval-requested`, `approval-responded`, `output-denied`. Tool call streaming on by default in AI SDK 5, so `part.input` is a partial object during `input-streaming`. https://ai-sdk.dev/docs/ai-sdk-ui/generative-user-interfaces and https://ai-sdk.dev/docs/ai-sdk-ui/chatbot-tool-usage
- No UI DSL. The model emits tool-call JSON args. The app maps tool name to a React component. Current major is 6. https://app.unpkg.com/ai@6.0.99/files/docs/05-ai-sdk-rsc/10-migrating-to-ui.mdx

### OpenAI Apps SDK

- DevDay 2025-10-06 per press coverage. SEP-1865 says the Apps SDK "launched in November 2025." https://thenewstack.io/openai-launches-apps-sdk-for-chatgpt-a-new-app-platform/ and https://modelcontextprotocol.io/seps/1865-mcp-apps-interactive-user-interfaces-for-mcp
- Model: an MCP server exposes tools. A tool's `_meta.ui.resourceUri` (alias `_meta["openai/outputTemplate"]`) points to a `ui://` HTML resource with MIME `text/html;profile=mcp-app`. ChatGPT loads it in a sandboxed iframe. https://developers.openai.com/apps-sdk/build/chatgpt-ui
- Tool result contract: `structuredContent` visible to model and component and must match `outputSchema`. `content` visible to both. `_meta` goes only to the component. https://developers.openai.com/apps-sdk/reference
- `window.openai` bridge: `toolInput`, `toolOutput`, `widgetState`, `theme`, `displayMode`, `maxHeight`, `safeArea`, `locale`. Methods `setWidgetState`, `callTool`, `sendFollowUpMessage`, `requestDisplayMode`, `requestModal`, `requestClose`, `notifyIntrinsicHeight`, `openExternal`, `uploadFile`, `selectFiles`, `requestCheckout`. `widgetState` can split `modelContent` and `privateContent`. Same URL.
- OpenAI now says to build on the MCP Apps bridge (`ui/*` JSON-RPC over postMessage) by default. https://developers.openai.com/apps-sdk/mcp-apps-in-chatgpt
- The model never emits UI. Developers ship a prebuilt HTML bundle per tool. ChatGPT web only. SDK license: unknown.

### MCP UI and MCP Apps (SEP-1865)

- MCP UI: Ido Salomon and Liad Yosef. Apache 2.0. 5.2k stars. Legacy resource shape `{type: 'resource', resource: {uri: 'ui://...', mimeType, text | blob}}` with `text/html`, `text/uri-list`, or `application/vnd.mcp-ui.remote-dom+javascript; framework=react|webcomponents`. Events via `onUIAction` with `tool`, `intent`, `prompt`, `notify`, `link`. https://github.com/idosal/mcp-ui and https://cdn.jsdelivr.net/npm/@mcp-ui/client@5.0.0/README.md
- MCP Apps: SEP created 2025-11-21, status Final, Extensions Track. Extension id `io.modelcontextprotocol/ui`. Stable spec version 2026-01-26. Apache 2.0. https://modelcontextprotocol.io/seps/1865-mcp-apps-interactive-user-interfaces-for-mcp and https://github.com/modelcontextprotocol/ext-apps
- Resource: `ui://` URI, MIME `text/html;profile=mcp-app`, served via `resources/read`. Tool links with `_meta.ui.resourceUri`. `_meta.ui.visibility` defaults to `["model","app"]`. https://raw.githubusercontent.com/modelcontextprotocol/ext-apps/main/specification/2026-01-26/apps.mdx
- JSON-RPC over postMessage. View to host: `ui/initialize`, `tools/call`, `resources/read`, `ui/open-link`, `ui/message`, `ui/request-display-mode`, `ui/update-model-context`, `ui/notifications/size-changed`. Host to view: `ui/notifications/tool-input`, `ui/notifications/tool-input-partial` (streaming), `ui/notifications/tool-result`, `ui/notifications/tool-cancelled`, `ui/notifications/host-context-changed`, `ui/resource-teardown`. Same URL.
- Sandboxing: double iframe with a sandbox proxy on a separate origin. Hosts MUST enforce CSP from declared `connectDomains`, `resourceDomains`, `frameDomains`, `baseUriDomains`. Default CSP `default-src 'none'`. `object-src 'none'` always. Same URL.
- Rationale: predeclared resources chosen over embedded resources. JSON-RPC chosen over a custom message set. A global API object (the `window.openai` pattern) was rejected. https://modelcontextprotocol.io/seps/1865-mcp-apps-interactive-user-interfaces-for-mcp
- Hosts: ChatGPT, Claude, VS Code, Goose, Postman, MCPJam, mcp-use Inspector, Alpic Playground.

### Microsoft Adaptive Cards

- MIT source. Nested JSON tree: root `{type: "AdaptiveCard", version, body: [...], actions: [...]}`. Containers nest children inline. Per-element `fallback` and `requires`. https://github.com/microsoft/AdaptiveCards and https://learn.microsoft.com/en-us/adaptive-cards/authoring-cards/universal-action-model
- Versions: features table up to 1.6. Teams and the live chat widget limited to 1.5. Bot Framework Web Chat supports 1.6 without `Action.Execute`. https://learn.microsoft.com/en-us/microsoft-copilot-studio/authoring-ask-with-adaptive-card
- Catalog: Container, ActionSet, ColumnSet, Media, RichTextBlock, Table, TextBlock, FactSet, ImageSet, Image, Input.Text, Input.Date, Input.Time, Input.Number, Input.Toggle, Input.ChoiceSet, Input.Rating, Rating, CompoundButton, Icon, CodeBlock, Chart elements. https://learn.microsoft.com/en-us/microsoftteams/platform/task-modules-and-cards/cards/charts-in-adaptive-cards
- Charts: Chart.Donut, Chart.Gauge, Chart.VerticalBar.Grouped, Chart.VerticalBar, Chart.HorizontalBar, Chart.HorizontalBar.Stacked, Chart.Line, Chart.Pie. Gated by host capability. Microsoft hosts "support a larger feature set than the open source offering." https://devblogs.microsoft.com/microsoft365dev/introducing-the-adaptive-cards-documentation-hub-and-new-adaptive-cards-updates/
- Templating: `${...}` syntax since May 2020. `$data`, `$root`, `$index`, `$when`. Expansion runs before rendering. One-way. No live data model. https://learn.microsoft.com/en-us/adaptive-cards/templating/language
- Actions: `Action.Submit` gathers inputs plus hidden `data`. `Action.Execute` (1.4) adds `verb` and an `adaptiveCard/action` invoke. The bot may reply with a replacement card. https://learn.microsoft.com/en-us/adaptive-cards/authoring-cards/universal-action-model
- Renderers: JavaScript, .NET, .NET WPF, .NET HTML, UWP, Android, iOS. Community React Native, Vue. Third-party Flutter. Rust: unknown. Streaming: none. Last releases in 2024. https://github.com/microsoft/AdaptiveCards
- Example: `{"type":"AdaptiveCard","version":"1.4","body":[{"type":"Input.Text","id":"firstName","placeholder":"What is your first name?"},{"type":"ActionSet","actions":[{"type":"Action.Execute","title":"Submit","verb":"personalDetailsFormSubmit","fallback":"Action.Submit"}]}]}`
- Adaptive Cards LLM emission accuracy: unknown. No published measurement found.

### Other model-emitted UI systems

- Thesys C1 and OpenUI: C1 is a managed generative UI platform with an OpenAI-compatible endpoint. OpenUI (thesysdev/openui, MIT, 10.4k stars) is built on OpenUI Lang, a compact line-oriented non-JSON language where each line renders as it arrives. Components are declared with zod schemas compiled into a system prompt. Gateway and Autofix repair invalid output. Renderers React, Vue 3, Svelte 5, Angular. Thesys says its first version used JSON and that "malformed output forced retries." https://www.openui.com/docs and https://github.com/thesysdev/openui
- Thesys vendor token benchmark (tiktoken GPT-5 encoder, 7 scenarios): OpenUI Lang 4,800 tokens vs 10,180 for json-render and 9,948 for C1 JSON (52.8% and 51.7% fewer). https://github.com/thesysdev/openui
- CopilotKit AG-UI: MIT. About 16 event types (RunStarted, TextMessageContent, ToolCallArgs, StateSnapshot, StateDelta as RFC 6902, Custom, Activity). An interaction protocol, not a UI schema. https://github.com/ag-ui-protocol/ag-ui and https://docs.ag-ui.com/concepts/events . CopilotKit groups generative UI into controlled (tool-based), declarative (A2UI, Open-JSON-UI), open-ended (MCP Apps). Open-JSON-UI is described as "open standardization of OpenAI's internal declarative Generative UI schema" with only an illustrative example. https://github.com/CopilotKit/generative-ui
- Flutter GenUI: BSD-3, alpha, requires Flutter 3.35.7+, uses A2UI v0.9. `A2uiTransportAdapter` parses the LLM text stream. A button press sends a `UserMessage` with the current data model. https://github.com/flutter/genui
- Shopify remote-dom: MIT. A sandboxed remote environment builds a DOM tree. `RemoteMutationObserver` serializes mutations (insert child, remove child, update text, update property) over postMessage. The `elements` option allowlists element names, properties, attributes, events, methods. https://github.com/Shopify/remote-dom
- Salesforce Agentforce: converts LLM responses into "structured message formats" rendered as seat maps, carousels, and selectable components across 4 million sessions. Format not published. https://engineering.salesforce.com/how-agentforce-converts-llm-responses-into-structured-ui-for-ai-agents-across-4m-sessions/

### Published measurements of models emitting these formats

- MDMA GenUI formats benchmark (MobileReality, merged 2026-07-27, 1,377 generations, 5 repeats per scenario, 3 models, not peer reviewed): "every time" rate (all 5 repeats rendered) for Opus 5 / GPT-5.6-terra / Gemma-4-26B: json-render 83.3% / 72.2% / 38.9%. A2UI 83.3% / 61.1% / 44.4%. AGenUI (A2UI v0.9 SDK) 38.9% / 72.2% / 77.8%. OpenUI Lang 83.3% / 100% / 55.6%. MDMA (authors' own format) 94.4% / 83.3% / 94.4%. At an 8k token ceiling AGenUI truncated 52.2% of Opus 5 generations vs 7.8% for A2UI. https://github.com/MobileReality/mdma/pull/62
- Very Good Ventures (Flutter GenUI over A2UI, 2026-07-23): one Gemini model emitted a bare array instead of a valid A2UI envelope on 6 of 15 turns. The GenUI library auto-repaired invalid catalog items and rendered anyway, which masked failures until schema validation was added outside the framework. https://verygood.ventures/blog/benchmarking-llms-for-gen-ui/ . Failure taxonomy (2026-07-31): structural, semantic, data, safety. https://verygood.ventures/blog/generative-ui-hallucinations-four-layer-defense.md
- Macaron-A2UI (arXiv 2605.24830): LoRA SFT plus RL models at 30B, 235B, 754B. Best model 75.6 on the authors' A2UI-Bench without schema hints. The model card says it targets A2UI v0.8 and fails on other versions. https://arxiv.org/abs/2605.24830 and https://friendli.ai/models/mindlab-research/Macaron-A2UI-Venti
- Structured Output Benchmark (arXiv 2604.25359, search summary): GPT-5.4 99.97% JSON pass rate but 0.486 perfect response rate. Models above 97% schema compliance still had 17% to 31% incorrect leaf values. https://arxiv.org/pdf/2604.25359
- EvoGenUI-Bench (EMNLP 2026, 150 five-turn tasks): strongest model 74.9% turn pass, 37.3% of episodes completed. Output is browser-executed web UI. https://arxiv.org/abs/2608.29387
- json-render official accuracy numbers: unknown beyond MDMA.

### Protocol comparison

| System | Wire shape | Streaming | Validation | Events back | Data binding | Renderers | License |
|---|---|---|---|---|---|---|---|
| A2UI v0.9/1.0 | JSONL, flat components with id refs | Incremental JSONL, updateComponents and updateDataModel | JSON Schema 2020-12 catalog, error message with path back to model | `action {name, surfaceId, sourceComponentId, context}` | Per-surface JSON model, JSON Pointer paths, two-way inputs | Lit, Angular, React, Flutter, Lynx. Rust unknown | Apache 2.0 |
| json-render | Flat `{root, elements{id}}` | JSONL of RFC 6902 patches | zod props per catalog | `emit(name)` to app handlers | `$state`, `$bindState`, `$cond`, `$template` | React, Vue, Svelte, Solid, RN, Ink, PDF, email | Apache 2.0 |
| AI SDK UI tool parts | Tool-call JSON args, no UI DSL | Partial args via `input-streaming` | Tool zod inputSchema | Chat messages or `addToolOutput` | App state | React only | Apache 2.0 |
| OpenAI Apps SDK | Prebuilt HTML in iframe, tool `structuredContent` | None for UI | outputSchema, CSP allowlists | `callTool`, `sendFollowUpMessage`, `setWidgetState` | `toolOutput` plus `widgetState` | ChatGPT web iframe | unknown |
| MCP Apps | `ui://` HTML resource | `tool-input-partial`, `tool-result` | Host CSP and sandbox | JSON-RPC `tools/call`, `ui/message` | Host pushes tool input and result | Any host iframe | Apache 2.0 |
| Adaptive Cards | Nested JSON tree | None, whole card replaced | JSON schema per version, per-element fallback | `Action.Submit`, `Action.Execute` with verb | `${}` templating before render | JS, .NET, UWP, Android, iOS | MIT |
| OpenUI Lang | Line-oriented custom language | Per-line progressive render | Gateway Autofix, zod schemas | unknown | unknown | React, Vue, Svelte, Angular | MIT |
| Flutter GenUI | A2UI v0.9 JSONL | A2UI stream | Catalog schema, auto-repair logged | UserMessage with full data model | Observable DataModel | Flutter | BSD-3 |
| Shopify remote-dom | Serialized DOM mutations | Mutation batches | Host allowlist | Proxied listeners | None | Web host DOM | MIT |

## 5. Rust UI frameworks, layout engines, and the Pi target

### Taffy

- "Currently implements the CSS Block, Flexbox and CSS Grid layout algorithms." Users: Servo, Blitz, Bevy, Takumi, iocraft, Slint, Lapce (via Floem), Zed (via GPUI). MIT. https://github.com/DioxusLabs/taffy
- Zed's gpui depends on taffy with features std, taffy_tree, flexbox, grid. https://raw.githubusercontent.com/zed-industries/zed/main/crates/gpui/Cargo.toml
- Latest 0.14.0 published 2026-08-24. MSRV 1.71. Adds min-content, max-content, fit-content, stretch sizing and `contain`. https://crates.io/api/v1/crates/taffy and https://github.com/DioxusLabs/taffy/releases/tag/v0.14.0
- no_std via `#![cfg_attr(not(feature = "std"), no_std)]`. Feature flags std, alloc, flexbox, grid, block_layout, taffy_tree, serde. https://docs.rs/taffy/latest/taffy/
- Benchmarks vs Yoga on an M1 Pro: "big trees (deep, auto size)" 100,000 nodes Yoga 2.3502 s, Taffy 164.24 ms. "super deep" 300 nodes Yoga did not finish in 9 min, Taffy 366.89 µs. Taffy slower on "big trees (deep, random size)" 69.952 ms vs Yoga 63.555 ms. https://github.com/DioxusLabs/taffy
- API: high-level `TaffyTree` with `new_leaf`, `new_with_children`, `compute_layout`, `compute_layout_with_measure`. Low-level traits `TraversePartialTree`, `LayoutPartialTree` (`get_core_container_style`, `set_unrounded_layout`, `compute_child_layout`), `LayoutFlexboxContainer`, `LayoutGridContainer`, `LayoutBlockContainer`, `CacheTree`. https://docs.rs/taffy/latest/taffy/tree/traits/trait.LayoutPartialTree.html

### Yoga, Morphorm, Stretch, Clay

- Yoga: "embeddable and performant flexbox layout engine with bindings for multiple languages." C++20. Used by React Native. Qt Quick ships Yoga 2.0.1. MIT. Rust crate `yoga` 0.5.0 (2025-01-16) via bindgen, whose README points to Taffy. https://github.com/facebook/yoga , https://www.yogalayout.dev/docs/about-yoga , https://docs.rs/crate/yoga/latest
- Morphorm 0.9.0 (2026-07-17): one-pass Row/Column/Overlay/Grid with Pixels/Percentage/Stretch/Auto. Used by Vizia. MIT. https://github.com/vizia/morphorm
- Stretch: Flexbox in Rust, last release 0.3.2 in 2019, abandoned. Taffy is its fork. https://github.com/vislyhq/stretch
- Clay: C header, flexbox-like, 4.8k LOC, arena memory, renderer agnostic, Zlib. Community Rust bindings. https://github.com/nicbarker/clay

### Dioxus and Blitz

- Dioxus: `rsx!` components, web, desktop (system webview), mobile, native (experimental, Blitz). MIT or Apache-2. https://github.com/DioxusLabs/dioxus and https://dioxuslabs.com/learn/0.7/beyond/project_structure
- Blitz: "A radically modular HTML/CSS rendering engine", beta. Stylo for CSS, Taffy for layout, Parley for text, winit via blitz-shell, AccessKit. GPU via Vello. Apache 2.0 or MIT. https://github.com/DioxusLabs/blitz . anyrender has vello, vello_cpu, skia backends and a softbuffer window renderer. https://github.com/DioxusLabs/anyrender
- Dioxus 0.7 (2025-09-08): first Dioxus Native. "Not every CSS feature is supported yet." "We have not focused on performance." https://dioxuslabs.com/blog/release-070
- DRM or framebuffer on Pi: not documented. winit lists X11 and Wayland only. https://docs.rs/winit/latest/winit/

### Iced

- Elm-style State, Messages, View, Update. iced_wgpu (Vulkan, Metal, DX12) and iced_tiny_skia software fallback. "Iced is currently experimental software." MIT. https://github.com/iced-rs/iced
- 0.14.0 published 2025-12-07: reactive rendering, time travel debugging, hot reloading, headless mode, `table` and `grid` widgets. No accessibility entry. https://github.com/iced-rs/iced/releases/tag/0.14.0
- DRM/KMS without a display server: no support found. winit X11/Wayland only.
- Accessibility: issue #552 open since 2020-10-05. libcosmic has an experimental a11y feature against a vendored iced. Iced not listed on accesskit.dev. https://github.com/iced-rs/iced/issues/552 and https://accesskit.dev/

### Makepad

- "AI-accelerated application and game development environment for Rust" with a live-editable UI DSL and shader-based GPU rendering (wasm/WebGL, macOS/Metal, Windows/DX11, Linux/OpenGL). MIT. makepad-widgets 1.0.0 (2025-05-13) MIT OR Apache-2.0. https://github.com/makepad/makepad and https://crates.io/api/v1/crates/makepad-widgets
- DSL hot reload: structs update at runtime when the DSL changes. https://publish.obsidian.md/makepad-docs/Tutorials/Image+Viewer/2+-+Creating+an+Empty+Window
- Accessibility: unknown. DRM/KMS: unknown. Maturity statement: none on the README.

### GPUI (Zed)

- "A fast, hybrid immediate and retained mode, GPU-accelerated UI framework for Rust." "For the near future, GPUI is tied to Zed." Pre-1.0 with breaking changes. Metal on macOS. Apache-2.0. gpui 0.2.2 published 2025-10-22. https://www.gpui.rs/ and https://crates.io/api/v1/crates/gpui
- Linux: gpui_linux has wayland, x11, headless. No drm or kms files. Zed uses Vulkan on Linux. https://api.github.com/repos/zed-industries/zed/contents/crates/gpui_linux/src/linux and https://zed.dev/docs/linux.md

### Slint on Raspberry Pi

- LinuxKMS: "eliminates the need for a windowing system such as Wayland or X11." OpenGL via KMS/DRI, Vulkan via wgpu, DRM dumb buffers for software rendering, LinuxFB fallback. libinput/libudev input. libseat avoids root. "Still experimental." Renderer selection: `linuxkms-femtovg`, `linuxkms-femtovg-wgpu`, `linuxkms-skia-opengl`, `linuxkms-skia-vulkan`, `linuxkms-skia-software`, `linuxkms-software`. Mouse cursor only with Skia and FemtoVG. Fontconfig required. `SLINT_DRM_OUTPUT`, `SLINT_DRM_MODE`, `SLINT_KMS_ROTATION`. Touch rotation via `LIBINPUT_CALIBRATION_MATRIX`. https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_linuxkms/
- Cargo features: `backend-linuxkms`, `backend-linuxkms-libinput`, `renderer-software` default, `accessibility` default. https://docs.slint.dev/latest/docs/rust/slint/docs/cargo_features/
- LinuxKMS introduced in Slint 1.2 (2023-09-04). https://www.slint.dev/blog/slint-1.2-released
- Embedded Linux needs OpenGL ES 2.0 or Vulkan for GPU renderers. Non-GPU ARM64 builds use software rendering. https://snapshots.slint.dev/master/docs/slint/guide/platforms/embedded
- Measured performance on Pi 4, Pi 5, Pi Zero: unknown.
- slint-interpreter: `Compiler::build_from_source` or `build_from_path`, `CompilationResult`, `ComponentDefinition::create()`, `ComponentInstance` with `get_property`, `set_property`, `set_callback`, `invoke`. `Value::Model(ModelRc<Value>)`. https://docs.rs/slint-interpreter/latest/slint_interpreter/
- ModelRc wraps `Rc<dyn Model>`. VecModel notifies the UI. Main-thread only. https://docs.rs/slint/latest/slint/struct.ModelRc.html
- Accessibility: Slint 1.1 (2023-06-26) "accessibility support is always enabled on all desktop platforms, thanks to the new AccessKit integration." 29 `AccessibleRole` values. On LinuxKMS: unknown. https://slint.dev/blog/slint-1.1-released
- Latest slint 1.18.1 published 2026-09-21. License "GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0". Royalty-free: "Use in embedded systems is excluded." GPLv3 covers embedded. Commercial covers embedded. https://crates.io/api/v1/crates/slint and https://github.com/slint-ui/slint/blob/master/LICENSE.md
- Slint uses Taffy for layout. https://github.com/DioxusLabs/taffy

### egui on Raspberry Pi

- "egui only repaints when there is interaction or an animation." "For most cases you can expect egui to take up 1-2 ms per frame." MIT OR Apache-2.0. egui 0.36.2 published 2026-09-08. https://raw.githubusercontent.com/emilk/egui/main/README.md and https://crates.io/api/v1/crates/egui
- Integrations: eframe, egui_glow, egui-wgpu, egui-winit. egui-winit features `accesskit`, `wayland`, `x11`. https://docs.rs/egui-winit/latest/egui_winit/
- Memory persists window positions, scroll offsets, and per-Id widget state between frames. https://docs.rs/egui/latest/egui/struct.Memory.html
- DRM/KMS backend: none found. egui-sdl2 (picoHz) has canvas, glow, wgpu backends and does not mention KMSDRM. SDL2 has a KMSDRM driver (`SDL_VIDEODRIVER=kmsdrm`) needing libgbm. A Pi 4 report on SDL 2.0.12 kmsdrm hit an EGL crash. https://github.com/picoHz/egui-sdl2 and https://discourse.libsdl.org/t/raspberry-pi-4-kmsdrm-no-x11-error-creating-egl-surface-sdl2-2-0-12/28332
- Measured egui performance on Pi: unknown.

### Tauri 2

- Rust core plus WRY (webview) and TAO (winit fork). macOS uses the system WebKit webview. Linux uses webkit2gtk 4.1, which needs GTK. MIT or Apache-2.0. https://v2.tauri.app/concept/architecture/ and https://v2.tauri.app/reference/webview-versions/
- IPC: `#[tauri::command]` with `invoke()`. Events are not type safe and JSON only. `tauri::ipc::Channel` is "the recommended mechanism for streaming data." https://v2.tauri.app/develop/calling-rust/
- Custom protocol: `register_uri_scheme_protocol` and an async variant. Origin `<scheme>://localhost/<path>` on macOS and Linux. https://docs.rs/tauri/latest/tauri/struct.Builder.html
- CSP: enabled only when set in config. Local scripts hashed, nonces appended at compile time. https://v2.tauri.app/security/csp/
- tauri 2.12.1 published 2026-09-30. 3.0.0-alpha.4 published 2026-10-01. https://crates.io/api/v1/crates/tauri
- Raspberry Pi: xc-tauri-rasp cross-compiles for Pi 4 and Pi Zero 2 W with libwebkit2gtk-4.1. A Tauri app on the Pi needs a display server (X11 or Wayland). https://github.com/shiritai/xc-tauri-rasp and https://github.com/tauri-apps/wry

### AccessKit

- Cross-platform accessibility tree. Adapters: Windows (UI Automation), macOS (NSAccessibility), Unix (AT-SPI), Android, iOS, winit. "Only the platform adapter needs to retain a complete accessibility tree in memory, which makes the design suitable for immediate-mode GUI toolkits." Apache 2.0 or MIT. accesskit 0.25.1 published 2026-09-25. https://github.com/AccessKit/accesskit and https://crates.io/api/v1/crates/accesskit
- Users listed on accesskit.dev: Bevy, egui, Freya, Slint, Xilem. Iced and Makepad not listed. https://accesskit.dev/

### Charting

- plotters: pure Rust, backends bitmap, SVG, canvas, GTK/Cairo. MIT. https://github.com/plotters-rs/plotters . plotters-iced for iced 0.13, plotters-iced2 for 0.14. https://docs.rs/plotters-iced2
- egui_plot 0.37.0 (2026-08-05), MIT OR Apache-2.0. https://crates.io/api/v1/crates/egui_plot
- Slint `Path` element with SVG commands, fill, stroke. Slint examples include a plotters demo. https://docs.slint.dev/latest/docs/slint/reference/elements/path/
- Dioxus/Blitz, Makepad, GPUI charts: unknown.

### Raspberry Pi touchscreens and kiosk without X

- Official 7-inch Touch Display: 800 x 480, ten touch points, DSI, overlay `vc4-kms-dsi-7inch`. https://www.raspberrypi.com/documentation/accessories/display.html
- Touch Display 2: 5-inch and 7-inch 720 x 1280 five-finger, 10-inch 1200 x 1920 ten-finger (Pi 5 and CM5 only). Portrait by default. https://www.raspberrypi.com/documentation/accessories/touch-display-2.html
- Touch input: kernel driver `raspberrypi-ts` registers an evdev input device with ABS_MT_POSITION_X/Y and 10 slots. https://github.com/raspberrypi/linux/blob/rpi-6.12.y/drivers/input/touchscreen/raspberrypi-ts.c
- Official kiosk tutorial relies on labwc plus Chromium `--kiosk` on the desktop image. https://www.raspberrypi.com/tutorials/how-to-use-a-raspberry-pi-in-kiosk-mode/
- cage: Wayland kiosk, runs with the KMS+DRM backend from a TTY, wlroots 0.20, MIT. https://github.com/cage-kiosk/cage
- Pi 5 issue reports `drmModeMapDumb` failures reading the active plane's framebuffer on VC6. https://github.com/raspberrypi/linux/issues/6860

### Rust framework comparison for a Pi touchscreen without a browser

| Name | Render backend | DRM/KMS or framebuffer | Touch input | Accessibility | Runtime-loadable UI | License |
|---|---|---|---|---|---|---|
| Slint 1.18.1 | FemtoVG (GLES2), wgpu (Vulkan), Skia, software (CPU) | Yes, LinuxKMS, DRM dumb buffers and LinuxFB fallback, experimental | libinput, udev calibration | AccessKit on desktop, LinuxKMS unknown | Yes, slint-interpreter | GPLv3 or Royalty-free (no embedded) or commercial |
| egui 0.36.2 | glow, wgpu, SDL2 canvas/glow/wgpu | No native backend. Possible via SDL2 kmsdrm, undocumented | winit or SDL2 | AccessKit | No | MIT OR Apache-2.0 |
| Iced 0.14.0 | wgpu, tiny_skia | No (winit X11/Wayland) | winit | Open issue #552 | No | MIT |
| Dioxus Native / Blitz | Vello on wgpu, vello_cpu, Skia | No (winit) | winit | AccessKit via blitz-shell | Partial (HTML strings) | MIT OR Apache-2.0 |
| Makepad 1.0 | Shader GPU: Metal, DX11, OpenGL, WebGL | unknown | unknown | unknown | Yes, live DSL | MIT or Apache-2.0 |
| GPUI 0.2.2 | Metal, Vulkan | No (wayland, x11, headless) | Wayland/X11 | unknown | No | Apache-2.0 |
| Tauri 2.12.1 | System webview | No, needs GTK and a display server | Via compositor | Browser engine | Yes, HTML | MIT OR Apache-2.0 |

## 6. Next Gen Lang sources (local)

- `/Users/bilal/projects/nextgenlang/CONTRIBUTING.md`: fragment format (Goal, Before, After, Rule, Open), 40 to 80 lines per doc, code in every doc, plain language, no essays.
- `/Users/bilal/projects/nextgenlang/knowledge/README.md`: living fragments, update in place.
- `/Users/bilal/projects/nextgenlang/knowledge/language-principles/01-structural-inference.md` through `11-short-typesafe-surfaces.md`: the eleven principles plus six added by the specification (effects inferred, no null, typed sinks, structured concurrency, memory model, plans before apply).
- `/Users/bilal/projects/nextgenlang/knowledge/language-design/00-vision-and-doctrine.md`: "Simple source, explicit generated plans. Symbols over strings." Working name `matter`.
- `/Users/bilal/projects/nextgenlang/knowledge/language-design/02-syntax-reference.md`: canonical spellings. `name Type modifiers` field form. `orElse` guards. `->` in stream declarations. `<-` enqueue. Declaration words `table`, `api`, `app`, `queue`, `cron`. Attributes are a rare escape hatch. Casing is formatter-enforced and compiler-invariant.
- `/Users/bilal/projects/nextgenlang/knowledge/language-design/program-kinds/*.md`: entry points, scripts, CLI apps, services, serverless, browser and WASM, embedded, simulations and agents, generated clients. Browser and WASM: "Rendering UI is out of scope for the language in the first version." Generated clients: "Whether a first-party UI layer will be specified in a later edition" is open.
- `/Users/bilal/projects/nextgenlang/knowledge/language-design/syntax/attributes.md`, `syntax/formatting-rules.md`, `data-ifc/serialization-and-boundaries.md`, `api/streaming-websocket-sse.md`, `types/effects.md`: supporting rules on attributes, the single formatter, wire pins, streams, and inferred effects.
- `/Users/bilal/projects/overseer/docs/overseer-rfc.md`: acceptance criteria shape (`AC-NN`, one sentence, bold `Verify:` clause).

## 7. Product context

- OpenAI announced "Dots" at DevDay 2026 (2026-09-29 per coverage): always-on cloud agents with their own remote machine and browser, available to Pro and Business tiers. Press: https://letsdatascience.com/news/openai-unveils-dots-agents-at-devday-2026-c079c982 and https://www.bgr.com/2272332/openai-devday-2026-announcements/ . A separate Jony Ive hardware project is reported for late 2026 or 2027. The owner's phrase "a better OpenAI dot" refers to this product category.
