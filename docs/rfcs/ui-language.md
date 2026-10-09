# RFC: a generative UI language for mote

## 1. Status

| Field | Value |
|---|---|
| Status | Draft for owner review |
| Date | 2026-10-08 |
| Author | mote daemon team |
| Scope | The wire format of the v0 UI tree, the long-term UI language that compiles to it, built-in affordances, state and event flow, streaming and validation rules, rendering targets, the compiler plan, and acceptance criteria |
| Out of scope | Model selection, prompt design beyond the UI block, voice pipeline, the daemon's tool system, cloud deployment |
| Companion | `docs/research/ui-language-research.md` holds every source and number cited here |
| Related | [mote RFC](mote.md) owns the daemon and the v0 window (MOTE-14 to MOTE-16). [Shared crates RFC](shared-crates.md) owns the crates the compiler lives beside. The README board gates this RFC on UI-00. |

### Relationship to the v0 UI tree

The daemon ships first on a validated declarative tree that models emit directly. This RFC decides that tree's wire format (section 5) and treats the tree as the compile target of the language (section 6). The two artifacts have one schema. Every example in section 6 appears in both forms. Acceptance criteria are tagged `[tree]` when they bind v0 and `[lang]` when they bind the language.

The tree is the contract. The language is the ergonomic front end for models. Both will exist for the life of the product. The tree stays the interop format for any model or tool that prefers JSON.

## Owner constraints added on 2026-10-08

These narrow sections 6 to 8 and win where they disagree. The full wording is in
`docs/handoff/generative-ui-renderer.md`.

1. The language is purposefully small. Layout is a flexbox subset plus anchors. Nothing more.
2. No style options at all beyond the theme. The theme is one token set per screen, inherited by everything, with no per-element overrides and most likely no semantic variants.
3. Elements are a short closed list: text, input, toggle, select, action, list, and the few the fixtures need.
4. Bindings are typed (value, field, action) and checked before render. The store and its observer wiring live in the daemon, never in what a model writes.
5. Custom layouts are not needed at this level. The language is for agents, and constraints are the point.

## 2. Why

Key points first:

1. v0 wire format: compact nested JSON, `type` first, `children` last, full-word keys. Measured 512 tokens for three fixtures against 920 pretty-printed. It wins because hosted APIs and Ollama can enforce a JSON Schema and small models fail on custom syntax.
2. Long-term format: the Mote UI language, file ending `.mote`. Brace-based, Next Gen Lang doctrine, no expressions or control flow. Measured 338 tokens, 34% below compact JSON. A model may emit it only once constrained decoding or a measured 95% first-try validity exists (UI-24).
3. No routes and no declared store. The compiler infers interaction slots from declarations. The daemon owns data bindings and surface history. The model is the state machine: it receives typed events and emits the next whole surface.
4. Events are `-> Name(args)`, the Next Gen Lang function-as-endpoint idea applied to UI. Built-ins such as Back, Dismiss and Copy never reach the model.
5. Built-in affordances (scrolling, focus, touch, loading, error and empty states, themes, accessibility) are implicit. HTML, CSS and JavaScript are never expressible.
6. Streaming is partial parse with fail-closed diagnostics. Text renders as it arrives. Interactive nodes render only when complete. Silent auto-repair is rejected.
7. Renderers: Tauri 2 webview for the Mac, Slint on Linux KMS for the Pi with egui as fallback. Slint's license is an open decision.
8. Mote UI is a sibling of Next Gen Lang sharing its syntax doctrine, not a program kind inside it.
9. Criteria UI-01 to UI-48 are in section 13, tagged [tree], [lang] or [both].


The owner's words set the problem.

"I don't think we need HTML, CSS, or JavaScript for all this crap." A model that emits HTML emits an open-ended programming surface. The daemon cannot validate it, the Pi cannot render it without a browser, and the token cost is high. The research confirms the cost: HTML runs 15% over compact JSON on the fixtures in this RFC and 60% over an indentation DSL in a published markup study (section 5). The UI needs a closed vocabulary the daemon can check and a Rust renderer can draw.

"There's always a scrollbar. There's always this or that." Scrolling, focus, keyboard and touch input, selection, loading, error, and empty states, responsive layout, light and dark theming, and accessibility are not features a model should remember to add. They are properties of the renderer. The model should get them for free and be unable to break them. Section 7 lists them.

"I don't know if we even need to internally have any route or any store concepts baked into the code. Maybe the agent can internally handle that." A route table and a declared store are two more things a model would have to keep consistent across turns. Section 8 shows they are not needed. The compiler infers interaction slots from declarations. The daemon owns data and history. The agent owns what to show next.

"Is JSON the best route? Maybe it's something else." Section 5 measures eleven formats on identical content and reads the published evidence. JSON wins on validation tooling and familiarity today. A terse language wins on tokens by a third and on streaming by construction. The answer is both, in sequence.

The models are the other constraint. A fast local model answers in under a second and must produce something the daemon can render while the thinking model is still working. Whatever the format, the daemon must parse it partially, validate it fail-closed, and render a stable skeleton for what is not there yet. Section 9 sets those rules.

## 3. Principles

Each principle is derived from Next Gen Lang. The table states whether the UI language reuses, refines, or departs from it. Each row has one example.

| # | Next Gen Lang principle | UI language position | Example |
|---|---|---|---|
| 1 | Structural inference with type-safe meta properties | Reuse. The element word and its modifiers imply type, layout, state slots, and the event contract. | `table "Invoices" select one` infers a selection slot, a `selected` symbol, checkbox semantics, and the `onSelect` event shape. |
| 2 | Top-level structural inheritance (flat declarations, no inheritance) | Reuse. There are no components to extend. Capability comes from words. | `card "Mail" collapsible` adds a toggle. There is no `class Card extends Panel`. |
| 3 | Guard-based linear execution | Refine. There is no execution. The only flow is a fallback on bound data. | `list "Inbox" from mail.unread { ... empty "Nothing unread" }` is the UI's `orElse`. |
| 4 | Symbol-driven infrastructure from code | Reuse. Actions, slots, and data references are symbols the compiler resolves against the daemon registry. | `-> ShowInvoice(id: selected.invoice)` fails to compile if `selected` is not in scope or `invoice` is not a column. |
| 5 | Explicit visibility with case invariance | Depart on visibility, reuse case invariance. Everything in a surface is visible by definition, so there is no `pub`. | `checkIn`, `CheckIn`, and `check_in` resolve to one action. The formatter rewrites to `CheckIn`. |
| 6 | Built-in formatter | Reuse. `mote fmt` is the only formatter. The canonical form is what the model sees next turn. | `select "one"` formats to `select one` with a warning the model receives. |
| 7 | Instant indexing | Refine. The analog is inferred table affordances: sortable and filterable columns come from column types. | `amount "Amount" money` is sortable with no modifier. |
| 8 | Function-as-endpoint | Reuse as action-as-endpoint. Every `-> Name(args)` is an endpoint the daemon derives a contract for. | `button "Create" submit -> CreateReminder {title, when, repeat}` yields contract `{title: str, when: DateTime, repeat: none|daily|weekly}`. |
| 9 | Native error tunneling | Refine. Validation errors tunnel back to the model as typed diagnostics, never as crashes or silent drops. | `E011 symbol not in scope: selected (line 4)` is sent with the retry request. |
| 10 | Pervasive destructuring and spreading | Reuse. `{a, b}` after an action gathers slots. `.field` inside a bound list projects a row. | `-> CreateReminder {title, when}` and `item .subject .from`. |
| 11 | Short type-safe surfaces | Reuse. This is the whole point. Every key the model does not write is a key the compiler infers. | `field title str "Title" min 3` is 9 tokens. The same node as compact JSON is 21. |
| 12 | Effects inferred, capabilities declared | Refine. Surfaces have no effects. Built-in actions that touch the world are capabilities the daemon declares to the model. | `Open(url)` exists only if the daemon exposes it for this device. |
| 13 | No null, no zero values, no `any` | Reuse. Optional is `?`. Absent is absent. | `field note str? "Note"` is absent from the event when empty. |
| 14 | Every sink has a type, strings never reach HTML | Reuse, strongly. `text` is plain text. `md` is a constrained subset. No string is ever markup. | `text "<b>hi</b>"` renders the literal characters. |
| 15 | Structured concurrency, no function coloring | Not applicable. The language has no concurrency. | |
| 16 | Automatic memory, `direct` code opt-out | Not applicable. | |
| 17 | Plans before apply, gates where the compiler cannot decide | Refine. The compiled tree plus diagnostics is the plan. `confirm` is the gate. | `button "Delete" danger confirm "Delete 3 invoices?" -> Delete(ids: selected.invoice)`. |

### Program kind or sibling language

Next Gen Lang leaves UI out of scope. `program-kinds/browser-and-wasm.md` says "Rendering UI is out of scope for the language in the first version." `program-kinds/generated-clients-and-frontends.md` lists "Whether a first-party UI layer will be specified in a later edition" as open.

Options:

- A. A `surface` program kind inside Next Gen Lang. Surfaces would be values of a `Surface` type, built with the full expression language, compiled into a Next Gen Lang binary.
- B. A sibling language that shares the syntax doctrine (declaration words, braces, `name Type modifiers`, `->` edges, no decorators, one formatter, case invariance) but has no expressions, functions, types beyond field kinds, or effects.

Recommendation: B, a sibling language.

- A surface is emitted per turn by a model and compiled by a daemon at run time. A program kind is compiled ahead of time into a binary. The lifecycle does not match.
- Giving a model the full expression language reopens the validation hole that QML and hyperscript have (section 4). The UI language must stay a data language.
- Sharing the doctrine keeps the two languages learnable together. A future `surface` program kind can embed Mote UI blocks and bind `api` actions to `->` events. That path stays open.
- The formatter, the diagnostics shape, and the symbol-resolution rules are copied from Next Gen Lang so the eventual merge is a merge, not a rewrite.

## 4. Prior-art findings

Full notes with every number are in the research file. The verdict column says what mote takes.

