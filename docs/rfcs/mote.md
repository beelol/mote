# RFC: mote, an always-on personal agent

Status: proposed by the owner on 2026-10-08. Decisions below were taken the same day.
Working name: mote. The owner may rename it. Acceptance criteria: MOTE-01 to MOTE-35 in the
root README under Ready for Go. Side RFCs: [shared crates](shared-crates.md) (CRATE-NN) and the
[UI language](ui-language.md) (UI-NN).

## Key points

1. mote is four parts: mote-cloud (the engine, `moted`, all state), mote-client (the chat app, lends the Mac when it runs there), mote-ui (the stateless renderer, alone on a Pi or inside the client) and mote-talk (voice). mote-cloud runs on the Mac today and a server later.
2. Four lanes: talk answers in 300 ms from a local model, a router classifies each turn, think runs Claude Code under the owner's login with tools, work hands coding to Overseer.
3. Models come from subscription logins and local Ollama. No API keys.
4. Models emit a validated UI tree, never HTML, CSS or JavaScript. A Tauri window renders it on the Mac now. A Raspberry Pi touchscreen comes later.
5. Every model call records what it saw and what it cost in a ledger. Memories change only through one tool, visibly.
6. Reusable Overseer code moves into shared crates in this repo. Overseer migrates onto them later.
7. Criteria MOTE-01 to MOTE-35 below are the contract. The README board holds the checkboxes.

## Why

OpenAI shipped dots on 2026-09-29: always-on agents with their own cloud computer that watch
Slack and GitHub and act in the background. The owner's team already runs them. The owner wants
the same thing, but better, and owned: an agent that answers instantly, thinks hard only when a
turn needs it, hands real work to the machine that can do it, draws its own interface, and keeps
its context under the owner's control. It runs in a window on the Mac today, on a Raspberry Pi
with a touchscreen later, and on any machine the owner picks as the cloud. It is open source.

## Decisions from the owner (2026-10-08)

| Topic | Decision |
| --- | --- |
| Shape | A Rust daemon (`moted`) that owns all state. Every UI is a client. Closing a window never ends a thread. |
| Place | A new repository at `~/projects/mote`, separate from Overseer. |
| Relationship to Overseer | mote hands coding and long work to `overseerd` over its Unix socket. Reusable parts of overseerd move into shared crates under `crates/` in this repo. mote consumes them directly. Overseer migrates onto them later through its own driver. See [shared crates](shared-crates.md). |
| Model access | Subscription logins only, as in Overseer: the Claude Code login through `claude -p`, Codex through its harness. Local models through Ollama are an option for every lane. No API keys. |
| Talk lane | A local model answers first. Haiku 5.5 through `claude -p --model haiku` is the fallback when the Mac cannot run the local model. |
| Routing | A cheap classifier picks `talk`, `think` or `work` for every turn. Its verdict is structured and parsed fail-closed. |
| Attention | A separate fast detector scores whether the owner addressed mote. Sure: answer. About half sure: ask. Below: stay silent. Added by the owner on 2026-10-08. |
| UI | Models emit a validated declarative UI tree. v0 renders it in a Tauri window on macOS. No HTML, CSS or JavaScript is ever the language the model writes. The long-term language is researched first in the [UI language RFC](ui-language.md); the owner turns that RFC into criteria and loops on it. The tree's wire format is an open decision there, with JSON as the default until the research says otherwise. |
| Context | Context engineering is in place from v0: a store, explicit per-call context packs, and a ledger of what each call saw and cost. |
| Later | Raspberry Pi touchscreen renderer, cloud connector or relay, voice. Deferred, each behind an approval criterion. |
| Chat surface | Keep the texting feel from dots: sent, delivered, read and typing states on every message. Characters (voice, tone, name) are options. Added by the owner on 2026-10-08. |
| Knowing its limits | mote says plainly when it cannot see something or lacks access, and names what it needs. Added by the owner on 2026-10-08. |
| Context layer | One layer of infrastructure reaches all of the owner's data (Slack, mail, calendar, files, code, the web) and serves retrieval and consumption for every lane. Its storage engine (SQLite with graph tables, or an embedded graph database) is an open decision for its RFC. Added by the owner on 2026-10-08. |
| Browser use | Two modes. Cloud: a browser on the machine the owner picked as the cloud, with only the sessions the owner gave it. Local: the owner's own signed-in browser on the Mac, used to make calls and reach sites as the owner. Pages are read as accessibility trees first. Every irreversible click waits for approval. Added by the owner on 2026-10-08. |
| Characters | The products are characters that run on mote. mote is the engine. A character changes voice, tone and name, never what mote may do. Added by the owner on 2026-10-08. |
| Charter | Two owner-edited files steer every character: `house.md` (standing rules for all characters and realms) and `<character>.md` (voice, tone, name, allowed tools, realms). Each has a small structured header the daemon enforces (tools, realms, approval classes, spend caps) and prose the model reads. They are always the first pack part, versioned in the event log, and changed only by the owner. A character may propose an edit through an approval card. Added 2026-10-08. |
| Audit | Every memory carries its source event, its writer, its validity interval and its realm. A memory browser shows why mote knows something, lets the owner retract or correct it reversibly, and sends a daily digest of what was learned. Every pack is stored so any answer can be replayed. Added 2026-10-08. |
| Visibility and control | Every running stage is visible and changeable from a dashboard hosted by the app itself: click a stage to see what it is doing and switch its parts, such as the model behind a lane or the harness behind the think lane, without a restart. The daemon computes everything in Rust so the view never lags. Added by the owner on 2026-10-08. |
| Open core | mote is open source. One machine the owner chooses acts as the cloud, the Mac today and any box later. Every other device is a client of it. A cloud service around the core (relay, hosted machine) is allowed later. Added by the owner on 2026-10-08. |

