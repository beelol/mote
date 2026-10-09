# Reuse: work lane through Overseer

What mote reuses to hand coding work to `overseerd` and show it as cards. Covers MOTE-12 and
MOTE-13. Researched 2026-10-08 against Overseer `main` at `a7a5cac0`, read-only.

## Key points

1. The work lane is a client of `overseerd` over its Unix socket. mote starts a task, follows the run's events, answers permissions and interrupts. mote never runs a coding harness itself and never touches git.
2. The protocol is newline JSON. Requests are `{"id","method","params"}`. Replies are `{"id","result"}` or `{"id","error":{"code","message"}}`. Notifications are `event`, `replayed` and `resync`. Requests are capped at 1 MiB.
3. `mote-ipc::client::Client` is the reuse. It is an async port of Overseer's TUI client: connect, hello, request, subscribe after a cursor, drop events at or below the cursor, resubscribe on `resync`.
4. Five methods cover the lane: `hello`, `task.create`, `events.subscribe`, `run.permission`, `run.interrupt`. Four more build the completion card: `run.follow_up`, `workspace.diff`, `workspace.pr_plan`, `workspace.cleanup_plan`.
5. Every Overseer event of a followed run is mirrored into mote's log under the thread scope with `overseer_seq` in the payload. The progress card is a fold over those events.
6. Worktrees are Overseer's job. `task.create` with `workspace_mode: "worktree"` creates one under Overseer's data directory and returns its path.
7. When overseerd is down the connect fails fast. mote shows a card with a Start action, stores the request as `queued`, and retries `hello` every 5 s.
8. Backoff uses `backon`. The `backoff` crate is unmaintained (RUSTSEC-2025-0012).

## What the concept needs technically

Socket. `mote-paths` reproduces Overseer's path: `~/Library/Application Support/Overseer/run/overseerd.sock`, or `/tmp/overseer-<uid>/<hash>-overseerd.sock` when that path is 100 bytes or longer. `overseerd socket-path` prints it. The peer uid must equal the owner's uid or Overseer drops the connection.

Hello. Send `{"id":1,"method":"hello","params":{"client":"mote","version":"<mote version>"}}`. The reply holds `protocol` (1), `version`, `pid`, `data_dir` and `socket`. Never send `client` as `vscode` or `tui`. Overseer counts those as watching UIs and posts an OS notice when the last one disconnects.

Start a run:

```json
{"id":2,"method":"task.create","params":{"repo":"/Users/bilal/projects/testbox/monarch","harness":"claude","prompt":"Fix the flaky probe test in ...","title":"flaky probe test","workspace_mode":"worktree","effort":"high","permission_mode":"acceptEdits"}}
```

The reply is `{"task":{...},"run":{"id":"r-...","status":"queued",...},"workspace":{"id":"w-...","path":"<worktree>","branch":"..."}}`, with `launch_error` when the harness could not start. Run statuses are `queued`, `starting`, `running`, `waiting_for_user`, then `completed`, `failed` or `interrupted`.

Follow:

```json
{"id":3,"method":"events.subscribe","params":{"after":<cursor>,"run_id":"r-..."}}
```

The reply is `{"subscribed":true,"after":n,"history_truncated":bool}`. Then `{"method":"event","params":{seq,ts,task_id,run_id,kind,source,confidence,payload}}` lines follow, then `{"method":"replayed","params":{"cursor":n}}`, then live events. `{"method":"resync","params":{"cursor":n}}` means the bus lagged, so resubscribe from that cursor. Overseer keeps 5,000 events per run and writes a `retention` marker. `events.list {run_id, after, limit}` backfills and returns `oldest_retained`.

Event kinds to fold into the card, from `daemon.rs` `apply_norm`:

| kind | payload | card field |
| --- | --- | --- |
| `status` | `status`, `why` | status badge |
| `turn_started` | `turn`, `snapshot` | turn counter |
| `output` | `role`, `text` | last message, transcript |
| `tool` | `name`, `id`, `summary` | current tool |
| `tool_result` | `id`, `output`, `status`, `is_error` | tool finished |
| `file_activity` | `paths`, `kind` | files changed |
| `permission` | `request_id`, `tool`, `input` | approval block |
| `permission_answered` | `request_id`, `allow` | clears the block |
| `usage` | `usage`, `total_cost_usd`, `rate_limits` | tokens and cost |
| `error` | `class`, `message` | banner, `auth` opens sign-in |
| `turn_done` | `ok`, `summary` | completion |
| `interrupt_requested` | none | interrupting |
| `retention` | `events_truncated_through_seq` | "earlier output pruned" |
| `child` | `child`, `evidence` | subagent count |

Permission. `{"id":4,"method":"run.permission","params":{"run_id":"r-...","request_id":"req-1","allow":true,"message":""}}`. Overseer forwards the reply to the harness, sets `running`, and emits `permission_answered` and `status`. Interrupt is `run.interrupt {run_id}`. Follow-up is `run.follow_up {run_id, prompt}`.

Progress card. Title, harness and model, status, elapsed since `created_ms`, turn number, current tool and summary, last assistant line, changed file count and paths, tokens and cost, a pending permission with Allow and Deny, and actions Interrupt and Open diff. The completion card adds the summary, `workspace.diff` against the base from `comparison.options`, Follow up, Open PR (an outward action, so it needs owner approval) and Cleanup. The need-input card is the permission block or the sign-in block with Retry.

Cursor. mote stores `overseer.cursor.<run_id>` in its `meta` table in the same transaction as the mirrored event.

Reconnect. A connect error or EOF yields `Msg::Disconnected`. backon retries from 200 ms to 5 s with jitter. After hello, mote resubscribes from the stored cursor for every followed run that is still active.

Down path. `UnixStream::connect` fails with `ENOENT` or `ECONNREFUSED`. A stale socket file with a refused connect means overseerd died. The card says "Overseer is not running" with Start. Start spawns `overseerd serve` from the `OVERSEER_BIN` setting or `PATH`, in its own process group, with stdio to Overseer's own log, then polls hello for 10 s. The request waits in `work_queue(id, thread_id, params, status, created_ms)`.

## Candidates

| name | repo | language | license | last release | verdict | reason | pin |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Overseer `server.rs` and protocol | ~/projects/overseer | Rust | MIT | a7a5cac0 | port into `mote-ipc` | Framing, limits, error codes and subscribe semantics are proven by `protocol.rs`. | a7a5cac0 |
| Overseer `tui/src/client.rs` | ~/projects/overseer | Rust | MIT | a7a5cac0 | port the idea | Reconnect loop, cursor compare-exchange, drop at or below cursor, resubscribe on resync. | a7a5cac0 |
| tokio-util `LinesCodec` | github.com/tokio-rs/tokio | Rust | MIT | 0.7.19, 2026-07-21 | embed | Newline framing over `UnixStream` with no extra layer. | 0.7.19 |
| tokio-serde | github.com/carllerche/tokio-serde | Rust | MIT or Apache-2.0 | 0.9.0, 2024-02-04 | skip | Second framing layer, lightly maintained. | none |
| interprocess | github.com/kotauskas/interprocess | Rust | 0BSD or Apache-2.0 | 2.4.4, 2026-09-03 | skip | Only pays off on Windows. | none |
| jsonrpsee | github.com/paritytech/jsonrpsee | Rust | MIT | 0.26.1, 2026-09-30 | skip | HTTP and WebSocket first, brings tower and hyper. Overseer's protocol is not JSON-RPC 2.0. | none |
| backon | github.com/Xuanwo/backon | Rust | Apache-2.0 | 1.6.0, 2025-10-18 | embed | RustSec's named replacement for `backoff`. Async and sync. | 1.6.0 |
| backoff | github.com/ihrwein/backoff | Rust | MIT or Apache-2.0 | 0.4.0, 2021-12-14 | skip | RUSTSEC-2025-0012 unmaintained. | none |
| tokio-retry2 | github.com/naomijub/tokio-retry | Rust | MIT | 0.9.1, 2026-01-11 | skip | backon is enough. | none |
| gix | github.com/GitoxideLabs/gitoxide | Rust | MIT or Apache-2.0 | 0.89.0, 2026-10-08 | skip in v0 | `add_worktree` exists, but Overseer owns worktrees. | none |
| git2 | github.com/rust-lang/git2-rs | Rust | MIT or Apache-2.0 | 0.21.0, 2026-10-06 | skip | Same reason. | none |
| apalis-sqlite | github.com/geofmureithi/apalis | Rust | MIT | 1.0.0-rc.11, 2026-10-07 | skip | A `work_queue` table is enough for one daemon. | none |
| effectum | github.com/dimfeld/effectum | Rust | MIT or Apache-2.0 | 0.7.0, 2024-07-23 | skip | Same reason, and stale. | none |