| System | What it gets right for model-emitted UI | What it gets wrong | Verdict for mote | Link |
|---|---|---|---|---|
| Google A2UI v0.9/1.0 | Closed catalog as JSON Schema. Validation errors with a path sent back to the model. JSON Lines streaming. Data model separate from layout. "Data, not code." | Flat id-linked components cost +20% tokens on the fixtures. Three incompatible envelope syntaxes across v0.8, v0.9, v1.0. No Rust renderer. | Take the error-to-model loop and the catalog-as-schema idea. Do not take the flat map. | https://a2ui.org/ |
| Vercel json-render | Flat `{root, elements}` spec. zod catalog doubles as the system prompt. JSON Patch streaming. Action names instead of code. | Patch lines are verbose. Unknown-component behavior undocumented. Labs prototype. | Take "catalog generates the prompt." Do not take JSON Patch as the model's output. | https://github.com/vercel-labs/json-render |
| Vercel AI SDK generative UI | Model emits typed tool args only. Partial args stream. | Every UI shape pre-coded as a tool. RSC streaming abandoned for flicker, abort, and quadratic transfer problems. | Take partial-input streaming states. Reject per-shape tools. | https://ai-sdk.dev/docs/ai-sdk-ui/generative-user-interfaces |
| OpenAI Apps SDK | Model-visible and widget-only data split is explicit. | Model never emits UI. Prebuilt HTML in an iframe. ChatGPT only. | Take the `modelContent` / `privateContent` split for event payloads. | https://developers.openai.com/apps-sdk/reference |
| MCP Apps (SEP-1865) and MCP UI | Real standard. Mandatory sandbox and CSP. JSON-RPC over postMessage. `tool-input-partial` streaming. | HTML only. Layout fixed at build time. | Take the JSON-RPC message discipline for renderer-to-daemon events. | https://modelcontextprotocol.io/seps/1865-mcp-apps-interactive-user-interfaces-for-mcp |
| Adaptive Cards | Mature multi-platform catalog with inputs, actions, host theming, fallback per element. | Nested JSON must be complete before render. No streaming. Version fragmentation (1.5 vs 1.6). Templating is a separate pre-render step. | Take the input and action catalog shape. Reject whole-card replacement. | https://learn.microsoft.com/en-us/adaptive-cards/ |
| Thesys OpenUI Lang | Line-oriented non-JSON language. Each line renders as it arrives. Vendor reports 52.8% fewer tokens than json-render. Thesys moved off JSON because "malformed output forced retries." | Vendor-measured. Weak on a 26B open model in the MDMA benchmark (55.6% every-time rate). | Strong precedent for the language direction. Take per-line rendering. | https://github.com/thesysdev/openui |
| Flutter GenUI | Catalog of widgets with schemas and builders. A2UI transport. | Alpha. Auto-repair masked failures in a published test. | Reject silent auto-repair. | https://github.com/flutter/genui |
| Shopify remote-dom | Allowlisted elements, properties, events. Mutation stream. | DOM-shaped. Remote owns state. | Take the allowlist idea. | https://github.com/Shopify/remote-dom |
| Slint DSL | Closed typed vocabulary. Compiler-enforced purity of bindings. Runtime compiler with diagnostics. Software renderer to DRM dumb buffers. Uses Taffy. AccessKit. | Imperative assignment kills bindings. Style fixed at compile time. LinuxKMS experimental. Royalty-free license excludes embedded. | Primary Pi renderer candidate. Take typed properties and diagnostics. Do not take `<=>` or expressions. | https://slint.dev/ |
| QML | Reactive bindings with dependency tracking. Anchors. | Binding language is JavaScript. Runtime string compile is "extremely slow" by Qt's own note. | Lesson: an unbounded expression language defeats validation. | https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html |
| SwiftUI | Containers give scrolling, selection, accessibility, dark mode for free. `@State` owner vs `Binding` borrower. | Swift API, not data. Closed platform. | Take "containers carry affordances." | https://developer.apple.com/documentation/swiftui/state |
| Jetpack Compose | Sealed `UiState` (loading, error, success). Unidirectional data flow. Ordered modifier list. | Kotlin code. Modifier order footgun. | Take ordered modifiers and the explicit state enum. | https://developer.android.com/develop/ui/compose/architecture |
| Flutter | Widget tree, 48 px targets, accessibility guidance. | Heavy runtime for a Pi without a compositor. | Take the 48 px touch rule. | https://docs.flutter.dev/ui |
| Elm architecture and elm-ui | Closed `Msg` sum type. Pure update. Nine-word layout vocabulary that excludes most CSS errors. | DOM. No accessibility or theme. | Take `Msg` as the model for action contracts. Take `fill`, `shrink`, `padding` vs `spacing`. | https://github.com/mdgriffith/elm-ui |
| egui | No retained tree. AccessKit roles and names. Headless testing by role. 1 to 2 ms per frame. | Rust per frame, not data. No DRM backend. | Fallback Pi renderer. Take role-based headless tests. | https://github.com/emilk/egui |
| Dioxus and Blitz | HTML and CSS without a browser. Taffy, Stylo, Vello, AccessKit. | Beta. Not every CSS feature. No DRM. Reintroduces CSS. | Reject for the Pi. | https://github.com/DioxusLabs/blitz |
| Iced | Elm-style. Software renderer via tiny-skia. | No DRM. Accessibility issue open since 2020. | Reject for the Pi. | https://github.com/iced-rs/iced |
| Makepad | Live-editable DSL at runtime. | Accessibility unknown. DRM unknown. No maturity statement. | Watch. | https://github.com/makepad/makepad |
| GPUI | Taffy layout, Metal on macOS. | Tied to Zed. No DRM. Vulkan on Linux. | Reject. | https://www.gpui.rs/ |
| Ratatui | Six-constraint layout vocabulary. Buffer diffing. | Terminal cells only. | Take the constraint vocabulary as the layout spec shape. | https://ratatui.rs/concepts/layout/ |
| HTMX, hyperscript, Datastar | Server holds state. Tiny attribute vocabulary. Patch-by-id over SSE. | Glued to HTML. `data-on` and hyperscript evaluate arbitrary expressions. | Take "the daemon is the state" and patch-by-id for data-bound nodes. | https://htmx.org/essays/hypermedia-driven-applications/ |
| Typst | Content separated from style by set and show rules. `auto`, `fr`, fixed track sizing. | Full scripting language inside markup. No events. | Take "the daemon owns style" and track sizing words. | https://typst.app/docs/reference/styling/ |
| Taffy | Block, Flexbox, Grid in Rust. no_std. Used by Servo, Bevy, Slint, Zed. 0.14.0 on 2026-08-24. MIT. | Layout only. | Reference layout engine for the tree's layout semantics. | https://github.com/DioxusLabs/taffy |
| Yoga | Flexbox. Mature. React Native. | C++ with bindgen bindings. Rust crate's own README points to Taffy. | Reject. | https://github.com/facebook/yoga |
| Outlines, llguidance, XGrammar, lm-format-enforcer | Grammar-constrained decoding for JSON Schema, regex, and CFG. llguidance about 50 µs per token. XGrammar under 40 µs for JSON. | Outlines has 3.5 s compile on llama.cpp. GBNF costs 25% to 28% throughput on Apple Silicon in one benchmark. Ollama supports JSON Schema only. | Use JSON Schema constraints through Ollama for v0. Use llguidance or GBNF through llama-server for the language in v1. | https://github.com/guidance-ai/llguidance |
| OpenAI and Anthropic structured outputs | 100% schema adherence on OpenAI evals. Strict tool use on Anthropic. OpenAI custom tools accept Lark grammars. | No recursive schemas on Anthropic. 20,000-char Lark limit on OpenAI. Format restriction can cut reasoning accuracy when applied to the whole response. | Constrain only the UI block, after reasoning. | https://developers.openai.com/api/docs/guides/structured-outputs |
| TOON | 39.9% fewer tokens than pretty JSON on uniform tables. | Loses to compact JSON on nested data by 11% to 20%. Generation syntax score 0.484 vs JSON 0.802 on open models. | Reject. | https://toonformat.dev/ |

## 5. Wire format decision for the v0 tree

### 5.1 Candidates

JSON (verbose and lean, pretty and compact), a flat id map (A2UI and json-render shape), Hiccup arrays (JsonML), YAML, TOON, S-expressions, a Slint-like syntax, an indentation DSL, the brace-based Mote UI language, and HTML as a reference.

### 5.2 Evidence table: measured tokens

Three fixtures with identical content. Tokenizer `o200k_base` unless noted. Baseline is compact JSON of the lean tree. The research file has `cl100k_base` and Qwen 2.5 columns, which rank the same.

| Format | Card | Table | Form | Total | vs baseline |
|---|---|---|---|---|---|
| JSON verbose, pretty (`type`, `props`, `children`) | 249 | 443 | 329 | 1021 | +99% |
| JSON lean, pretty | 209 | 429 | 282 | 920 | +80% |
| JSON lean, compact (baseline) | 120 | 246 | 146 | 512 | 0% |
| JSON flat id map, compact | 160 | 262 | 193 | 615 | +20% |
| JSON Hiccup arrays, compact | 113 | 242 | 139 | 494 | -4% |
| YAML | 132 | 286 | 180 | 598 | +17% |
| TOON (official encoder) | 142 | 226 | 184 | 552 | +8% |
| S-expression | 87 | 187 | 84 | 358 | -30% |
| Slint-like | 109 | 293 | 135 | 537 | +5% |
| Indentation DSL | 82 | 177 | 67 | 326 | -36% |
| Mote UI (braces) | 86 | 181 | 71 | 338 | -34% |
| HTML | 119 | 328 | 141 | 588 | +15% |

Two variants settle two questions. Short JSON keys (`t`, `c`, `l`) save 0%, because common words are already one token. Newlines in JSON cost +18% to +27%, one-space indent +55% to +73%, so compact JSON is the only JSON worth emitting.

### 5.3 Evidence table: qualities

| Format | Partial parse while streaming | Validation and constrained decoding | Model familiarity | Error recovery | Human readability |
|---|---|---|---|---|---|
| JSON compact | Mature libraries (Vercel `fixJson`, jiter, partial-json-parser). Anthropic buffers one property at a time unless eager streaming is on. | JSON Schema everywhere. OpenAI 100% on evals. Anthropic strict mode. Ollama `format`. llguidance, XGrammar, GBNF. | Highest. Every model, every size. | Repair libraries exist. Truncation closes cleanly. | Low when compact. |
| JSON flat id map | Per-component lines stream well. | Same as JSON. | High, but ids are a new discipline. A2UI v0.8 fine-tune fails on v0.9. | Dangling child ids need a rule. | Medium. |
| Hiccup arrays | Same as JSON. | Tuple schemas use `prefixItems`, which OpenAI strict mode and llama.cpp's schema converter do not support. | Medium. Clojure-adjacent. | Same as JSON. | Medium. |
| YAML | No LLM-oriented partial YAML parser found. Indentation errors cascade. | Schema via JSON Schema after parse only. No constrained decoding path on hosted APIs. | High for config, lower for generation. | Poor. One bad indent breaks the rest. | High. |
| TOON | Reference lib has `decodeStream`. `[N]` detects truncation. | Spec is for prompts, not generation. No constrained decoding path. | Low. Syntax score 0.484 vs 0.802 for JSON on open models. 0% one-shot on a nested invoice case. | Strict mode rejects, non-strict guesses. | High for tables. |
| S-expression | Trivial: unclosed parens mark open nodes. No published parser for LLM streams. | CFG constrained decoding only (llguidance, XGrammar, GBNF). No hosted strict mode. | Medium. No published generation accuracy. | Paren balance repair at end of stream. | Medium. |
| Slint-like | Brace-delimited, streams by node. | CFG only. | Low. Slint is rare in training data. | Brace repair. | High. |
| Indentation DSL | Per line. Trivial. | CFG only. Whitespace-sensitive grammars are harder for GBNF. | Medium. Pug is rare. OpenUI Lang shows frontier models at 83% to 100% every-time, a 26B open model at 56%. | A bad indent only hurts one subtree. | High. |
| Mote UI (braces) | Per line plus explicit close. | CFG only. OpenAI Lark custom tools and llama-server GBNF can constrain it. Ollama cannot. | Same as the indentation DSL, with explicit braces reducing ambiguity. | Brace repair at end of stream. Per-line skip. | High. |
| HTML | Tag-balanced partial parse exists. | HTML is not a closed vocabulary. | Highest. | Browser-grade. | Medium. |

### 5.4 Published evidence that changes the decision