## Topology (owner, 2026-10-08)

mote is four parts. Only one of them holds state.

| Part | What it is | Runs on | Holds |
| --- | --- | --- | --- |
| mote-cloud | The engine, process `moted`: lanes, router, memory graph, context layer, event log, ledger, watchers, charter, cloud browser. | The owner's Mac today ("I am running mote-cloud locally"). Any server later. | All state. |
| mote-client | The app: chat with sent, delivered, read and typing, generative cards inline, approvals, settings. mote-talk (voice) is an option inside it. When it runs on the Mac it also lends the Mac to mote-cloud: signed-in browser sessions, files, Overseer. | Mac, phone. | A cursor and a cache. Nothing durable. |
| mote-ui | The renderer: a surface tree in, pixels out, input events back. Standalone on a Pi touchscreen or any computer, and embedded inside mote-client for inline cards. | Pi, any computer, inside the client. | Nothing. |
| mote-talk | Voice in and out. | Wherever the client or mote-ui runs. | Nothing. |

Rules that follow:

- One Mac is the degenerate case: mote-cloud and mote-client on the same machine over the socket. The client shows "connected to mote-cloud on this Mac" and can be pointed at a remote mote-cloud with no code change.
- The Pi stores nothing. It renders, sends input, and shows the last surface with an offline mark when the gateway drops. Nothing else about the Pi matters yet except that mote-ui must run there with a screen.
- "Connect if the computer is on, with options": mote-client on the Mac advertises local capabilities to mote-cloud, each one a switch in settings. The coverage map lists them as reachable only while that client is connected.
- A chat answer can be a surface. Asked where a DoorDash order is, a character may draw a map from what the browser session sees, embed a provider widget where one exists, or show a food bag with the minutes underneath. The model picks the form. mote-ui draws it. The browser lane fetches it.

## Running everything locally

With no cloud at all, every part runs on machines in the owner's home. This is the first
deployment, not a special case.

| Part | Where | Notes |
| --- | --- | --- |
| mote-cloud | The Mac, as a launchd agent, or a small always-on box such as a Mac mini | The Mac must stay awake. A Mac mini is the cheapest box that also runs the local models well. |
| mote-client | The same Mac, and the phone over the home network | The phone reaches the gateway through the Noise handshake from Overseer's phone remote. Away from home, a WireGuard or Tailscale tunnel to the Mac replaces a relay. |
| mote-ui | Inside the client, or a Pi on the same network | The Pi needs only the gateway address and a pairing code. |
| Models | Ollama on the Mac, Claude Code under the owner's login on the Mac | No API keys. No cloud model calls beyond the subscription itself. With no subscription at all, the three roles below run locally. |
| Browser | Cloud mode is a headless Chrome profile on the Mac. Local mode is the owner's own Chrome on the same Mac | Both modes live on one machine until a server exists. |
| Connectors | Pull only: Slack Socket Mode, GitHub polling, IMAP IDLE, CalDAV, local files | No inbound ports. Webhooks wait for a server with a public address. |
| Overseer | The same Mac | Already there. |