## What to reuse from Overseer

- `server.rs` subscribe: subscribe to the bus before replaying, replay in batches of 1,000, send `replayed`, drop live events at or below the cursor, send `resync` on lag. The client mirrors every step.
- `tui/src/client.rs`: the 250 ms connect loop, `set_cursor_if_unset` from the first `state`, the `drop_connection` test hook, and the start-daemon hook that runs once per outage.
- `daemon/tests/common/mod.rs`: `Daemon::start` spawns a real `overseerd serve` with a temporary `OVERSEER_HOME` and harness paths pointed at `/nonexistent`. mote's work-lane tests start overseerd the same way.
- `fixtures/fake-harness/claude-fixture.js` modes `permission` and `slow` drive the permission and interrupt tests through a real overseerd.
- `daemon.rs` event kinds and statuses in the table above.

## Recommendation

```toml
# crates/mote-ipc/Cargo.toml
[dependencies]
anyhow = "1"
serde_json = "1"
tokio = { version = "1.53", features = ["net", "io-util", "sync", "time", "macros"] }
tokio-util = { version = "0.7.19", features = ["codec"] }
backon = "1.6.0"
libc = "0.2"
mote-eventlog = { path = "../mote-eventlog" }
mote-paths = { path = "../mote-paths" }
```

Manual checks against the owner's overseerd:

```bash
overseerd socket-path
overseerd ctl hello
overseerd ctl state | jq '.runs[] | {id, status, title}'
overseerd ctl events.subscribe '{"after":0,"run_id":"r-xxxx"}'
```

## Efficiency targets and what to measure

