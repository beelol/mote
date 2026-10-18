# mote

An always-on personal agent you own. A Rust daemon (`moted`) answers instantly from a local
model, routes hard turns to a thinking model, hands real work to Overseer, draws its own
interface, and keeps its context in a store and a ledger you can inspect. It runs in a window on
the Mac now, on a Raspberry Pi touchscreen later, and on any machine you pick as the cloud. It is open
source, with room for a service around it.

Working name. The owner may rename it.

## Read first

- [Key concepts](docs/concepts.md): the fourteen edges and which reuse document covers each.
- [mote RFC](docs/rfcs/mote.md): decisions, lanes, journeys, security, criteria MOTE-NN.
- [Presentation](docs/presentation/key-concepts.html): the sliding deck of the fourteen concepts.
- [Renderer handoff](docs/handoff/generative-ui-renderer.md): notes for the agent that designs the pixel pipeline.
- [Shared crates RFC](docs/rfcs/shared-crates.md): what moves out of Overseer into `crates/`, and how Overseer migrates later. Criteria CRATE-NN.
- [UI language RFC](docs/rfcs/ui-language.md): research and the proposed generative UI language. Criteria UI-NN.
- [Matrix walkthrough](matrix/README.md#one-working-program-from-source-to-pixels): the actual dashboard source, its compiled tree and the renderer path.
- [Context layer RFC](docs/rfcs/context-layer.md): connectors, index, retrieval, coverage map, realms, and the storage decision. Criteria CTXL-NN.
- [AGENTS.md](AGENTS.md): rules for every agent.

## Context for a new reader

Nothing here assumes you were in the conversation that started it.

- **Dots** are OpenAI's always-on ChatGPT agents, launched on 2026-09-29. Each one has its own cloud computer, watches Slack and GitHub, and acts in the background. mote is the owner's own version of that idea.
- **Overseer** is the owner's existing Rust daemon at `~/projects/overseer` (its own repository). It runs coding agents (Claude Code, Codex, OpenCode) on the Mac under subscription logins and owns their state. mote does not run coding work itself. It hands that work to Overseer and reuses Overseer's code as shared crates.
- **Next Gen Lang** is the owner's language specification at `~/projects/nextgenlang/knowledge`. The UI language in this repo follows its principles.
- **Tools named in the criteria.** Ollama runs local models on the Mac. `claude -p` is the Claude Code command line, which gives the thinking model through the owner's subscription, with no API key. Tauri builds the Mac window. launchd starts the daemon at login.
- **Parts.** mote-cloud is the engine and holds all state (the Mac today, a server later). mote-client is the chat app and lends the Mac when it runs there. mote-ui is the stateless renderer, alone on a Pi or inside the client. mote-talk is voice.
- **IDs.** MOTE-NN, CRATE-NN, UI-NN and DEF-NN are acceptance criteria. Their full text and Verify clauses live in the linked RFCs. The checkboxes below are the only completion record.

## Status

Planning finished on 2026-10-08. The headless daemon, native client, compact v0 renderer and first
Matrix compiler-to-host slice are implemented on `main`. Toolchain on the owner's Mac: cargo 1.89,
Claude Code 2.1.296, Ollama, 128 GB RAM.

Every product default for the first build is fixed in [docs/rfcs/v0.md](docs/rfcs/v0.md). A driver does not need the owner for any item in the v0 list below. The owner has also approved the native iPad host feature and the browser subset named in its design. Flipping `models.local_only`, a Slack bot token, the Overseer work lane, the Pi, voice and the cloud relay remain deferred in [docs/backlog.md](docs/backlog.md).

## Try the interface

Open the Storybook-like native component gallery. It is one scrollable immediate-mode screen with
every currently rendered control, including the Matrix slider, toggle, select, anchored layout and
action. It does not start the daemon or call a model.

```text
cargo run -p mote-client -- --gallery
```

The checked-in dashboard is a short Matrix program in
[`matrix/examples/mote-dashboard.matrix`](matrix/examples/mote-dashboard.matrix). Start the daemon
in one terminal, then compile and show that dashboard from another terminal.

```text
cargo run -p moted
```

```text
cargo run -p mote-client -- --matrix matrix/examples/mote-dashboard.matrix
```

The dashboard demonstrates an anchored header and footer, a stretching body, a toggle, three
selects and a Refresh action. Configuration changes go through the daemon and apply on the next
turn.

Once `moted install` has registered the persistent Mac host, these commands control only Mote's
`com.mote.moted` LaunchAgent. Stop preserves the installed service, configuration and data.

```text
moted host start
moted host status
moted host restart
moted host stop
```

## Ready for Go

### Continue development

Copy this goal into a driver. Its current scope is the distinct device build, internal TestFlight phase, manual Mote-only CI release path and agent-layout transition verification. The separately activated memory goal and its criteria are below under Memory backend comparison.

```text
Continue mote from repository state at ~/projects/mote without relying on chat history.

Read AGENTS.md, docs/rfcs/v0.md, docs/superpowers/specs/2026-10-11-ipad-host.md, root README.md -> Ready for Go, and docs/HANDOFF.md if present. IPAD-06, IPAD-11 and IPAD-13 are verified. Active device-delivery ACs are IPAD-08 and IPAD-12. HOST-05 under Native iPad Simulator host and V0-53 under the v0 build list also remain open. Do not mark them passed or remove their Verify clauses without completing their literal checks. Recheck affected physical behavior after distribution changes. Backlog, external testing, App Review, public release and cloud relay remain out of scope.

Complete the Mote-only manual CI upload path and repeat the named IPAD-08 journeys against the installed internal TestFlight build on the connected physical iPad. The final outcome remains the same Rust immediate-mode Matrix UI, paired host gateway, inferred surfaces and browser approval on that iPad. It must not collide with any existing app. TestFlight does not make the Mac reachable from another city. Do not claim remote use without a separately verified route. Keep models.local_only true, use only the approved Ollama models, ledger every model call and redact stored events.

Choose the smallest actionable AC or coherent dependency set. Inspect existing code and Apple account state first. If delegation is available and authorized, send bounded work to the least costly capable subagent with AC IDs, owned files, dependencies and evidence requirements. Inspect every result. Otherwise work directly. Add an AC for any discovered in-scope gap and reopen its parent. Never remove or weaken an unmet requirement.

Build device-capable Rust/Xcode targets without breaking Simulator behavior. Create a guarded dry-run and archive/export script before any Apple mutation. It must validate the selected bundle ID, team, version, build number and output paths, fail closed on collisions, keep signing material out of Git and logs, and require a separate explicit upload step. Do not guess the owner's Apple team or identifiers. A new App Store Connect record requires the owner's confirmed identity and suitable access. Do not modify another app. Record creation, signed export and upload are separate observable outcomes.

Verify full AC clauses with relevant tests and real UI or device journeys, including refusal and recovery. Run cargo test --workspace and cargo clippy --workspace -- -D warnings before every commit. Record commands or manual steps, output, environment, date and commit in docs/verification/<ID>.md. Simulator evidence cannot close a physical-device AC. Archive or upload alone cannot close installed TestFlight verification. Leave any owner-only step open while independent work continues.

Deliver small verified Conventional Commits to main and push to the confirmed configured origin. Include evidence and AC checkmarks in the verified delivery. Check a parent only when every child is verified. After every integration, reread the active board and recheck affected completed ACs. Stop only for completion, an owner stop, a budget limit or when all remaining work needs unavailable input, access or authorization. State exact blockers. No external TestFlight group, App Review, public release, cloud spending or relay is authorized by this goal.

Replace docs/HANDOFF.md with current ACs, branch, delegated tasks, uncommitted work, results, limits, blockers and exact next action. Checkpoint before delegation or long operations and after each edit batch, subtask, test run, commit, push or blocker, at most two minutes apart during active work. Record in-flight handles before blocking calls and checkpoint on return. Save before stopping. On resume reconcile files and commits against the handoff. Delete it only when nothing needs transfer.
```

### Phase 2 durable personal agent

The owner activated Phase 2 on 2026-10-17. The active implementation criteria are P2-02 through
P2-12, P2V-01 through P2V-10 and MBE-01 through MBE-08. P2-13 remains a separate later decision.
The v0 behavior, SQLite memory default, local-model allowlist and iPad approval boundary remain
authoritative. Physical iPad voice testing and external Apple work are deferred.

[Concise Phase 2 plan and driver goal](docs/phase2/README.md) ·
[technical runtime plan](docs/superpowers/plans/2026-10-18-phase2-runtime.md) ·
[technical voice plan](docs/superpowers/plans/2026-10-18-shared-voice.md) ·
[technical memory plan](docs/superpowers/plans/2026-10-18-memory-backends.md)

- [ ] **[P2 durable personal agent](docs/phase2/README.md)**
  - [x] **P2-00** The owner chose the native Rust engine direction, one active task plus a queue,
    selectable OpenCode and Mote harnesses and the crate name `mote-hrns`.
  - [x] **P2-01** The owner activated Phase 2 and named the active child criteria in Ready for Go.
    [evidence](docs/verification/P2-01.md)
  - [x] **P2-02** Common task, action, checkpoint, model-step and harness contracts exist without
    adapter-specific state. [evidence](docs/verification/P2-02.md)
  - [x] **P2-03** One active task, the FIFO queue and atomic redacted transition events survive
    failure and restart. [evidence](docs/verification/P2-03.md)
  - [ ] **P2-04** Startup and live harness selection apply only at a safe checkpoint and never
    fall back silently.
  - [ ] **P2-05** OpenCode runs behind `mote-hrns` with built-in tools disabled and Mote-owned
    continuity.
  - [ ] **P2-06** The native adapter performs ledgered structured steps through approved local
    models and fails closed.
  - [ ] **P2-07** Stable action IDs, approvals and commit receipts prevent duplicate effects.
  - [ ] **P2-08** Bounded context preserves the charter, pinned instructions and newest owner
    turn while progressively loading skills.
  - [ ] **P2-09** Matrix exposes task, queue, step, approval, harness, checkpoint, retry, cancel
    and final sourced result state.
  - [ ] **P2-10** Both harnesses complete the same bounded non-code personal-task journeys through
    Mote-owned tools and policy.
  - [ ] **P2-11** Coding work delegates to Overseer without moving non-code task authority out of
    Mote.
  - [ ] **P2-12** The common benchmark records safety, recovery, quality and latency before any
    default change.
  - [ ] **P2-13** OpenCode removal waits for a separate owner decision after a stable
    native-default release. This criterion is not active.

- [ ] **[P2 shared voice foundations](docs/superpowers/specs/2026-10-17-shared-voice.md)**
  - [ ] **P2V-01** The voice protocol is product-neutral and golden-compatible with the pinned
    Overseer source without carrying production PCM.
  - [ ] **P2V-02** PCM, gate, endpoint and word logic run deterministically without OS, model,
    daemon or network dependencies.
  - [ ] **P2V-03** The optional macOS crate provides local capture, call detection, Whisper and
    in-memory speech.
  - [ ] **P2V-04** The listener enforces audio bounds and opens no outbound socket.
  - [ ] **P2V-05** `moted` supervises voice behind an off-by-default setting without breaking
    typed interaction.
  - [ ] **P2V-06** Final utterances enter `mote-hrns` once while partial events never invoke a
    harness.
  - [ ] **P2V-07** Speech interruption preserves played context and outstanding approvals.
  - [ ] **P2V-08** Matrix shows every voice state on Mac and through the iPad host protocol.
  - [ ] **P2V-09** Derived crates record source, revision, licenses and redacted fixtures.
  - [ ] **P2V-10** A clean Overseer worktree consumes the pinned shared crates without boundary
    regressions.

### Later

Every other criterion written during planning remains in [docs/backlog.md](docs/backlog.md). The native iPad ACs incorporate their approved Matrix and browser behavior directly and do not activate additional backlog IDs. The older open Matrix parent remains a historical completion record and does not add Raspberry Pi work to the current driver. The owner must explicitly activate anything else.

Phase 2 work outside the active criteria remains in [docs/phase2/README.md](docs/phase2/README.md).
OpenCode removal, Pi voice, iPad microphone capture, cloud speech and a memory default switch need
later owner decisions.

### v0 build list

Work top to bottom. Each item stands alone: what, how to verify, and the section of [docs/rfcs/v0.md](docs/rfcs/v0.md) that defines it. Nothing here needs an owner decision.

- [x] **UI-00** The owner confirmed the four Matrix defaults and activated UI-01 through UI-48.
  Existing v0 behavior remains authoritative until each versioned replacement is verified.
  Hardware-only evidence may remain open while independent implementation continues.
  [decisions](matrix/decisions.md) [evidence](docs/verification/UI-00.md)
- [x] **Matrix toolchain** Implement UI-01 through UI-12, UI-32, UI-35 and UI-45 using the
  activated plan. **Verify:** complete every criterion's full Verify clause in `docs/backlog.md` and
  record separate evidence files. [spec](docs/superpowers/specs/2026-10-10-matrix-toolchain.md)
  [plan](docs/superpowers/plans/2026-10-10-matrix-toolchain.md)
  Verified: [UI-01](docs/verification/UI-01.md), [UI-02](docs/verification/UI-02.md),
  [UI-03](docs/verification/UI-03.md), [UI-04](docs/verification/UI-04.md),
  [UI-05](docs/verification/UI-05.md), [UI-06](docs/verification/UI-06.md),
  [UI-07](docs/verification/UI-07.md), [UI-08](docs/verification/UI-08.md),
  [UI-09](docs/verification/UI-09.md), [UI-10](docs/verification/UI-10.md),
  [UI-11](docs/verification/UI-11.md),
  [UI-12](docs/verification/UI-12.md), [UI-32](docs/verification/UI-32.md),
  [UI-35](docs/verification/UI-35.md) and [UI-45](docs/verification/UI-45.md).
  The checked-in dashboard activates the first versioned consumer slice with anchored header,
  scrolling body, footer and direct-child fill. Its dual payload and contract 1 control paths are
  verified in [V0-45](docs/verification/V0-45.md).
- [ ] **Matrix host and runtime** Implement UI-13 through UI-31, UI-33 through UI-44 and UI-46
  through UI-48 after the toolchain, including a versioned full-catalog consumer contract.
  **Verify:** complete every criterion's full Verify clause in `docs/backlog.md`; do not let unavailable
  hardware block independent code and headless evidence. [catalog](matrix/catalog.md)
  [runtime](matrix/runtime.md)
  [Consumer contract 1](matrix/consumer-contract-v1.md) is verified for the first frozen slice.
  The later catalog, native Mac accessibility capture, Pi and iPad checks remain open.
  Complete validated streaming, inert candidates and stable committed layout are verified in
  [UI-31](docs/verification/UI-31.md), [UI-33](docs/verification/UI-33.md) and
  [UI-34](docs/verification/UI-34.md).
  Literal rendering and forbidden source semantics have partial v0 evidence in
  [UI-13](docs/verification/UI-13.md), [UI-21](docs/verification/UI-21.md),
  [UI-23](docs/verification/UI-23.md), [UI-26](docs/verification/UI-26.md) and
  [UI-27](docs/verification/UI-27.md). Their full Verify clauses remain open.
  Structural interaction identity and keyboard behavior have partial v0 evidence in
  [UI-16](docs/verification/UI-16.md) and [UI-25](docs/verification/UI-25.md).
  Native focus and the broader keyed collection contract remain open.
  Runtime overflow and hit targets have partial headless evidence in
  [UI-15](docs/verification/UI-15.md) and [UI-17](docs/verification/UI-17.md).
  Native Mac overflow captures and later approved touch targets remain open.
  Production egui accessibility has exact filtered AccessKit role trees for all ten fixtures and
  the gallery at both approved viewports in [UI-22](docs/verification/UI-22.md).
  UI-22 remains open for the native macOS dump and the other target.
  The first Mac consumer visibly renders all eleven unchanged v0 variants through eframe and egui
  in [UI-39](docs/verification/UI-39.md). This does not close the full-catalog or hardware work.
- [x] **UI-43** Headless runtime layout respects clipping, reading order and minimum hit regions.
  **Verify:** measure every supported fixture at 800 by 480 and 1280 by 800; assert no unintended
  overlapping hit regions, no paint escaping the intended scroll clip and 48 by 48 point targets.
  Repeat the invariant suite for any later approved Taffy path.
  [evidence](docs/verification/UI-43.md)

- [x] **V0-01** The Cargo workspace builds the `moted` and `mote-client` binaries and the nine crates named in the table, and both `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` pass on the empty project. **Verify:** Run both commands. [§1](docs/rfcs/v0.md#1-workspace) [evidence](docs/verification/V0-01.md)
- [x] **V0-02** `mote-redact` replaces the same secret patterns and secret keys as Overseer's `redact.rs`, with Overseer's tests carried over. **Verify:** `cargo test -p mote-redact`. [§1](docs/rfcs/v0.md#1-workspace) [evidence](docs/verification/V0-02.md)
- [x] **V0-03** `mote-paths` returns the macOS data directory and socket path and creates the directory with mode 0700. **Verify:** Unit test checks the path and the mode. [§1](docs/rfcs/v0.md#1-workspace) [evidence](docs/verification/V0-03.md)
- [x] **V0-04** Appending an event returns a strictly increasing `seq` and stores already-redacted JSON. **Verify:** Test appends 1,000 events, checks order, and checks that a planted token is redacted. [§2](docs/rfcs/v0.md#2-event-log) [evidence](docs/verification/V0-04.md)
- [x] **V0-05** Subscribing with a cursor replays every event after it in order and then streams live events with no duplicate. **Verify:** Test with two subscribers at different cursors. [§2](docs/rfcs/v0.md#2-event-log) [evidence](docs/verification/V0-05.md)
- [x] **V0-06** Retention keeps 5,000 events per scope behind one retention marker, and no cursor moves backwards after pruning. **Verify:** Test appends 12,000 events to one scope. [§2](docs/rfcs/v0.md#2-event-log) [evidence](docs/verification/V0-06.md)
- [x] **V0-07** A subscriber more than 1,000 events behind receives `resync` and recovers from the log without loss. **Verify:** Test with a stalled subscriber. [§2](docs/rfcs/v0.md#2-event-log) [evidence](docs/verification/V0-07.md)
- [x] **V0-08** `moted` listens on the socket with mode 0600 and a second `moted` exits non-zero naming the socket path. **Verify:** Start twice; check the mode and the exit code. [§3](docs/rfcs/v0.md#3-ipc) [evidence](docs/verification/V0-08.md)
- [x] **V0-09** A request over 1 MiB, malformed JSON and an unknown method each get the matching error code. **Verify:** Test sends each and checks the code. [§3](docs/rfcs/v0.md#3-ipc) [evidence](docs/verification/V0-09.md)
- [x] **V0-10** A peer with a different uid is refused before any request is read. **Verify:** Unit test on the peer credential check. [§3](docs/rfcs/v0.md#3-ipc) [evidence](docs/verification/V0-10.md)
- [x] **V0-11** `events.subscribe` streams from a cursor over the socket, and the client crate reconnects and resubscribes from its cursor after a dropped connection. **Verify:** Test kills the connection mid-stream and checks nothing is delivered twice. [§3](docs/rfcs/v0.md#3-ipc) [evidence](docs/verification/V0-11.md)
- [x] **V0-12** First start writes `config.toml` with exactly the defaults shown. **Verify:** Delete the file, start, diff against the RFC block. [§4](docs/rfcs/v0.md#4-config) [evidence](docs/verification/V0-12.md)
- [x] **V0-13** `config.set` validates the value, writes a `config.changed` event with old and new, applies it on the next turn, and `config.revert` restores it. **Verify:** Test changes `models.talk`, reverts, and inspects both events. [§4](docs/rfcs/v0.md#4-config) [evidence](docs/verification/V0-13.md)
- [x] **V0-14** Setting a model whose file size exceeds the memory gate is refused with a reason that names free memory and model size. **Verify:** Test against the scripted server reporting a 500 GB model. [§4](docs/rfcs/v0.md#4-config) [evidence](docs/verification/V0-14.md)
- [x] **V0-15** The client streams `/api/chat` with `keep_alive` of 30 minutes and an explicit `num_ctx` on every call. **Verify:** Scripted-server test checks the request body. [§5](docs/rfcs/v0.md#5-local-model-client) [evidence](docs/verification/V0-15.md)
- [x] **V0-16** `ensure(model)` pulls nothing on its own, reports a missing model, enforces the memory gate, and returns the measured load time. **Verify:** Scripted-server tests for each case. [§5](docs/rfcs/v0.md#5-local-model-client) [evidence](docs/verification/V0-16.md)
- [x] **V0-17** Structured output sends `format` with a JSON schema and parses the reply fail-closed. **Verify:** Five malformed fixtures all parse as failure. [§5](docs/rfcs/v0.md#5-local-model-client) [evidence](docs/verification/V0-17.md)
- [x] **V0-18** The three approved models are pulled and `ensure()` succeeds for each on the Mac. **Verify:** `MOTE_LIVE=1` test; load times in the evidence file. [§0](docs/rfcs/v0.md#0-ground-rules) [evidence](docs/verification/V0-18.md)
- [x] **V0-19** A user message produces `talk.delta` events and one `talk.done` that carries a ledger row id. **Verify:** Scripted-server test. [§6](docs/rfcs/v0.md#6-talk-lane) [evidence](docs/verification/V0-19.md)
- [x] **V0-20** The talk pack holds the five parts in the fixed order within 600 tokens. **Verify:** Test inspects the stored pack row. [§6](docs/rfcs/v0.md#6-talk-lane) [evidence](docs/verification/V0-20.md)
- [x] **V0-21** First token p50 is under 300 ms with `qwen3:4b` warm. **Verify:** `MOTE_LIVE=1 moted bench talk`; output in the evidence file. [§6](docs/rfcs/v0.md#6-talk-lane) [evidence](docs/verification/V0-21.md)
- [x] **V0-22** When the talk model cannot load, the lane emits `talk.unavailable` with the reason and uses no other model. **Verify:** Scripted server returns 404 for the model. [§6](docs/rfcs/v0.md#6-talk-lane) [evidence](docs/verification/V0-22.md)
- [x] **V0-23** The router runs on every turn in parallel with the talk lane, and any invalid verdict routes to `think`. **Verify:** Six malformed fixtures all route to `think`. [§7](docs/rfcs/v0.md#7-router) [evidence](docs/verification/V0-23.md)
- [x] **V0-24** A `think` verdict makes the talk lane finish its sentence, emit `talk.handoff`, and give no second answer. **Verify:** Scripted test checks the event order. [§7](docs/rfcs/v0.md#7-router) [evidence](docs/verification/V0-24.md)
- [x] **V0-25** The router reaches 0.85 macro-F1 on the 60 labelled turns in the test kit against the live `qwen3:0.6b`. **Verify:** `MOTE_LIVE=1` test prints the score; if lower, record it and leave this open. [§7](docs/rfcs/v0.md#7-router) [evidence](docs/verification/V0-25.md)
- [x] **V0-26** Router p50 is under 150 ms. **Verify:** `MOTE_LIVE=1 moted bench router`; output in the evidence file. [§7](docs/rfcs/v0.md#7-router) [evidence](docs/verification/V0-26.md)
- [x] **V0-27** `mote-harness` launches OpenCode with the configured model and parses its stream into the four think event kinds. **Verify:** Fixture test with a recorded OpenCode transcript. [§8](docs/rfcs/v0.md#8-think-lane-local) [evidence](docs/verification/V0-27.md)
- [x] **V0-28** `moted mcp` serves the four tools over stdio and a think turn calls `memory_search` through it. **Verify:** Scripted test with the fake harness. [§8](docs/rfcs/v0.md#8-think-lane-local) [evidence](docs/verification/V0-28.md)
- [x] **V0-29** A live think turn with `qwen3-coder:30b-64k` reads a file through `file_read` and answers, with first token p50 under 3 s. **Verify:** `MOTE_LIVE=1` test; transcript and timing in the evidence file. [§8](docs/rfcs/v0.md#8-think-lane-local) [evidence](docs/verification/V0-29.md)
- [x] **V0-30** With `local_only` true and the think harness set to Claude or Codex, `moted` refuses at startup with a logged reason. **Verify:** Test. [§8](docs/rfcs/v0.md#8-think-lane-local) [evidence](docs/verification/V0-30.md)
- [x] **V0-31** Every model call writes exactly one ledger row and one pack row with hashed parts and a token estimate. **Verify:** Test counts rows after three turns. [§9](docs/rfcs/v0.md#9-context-store-packs-ledger) [evidence](docs/verification/V0-31.md)
- [x] **V0-32** Packs older than 30 days are deleted by a daily job. **Verify:** Test with a controlled clock. [§9](docs/rfcs/v0.md#9-context-store-packs-ledger) [evidence](docs/verification/V0-32.md)
- [x] **V0-33** `memory_write` stores a node with embedding, `source_seq` and realm in one transaction with no model call. **Verify:** Test asserts zero scripted-server calls during the write. [§10](docs/rfcs/v0.md#10-memory-graph) [evidence](docs/verification/V0-33.md)
- [x] **V0-34** Retrieval fuses FTS, vector, bounded 2-hop walk and recency as specified and returns within the lane budget, p50 under 40 ms on a 10k-node fixture. **Verify:** Test on the fixture prints p50. [§10](docs/rfcs/v0.md#10-memory-graph) [evidence](docs/verification/V0-34.md)
- [x] **V0-35** Forgetting sets `superseded_by` and the next pack omits that memory. **Verify:** Test. [§10](docs/rfcs/v0.md#10-memory-graph) [evidence](docs/verification/V0-35.md)
- [x] **V0-36** `moted bench memory` reports write and retrieve p50 and p95 at 10k and 100k nodes. **Verify:** `MOTE_LIVE=1` run; output in the evidence file. [§10](docs/rfcs/v0.md#10-memory-graph) [evidence](docs/verification/V0-36.md)
- [x] **V0-37** `house.md` and `characters/mote.md` are created on first start with the front matter shown. **Verify:** Test. [§11](docs/rfcs/v0.md#11-charter) [evidence](docs/verification/V0-37.md)
- [x] **V0-38** A tool call not listed in the character's `tools` is refused with a logged reason. **Verify:** Test. [§11](docs/rfcs/v0.md#11-charter) [evidence](docs/verification/V0-38.md)
- [x] **V0-39** Editing a charter file emits `charter.changed` and the next pack carries the new text. **Verify:** Test with the file watcher. [§11](docs/rfcs/v0.md#11-charter) [evidence](docs/verification/V0-39.md)
- [x] **V0-40** The validator accepts all ten fixtures and rejects an unknown type, an unknown key, depth 13, 2,001 nodes and 257 KB, each with a named diagnostic. **Verify:** Tests. [§12](docs/rfcs/v0.md#12-surface-tree) [evidence](docs/verification/V0-40.md)
- [x] **V0-41** `schemas/tree.v0.json` is generated from the Rust types and a test fails when the file drifts. **Verify:** Test. [§12](docs/rfcs/v0.md#12-surface-tree) [evidence](docs/verification/V0-41.md)
- [x] **V0-42** Each fixture renders to a PNG through the test renderer and matches its golden image. **Verify:** Tests. [§12](docs/rfcs/v0.md#12-surface-tree) [evidence](docs/verification/V0-42.md)
- [x] **V0-43** `mote-client` connects, subscribes from its saved cursor, shows threads as chat, and after close and reopen shows nothing twice. **Verify:** Live run; screenshot and cursor check in the evidence file. [§13](docs/rfcs/v0.md#13-mote-client) [evidence](docs/verification/V0-43.md)
- [x] **V0-44** Every message shows one of sending, sent, delivered, read, and a typing indicator appears while `talk.delta` events arrive. **Verify:** Screenshots of each state. [§13](docs/rfcs/v0.md#13-mote-client) [evidence](docs/verification/V0-44.md)
- [x] **V0-45** A reply that carries a surface renders inline, and a button press arrives at the daemon as `ui.event`. **Verify:** Live run; event log excerpt in the evidence file. [§13](docs/rfcs/v0.md#13-mote-client) [evidence](docs/verification/V0-45.md)
- [x] **V0-46** The client follows the system light and dark setting. **Verify:** Two screenshots. [§13](docs/rfcs/v0.md#13-mote-client) [evidence](docs/verification/V0-46.md)
- [x] **V0-47** The pipeline screen lists the five stages and each opens Now, Config, Numbers and Requests tabs. **Verify:** Screenshots during a live turn. [§14](docs/rfcs/v0.md#14-pipeline-view) [evidence](docs/verification/V0-47.md)
  Native live-turn captures cover all five stages and all four tabs. Headless production-shell
  coverage exercises all 20 stage and tab combinations with populated, stage-distinct content.
- [x] **V0-48** Changing `models.talk` from the Config tab calls `config.set` and the next turn uses the new model. **Verify:** Live run; ledger rows before and after. [§14](docs/rfcs/v0.md#14-pipeline-view) [evidence](docs/verification/V0-48.md)
  The native Config Apply changed the approved talk model. The next native turn recorded that model.
- [x] **V0-49** The Requests tab shows every outbound call with a redacted payload and timing. **Verify:** Grep the stored rows for the redaction patterns; none match. [§14](docs/rfcs/v0.md#14-pipeline-view) [evidence](docs/verification/V0-49.md)
- [x] **V0-50** Opening the pipeline view adds under 5 ms at p50 to the talk lane. **Verify:** `moted bench talk` with the view closed, then open. [§14](docs/rfcs/v0.md#14-pipeline-view) [evidence](docs/verification/V0-50.md)
- [x] **V0-51** A `schedules` row fires its prompt into a thread at its cron time. **Verify:** Test with a controlled clock. [§15](docs/rfcs/v0.md#15-watchers-and-approvals) [evidence](docs/verification/V0-51.md)
- [x] **V0-52** A `file_write` outside the data directory creates an approval card, nothing executes until Approve, and Deny or expiry leaves it unexecuted. **Verify:** Test plus one screenshot of the card. [§15](docs/rfcs/v0.md#15-watchers-and-approvals) [evidence](docs/verification/V0-52.md)
- [ ] **V0-53** `moted install` loads the launchd agent, the daemon survives logout and login, and `moted uninstall` removes it. **Verify:** Live; `launchctl print` output in the evidence file. [§16](docs/rfcs/v0.md#16-service)
- [x] **V0-54** `moted status` prints socket, uptime, loaded models and the last event seq. **Verify:** Live output in the evidence file. [§16](docs/rfcs/v0.md#16-service) [evidence](docs/verification/V0-54.md)
- [x] **V0-55** All three bench commands write one JSON line each to `bench.jsonl`. **Verify:** Test. [§17](docs/rfcs/v0.md#17-bench) [evidence](docs/verification/V0-55.md)

### Native iPad host feature

The Simulator, inferred UI and local browser approval journeys are complete. The owner activated
the distinct device build and internal TestFlight phase on 2026-10-11. HOST-05's literal Mac sleep
and wake run is deferred to an isolated test session and is not counted as device distribution
evidence.
The [approved design](docs/superpowers/specs/2026-10-11-ipad-host.md) defines the journeys,
security boundary, incorporated Matrix and browser behavior, and deferred physical delivery.
Execution is split into the [host foundation](docs/superpowers/plans/2026-10-11-ipad-simulator-host.md),
[Simulator UI](docs/superpowers/plans/2026-10-11-ipad-simulator-ui.md), and
[inferred UI and browser](docs/superpowers/plans/2026-10-11-ipad-agent-browser.md) plans.

- [ ] **[Native iPad Simulator host](docs/superpowers/specs/2026-10-11-ipad-host.md)**
  - [x] **HOST-01** A versioned transport-independent host protocol carries requests, events,
    committed Matrix surfaces, typed interactions, approvals, cursors and structured errors while
    the existing Unix transport keeps passing its contract. **Verify:** protocol golden tests and
    the existing Unix reconnect suite pass with no wire-type dependency on a socket implementation.
  - [x] **HOST-02** Every host has a stable cryptographic identity, and the client keeps credentials,
    cursor, thread, surface and approval state isolated per host. **Verify:** pair two fixture hosts
    with colliding thread and approval IDs, switch repeatedly and prove no state or action crosses.
  - [x] **HOST-03** Owner-present pairing uses the `MOTE1` offer and `mote-gateway-v1` Noise
    protocol to mutually authenticate an iPad and host, encrypt ordered frames, pin both identities
    and support device revocation. **Verify:** pass shared Noise vectors, complete one pairing and
    reject expired or reused offers, altered identities, replayed counters, tampered frames and a
    revoked device before any application action runs.
  - [x] **HOST-04** Bonjour discovers a local Mote gateway without treating its address as identity,
    and a known host can change addresses only after identity proof. **Verify:** discover `_mote._tcp`
    with only version and fingerprint in TXT on the Mac and Simulator network stack, move the
    loopback endpoint, reconnect to the same identity, reject an impersonator and verify the checked-in
    local-network purpose text. The physical privacy prompt remains IPAD-06 evidence.
  - [ ] **HOST-05** Reconnect resumes the selected host's cursor without duplicates, retention loss
    causes an authoritative resync, and the client visibly distinguishes connecting, online, offline,
    incompatible and identity-changed states without silent failover. **Verify:** scripted disconnect,
    lag, restart, version mismatch and identity-change tests plus one live sleep and wake run.
  - [x] **IPAD-01** The shared Rust Matrix consumer and protocol build for Apple Silicon simulator and
    physical iPad targets behind a small native application shell. **Verify:** build both Rust targets
    and the Xcode app with the existing Mac client and workspace tests still green; record the exact
    Rust, Xcode, SDK and dependency versions.
  - [x] **IPAD-02** The iPad Simulator runs separate Layout, Toggle, Slider, Select, List and Form
    gallery pages, the checked-in dashboard and a fixture surface. Selecting a page rebuilds the
    layout immediately while changed red borders split into square pixels and materialize within
    120 ms. Unchanged nodes do not restart and hit testing uses the new layout on the first frame.
    **Verify:** drive scrolling, text entry and every control in two iPad sizes and both orientations;
    capture first, midpoint and final transition frames at 60 frames per second; reduced motion must
    swap immediately with only a brief red outline pulse.
    Interactive shell, GPU capture and renderer-cadence evidence are recorded with the explicit
    presentation-timing boundary and acceptance decision in
    [IPAD-02](docs/verification/IPAD-02.md).
  - [x] **IPAD-03** The simulator pairs with `moted` on the Mac, streams live chat and committed Matrix
    surfaces, returns typed interactions and reconnects from its cursor. **Verify:** complete a live
    chat and dashboard edit, kill and relaunch the app, observe no duplicate event, then switch between
    two paired fixture hosts without mixed state.
  - [x] **IPAD-04** A natural-language request produces ordinary prose and at most one Matrix surface
    using only an approved local model and host-owned bindings and actions. The host parses, resolves,
    type-checks, lowers and validates it before commitment. Invalid generation cannot replace or
    activate the previous surface and receives at most one diagnostic retry inside the rate budget.
    **Verify:** run 200 representative prompts per permitted model, require 95 percent first-try
    validity or keep that model disabled, verify ledger rows and typed event rejection, and capture
    success, one-retry recovery, stable prior surface and final prose fallback on the simulator.
    [Evidence](docs/verification/IPAD-04.md) records the 192-of-200 live gate and four paired native captures.
  - [x] **IPAD-05** A Mac-hosted browser fixture uses a per-character Mote Chrome for Testing profile
    and can browse, read and fill automatically only on an approved site. Mote never opens the default
    Chrome profile or its keychain item. Submit, purchase, message, download, OAuth and non-GET actions
    remain blocked until a durable host-bound approval is accepted from the iPad. **Verify:** record
    stable-ref snapshot, read, fill and stale-ref refusal; test submit, purchase, message, download,
    OAuth and non-GET fixtures plus an unknown or malformed outward-action verdict; assert that each
    creates a durable approval and has zero effect before approval, that deny and expiry stay inert,
    and that one approved action executes exactly once after page-state recheck. Also record prompt-
    injection refusal, redacted events, ledger and HAR, before and after commit screenshots and a
    final validated Matrix result card. The [verification record](docs/verification/IPAD-05.md)
    includes the headed fixture and paired native Simulator evidence.
  - [x] **IPAD-13** When an agent commits a replacement Matrix layout on the paired iPad, changed
    red borders, header and footer rules, and slider rails materialize as square pixels within
    120 ms. Unchanged strokes stay painted. New controls are interactive on the first frame and
    retain revision-specific state. **Verify:** replace a live paired surface with a changed
    layout in the iPad Simulator, capture the first, midpoint and settled frames, and exercise
    a changed control immediately. Repeat with Reduce Motion, which must swap immediately with
    a brief red outline pulse. [partial evidence](docs/verification/IPAD-13.md)
  - [x] **IPAD-14** Every Matrix and egui single-line field uses one reusable iOS text-input bridge
    while egui remains the visual renderer. A tap anywhere in the 48-point field focuses it and the
    bridge supports the system keyboard, cursor movement, selection, marked text, native copy, cut
    and paste menus, dictation and hardware keyboards in generated forms, first-run setup and chat.
    **Verify:** run focused Rust bridge tests and an iPad Simulator UI test covering edge focus,
    keyboard input, cursor edits, selection and native copy, cut and paste menus. Repeat the flow on
    the physical iPad, including marked-text composition and dictation, and record the device,
    iPadOS version, commands and observed results. [evidence](docs/verification/IPAD-14.md)

### Physical iPad and internal TestFlight

The separate App Store Connect listing is [Mote Agent](https://appstoreconnect.apple.com/apps/6822101041/distribution/info), Apple ID `6822101041`, bundle ID `com.beelol.mote`, SKU `mote-ipad-001`, on team `FQ6YGD7554`. The native app still displays `Mote`. The owner chose the listing name after Apple rejected `Mote` as already in use.

- [ ] **[Physical iPad and TestFlight](docs/superpowers/specs/2026-10-11-ipad-host.md#distribution)**
  - [x] **IPAD-06** A physical iPad passes touch, keyboard, rotation, split view, background recovery,
    local-network permission, light and dark appearance, reduced motion and VoiceOver checks.
    **Verify:** repeat the named journeys on the device, including center and edge taps for every
    control, and record the device, iPadOS version, role tree, screenshots and observed recovery.
  - [x] **IPAD-07** After device-capable build and final native Simulator UI verification, Mote has a new
    explicit bundle ID, signing configuration and App Store Connect record that do not reuse or alter
    another app. **Verify:** record the approved display name, bundle ID, SKU and team, confirm their
    uniqueness against the owner's existing apps, archive the same native client and retain the App
    Store Connect record link.
  - [ ] **IPAD-08** After IPAD-06 and IPAD-11, the processed build installs through internal TestFlight and repeats
    pairing, live chat, generated Matrix UI, browser approval, reconnect and host-switch isolation.
    **Verify:** record the build and processing state, install it on the physical iPad, run every
    journey and keep external testing, App Review and public release disabled.
  - [x] **IPAD-09** The existing Rust and Xcode client builds for a physical iPad in addition to the
    existing Simulator build without changing its bundle identity, behavior or tests. **Verify:**
    `aarch64-apple-ios` Rust check, an unsigned `iphoneos` build, unchanged Simulator
    gallery and paired-host journeys, and recorded Xcode, SDK and target versions.
  - [x] **IPAD-10** A repeatable, guarded distribution script performs a no-mutation preflight,
    then an explicit signed archive and `.ipa` export for only the confirmed Mote team and bundle
    ID. Upload is a separate opt-in command. **Verify:** test missing and colliding identity,
    absent signing, duplicate build number and bad output path fail closed without Apple mutation;
    inspect a real exported archive and IPA for exact identity and entitlements, prove no secrets
    entered Git or logs, and record one deliberate upload and processing outcome only after IPAD-07.
  - [x] **IPAD-11** The processed Mote build is available only to an internal TestFlight group before
    physical installation. **Verify:** record the build identifier and processing state in the separate
    Mote app, assign only an internal group with an eligible tester, and confirm external testing and
    App Review remain disabled. Physical installation is verified separately by IPAD-08.
  - [ ] **IPAD-12** A GitHub Actions workflow can build Mote from `main` and optionally upload to its
    internal TestFlight app only when manually dispatched. It never runs on push or a schedule.
    **Verify:** inspect the dispatch trigger and branch guard, prove the live inventory, app identity,
    Mote-only provisioning profile, duplicate-build, signed IPA and internal-only export checks fail
    closed, configure isolated Mote release credentials, then run one build-only dispatch and one
    deliberate upload dispatch. Record each run and confirm no other app or TestFlight group changed.

### Memory backend comparison

The owner activated this goal on 2026-10-13. It is independent of the remaining iPad and TestFlight checks. SQLite remains the production default. [The memory RFC](docs/rfcs/memory-backend-comparison.md) defines the boundary and the resource ceiling. This goal does not activate the full context-layer backlog or authorize a default backend switch.

```text
Continue Mote's owner-activated memory backend comparison from repository state. Do not rely on this chat.

Read AGENTS.md, docs/rfcs/v0.md, root README.md -> Ready for Go -> Memory backend comparison -> the nested MBE checkboxes, docs/rfcs/memory-backend-comparison.md, docs/HANDOFF.md if present, relevant code and tests. Only the MBE criteria are authorized by this goal. The iPad goal is separate. The context-layer backlog, full Tencent proxy, Pi, cloud relay and automatic default switch remain out of scope. SQLite stays default. Keep models.local_only true, use only the three approved Ollama models, ledger every generative call and redact before storage.

Pick the smallest actionable unchecked MBE criterion or coherent dependency set. Reproduce suspected defects before fixing them. If delegation is available and authorized, give a bounded task to the least costly capable subagent with AC IDs, owned files, constraints and evidence requirements. Inspect its result. Otherwise work directly. Keep candidate experiments isolated from personal data and production state.

Implement and verify behavior, including failure, correction, deletion, restart and realm changes where relevant. Use the same synthetic fixtures and output budget for SQLite and Tencent. Respect the RFC ceiling of 100 memories, 30 queries and 100 optional local extraction calls; do not fetch large corpora or run an unattended service. Stop the experiment on an outbound non-loopback call, a cross-realm hit or a deletion leak. No paid model or new model pull is authorized.

Record each fully verified AC in docs/verification/<ID>.md with command, output, environment, date, commit and limits. Recheck affected v0 evidence. Before every commit run cargo test --workspace and cargo clippy --workspace -- -D warnings. Inspect the diff. Commit small coherent Conventional Commits on main and push to the confirmed origin. Include evidence and checkbox changes in the verified delivery. An unpushed or unmerged change is not completed mainline work. Never infer permission to spend, deploy, publish or destroy unrelated data.

Check the Memory backend comparison parent only when every MBE child is verified. If work reveals an in-scope gap, add a testable AC and reopen its parent. Never weaken an unmet Verify clause. After integration reread all MBE criteria, reconcile the next action and recheck affected completed ACs. A Tencent win may enable opt-in use only after its measured gate. Do not make it the default without a later owner decision and migration/rollback evidence.

Replace docs/HANDOFF.md with current ACs, checkout, delegated work, unmerged changes, verified results and limits, blockers and exact next action. Checkpoint before delegation or long operations and after every edit batch, subtask, test run, commit, push or blocker, at most two minutes apart during active work. Record in-flight handles before blocking calls and checkpoint on return. Save before stopping. On resume reconcile files and commits with the handoff. Delete it only when nothing needs transfer. Keep durable facts in the RFC and evidence files, not a diary.

Continue until all authorized MBE criteria are verified and pushed. If one is blocked, do independent work. Stop only for completion, an owner stop, a budget limit or when all remaining work requires unavailable input, access or authority. State exact blockers honestly.
```

- [ ] **[Reliable and replaceable memory](docs/rfcs/memory-backend-comparison.md)**
  - [ ] **MBE-01** SQLite writes are atomic and never mix embedding models.
  - [ ] **MBE-02** Agent search and turn context use the same sourced retrieval path.
  - [ ] **MBE-03** Every memory operation enforces account and realm isolation.
  - [ ] **MBE-04** Long-history context preserves pinned instructions and the newest turn.
  - [ ] **MBE-05** Matrix exposes memory source, correction and forget controls.
  - [ ] **MBE-06** SQLite passes the common backend contract and remains default.
  - [ ] **MBE-07** Tencent runs only as a pinned, loopback, synthetic-data comparison adapter.
  - [ ] **MBE-08** A bounded report compares recall, safety, deletion, latency and resources.