Three local models cover the three roles with no subscription:

| Role | Model | Size | Called through | Notes |
| --- | --- | --- | --- | --- |
| Decision | clef-flash or nimble, 9B, Apache-2.0 | 9.3 GB | Ollama decision endpoint | Router judge and the attention ask-band. The trained head runs first. |
| Fast | qwen3:4b, with qwen3:0.6b beside it for the attention scorer | 2.5 GB | Ollama chat, kept warm | 22 to 90 ms to first token when warm. |
| Big | qwen3-coder:30b with a 64k context, or gpt-oss-120b | 19 GB or about 65 GB | OpenCode through mote-harness against Ollama | qwen3-coder:30b completed real tool-using work in Overseer's Continuity mode. The context must be set to at least 32k explicitly, since Ollama's 4k default truncates silently. |

The think lane swaps its harness from Claude Code to OpenCode and keeps the same tool server.
Every lane has a per-lane fallback chain in settings: local first, subscription when allowed,
never an API key.

What "computer off" means with no cloud: nothing runs. The honest options are to keep the
Mac awake, or to make a Mac mini the home cloud so the laptop can sleep. Both keep every byte
at home.

## Visibility and control

The owner's words: "full visibility of running systems. I should be able to click on a
pipeline model to see what's happening and how I can modify it, like switch out a model,
local, harness to model, API request. With a little UI utility or dashboard hosted with the
app itself. The Rust part is for speed."

The pipeline view lives inside mote-client and is drawn by mote-ui from the same surface
trees as everything else. There is no separate web stack. Each stage is a node:

```
attention -> talk -> router (head, judge) -> think (harness -> model -> tools) -> work (Overseer)
                                           -> browser (cloud, local)
context layer (connectors -> index -> retrieval -> packs) -> memory graph -> ledger
watchers -> approvals            event log            gateway (clients)
```

Clicking a node shows four things:

1. **Now.** The live event stream for that stage, the request in flight, its latency so far.
2. **Config.** What the stage is built from: model, harness, endpoint, context size, thresholds, memory gate, realm. Each value is a control.
3. **Numbers.** p50 and p95 latency, tokens, cost and error rate over the last hour and day, against the stage's budget. Over budget turns red.
4. **Requests.** Every outbound call the stage made: subprocess argv or HTTP request, redacted payload, response size, timing, ledger row.

Changing a control applies hot. The daemon validates the change fail-closed before it takes
effect (a model that fails the memory gate is refused with the reason), writes the change as an
event so it is versioned and revertible, and switches the stage on its next turn. Switching
the think lane from Claude Code to OpenCode on a local model, or the talk lane from local to
Haiku, is one control each. A stage can be paused, drained and resumed.

Speed is the constraint. Metrics are computed in the daemon in Rust from a ring buffer and the
ledger, not in the client. The view subscribes over the same gateway as every client and
updates within one frame of the event. Opening a node must not slow the lane it inspects.

## What mote is

mote is one daemon with four lanes and three stores.

Lanes:

1. **Talk.** A fast model answers within about 300 ms and keeps the conversation moving. It never
   calls tools. It may say "checking" and hand the turn on.
2. **Router.** Every turn also goes to a cheap classifier. It emits one JSON object,
   `{"route": "talk" | "think" | "work", "reason": "..."}`. Malformed output never routes to `talk`.
3. **Think.** A thinking model with tools handles hard turns. v0 runs it as a Claude Code
   subprocess under the owner's login. Tools: memory, files, GitHub through `gh`, Slack read when
   a token exists, and `delegate`.
4. **Work.** Coding and long tasks become an overseerd run. mote follows the run's events and
   reports back as cards.