- Format restriction can hurt reasoning when applied to the whole response. Tam et al. report GSM8K on Claude 3 Haiku dropping from 86.51 to 23.44 under JSON schema. Rebuttals with better prompts show no drop on Llama 3 8B. The 2026 "Capacity, Not Format" study finds truncation drives most of the loss and that deferred formatting (reason first, format after) recovers to 80% to 87%. Rule for mote: the UI block is the last part of a turn. Reasoning and prose come first, unconstrained. Only the block is constrained.
- Schema adherence is not correctness. One benchmark reports GPT-5.4 at 99.97% JSON pass rate but only 0.486 perfect responses, and models above 97% schema compliance with 17% to 31% wrong leaf values. The daemon's semantic checks (scope, contracts, limits) matter as much as syntax.
- Silent repair hides failures. Very Good Ventures found a Gemini model emitting a bare array instead of an A2UI envelope on 6 of 15 turns, masked by the SDK's auto-repair. Rule for mote: repair is syntactic and bounded, every repair is a diagnostic, and the model sees it.
- Small models are worse at custom syntax. TOON generation collapsed on Gemma 3 4B (0.045 vs 0.779 for JSON). OpenUI Lang's every-time rate on Gemma-4-26B was 55.6% against 83% to 100% on frontier models. The fast local model is the risk for a custom language. Constrained decoding removes that risk only where the inference server supports grammars.
- Ollama supports JSON Schema only. GBNF was closed unmerged in favor of JSON schema. A custom language constrained on a local model needs llama-server (GBNF or llguidance) or an MLX logits processor. On Apple Silicon one benchmark puts the GBNF throughput tax at 25% to 28% and llguidance-class engines near 50 µs per token.
- Precedent exists. Thesys built on JSON, hit retries from malformed output, and moved to a line-oriented language with a reported 52.8% token saving.

### 5.5 Decision

v0: the tree's wire format is compact JSON, nested, with full-word keys.

v1: the language is the emit format for models whose backend supports grammar-constrained decoding or whose measured validity rate on the language meets UI-24. The daemon compiles it to the same tree. JSON stays accepted forever.

Reasons:

1. JSON is the only format with strict-mode guarantees on both hosted APIs and on Ollama today. That is where the v0 risk is.
2. The nested tree beats the flat id map by 20% in tokens and needs no id discipline. Streaming works through partial parse because the model emits nodes in visual order.
3. Hiccup's 4% gain does not pay for losing strict-mode schemas.
4. YAML and TOON are worse than compact JSON on nested UI data in both the fixtures and the published studies.
5. The language saves 34% over compact JSON and 63% over pretty JSON, streams per line, and reads well in the model's context on the next turn. Those are the long-term wins. They are gated on constrained decoding or measured validity because small models are the risk.

### 5.6 Tree format rules

```json
{"type":"card","title":"Flight TP 1234","children":[{"type":"text","text":"Departs LIS 18:40"}]}
```

- One object per node. `type` is the first key. Children are last, under `children`. Props sit between. Key order is a formatter rule, not a parser rule.
- Keys are full words in camelCase. No abbreviations.
- Strings are plain text. The `$` prefix marks a symbol reference (`"$selected.invoice"`, `"$title"`, `"$.id"` inside an item template).
- Actions are `{"name":"CheckIn","args":{...}}`. Built-in actions have the same shape.
- Data bindings are `"from":"mail.unread"` and `"of":"job.book_table"`.
- No ids unless the model sets `"id"`. Reconciliation uses path and type otherwise.
- No whitespace on the wire. The daemon pretty-prints for logs and for the model's context only when the language is unavailable.
- A JSON Schema for the tree is published at `schemas/tree.v0.json` (compiler output, UI-02). It uses `$ref` recursion for `children`, which OpenAI strict mode supports and Anthropic strict mode does not. For Anthropic the daemon unrolls the schema to depth 8 and rejects deeper trees with `E020`.
- Framing: the model emits the tree as a tool call named `surface` when the backend supports tools, else inside a ```` ```json ```` fence as the last block of the turn.
- Limits: depth 12, 2,000 nodes, 5,000 rows, 20,000 chars per string, 256 KB per surface.

## 6. Language proposal

Working name: Mote UI. File ending: `.mote`. The compile target is the v0 tree (section 5).

### 6.1 Shape of the language

One surface per turn. A surface is a tree of nodes. A node is one line, or one line plus a brace block.

```text
node     = word args? modifier* event? block?
args     = literal | symbol | record       (comma separated, positional)
modifier = word literal?                   (select one, min 3, money, primary)
event    = "->" Action ( "(" named args ")" | "{" fields "}" )?
block    = "{" node* "}"
```

Rules the reader needs:

- The first word names the element. The catalog of words is closed (section 7).
- The first quoted string is the label. Labels never carry markup.
- Modifiers are bare words, or a word and one literal. They never take strings that name other symbols.
- `->` sends an event. The daemon derives the event contract from the arguments.
- `{ }` holds children. Children stack vertically unless the parent is `row` or `grid`.
- Symbols (`selected`, `title`, `invoices`) refer to slots the compiler infers (section 8) or to data the daemon holds.
- There are no expressions, no conditionals, no loops, no functions. The model computes. The surface declares.
- Casing is compiler-invariant. `checkIn`, `CheckIn`, and `check_in` resolve to one action. The formatter rewrites to the declared spelling.

### 6.2 Examples

Each example shows the language and then the same surface in the v0 tree. The tree is shown pretty-printed for reading. On the wire it is compact (section 5).

#### Example 1: card

```text
card "Flight TP 1234" {
    text "Departs LIS 18:40, arrives CDG 22:25"
    text "Gate 12. Boarding at 18:05."
    row {
        button "Check in" primary -> CheckIn(flight: "TP1234")
        button "Directions" -> Open(url: "https://maps.example/lis-gate-12")
    }
}
```

```json
{"type":"card","title":"Flight TP 1234","children":[
  {"type":"text","text":"Departs LIS 18:40, arrives CDG 22:25"},
  {"type":"text","text":"Gate 12. Boarding at 18:05."},
  {"type":"row","children":[
    {"type":"button","label":"Check in","style":"primary","action":{"name":"CheckIn","args":{"flight":"TP1234"}}},
    {"type":"button","label":"Directions","action":{"name":"Open","args":{"url":"https://maps.example/lis-gate-12"}}}
  ]}
]}
```

What the compiler inferred: `Open` is a daemon built-in and never reaches the model. `CheckIn` is a model action with contract `{flight: str}`.

#### Example 2: list

```text
list "Today" {
    item "09:30 Standup" "Zoom, 15 min" -> Show(event: "ev_81")
    item "12:00 Lunch with Ana" "Café Lisboa" -> Show(event: "ev_82")
    item "18:40 Flight TP 1234" "Gate 12" -> Show(event: "ev_83")
}
```

```json
{"type":"list","title":"Today","children":[
  {"type":"item","label":"09:30 Standup","detail":"Zoom, 15 min","action":{"name":"Show","args":{"event":"ev_81"}}},
  {"type":"item","label":"12:00 Lunch with Ana","detail":"Café Lisboa","action":{"name":"Show","args":{"event":"ev_82"}}},
  {"type":"item","label":"18:40 Flight TP 1234","detail":"Gate 12","action":{"name":"Show","args":{"event":"ev_83"}}}
]}
```

An `item` takes a label and an optional detail. A list scrolls when it overflows. The model never writes that.

#### Example 3: list bound to daemon data

```text
list "Inbox" from mail.unread {
    item .subject .from -> Open(mail: .id)
    empty "Nothing unread"
}
```

```json
{"type":"list","title":"Inbox","from":"mail.unread","itemTemplate":{"type":"item","label":"$.subject","detail":"$.from","action":{"name":"Open","args":{"mail":"$.id"}}},"empty":"Nothing unread"}
```

`from mail.unread` binds the list to a data symbol the daemon holds (a tool result). The model does not retype the rows. `.subject` is a field of each row. The compiler checks the fields against the data symbol's known shape and rejects unknown ones.

#### Example 4: table with selection

```text
table "Open invoices" select one -> ShowInvoice(id: selected.invoice) {
    columns invoice "Invoice", client "Client", amount "Amount" money, due "Due" date
    rows {
        "INV-1041", "Acme", 1200.00, 2026-10-12
        "INV-1042", "Globex", 850.50, 2026-10-15
        "INV-1043", "Initech", 3400.00, 2026-10-20
        "INV-1044", "Umbrella", 210.00, 2026-10-22
        "INV-1045", "Hooli", 99.99, 2026-10-30
    }
    button "Send reminders" -> RemindSelected(ids: selected.invoice)
}
```

```json
{"type":"table","title":"Open invoices","select":"one",
 "columns":[{"key":"invoice","label":"Invoice"},{"key":"client","label":"Client"},{"key":"amount","label":"Amount","format":"money"},{"key":"due","label":"Due","format":"date"}],
 "rows":[{"invoice":"INV-1041","client":"Acme","amount":1200.0,"due":"2026-10-12"},
         {"invoice":"INV-1042","client":"Globex","amount":850.5,"due":"2026-10-15"},
         {"invoice":"INV-1043","client":"Initech","amount":3400.0,"due":"2026-10-20"},
         {"invoice":"INV-1044","client":"Umbrella","amount":210.0,"due":"2026-10-22"},
         {"invoice":"INV-1045","client":"Hooli","amount":99.99,"due":"2026-10-30"}],
 "onSelect":{"name":"ShowInvoice","args":{"id":"$selected.invoice"}},
 "children":[{"type":"button","label":"Send reminders","action":{"name":"RemindSelected","args":{"ids":"$selected.invoice"}}}]}
```

`select one` infers a selection slot. `selected` is a symbol only inside a node with `select`. Using it elsewhere is a compile error. `money` and `date` are column formats. The renderer right-aligns money and localizes dates. Sorting by column header is a built-in affordance.

#### Example 5: form with validation and submit

```text
form "New reminder" {
    field title str "Title" min 3
    field when DateTime "When"
    field repeat [none, daily, weekly] "Repeat" = none
    field note str? "Note"
    row {
        button "Create" submit -> CreateReminder {title, when, repeat, note}
        button "Cancel" -> Cancel
    }
}
```

```json
{"type":"form","title":"New reminder","children":[
  {"type":"field","name":"title","label":"Title","kind":"str","min":3},
  {"type":"field","name":"when","label":"When","kind":"datetime"},
  {"type":"field","name":"repeat","label":"Repeat","kind":"choice","options":["none","daily","weekly"],"default":"none"},
  {"type":"field","name":"note","label":"Note","kind":"str","optional":true},
  {"type":"row","children":[
    {"type":"button","label":"Create","submit":true,"action":{"name":"CreateReminder","args":{"title":"$title","when":"$when","repeat":"$repeat","note":"$note"}}},
    {"type":"button","label":"Cancel","action":{"name":"Cancel"}}
  ]}
]}
```

Fields are required unless the type ends in `?`. Validation runs in the renderer before submit and again in the daemon before the event reaches the model. The event contract for `CreateReminder` is `{title: str, when: DateTime, repeat: none|daily|weekly, note: str?}`. The compiler derives it from the field declarations.

#### Example 6: chart

```text
chart bar "Steps this week" {
    series steps "Steps"
    rows {
        "Mon", 8200
        "Tue", 10400
        "Wed", 6100
        "Thu", 12050
        "Fri", 9800
    }
}
```

```json
{"type":"chart","kind":"bar","title":"Steps this week","series":[{"key":"steps","label":"Steps"}],
 "rows":[["Mon",8200],["Tue",10400],["Wed",6100],["Thu",12050],["Fri",9800]]}