- Connect plus hello under 50 ms.
- Event to card update p50 under 100 ms, with card re-renders coalesced to 10 per second.
- Mirror throughput: a 12,000 output-line run (Overseer's ac10 burst) mirrored with no gap in under 15 s.
- Reconnect within 1 s after the socket returns.
- One socket connection per overseerd and one subscription per followed run.
- Fold state under 64 KiB per run. The transcript lives in the log, not in memory.
- `moted bench work` against a test overseerd prints these numbers.

## Open decisions with defaults

1. Mirror everything or summarize. Default: mirror every kind in the table and drop `child` text. The payload keeps `overseer_seq`.
2. Harness and profile. Default: `claude` with Overseer's system profile. `codex` on request.
3. Repo selection. Default: the delegate tool takes an absolute `repo`. mote checks the directory exists. Overseer validates the git toplevel.
4. Approval to start a run. Default: none. Work happens in a worktree. Opening a PR needs approval.
5. Starting overseerd. Default: spawn `overseerd serve` detached. `launchctl kickstart` only once Overseer has a launchd label for the daemon (unknown today, only its menubar app has one).
6. Permission mode. Default: `acceptEdits` so edits do not block. Bash still prompts and mote shows the card.
7. Diff rendering. Default: a card with changed paths and counts. A full diff view is a later UI item.
8. Cursor storage. Default: `meta` in mote's SQLite.
9. Protocol version. Default: refuse anything but 1 and show a card.
10. Truncated history. Default: show "earlier output pruned" with `oldest_retained`.

## Acceptance criteria

- **WORK-01** `mote-ipc::client::Client::connect` reaches overseerd at the `mote-paths` socket, sends hello with client `mote`, and refuses a protocol other than 1. **Verify:** `cargo test -p mote-ipc client_hello` against the crate's test server, plus the live `hello` reply in the evidence record.
- **WORK-02** The delegate tool calls `task.create` in worktree mode and stores task id, run id, workspace id and path in the thread's working set. **Verify:** a test overseerd with the generic harness, plus one live run.
- **WORK-03** Following subscribes after the stored cursor with a run filter, mirrors every event once with `overseer_seq`, and writes the cursor in the same transaction. **Verify:** `cargo test -p moted work::mirror` cuts the connection at ten points and compares seqs with `events.list`.
- **WORK-04** A `resync` or a dropped connection leads to reconnect with backoff and resubscribe from the cursor with no duplicate. **Verify:** the test kills and restarts the test overseerd mid-run and checks the mirrored seqs.
- **WORK-05** The progress card shows status, elapsed, harness, current tool, last output line, changed file count and usage, updated within 100 ms of the event. **Verify:** screenshot of the running state plus event timestamps in the log.
- **WORK-06** A `permission` event becomes a card, Allow or Deny calls `run.permission`, and `permission_answered` clears it. **Verify:** `claude-fixture.js` permission mode through a test overseerd for allow and deny.
- **WORK-07** Interrupt calls `run.interrupt` and the card ends `interrupted`. **Verify:** fixture `slow` mode through a test overseerd.
- **WORK-08** The completion card offers Open diff, Follow up, Open PR behind approval, and Cleanup. **Verify:** screenshots of running, need-input and completed cards (MOTE-13).
- **WORK-09** With no overseerd the delegate yields a "not running" card with Start, stores the request as `queued`, and starts it once hello succeeds. **Verify:** test with no socket, then start the test overseerd, then the queued run starts.
- **WORK-10** Start spawns overseerd in its own process group and the child outlives moted. **Verify:** test with a fake `overseerd` script, kill moted with SIGKILL, the fake still answers.
- **WORK-11** mote never creates worktrees itself. **Verify:** `grep -rn "worktree add" crates/ moted/` prints nothing outside fixtures, and the live run's path sits under Overseer's `worktrees` directory.
- **WORK-12** A 12,000-line run mirrors with no gap in under 15 s at no more than 10 card renders per second. **Verify:** `moted bench work` output in the evidence record.

## Sources

- Overseer `daemon/src/server.rs`, `daemon.rs`, `paths.rs`, `tui/src/client.rs`, `daemon/tests/common/mod.rs`, `daemon/tests/protocol.rs` at `a7a5cac0` (local, read-only)
- tokio-util codec: https://docs.rs/tokio-util/latest/tokio_util/codec/index.html
- tokio UnixStream: https://docs.rs/tokio/latest/tokio/net/struct.UnixStream.html
- tokio process Command: https://docs.rs/tokio/latest/tokio/process/struct.Command.html
- interprocess: https://crates.io/crates/interprocess
- jsonrpsee: https://github.com/paritytech/jsonrpsee
- backon: https://crates.io/crates/backon
- RUSTSEC-2025-0012: https://rustsec.org/advisories/RUSTSEC-2025-0012
- tokio-retry2: https://crates.io/crates/tokio-retry2
- SSE Last-Event-ID: https://html.spec.whatwg.org/multipage/server-sent-events.html
- Reconnect-safe streams for coding agents: https://hackernoon.com/designing-reconnect-safe-event-streams-for-remote-coding-agents
- Resume tokens: https://ably.com/blog/resume-tokens-last-event-id-llm-streaming-reconnection
- gix worktrees: https://github.com/GitoxideLabs/gitoxide/blob/main/crate-status.md
- git2: https://crates.io/crates/git2
- Copilot coding agent sessions: https://docs.github.com/en/copilot/how-tos/use-copilot-agents/coding-agent/track-copilot-sessions
- Claude Code on the web: https://code.claude.com/docs/en/claude-code-on-the-web
- Cursor cloud agents: https://cursor.com/docs/cloud-agent
- launchctl kickstart: https://keith.github.io/xcode-man-pages/launchctl.1.html
- apalis-sqlite: https://crates.io/crates/apalis-sqlite
- effectum: https://crates.io/crates/effectum