Stores:

1. **Event log.** Append-only, cursor replay. Every lane, tool, watcher and UI action writes here.
2. **Context store.** Profile, durable memories, per-thread working set, connector snapshots.
3. **Ledger.** One row per model call: lane, model, context pack ids, token estimate, cost, latency.

Transport: a Unix socket for local clients. A WebSocket gateway later, with the Noise handshake
and frame shapes from Overseer's phone-remote protocol, for the Pi, the phone and the cloud.

## Journeys

| Journey | Trigger | Expected | Recovery |
| --- | --- | --- | --- |
| Instant talk | The owner types "morning" | The talk lane streams a reply within 300 ms. The router says `talk`. Nothing else runs. | Local model unavailable: Haiku answers, and the card says which lane answered. |
| Think | "What is blocking PR 1141?" | The talk lane says it is checking. The router says `think`. The think lane calls `gh`, emits a card with the answer, and the talk lane narrates one line. | Claude login expired: a sign-in card appears, the turn stays queued, and the owner's `claude /login` resumes it. |
| Work | "Fix the flaky probe test in monarch" | The router says `work`. mote starts an overseerd run. A progress card updates from run events. Completion or a need for input becomes a card with actions. | overseerd not running: mote says so, offers to start it, and keeps the request queued. |
| Interruption | The owner closes the window mid-turn | The daemon keeps going. Reopening replays from the client's cursor. Nothing is shown twice. | None needed. |
| Watcher | Slack mentions the owner at 03:00 | The watcher wakes the router. The think lane drafts a reply as a card. The owner approves before anything posts. | No Slack token: the watcher reports itself disabled in the window. |
| Offline | Wi-Fi drops | The talk lane stays local. Think and work lanes pause and say why. They resume when the probe succeeds. | None needed. |

## Security and data

- The data directory is owner-only (0700). The socket path is inside it. A second daemon refuses to start.
- Secrets never enter a model call. Redaction runs on every event before it is stored, as in Overseer.
- Subscription logins stay in their harness's own store. mote never reads or copies them.
- Watchers are read-only. Every outward action (a Slack post, a PR comment, an email) needs owner approval in the window in v0.
- The think lane writes memories only through the memory tool. Every write is visible and reversible in the window.
- Writes to SQLite are transactional. The event log is append-only. Replay is idempotent by cursor.

## Platform limits

- macOS now, on the owner's Mac (128 GB RAM, cargo 1.89, Claude Code 2.1.289, Ollama, cargo-tauri). Linux and the Pi are unverified. Cloud is out of scope.
- `claude -p` flags used: `--output-format stream-json --include-partial-messages --model <name> --resume <id> --json-schema <schema> --mcp-config <file> --allowedTools`. Verified against `claude --help` on 2026-10-08. `--bare` is never passed: bare mode skips the OAuth login and needs an API key, which mote does not use.
- Ollama chat API with streaming. Installed models today are coder variants and `qwen2.5:0.5b`. A chat model for the talk lane (an 8B class instruction model) must be pulled with the owner's go-ahead.
- A Slack token is an owner decision. Without it the Slack watcher is disabled, not mocked.

## Verification

- `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` on every change.
- A scripted model server (zero RAM, speaks the Ollama chat API) and a fake `claude` shim make every lane testable with no paid turns and no network.
- Live checks against the real local model, the real Claude login and the real overseerd are recorded in `docs/verification/<ID>.md` with the command, the output and the date.
- Window behaviour is checked in the running Tauri window with screenshots in the evidence record.

## Open decisions

1. Tree wire format. The [UI language RFC](ui-language.md) recommends compact nested JSON for v0 (measured 512 tokens for its three fixtures against 920 pretty-printed) and the `.mote` language later (338 tokens). Default: that recommendation, pending UI-00.
2. Talk model. Default: the smallest instruction model that holds a conversation on this Mac; the owner approves the pull.
3. Router model. Default: `qwen2.5:0.5b` if it meets the latency and accuracy criteria, else the talk model with a short prompt.
4. Codex as a think lane option. Default: later, as a deferred item.
5. Launch at login. Default: a launchd agent, installed by `moted install`.
6. License. Default: MIT, matching Overseer and CRATE-17.