```

Supported kinds: `bar`, `line`, `pie`, `sparkline`. Axes, ticks, colors, and the legend come from the theme. The model never sets a color.

#### Example 7: progress for a long-running agent task

```text
progress "Booking a table for Friday" of job.book_table {
    step "Found 4 restaurants nearby" done
    step "Checking availability at Nopa" active
    step "Confirm with you"
    button "Stop" danger -> Cancel(job: job.book_table)
}
```

```json
{"type":"progress","title":"Booking a table for Friday","of":"job.book_table","children":[
  {"type":"step","label":"Found 4 restaurants nearby","state":"done"},
  {"type":"step","label":"Checking availability at Nopa","state":"active"},
  {"type":"step","label":"Confirm with you"},
  {"type":"button","label":"Stop","style":"danger","action":{"name":"Cancel","args":{"job":"job.book_table"}}}
]}
```

`of job.book_table` binds the node to a daemon job. The daemon updates step states and the bar from job events without a model turn. A `progress` node with no steps renders an indeterminate bar.

#### Example 8: confirm dialog

```text
dialog "Send 3 reminders?" {
    text "Acme, Globex, and Initech will get an email now."
    row {
        button "Send" primary -> RemindSelected(ids: ["INV-1041", "INV-1042", "INV-1043"])
        button "Cancel" -> Dismiss
    }
}
```

```json
{"type":"dialog","title":"Send 3 reminders?","children":[
  {"type":"text","text":"Acme, Globex, and Initech will get an email now."},
  {"type":"row","children":[
    {"type":"button","label":"Send","style":"primary","action":{"name":"RemindSelected","args":{"ids":["INV-1041","INV-1042","INV-1043"]}}},
    {"type":"button","label":"Cancel","action":{"name":"Dismiss"}}
  ]}
]}
```

A dialog traps focus, closes on Escape, and sends `Dismiss` on close. `Dismiss` is a built-in. A `button` with `danger` and no `confirm` modifier still acts at once. `confirm "..."` adds a second step.

#### Example 9: chat transcript

```text
chat {
    message user "What's on today?"
    message agent "Three things. Standup at 09:30, lunch with Ana at noon, and your flight at 18:40."
    message agent {
        list "Today" from calendar.today {
            item .title .when -> Show(event: .id)
        }
    }
    typing
    compose -> Say {text}
}
```

```json
{"type":"chat","children":[
  {"type":"message","role":"user","text":"What's on today?"},
  {"type":"message","role":"agent","text":"Three things. Standup at 09:30, lunch with Ana at noon, and your flight at 18:40."},
  {"type":"message","role":"agent","children":[
    {"type":"list","title":"Today","from":"calendar.today","itemTemplate":{"type":"item","label":"$.title","detail":"$.when","action":{"name":"Show","args":{"event":"$.id"}}}}
  ]},
  {"type":"typing"},
  {"type":"compose","action":{"name":"Say","args":{"text":"$text"}}}
]}
```

A `message` carries text or children. `typing` shows the agent is still generating. `compose` is the input box. The chat scrolls to the newest message unless the user scrolled up.

#### Example 10: touchscreen home screen

```text
screen Home {
    clock
    row {
        tile "Weather" "18° clear" -> Show(view: "weather")
        tile "Next" "Standup 09:30" -> Show(event: "ev_81")
        tile "Reminders" "2 due" -> Show(view: "reminders")
    }
    status "Listening" live
    button "Talk" large -> Listen
}
```

```json
{"type":"screen","name":"Home","children":[
  {"type":"clock"},
  {"type":"row","children":[
    {"type":"tile","label":"Weather","detail":"18° clear","action":{"name":"Show","args":{"view":"weather"}}},
    {"type":"tile","label":"Next","detail":"Standup 09:30","action":{"name":"Show","args":{"event":"ev_81"}}},
    {"type":"tile","label":"Reminders","detail":"2 due","action":{"name":"Show","args":{"view":"reminders"}}}
  ]},
  {"type":"status","text":"Listening","live":true},
  {"type":"button","label":"Talk","size":"large","action":{"name":"Listen"}}
]}
```

`screen` is the root for a full-screen target. On the Pi every touch target is at least 48 px. `tile` is a large tappable card. `Listen` is a daemon built-in that opens the microphone.

#### Example 11: composed page

```text
page "Invoices" {
    row {
        stat "Open" 5
        stat "Overdue" 1 danger
        stat "Total" 5760.49 money
    }
    table "Open invoices" from billing.open select many {
        columns invoice "Invoice", client "Client", amount "Amount" money, due "Due" date
        button "Send reminders" -> RemindSelected(ids: selected.invoice)
        empty "No open invoices"
    }
    card "Suggestion" {
        text "Umbrella is 3 days late. Want me to call them?"
        row {
            button "Call" primary -> Call(client: "Umbrella")
            button "Not now" -> Dismiss
        }
    }
}
```

```json
{"type":"page","title":"Invoices","children":[
  {"type":"row","children":[
    {"type":"stat","label":"Open","value":5},
    {"type":"stat","label":"Overdue","value":1,"style":"danger"},
    {"type":"stat","label":"Total","value":5760.49,"format":"money"}
  ]},
  {"type":"table","title":"Open invoices","from":"billing.open","select":"many",
   "columns":[{"key":"invoice","label":"Invoice"},{"key":"client","label":"Client"},{"key":"amount","label":"Amount","format":"money"},{"key":"due","label":"Due","format":"date"}],
   "empty":"No open invoices",
   "children":[{"type":"button","label":"Send reminders","action":{"name":"RemindSelected","args":{"ids":"$selected.invoice"}}}]},
  {"type":"card","title":"Suggestion","children":[
    {"type":"text","text":"Umbrella is 3 days late. Want me to call them?"},
    {"type":"row","children":[
      {"type":"button","label":"Call","style":"primary","action":{"name":"Call","args":{"client":"Umbrella"}}},
      {"type":"button","label":"Not now","action":{"name":"Dismiss"}}
    ]}
  ]}
]}
```

#### Example 12: error and empty states written explicitly

```text
card "Mail" {
    error "Could not reach the mail server" retry -> Retry(tool: "mail.unread")
}
```

```json
{"type":"card","title":"Mail","children":[
  {"type":"error","text":"Could not reach the mail server","action":{"name":"Retry","args":{"tool":"mail.unread"}}}
]}
```

Most error and empty states are implicit (section 7). The model writes them only to change the wording or add an action.

#### Example 13: key-value detail

```text
card "INV-1044" {
    kv {
        "Client" "Umbrella"
        "Amount" 210.00 money
        "Due" 2026-10-22 date
        "Status" "Overdue" danger
    }
    row {
        button "Remind" primary -> Remind(id: "INV-1044")
        button "Back" -> Back
    }
}
```

```json
{"type":"card","title":"INV-1044","children":[
  {"type":"kv","rows":[
    {"key":"Client","value":"Umbrella"},
    {"key":"Amount","value":210.0,"format":"money"},
    {"key":"Due","value":"2026-10-22","format":"date"},
    {"key":"Status","value":"Overdue","style":"danger"}
  ]},
  {"type":"row","children":[
    {"type":"button","label":"Remind","style":"primary","action":{"name":"Remind","args":{"id":"INV-1044"}}},
    {"type":"button","label":"Back","action":{"name":"Back"}}
  ]}
]}
```

`Back` is a built-in. The daemon keeps the surface history (section 8).

### 6.3 Literals and types

| Literal | Example | Tree value |
|---|---|---|
| string | `"Check in"` | JSON string |
| int, float | `5`, `99.99` | JSON number |
| money | `1200.00` with `money` modifier | number plus `"format":"money"` |
| date, datetime | `2026-10-22`, `2026-10-22T18:40` | ISO string plus `"format":"date"` or `"datetime"` |
| bool | `true`, `false` | JSON bool |
| list | `[none, daily, weekly]` | JSON array |
| symbol | `selected.invoice`, `.id`, `title` | `"$selected.invoice"`, `"$.id"`, `"$title"` |
| record | `{title, when}` | `{"title":"$title","when":"$when"}` |

Field types: `str`, `int`, `float`, `bool`, `Money`, `Date`, `DateTime`, `Time`, `Email`, `Url`, `Phone`, `[a, b, c]` (choice), `text` (multi-line). `?` after a type means optional. There is no null. An absent optional field is absent in the event.

### 6.4 Words that look like code but are not

- `->` is an event edge, not a function call. Nothing executes in the surface.
- `{title, when, repeat}` after an action is a gather of field slots, not an object literal with expressions.
- `from billing.open` is a data binding to a symbol the daemon owns, not a query.
- `.subject` inside a bound list is a field path on the bound row, not a method.

### 6.5 What the formatter does

- Four-space indentation, one node per line, `{` on the node's line.
- Rewrites casing of actions and symbols to the declared or built-in spelling.
- Normalizes literal spelling (`1200` with `money` becomes `1200.00`).
- Orders modifiers in catalog order (`primary` before `large`, `select` before `->`).
- Never adds, removes, or reorders nodes.
- Runs on every surface before the canonical form is stored or shown to the model.

## 7. Built-in affordances catalog

The owner's rule: "There's always a scrollbar. There's always this or that." The renderer owns these. The model never writes them. The table lists what is implicit, what a model may opt out of, and what no surface can express.

### 7.1 Always there (implicit)

| Affordance | Behavior | Where it lives |
|---|---|---|
| Scrolling | Any container taller than the viewport scrolls. Lists, tables, and chat scroll inside their box. Scroll position survives re-render of the same node. | renderer |
| Focus | One focused element at a time. Tab order follows tree order. Focus survives re-render when the node key matches. Dialogs trap focus. | renderer |
| Keyboard | Enter activates the focused button or submits the focused form. Escape dismisses a dialog. Arrow keys move inside lists and tables. | renderer |
| Touch | Every interactive node has a minimum 48 px target on touch targets. Tap, long press, swipe to scroll. | renderer |
| Selection | `select one` and `select many` give checkbox or radio semantics, shift-click ranges, and a `selected` symbol. | renderer, compiler |
| Loading state | A node bound with `from` or `of` shows a skeleton until data arrives. A surface still streaming shows a skeleton for the incomplete tail. | renderer, daemon |
| Error state | A bound node whose data failed shows a short error and a Retry action. A rejected event shows the daemon's message inline. | renderer, daemon |
| Empty state | A bound list or table with zero rows shows "Nothing here" unless `empty "..."` overrides. | renderer |
| Responsive layout | `row` wraps on narrow widths. `grid N` reduces columns. Tables collapse to stacked cards below a width threshold. | renderer |
| Theming | Light and dark follow the system. Colors come from the theme only. `primary`, `danger`, `secondary` are roles, not colors. | renderer |
| Accessibility | Every node maps to an accessibility role. Labels are the accessible names. Images require `alt`. Live regions for `status live` and `typing`. | renderer |
| Text wrapping and truncation | Text wraps. `item` detail truncates with an ellipsis and a tooltip. | renderer |
| Number and date formatting | `money`, `date`, `datetime`, `pct` format per locale. | renderer |
| Sorting | Table headers sort by column when the column type is sortable. | renderer |
| Feedback on actions | A button shows a pending state after a tap until the daemon acknowledges the event. Double taps are ignored. | renderer |
| Back | `Back` returns to the previous surface in daemon history. | daemon |
| Dismiss | Closes the nearest dialog or sheet. | renderer, daemon |
| Validation | Field constraints run on blur and on submit. The submit button disables until the form is valid. | renderer, daemon |
| Reconciliation | A new surface is diffed against the current one. Matching nodes keep interaction state. | daemon, renderer |
| Timestamps on chat | Messages get a time on hover or on tap. | renderer |
| Copy | Long press or right click on text offers Copy. | renderer |

### 7.2 Opt-out or override (modifiers)

| Modifier | On | Effect |
|---|---|---|
| `empty "..."` | list, table | Custom empty message. |
| `error "..."` | any container | Custom error message, optional `-> Retry(...)`. |
| `noscroll` | list, table | Container grows instead of scrolling. Use inside a scrolling page. |
| `select one` / `select many` | list, table | Enables selection. Default is none. |
| `sort by <column>` | table | Initial sort. Default is tree order. |
| `collapsible` / `collapsed` | card | Toggle body. Default is open. |
| `confirm "..."` | button | Two-step confirmation. Default is one tap. |
| `disabled` | button, field | Not interactive. Still rendered. |
| `hidden` | any | Not rendered, still validated. For agents preparing a node that a later patch reveals. |
| `large` / `small` | button, text, tile | Size role. |
| `wide` / `fill` | any in a row | Grow to available width. |
| `primary` / `danger` / `secondary` | button, text, stat, badge | Visual role. One `primary` per row. |
| `live` | status, text | Announces changes to assistive technology. |
| `mono` | text | Monospace. For ids and code. |
| `alt "..."` | image | Required accessible description. |

### 7.3 Never expressible

The language and the tree reject these at compile time. The renderer has no code path for them.

- Pixel sizes, fixed widths, margins, paddings, z-index, absolute positioning.
- Colors, fonts, font sizes, border radii, shadows.
- Markup inside strings. `text` is plain text. `md` is a constrained Markdown subset: emphasis, lists, inline code, links. No raw HTML, no images via Markdown, no headings deeper than two levels.
- Scripts, expressions, handlers, timers, or any code.
- Network requests. Only the daemon fetches. `image` takes a daemon asset symbol or a URL the daemon fetches and caches.
- Arbitrary focus or scroll control. A surface can set `focus` on at most one field by the `focus` modifier.
- Custom components. The catalog is closed per language version.
- Animations beyond the theme's built-in transitions.
- Haptics, sound, or vibration. `Listen` and `Speak` are daemon built-ins with fixed behavior.
- Clipboard writes except the user-initiated Copy affordance.

### 7.4 Catalog words (v1)

Containers: `page`, `screen`, `card`, `row`, `col`, `grid`, `dialog`, `sheet`.
Text: `title`, `text`, `md`, `label`, `badge`, `stat`, `kv`, `divider`, `icon`, `image`, `clock`, `status`.
Collections: `list`, `item`, `table`, `columns`, `rows`, `tile`.
Input: `form`, `field`, `button`, `toggle`, `compose`, `search`.
Feedback: `progress`, `step`, `empty`, `error`, `typing`.
Chat: `chat`, `message`.
Chart: `chart`, `series`.

Built-in actions (daemon handles, model never sees): `Open(url)`, `Back`, `Dismiss`, `Copy(text)`, `Listen`, `Speak(text)`, `Retry(tool)`, `Cancel(job)`.

Everything else after `->` is a model action. The compiler records its contract and the daemon forwards the event to the model.

## 8. State, store, routes

The owner's question: "I don't know if we even need to internally have any route or any store concepts baked into the code. Maybe the agent can internally handle that. Ideally, if it could all be one thing and the store is either inferred..."

### 8.1 What state exists

Four kinds of state show up in every generated UI. Naming them makes the options clear.

| Kind | Examples | Changes how often | Needs the model? |
|---|---|---|---|
| Interaction | focus, scroll offset, text typed into a field, selected rows, open dialog, expanded card | every keystroke | never |
| Data | tool results, inbox rows, job progress, sensor values | per tool call or per tick | no, the daemon has it |
| Semantic | what the user is doing, which surface to show next, what the agent believes | per turn | yes |
| History | the surfaces shown so far, for Back and for restart | per turn | no |

### 8.2 Options

#### Option A: the agent is the store

Every state change is a model turn. The model receives each event and emits a whole surface.

- Fits: semantic state. The model already decides what comes next.
- Breaks: interaction state. A keystroke cannot wait 300 ms for a local model, and the model would have to echo back the text the user typed. Progress updates every second would cost a turn each.
- Verdict: right for semantic state, wrong for everything else.

#### Option B: store inferred from declarations

The compiler reads each declaration and allocates the slots it implies. `field title` allocates a `title` slot. `select one` allocates a `selected` slot. `collapsible` allocates an open/closed slot. `progress of job.x` binds a slot to a daemon job. The model never declares a store. It only references slots by symbol.

- Fits: interaction state and data. Slots are typed from the declaration, so events carry typed values.
- Breaks: nothing in the common case. Cross-surface state (a draft kept across turns) needs a rule, see 8.4.
- Verdict: adopt. This is structural inference applied to UI.

#### Option C: thin declared store

The model writes a `state { ... }` block with named values and local update rules such as `toggle dark -> set theme`.

- Fits: local toggles without a model turn.
- Breaks: the principle that the surface declares and the model computes. It introduces a second state machine the model must reason about, and a mini language of update rules that grows into JavaScript. Every rule is a place for a bug the daemon cannot validate semantically.
- Verdict: reject for v1. Cover the common local toggles with modifiers (`collapsible`, `confirm`, `select`) and built-in actions.

#### Option D: routes

A route table maps names to surfaces.

- Fits: deep links and bookmarks.
- Breaks: the model picks what to show per turn, so a route table would be a second source of truth. Routes also force the model to name screens before it knows what the user wants.
- Verdict: reject. Keep a surface history in the daemon. `Back` pops it. A `Show(view: "weather")` action is just an event the model answers with a surface. If deep links ever matter, the daemon can map a URL to a synthetic event.

### 8.3 Recommendation

Three owners, one language, no routes, no declared store.

1. The renderer owns interaction state in slots the compiler infers from declarations.
2. The daemon owns data and history. Nodes bind to data with `from` and `of`. The daemon patches bound nodes without a model turn.
3. The model owns semantic state. It receives typed events and emits the next surface.

The model sees one thing: the current canonical surface plus the event. It never sees a store.

### 8.4 Rules

- A slot's lifetime is the node's lifetime. When reconciliation matches a node across surfaces, its slots survive. When the node disappears, its slots die.
- Reconciliation matches by `id` when present, else by path and word. The formatter does not add ids. The model may add `id foo` to a node it expects to keep across surfaces.
- Typed field text survives a re-render of the same form. A new form with the same field name but a different `id` starts empty.
- `selected` is scoped to its node. Two tables on one surface have two `selected` symbols. An action argument `selected.invoice` resolves to the nearest enclosing node with `select`.
- A bound node (`from`, `of`) keeps rendering its last data while the daemon refreshes. Stale data shows a subtle indicator after 30 s.
- The daemon stores every canonical surface with a sequence number. Restart restores the last surface and its slots. Interaction state restores on a best-effort basis (scroll and selection yes, focus no).

### 8.5 Event flow

```text
renderer                     daemon                            model
   |-- event{surface, node, ---->|                                 |
   |   action, args}             |-- validate against contract     |
   |                             |-- if built-in: handle, patch -->|  (no model turn)
   |<-- ack / patch -------------|                                 |
   |                             |-- else: forward -------------->|
   |                             |<-- surface stream (partial) ----|
   |<-- validated partial tree --|                                 |
   |<-- validated final tree ----|                                 |
```

Event payload on the wire, compact JSON:

```json
{"ev":"CreateReminder","surface":412,"node":"form.1.row.1.button.1","args":{"title":"Call Ana","when":"2026-10-09T17:00","repeat":"none"},"at":"2026-10-08T19:14:03Z"}
```

The daemon rejects an event whose `surface` is not the current one, whose node path does not exist, or whose args fail the contract. Rejection returns a diagnostic to the renderer (inline message) and nothing to the model.

The model receives the event as a structured message, not as prose:

```json
{"event":"CreateReminder","args":{"title":"Call Ana","when":"2026-10-09T17:00","repeat":"none"},"surface":"<canonical .mote text>"}
```

Including the canonical surface in the model's context is deliberate. The model sees what the user saw, in the same language it will emit next. This keeps emissions consistent across turns and lets a small model copy structure instead of inventing it.

### 8.6 What "the agent is the state machine" means in practice

- The agent decides transitions. The daemon decides validity.
- The agent never manipulates slots. It reads their values in events.
- The agent never issues patches to interaction state. It emits surfaces. The daemon computes the diff.
- The daemon patches data-bound nodes and job progress on its own. The agent is told when a job finishes, as an event, and then emits what comes next.

## 9. Streaming and validation

The daemon validates everything a model emits before anything renders. It also renders while the model is still generating. These two goals pull against each other. The rules below resolve the pull in favor of validation.

### 9.1 Pipeline

```text
model tokens -> framing -> incremental parse -> check -> lower -> diff -> renderer
                  |             |                |         |
                  |             |                |         +-- diagnostics (to log and to model)
                  |             |                +-- catalog, types, action contracts, limits
                  |             +-- partial tree with "open" nodes
                  +-- strip prose before and after the surface block
```

Framing: the model emits the surface inside a fenced block (` ```mote ` for the language, ` ```json ` for the tree). Text outside the block is the agent's spoken or chat text. A turn with no block renders the text as a `message`.

### 9.2 Partial parse

Tree format (v0): a partial JSON parser closes open strings, arrays, and objects at the current position to produce a prefix tree. Every node in the prefix tree is marked `complete` or `open`.

Language (v1): the parser is line-oriented. A line is a node once its newline arrives. An unclosed `{` marks the parent as `open`.

Both parsers produce the same partial-tree structure with the same `open` flags.

### 9.3 What renders while a tree is incomplete

| Node state | Rendered as |
|---|---|
| complete, valid | the node |
| open container with complete children | the container with its children and a trailing skeleton row |
| open text, label, or md | the text so far, with a cursor, in a non-interactive style |
| open button, field, item with action | a skeleton of the same size, not interactive |
| open table | header plus complete rows plus a skeleton row |
| open chart | skeleton box |
| invalid node | dropped in production, red placeholder with the diagnostic in dev mode |

Rules:

- Interactive nodes render only when complete and valid. A half-written action never fires.
- Text streams. Nothing else streams character by character.
- A surface becomes interactive as a whole only after the final token and a full check. Until then taps are ignored and the renderer shows a quiet "still writing" indicator.
- Layout is stable during streaming. A skeleton reserves one line of height. Nothing jumps more than one row when a node completes.

### 9.4 Fail-closed rules

| Condition | Outcome |
|---|---|
| Unknown word or node type | node rejected, diagnostic `E001 unknown element` |
| Unknown modifier | node rejected, `E002 unknown modifier on <word>` |
| Wrong literal type for a modifier | node rejected, `E003 <modifier> expects <type>` |
| Action args do not match a contract | the action is removed, the button renders disabled, `E010 action contract mismatch` |
| Symbol not in scope (`selected` outside `select`) | node rejected, `E011 symbol not in scope` |
| Data symbol unknown to the daemon | node renders its error state, `E012 unknown data symbol` |
| Depth over 12, nodes over 2000, rows over 5000, string over 20 000 chars | surface truncated at the limit, `E020 limit` |
| Markup in a string | string rendered as plain text, `W001 markup ignored` |
| Two `primary` buttons in one row | second downgraded, `W002 one primary per row` |
| Duplicate field name in a form | second rejected, `E013 duplicate field` |
| Malformed stream at end of turn | last valid prefix stays, `E030 unterminated surface` |

A rejected node never renders. A surface with any `E` diagnostic still renders its valid nodes. The daemon attaches the diagnostics to the turn and sends them to the model only when the model's next turn needs them (section 9.6).

### 9.5 Repair

Repair is bounded and syntactic. The daemon never guesses content.

- End of stream: close open strings, arrays, objects, and braces. Drop a trailing comma. Nothing else.
- A line the language parser cannot read: skip the line, record `E004 unreadable line <n>`, continue. Do not resynchronize inside the line.
- Unbalanced closing brace: ignore it, record `W003 stray brace`.
- Casing drift: resolve case-invariantly, no diagnostic.
- Quoted modifier that should be bare (`select "one"`): accept, record `W004 quoted modifier`, formatter fixes it.

The daemon does not call a model to repair output. A second model call is a retry (9.6), not a repair.

### 9.6 Retry policy

- If the final surface has any `E` diagnostic, the daemon sends the model one retry message with the diagnostics and the canonical text of the valid prefix. The retry is one turn. It is not shown as a new message to the user.
- If the retry also fails, the daemon renders the valid prefix plus the model's prose as a `message`. The user always gets something.
- `W` diagnostics never trigger a retry. They are logged and included in the next normal turn's context as a short list, so the model learns the formatter's spelling.
- Retry budget: one per turn, at most three per minute per surface. Beyond that, prose only.

### 9.7 Validation in the daemon versus the renderer

- The daemon is the only validator. The renderer trusts the validated tree.
- The renderer still clamps: it ignores events for nodes it does not have, and it never evaluates strings.
- Field validation runs in both places. The renderer gives instant feedback. The daemon decides.

### 9.8 Constrained decoding

Where the model backend supports it, the daemon constrains generation so invalid output cannot be produced.

- Hosted models with JSON structured outputs: the daemon supplies the tree's JSON Schema. Strict mode removes most `E00x` cases.
- Local models through llama.cpp: the daemon supplies a GBNF grammar generated from the language's grammar or from the tree's schema. Section 5 has the measured throughput cost.
- The validator still runs. Constrained decoding guarantees shape, not scope rules, contracts, or limits.

## 10. Rendering targets

### 10.1 v0: Tauri webview on macOS

- Tauri 2.12 (2026-09-30), Rust core, WKWebView-class WebKit on macOS. MIT or Apache-2.0.
- The daemon runs as a Rust process. The Tauri shell is a thin window. The validated tree reaches the webview over `tauri::ipc::Channel`, which Tauri documents as the mechanism for streaming data. Each channel message is a partial-tree update with `open` flags (section 9).
- The web renderer is a single hand-written module that maps each tree `type` to DOM nodes with a fixed stylesheet. No framework. No model-emitted HTML, CSS, or JavaScript ever enters the webview. CSP is set in `tauri.conf.json` with `default-src 'none'`, `script-src` by hash, and no `connect-src`.
- Events go back through `invoke("event", payload)` with the shape in section 8.5.
- The webview is a renderer, not a validator. It never evaluates strings.

### 10.2 v1: Raspberry Pi touchscreen, no browser

Facts that decide the choice:

- Slint 1.18.1 ships a LinuxKMS backend that runs with no X11 or Wayland, renders through DRM dumb buffers with a LinuxFB fallback in software mode, takes touch from libinput, and is labeled experimental. Slint has a runtime compiler (`slint-interpreter`) that returns diagnostics. Slint integrates AccessKit on desktop. Slint uses Taffy for layout. Slint's Royalty-free license excludes embedded. GPLv3 and the commercial license cover embedded.
- egui 0.36.2 has no DRM or framebuffer backend. A path exists through SDL2's kmsdrm driver or a custom DRM presenter over `egui_software_backend`. egui has AccessKit and headless role-based tests. MIT or Apache-2.0.
- Iced, Dioxus Native, GPUI, and Makepad have no documented DRM path. Tauri on the Pi needs WebKitGTK and a compositor.
- The official 7-inch display is 800 x 480 over DSI with ten-point touch on evdev. Touch Display 2 is 720 x 1280 portrait.

Decision: Slint on LinuxKMS is the primary v1 renderer. egui over a custom DRM presenter is the fallback. The license choice is open decision 7.

How the tree renders in Slint:

- The daemon compiles each surface to `.slint` source and loads it with `slint-interpreter`. Rows and items bind to `ModelRc` models the daemon owns, so bound data updates without recompiling.
- A fixed `.slint` component library implements every catalog word with the built-in affordances (ListView virtualization, FocusScope, styles with light and dark).
- Compile time per surface is measured in UI-47. If it exceeds 50 ms on a Pi 5 for the composed-page fixture, the fallback is a fixed generic component tree driven by models.

How the tree renders in egui (fallback):

- The renderer walks the tree every frame. Interaction slots live in egui `Memory` keyed by node path.
- Presentation goes through `egui_software_backend` into a DRM dumb buffer. Touch comes from evdev through a small reader.

### 10.3 Requirements on the tree so both targets render the same thing

- No pixel units in the tree. Sizes are roles (`small`, `large`, `wide`, `fill`) and the renderer's theme maps roles to pixels per target.
- Layout semantics are a written flexbox subset: `col` stacks with gap, `row` lays out horizontally and wraps when the sum of minimum widths exceeds the container, `grid N` divides width into N tracks and drops to 1 track below a threshold, `fill` grows, `wide` spans. Nothing else.
- Every container clips and scrolls its own overflow. Only `page` and `screen` scroll the viewport.
- Touch target minimum is 48 logical px on `screen` surfaces and 32 on `page` surfaces. Both targets enforce it.
- Fonts are a role scale (`title`, `text`, `small`, `mono`). The theme defines sizes per target.
- Every node has an accessibility role and name derived from its word and label. Both targets emit the same role tree (DOM ARIA on the web, AccessKit on Slint and egui).
- Golden fixtures: each example in section 6 has a golden screenshot per target at two widths and two themes. A layout-only reference implementation on Taffy computes boxes headlessly and asserts invariants (no overlap, no overflow, min touch size) independent of either renderer.

### 10.4 Layout engine choice

Taffy is the reference layout engine for the tree's layout semantics.

- Taffy 0.14.0 (2026-08-24, MIT, no_std) implements Block, Flexbox, and Grid and already sits under Slint, Blitz, Bevy, and Zed.
- The web target uses the browser's flexbox. The Slint target uses Slint's layouts, which sit on Taffy. The daemon's headless layout tests use Taffy directly. Three engines, one written subset, one fixture set.
- Yoga is rejected: C++ with bindgen bindings whose own README points to Taffy.
- Morphorm and Clay are rejected: smaller communities and no tie to the chosen renderers.
- A future custom GPU renderer (wgpu plus Taffy plus AccessKit) stays possible because the tree never depends on a renderer's layout quirks.

## 11. Compiler plan

One Rust crate, `mote-ui`, inside the daemon. No separate binary in v0. A `mote` CLI wraps it for fixtures and formatting.

### 11.1 Stages

| Stage | Input | Output | Notes |
|---|---|---|---|
| Framing | model text stream | surface text stream plus prose | Finds the fenced block or the tool call. Streams the block body. |
| Lexer | surface text | tokens with spans | Line-oriented. Strings, numbers, dates, symbols, words, `->`, braces, commas, `=`. |
| Parser | tokens | partial syntax tree with `open` flags | Incremental. Each newline commits a node. Unreadable lines are skipped with `E004`. |
| Resolver | syntax tree, daemon registry | resolved tree | Catalog words, modifiers, action contracts, data symbols, slot scopes. Case-invariant. |
| Checker | resolved tree | diagnostics | Types, limits, scope rules, one primary per row, duplicate fields. |
| Lowering | resolved tree | v0 tree (JSON value) | Pure. Deterministic. |
| Formatter | resolved tree | canonical surface text | Shares the resolver. Layout-only mode when resolution fails. |
| Decompiler | v0 tree | surface text | For logs, for showing a JSON-emitting model its surface in the language, and for round-trip tests. |

The JSON path shares everything from the resolver down. A JSON surface is parsed by a partial-JSON parser into the same partial tree shape and continues at the resolver.

### 11.2 Diagnostics

```json
{"code":"E011","severity":"error","message":"symbol not in scope: selected","span":{"line":4,"col":36,"len":8},"hint":"selected is only available inside a node with select"}
```

- Codes are stable. `E` blocks the node. `W` is advisory.
- Every diagnostic has a span in the surface text and a path in the tree.
- The daemon sends diagnostics to the model in the retry message, in a short list, with the surface's canonical text.
- The daemon logs every diagnostic with the surface sequence number.

### 11.3 Formatter

- `mote fmt` has no options.
- Four-space indent, one node per line, braces on the node line, modifiers in catalog order, canonical casing, canonical literal spelling.
- The formatter runs on every surface before the daemon stores it or shows it to the model.
- Property tests: `fmt(fmt(x)) == fmt(x)`, `parse(fmt(x)) == parse(x)`, `lower(parse(fmt(x))) == lower(parse(x))`.

### 11.4 Grammar and schema export

- `mote grammar --gbnf` emits a GBNF grammar for the language for llama-server.
- `mote grammar --lark` emits a Lark grammar for OpenAI custom tools and llguidance.
- `mote schema --json` emits `schemas/tree.v0.json` and a depth-8 unrolled variant for Anthropic strict mode.
- All three are generated from the catalog definition, never hand-edited.

### 11.5 Test fixtures

```text
fixtures/
  card.mote              card.tree.json
  list.mote              list.tree.json
  list-bound.mote        list-bound.tree.json
  table-select.mote      table-select.tree.json
  form.mote              form.tree.json
  chart.mote             chart.tree.json
  progress.mote          progress.tree.json
  dialog.mote            dialog.tree.json
  chat.mote              chat.tree.json
  home.mote              home.tree.json
  page.mote              page.tree.json
  errors/
    unknown-word.mote    unknown-word.diag.json
    scope.mote           scope.diag.json
    limits.mote          limits.diag.json
  streams/
    card.stream          card.frames.json
    table.stream         table.frames.json
```

- Each `.mote` lowers to its `.tree.json` byte for byte after canonical JSON serialization.
- Each `.diag.json` lists the expected diagnostics with codes and spans.
- Each `.stream` is the surface cut at byte offsets. `frames.json` lists the expected partial tree and `open` flags at each cut.
- Golden screenshots per target live beside the fixtures under `golden/<target>/<width>/<theme>/`.

### 11.6 Milestones

1. Tree schema, JSON partial parser, resolver, checker, lowering, web renderer. v0 ships here.
2. Language lexer, parser, formatter, decompiler, fixtures. Daemon accepts both formats.
3. Grammar export. Constrained decoding on llama-server. Validity measurement (UI-24).
4. Slint renderer on LinuxKMS. Golden parity.

## 12. Open decisions

Each has a recommended default. The owner can flip any of them.

1. Language name and file ending. Default: Mote UI, `.mote`. Alternative: `.ui`.
2. Braces or indentation. Default: braces, 4% more tokens, explicit close for partial parse, consistent with Next Gen Lang. Alternative: indentation, with a whitespace-sensitive grammar that GBNF handles less well.
3. Tool call or fenced block for framing on hosted models. Default: tool call named `surface` when the backend has strict tools, fenced block otherwise.
4. Model receives the surface in the language or in JSON on the next turn. Default: the language, via the decompiler, even when the model emitted JSON. Reason: the language is 34% cheaper in context and more readable.
5. Whole-surface replacement or model-emitted patches between turns. Default: whole surface. The daemon diffs. Patches are a v2 question once surfaces exceed 2,000 nodes in practice.
6. `md` subset. Default: emphasis, lists, inline code, links. No headings, no images, no tables, no HTML.
7. Slint license for the Pi. Default: GPLv3 if mote is open source, else the commercial license. The Royalty-free license cannot be used on the Pi. Fallback: egui with a custom DRM presenter under MIT.
8. Slint runtime compile per surface or a generic model-driven component tree. Default: compile per surface through `slint-interpreter`, measured by UI-47.
9. Hosted model strict-mode depth. Default: unroll to depth 8 for Anthropic. Reject deeper.
10. Built-in action set. Default: `Open`, `Back`, `Dismiss`, `Copy`, `Listen`, `Speak`, `Retry`, `Cancel`. Anything else is a model action.
11. Whether the daemon renders the model's prose when the surface fails twice. Default: yes, as a `message`.
12. Reconciliation key. Default: `id` when present, else path plus type. Alternative: label-based matching, rejected for ambiguity.
13. Charts in v0. Default: `bar`, `line`, `pie`, `sparkline` only. Scatter, area, stacked are v2.
14. Sort and filter affordances on tables. Default: sort by header on sortable column types. Filter is v2.
15. Interaction state persistence across daemon restart. Default: scroll and selection restore, focus and typed text do not.
16. Minimum touch target on `page` surfaces. Default: 32 px. `screen` surfaces: 48 px.
17. Constrained decoding on the local model in v0. Default: on, JSON Schema through Ollama `format`. The language's grammar path waits for llama-server.
18. Whether a `surface` program kind joins Next Gen Lang later. Default: keep the sibling language. Revisit when Next Gen Lang specifies a UI layer.

## 13. Acceptance criteria

Each criterion is one sentence plus a Verify clause. `[tree]` binds the v0 tree. `[lang]` binds the language. `[both]` binds both.

### Wire format and schema

- [ ] **UI-01 [tree] Compact nested JSON is the only wire form.** The daemon accepts a surface as compact JSON with `type` first and `children` last and rejects any other top-level shape. **Verify:** send the eleven fixtures as compact JSON and confirm each renders. Send a flat id map and a Hiccup array and confirm `E001` with the top-level path.
- [ ] **UI-02 [tree] The tree schema is generated, not hand-written.** `mote schema --json` emits `schemas/tree.v0.json` and a depth-8 unrolled variant from the catalog definition. **Verify:** run the command twice and diff. Validate all eleven fixture trees against the schema with a JSON Schema 2020-12 validator. Confirm the unrolled variant loads in Anthropic strict mode and the recursive variant in OpenAI strict mode.
- [ ] **UI-03 [tree] Short keys are never used.** Every key in the schema is a full camelCase word. **Verify:** grep the schema for keys shorter than three characters and expect only `of` and `id`.
- [ ] **UI-04 [tree] Token cost is measured in CI.** A test counts `o200k_base` tokens for the three measurement fixtures in compact JSON and fails if any exceeds the RFC's numbers by more than 10% (card 120, table 246, form 146). **Verify:** run the token test. Change a fixture key to a longer word and confirm the test fails.
- [ ] **UI-05 [tree] Limits are enforced.** Depth over 12, more than 2,000 nodes, more than 5,000 rows, a string over 20,000 chars, or a surface over 256 KB produces `E020` and a truncated render. **Verify:** send one fixture per limit and inspect the diagnostic and the rendered prefix.

### Language

- [ ] **UI-06 [lang] Every section 6 example parses, lowers, and matches its tree.** Each `.mote` fixture lowers to its `.tree.json` byte for byte after canonical serialization. **Verify:** run the fixture test. Confirm eleven passes.
- [ ] **UI-07 [lang] The decompiler round-trips.** For each fixture, `decompile(lower(parse(x)))` formats to the same text as `fmt(x)`. **Verify:** run the round-trip property test on the fixtures and on 1,000 generated surfaces.
- [ ] **UI-08 [lang] The formatter is idempotent and semantics-preserving.** `fmt(fmt(x)) == fmt(x)` and `lower(parse(fmt(x))) == lower(parse(x))` for every fixture and 1,000 generated surfaces. **Verify:** run the property tests.
- [ ] **UI-09 [lang] Casing is compiler-invariant.** `checkIn`, `CheckIn`, and `check_in` resolve to the same action and format to the declared spelling. **Verify:** parse a surface with the three spellings and confirm one action id in the tree and `CheckIn` in the formatted text.
- [ ] **UI-10 [lang] Unknown words and modifiers fail closed.** An unknown element word yields `E001` and the node is absent from the tree. An unknown modifier yields `E002`. **Verify:** run `fixtures/errors/unknown-word.mote` and compare to its `.diag.json`.
- [ ] **UI-11 [lang] Scope rules are checked.** `selected` outside a `select` node and `.field` outside a bound list yield `E011`. **Verify:** run `fixtures/errors/scope.mote` and compare diagnostics.
- [ ] **UI-12 [lang] Action contracts are derived from declarations.** For the form fixture the compiler reports the contract `{title: str, when: DateTime, repeat: none|daily|weekly, note: str?}` for `CreateReminder`. **Verify:** run `mote contracts fixtures/form.mote` and compare the JSON output.
- [ ] **UI-13 [lang] Strings never carry markup.** `text "<b>hi</b>"` renders the literal characters on both targets and yields `W001`. **Verify:** render the string and inspect the DOM text node and the Slint text property.
- [ ] **UI-14 [lang] Grammars are exported and accepted.** `mote grammar --gbnf` loads in llama-server and `mote grammar --lark` loads as an OpenAI custom tool grammar under 20,000 chars. **Verify:** start llama-server with the grammar and generate the card fixture. Submit the Lark grammar in a custom tool definition and confirm no API error.

### Affordances

- [ ] **UI-15 [both] Scrolling is implicit.** A list with 200 items and a table with 500 rows scroll inside their box without any modifier, and the page scrolls when its content exceeds the viewport. **Verify:** render on both targets at 800 x 480 and 1280 x 800 and confirm scroll regions by screenshot and by driving the scroll position.
- [ ] **UI-16 [both] Focus and keyboard work without declarations.** Tab moves through interactive nodes in tree order, Enter activates the focused button, Escape closes a dialog, and arrows move within a table. **Verify:** drive the keys on the dialog and table fixtures on the web target and on the Slint target with a USB keyboard and record the focused node after each key.
- [ ] **UI-17 [both] Touch targets meet the minimum.** Every interactive node on a `screen` surface measures at least 48 x 48 logical px and on a `page` surface at least 32 x 32. **Verify:** the Taffy headless layout test asserts the minimum for the home and page fixtures. A screenshot on the Pi display confirms.
- [ ] **UI-18 [both] Selection is implicit from `select`.** `select one` renders radio semantics, `select many` renders checkboxes with shift-range, and `selected.invoice` resolves to the chosen rows in the event. **Verify:** select rows in the table fixture and inspect the event payload.
- [ ] **UI-19 [both] Loading, empty, and error states appear without declarations.** A node bound with `from` shows a skeleton before data, "Nothing here" on zero rows, and an error with Retry when the data symbol fails. **Verify:** drive the bound list fixture through the three states in the daemon and screenshot each.
- [ ] **UI-20 [both] Theme follows the system.** Switching macOS appearance re-renders the current surface in the other theme with no model turn. **Verify:** toggle appearance and screenshot. Confirm no daemon-to-model traffic in the log.
- [ ] **UI-21 [both] Responsive layout is implicit.** `row` wraps and `grid 3` drops to one column below the threshold, and a table collapses to stacked cards. **Verify:** render the page fixture at 400 px and 1200 px widths on both targets and compare to golden screenshots.
- [ ] **UI-22 [both] Every node has an accessibility role and name.** The role tree for each fixture matches a golden role list on the web (ARIA) and on Slint or egui (AccessKit). **Verify:** dump the role tree per target and diff against `golden/roles/<fixture>.json`.
- [ ] **UI-23 [both] Style words are the only styling.** The tree and the language reject pixel sizes, colors, fonts, and margins. **Verify:** send `{"type":"text","color":"red"}` and `text "hi" color red` and confirm `E002` or `E003`.

### State, store, routes

- [ ] **UI-24 [both] Validity rate is measured per model and format.** A harness runs 200 prompts across the eleven fixture kinds against the fast local model and the thinking model in JSON and in the language and reports first-try validity and after-retry validity. **Verify:** run the harness. The language path is enabled for a model only when its first-try validity is at least 95% or constrained decoding is active.
- [ ] **UI-25 [both] Interaction state survives re-render.** Typed text in a form field, scroll offset in a list, and selection in a table survive a new surface whose matching nodes keep the same path or `id`. **Verify:** type, scroll, select, then push a surface with one added node above and confirm the three states.
- [ ] **UI-26 [both] No declared store exists.** The grammar has no `state` block and the schema has no store object. **Verify:** grep the grammar and schema. Send a surface with a `state` key and confirm `E001`.
- [ ] **UI-27 [both] No routes exist.** `Back` pops the daemon's surface history and a `Show(view: ...)` action reaches the model as an ordinary event. **Verify:** emit three surfaces, send `Back` twice, confirm the first surface renders with its selection restored, and confirm `Show` appears in the model's inbound event log.
- [ ] **UI-28 [both] Built-in actions never reach the model.** `Open`, `Back`, `Dismiss`, `Copy`, `Listen`, `Speak`, `Retry`, and `Cancel` are handled in the daemon. **Verify:** trigger each from a fixture and confirm no model turn in the log.
- [ ] **UI-29 [both] Events are validated against contracts.** An event with a missing required field, a wrong type, a stale surface number, or an unknown node path is rejected with a diagnostic and never forwarded. **Verify:** send one bad event per case from a test client and inspect the rejection and the model log.
- [ ] **UI-30 [both] Data-bound nodes update without a model turn.** A `progress of job.x` node advances as the daemon emits job events, and a `list from mail.unread` node refreshes when the tool result changes. **Verify:** drive the job and the tool result in the daemon and confirm renders with no model traffic.

### Streaming and validation

- [ ] **UI-31 [tree] Partial JSON renders progressively.** Cutting the card fixture at every byte offset yields a monotone sequence of partial trees where each complete node renders and open interactive nodes render as skeletons. **Verify:** run `fixtures/streams/card.stream` against `card.frames.json`.
- [ ] **UI-32 [lang] Partial language renders per line.** Cutting the table fixture at every newline yields complete rows rendered and the open table marked `open`. **Verify:** run `fixtures/streams/table.stream` against `table.frames.json`.
- [ ] **UI-33 [both] Interactive nodes never fire while open.** A tap on a skeleton button and an Enter on an open form produce no event. **Verify:** stream the form fixture halfway, drive input, confirm no event in the daemon log.
- [ ] **UI-34 [both] Layout is stable during streaming.** No node moves more than one row when a sibling completes. **Verify:** record bounding boxes per frame on the web target for the page fixture stream and assert the bound.
- [ ] **UI-35 [both] Repair is bounded and visible.** End-of-stream repair closes open containers only, and every repair produces a diagnostic. **Verify:** truncate each fixture at three offsets, confirm the rendered prefix, and confirm one `E030` per truncation.
- [ ] **UI-36 [both] Retry is one turn with diagnostics.** A surface with an `E` diagnostic triggers one retry message containing the diagnostics and the canonical valid prefix, and a second failure renders the prefix plus prose. **Verify:** inject a failing surface twice and inspect the model messages and the final render.
- [ ] **UI-37 [both] Constrained decoding is wired where supported.** With Ollama the daemon passes the tree schema as `format`. With llama-server the daemon passes the exported grammar. With OpenAI and Anthropic the daemon uses strict tools. **Verify:** inspect outbound requests for each backend in a capture.
- [ ] **UI-38 [both] The UI block is last and unconstrained reasoning precedes it.** The daemon's prompt and tool configuration place the surface after prose, and constrained decoding applies only to the block. **Verify:** inspect the prompt template and a captured request. Confirm the reasoning text is unconstrained in the response.

### Rendering targets

- [ ] **UI-39 [tree] The Tauri renderer has no model-reachable HTML, CSS, or JS.** The webview's CSP is `default-src 'none'` with hashed scripts, and no code path inserts a model string as markup. **Verify:** inspect `tauri.conf.json`. Grep the renderer for `innerHTML`, `insertAdjacentHTML`, and `eval` and expect none.
- [ ] **UI-40 [tree] Streaming reaches the webview over a Channel.** Partial trees arrive through `tauri::ipc::Channel` and render within 50 ms of arrival on an M-series Mac. **Verify:** timestamp channel sends and DOM commits for the page fixture stream.
- [ ] **UI-41 [lang] Slint renders every fixture on the Pi over DRM.** On Raspberry Pi OS Lite with no X11 or Wayland, `SLINT_BACKEND=linuxkms-software` renders the eleven fixtures on the official 7-inch display with touch from libinput. **Verify:** photograph or capture the DRM framebuffer per fixture and drive a tap on each interactive node.
- [ ] **UI-42 [lang] Golden parity across targets.** Each fixture's screenshot on the web and on Slint matches the golden within a structural diff tolerance (same role tree, same reading order, same wrap points). **Verify:** run the golden comparison at 800 x 480 and 1280 x 800 in light and dark.
- [ ] **UI-43 [both] Headless layout invariants hold.** The Taffy reference layout for every fixture has no overlapping boxes, no overflow outside a scrolling container, and no interactive box below the minimum size. **Verify:** run the layout test suite.
- [ ] **UI-44 [lang] Fallback renderer is proven.** egui over a DRM presenter renders the card and home fixtures on the Pi with AccessKit roles present. **Verify:** capture the framebuffer and dump the AccessKit tree.

### Compiler

- [ ] **UI-45 [both] Diagnostics are stable and machine-readable.** Every diagnostic has a stable code, a span, a tree path, and a hint, and the code list is documented. **Verify:** run the error fixtures and validate output against `schemas/diagnostic.json`.
- [ ] **UI-46 [lang] Compile time is bounded.** Parsing, checking, and lowering the page fixture takes under 2 ms on an M-series Mac and under 10 ms on a Pi 5. **Verify:** run the benchmark on both machines and record p95.
- [ ] **UI-47 [lang] Slint compile per surface is bounded or replaced.** Loading the page fixture through `slint-interpreter` takes under 50 ms on a Pi 5, or the generic model-driven tree is adopted (open decision 8). **Verify:** run the benchmark. Record the decision in this RFC.
- [ ] **UI-48 [both] Fixtures are the contract.** Any change to the catalog updates a fixture, its tree, and its golden in the same commit, enforced by a CI check that fails when the catalog hash changes without fixture changes. **Verify:** change a catalog word and confirm CI fails until fixtures update.

## 14. Sources

Local:

- `/Users/bilal/projects/nextgenlang/CONTRIBUTING.md`
- `/Users/bilal/projects/nextgenlang/knowledge/README.md`
- `/Users/bilal/projects/nextgenlang/knowledge/language-principles/01-structural-inference.md` through `11-short-typesafe-surfaces.md`
- `/Users/bilal/projects/nextgenlang/knowledge/language-design/00-vision-and-doctrine.md`
- `/Users/bilal/projects/nextgenlang/knowledge/language-design/02-syntax-reference.md`
- `/Users/bilal/projects/nextgenlang/knowledge/language-design/program-kinds/*.md`
- `/Users/bilal/projects/nextgenlang/knowledge/language-design/syntax/attributes.md`, `syntax/formatting-rules.md`, `data-ifc/serialization-and-boundaries.md`, `api/streaming-websocket-sse.md`, `types/effects.md`
- `/Users/bilal/projects/overseer/docs/overseer-rfc.md` (acceptance criteria shape)
- `docs/research/ui-language-research.md` (this repo, all measured numbers and every URL below with the facts taken)

Generative UI protocols:

- https://a2ui.org/ , https://a2ui.org/roadmap/ , https://a2ui.org/concepts/data-binding/ , https://a2ui.org/concepts/actions/ , https://github.com/google/A2UI
- https://github.com/vercel-labs/json-render , https://json-render.dev/docs/streaming , https://json-render.dev/docs/catalog
- https://ai-sdk.dev/docs/ai-sdk-ui/generative-user-interfaces , https://ai-sdk.dev/docs/ai-sdk-rsc/migrating-to-ui
- https://developers.openai.com/apps-sdk/reference , https://developers.openai.com/apps-sdk/mcp-apps-in-chatgpt
- https://modelcontextprotocol.io/seps/1865-mcp-apps-interactive-user-interfaces-for-mcp , https://github.com/modelcontextprotocol/ext-apps , https://github.com/idosal/mcp-ui
- https://learn.microsoft.com/en-us/adaptive-cards/authoring-cards/universal-action-model , https://learn.microsoft.com/en-us/adaptive-cards/templating/language , https://github.com/microsoft/AdaptiveCards
- https://github.com/thesysdev/openui , https://www.openui.com/docs
- https://github.com/flutter/genui , https://docs.flutter.dev/ai/genui
- https://github.com/Shopify/remote-dom
- https://github.com/ag-ui-protocol/ag-ui , https://github.com/CopilotKit/generative-ui
- https://github.com/MobileReality/mdma/pull/62 , https://verygood.ventures/blog/benchmarking-llms-for-gen-ui/ , https://arxiv.org/abs/2605.24830 , https://arxiv.org/pdf/2604.25359 , https://arxiv.org/abs/2608.29387
- https://arxiv.org/abs/2604.09577 , https://arxiv.org/abs/2508.19227

Declarative UI languages:

- https://docs.slint.dev/latest/docs/slint/guide/language/concepts/reactivity/ , https://docs.slint.dev/latest/docs/slint/guide/language/coding/positioning-and-layouts/ , https://docs.slint.dev/latest/docs/rust/slint_interpreter/ , https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backend_linuxkms/ , https://github.com/slint-ui/slint/blob/master/LICENSE.md
- https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html , https://doc.qt.io/qt-6/qtqml-javascript-topic.html
- https://developer.apple.com/documentation/swiftui/state , https://developer.apple.com/documentation/swiftui/list
- https://developer.android.com/develop/ui/compose/architecture , https://developer.android.com/develop/ui/compose/modifiers
- https://docs.flutter.dev/ui , https://docs.flutter.dev/ui/accessibility-and-internationalization/accessibility
- https://guide.elm-lang.org/architecture/ , https://github.com/mdgriffith/elm-ui
- https://htmx.org/docs/ , https://htmx.org/essays/hypermedia-driven-applications/ , https://hyperscript.org/docs/events , https://github.com/starfederation/datastar
- https://typst.app/docs/reference/styling/ , https://typst.app/docs/reference/layout/
- https://github.com/emilk/egui , https://raw.githubusercontent.com/emilk/egui/main/docs/accessibility.md , https://docs.rs/egui_kittest
- https://ratatui.rs/concepts/layout/

Rust stacks and layout:

- https://github.com/DioxusLabs/taffy , https://docs.rs/taffy/latest/taffy/ , https://github.com/DioxusLabs/taffy/releases/tag/v0.14.0
- https://github.com/facebook/yoga , https://docs.rs/crate/yoga/latest
- https://github.com/vizia/morphorm , https://github.com/nicbarker/clay
- https://github.com/DioxusLabs/blitz , https://dioxuslabs.com/blog/release-070
- https://github.com/iced-rs/iced , https://github.com/iced-rs/iced/issues/552
- https://github.com/makepad/makepad
- https://www.gpui.rs/ , https://zed.dev/docs/linux.md
- https://v2.tauri.app/concept/architecture/ , https://v2.tauri.app/develop/calling-rust/ , https://v2.tauri.app/security/csp/ , https://github.com/tauri-apps/wry
- https://github.com/AccessKit/accesskit , https://accesskit.dev/
- https://github.com/plotters-rs/plotters , https://crates.io/api/v1/crates/egui_plot , https://docs.slint.dev/latest/docs/slint/reference/elements/path/
- https://www.raspberrypi.com/documentation/accessories/display.html , https://www.raspberrypi.com/documentation/accessories/touch-display-2.html , https://github.com/cage-kiosk/cage

Wire formats and constrained decoding:

- https://toonformat.dev/ , https://raw.githubusercontent.com/toon-format/spec/main/SPEC.md , https://www.improvingagents.com/blog/toon-benchmarks , https://arxiv.org/html/2601.12014v1 , https://arxiv.org/html/2603.03306v1 , https://arxiv.org/html/2605.29676v1
- https://www.improvingagents.com/blog/best-input-data-format-for-llms , https://www.improvingagents.com/blog/best-nested-data-format/ , https://arxiv.org/html/2310.10358v1 , https://blog.matt-rickard.com/p/a-token-efficient-language-for-llms
- https://arxiv.org/html/2408.02442v1 , https://blog.dottxt.ai/say-what-you-mean.html , https://dylancastillo.co/posts/say-what-you-mean-sometimes.html , https://arxiv.org/html/2501.10868v2 , https://arxiv.org/abs/2606.09410
- https://github.com/dottxt-ai/outlines , https://github.com/guidance-ai/llguidance , https://github.com/ggml-org/llama.cpp/blob/master/docs/llguidance.md , https://arxiv.org/html/2411.15100v3 , https://github.com/noamgat/lm-format-enforcer , https://docs.vllm.ai/en/stable/features/structured_outputs/ , https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md
- https://ollama.com/blog/structured-outputs , https://github.com/ollama/ollama/pull/2404 , https://contracollective.com/blog/structured-outputs-apple-silicon-outlines-llama-cpp-grammars-mlx-2026 , https://lmstudio.ai/blog/lmstudio-v0.3.4
- https://developer.apple.com/videos/play/wwdc2025/286 , https://arxiv.org/pdf/2507.13575
- https://openai.com/index/introducing-structured-outputs-in-the-api/ , https://developers.openai.com/api/docs/guides/structured-outputs , https://developers.openai.com/api/docs/guides/function-calling
- https://platform.claude.com/docs/en/build-with-claude/structured-outputs.md , https://platform.claude.com/docs/en/build-with-claude/streaming , https://platform.claude.com/docs/en/agents-and-tools/tool-use/fine-grained-tool-streaming.md
- https://github.com/vercel/ai/blob/main/packages/ai/src/util/fix-json.ts , https://simonwillison.net/2024/Sep/22/jiter/ , https://github.com/promplate/partial-json-parser , https://github.com/mangiucugna/json_repair
- https://hackernoon.com/we-measured-the-llm-token-cost-of-4-markup-formats-tsx-costs-94percent-more-than-pug , https://arxiv.org/abs/2508.13666 , https://blog.cloudflare.com/markdown-for-agents/ , https://www.skyvern.com/blog/html-vs-json-llm-tokens-cost-reduction-success-rate/ , https://en.wikipedia.org/wiki/JsonML