## Acceptance criteria

Each criterion is one sentence plus a Verify clause. The checkboxes live in the root README.

### Daemon core
- **MOTE-01** `moted` starts as one binary, creates an owner-only data directory with a SQLite store and a Unix socket, and refuses to start twice. **Verify:** `cargo test -p moted core::` and a second `moted` exits non-zero with the message naming the socket.
- **MOTE-02** Every lane, tool, watcher and UI action appends to one event log, and a client that reconnects with a cursor misses nothing and sees nothing twice. **Verify:** `cargo test -p mote-eventlog` replay tests plus a window close and reopen mid-turn.
- **MOTE-03** A scripted model server and a fake `claude` shim let every lane test run with no network and no paid turns. **Verify:** `cargo test --workspace` passes with networking disabled.

### Talk lane
- **MOTE-04** The talk lane uses a local model when free memory is at least 1.25 times the model size, and its first token arrives within 300 ms at the median on the owner's Mac. **Verify:** `moted bench talk` prints p50 and p95 over 20 turns; the evidence record keeps the output.
- **MOTE-05** When the local model is unavailable the talk lane falls back to Haiku 5.5 through `claude -p --model haiku`, and the reply card names the lane that answered. **Verify:** stop Ollama, send a turn, inspect the card and the ledger row.
- **MOTE-06** The talk lane streams while the router runs, and a `think` or `work` verdict lets the talk lane finish a short acknowledgement and hand off without a second answer. **Verify:** scripted test with a slow router and an inspection of the event order.

### Router
- **MOTE-07** The router emits one JSON verdict with `route` and `reason`, and any malformed or unknown verdict routes to `think`, never to `talk`. **Verify:** unit tests feed empty, truncated, prose and unknown-route outputs.
- **MOTE-08** The router adds no more than 150 ms at the median to a `talk` turn. **Verify:** `moted bench router` output in the evidence record.

### Think lane
- **MOTE-09** The think lane runs Claude Code under the owner's login with streaming partial messages, and its text, thinking and tool events reach the event log. **Verify:** a live turn with `--include-partial-messages` recorded in the evidence file, plus a shim-based test.
- **MOTE-10** The think lane has memory, files, GitHub and delegate tools, and every tool call writes a ledger row with lane, model, pack ids, tokens and latency. **Verify:** `cargo test -p moted tools::` and a `SELECT` over the ledger after a live turn.
- **MOTE-11** Each thread keeps its Claude session through `--resume`, and an expired login shows a sign-in card and keeps the turn queued until login succeeds. **Verify:** shim test for the expired-login error class plus one live expiry.

### Work lane
- **MOTE-12** The think lane can start an overseerd run over overseerd's Unix socket and mote follows that run's events into its own log. **Verify:** live run against the owner's overseerd recorded in the evidence file.
- **MOTE-13** A work run shows as a progress card that updates live, and completion or a need for input becomes a card with actions. **Verify:** screenshots of the three card states.

### Generative UI v0
- **MOTE-14** The daemon validates every emitted UI tree against the v0 schema, rejects unknown nodes fail-closed, and shows a plain text card in place of an invalid tree. **Verify:** `cargo test -p mote-ui` with valid, partial and invalid fixtures.
- **MOTE-15** mote-client renders the v0 tree, inline in chat and as full surfaces, with built-in scrolling, focus, light and dark theme, streams partial trees, and returns every button press or form submit to mote-cloud as an event. **Verify:** window screenshots and the event log after a form submit.
- **MOTE-16** Closing and reopening the window keeps every thread and its cards. **Verify:** close mid-turn, reopen, compare with the event log.

### Context engine
- **MOTE-17** The context store holds profile, memories, per-thread working set and connector snapshots, and every model call records its context pack and cost in the ledger. **Verify:** `cargo test -p mote-context` and a ledger `SELECT` after a live turn.
- **MOTE-18** Each lane has a context budget, packs stay inside it, and the window can show what a call saw. **Verify:** a "what did you know" action on a card lists the pack.
- **MOTE-19** Memories change only through the memory tool, and every change is visible and reversible in the window. **Verify:** write, inspect, revert, inspect.

### Attention detector
- **MOTE-22** A separate detector scores every incoming utterance or message for whether the owner addressed mote, as a sureness from 0 to 100, with no tool calls and no memory writes. **Verify:** `cargo test -p moted attention::` on a labelled fixture set of addressed and not-addressed samples.
- **MOTE-23** Above the answer threshold mote replies through the talk lane, inside the ask band it asks one short question, below the band it stays silent, and all three thresholds are settings. **Verify:** scripted test for the three bands plus a live session recorded in the evidence file.
- **MOTE-24** Every detector verdict and its reason land in the event log and the window can show why mote spoke or stayed quiet. **Verify:** inspect the log after a silent sample and an answered sample.

### Charter and audit
- **MOTE-25** `house.md` and one `<character>.md` per character are loaded as the first pack part of every call, their structured headers (allowed tools, realms, approval classes, spend caps) are enforced by the daemon rather than trusted to the model, every edit is an event, and only the owner can change them. **Verify:** a character that proposes an edit produces an approval card and no file change; a tool outside the header is refused with a logged reason.
- **MOTE-26** A memory browser lists, searches and times every memory, opens the source event behind each one, retracts or corrects with a superseding row instead of a delete, and a daily digest card lists what was learned with one action to forget each item. **Verify:** retract one memory, confirm the old row is marked superseded and the next pack omits it; screenshot the digest.
- **MOTE-27** Every context pack is stored for 30 days with its part hashes, and any card can replay the exact pack behind its answer. **Verify:** open "what did you know" on a card from yesterday and compare the shown parts with the ledger row.

### Topology
- **MOTE-28** mote-cloud and mote-client are separate processes with a documented protocol, the client shows "connected to mote-cloud on this Mac" when both run locally, and the same client build connects to a remote mote-cloud over the gateway. **Verify:** run both on the Mac, then point the client at a second mote-cloud on another port and complete a turn.
- **MOTE-29** mote-client on the Mac lends local capabilities to mote-cloud (browser sessions, files, Overseer), each behind an owner-controlled switch, and the coverage map lists them as reachable only while that client is connected. **Verify:** turn one switch off, ask for something that needs it, and get a "cannot reach" answer that names the switch.
- **MOTE-30** mote-ui runs standalone with no state: it renders surfaces from mote-cloud, returns input events, and after a restart shows the current surface from its cursor. **Verify:** kill and restart mote-ui mid-surface on a second machine or the Pi.
- **MOTE-31** A chat answer can be an inline surface the model chooses, and for a delivery question the three forms (a map from the browser session, a provider widget, a status card with minutes) all render in the chat. **Verify:** scripted fixtures for all three plus one live check against a real order through the local browser session.

### Visibility and control
- **MOTE-32** The pipeline view shows every stage as a node with its live events, config, numbers and requests, drawn by mote-ui inside mote-client from surface trees. **Verify:** screenshots of one node in all four tabs during a live turn.
- **MOTE-33** Any stage control (model, harness, endpoint, context size, threshold) changes hot through a settings event that the daemon validates fail-closed, applies on the next turn, and can revert from the event log. **Verify:** switch the think lane to OpenCode on a local model mid-session, complete a turn, revert, complete another; attempt a model over the memory gate and read the refusal.
- **MOTE-34** The request inspector lists every outbound subprocess and HTTP call with argv or URL, redacted payload, timing and its ledger row, and no secret ever appears in it. **Verify:** inspect a Slack read and a Claude turn; grep the stored inspector rows for the token patterns in mote-redact.
- **MOTE-35** Every stage has a latency budget, the view shows p50 and p95 against it, and opening the view adds no measurable latency to the lane it inspects. **Verify:** `moted bench talk` with the view closed and open differs by under 5 ms at p50.

### Always-on
- **MOTE-20** `moted install` registers a launchd agent so the daemon starts at login and survives window close. **Verify:** `launchctl list | grep mote` after a reboot.
- **MOTE-21** A cron watcher and a Slack mention watcher wake the router, and every outward action needs owner approval in the window. **Verify:** scripted watcher test plus a live Slack mention when the owner provides a token; without a token the watcher reports itself disabled.
