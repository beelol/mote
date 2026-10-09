# RFC: Shared crates from Overseer

Status: proposed on 2026-10-08 from a read-only audit of Overseer. Nothing in Overseer was
changed. Acceptance criteria: CRATE-01 to CRATE-17 below. The root README holds the checkbox
CRATE-00 (the owner approves this RFC and its extraction order). The product RFC is
[mote](mote.md).

## Status and scope

Source revisions audited:

| Source | Revision | Date | What it holds |
| --- | --- | --- | --- |
| `~/projects/overseer`, branch `main` | `a7a5cac0` | 2026-09-26 | `daemon/src` (15 files, 5,216 lines), `tui/src` (3,683 lines), `daemon/tests/protocol.rs` (1,598 lines), the RFCs. |
| `~/projects/overseer`, branch `claude/phone-remote-vscode-control-b48a34` | `1488b4be` | 2026-09-28 | The same daemon plus `daemon/src/gateway/` (11 files, 2,630 lines), Continuity (`sys.rs`, `local.rs`, `net.rs`, `continuity.rs`, `handoff.rs`, `downloads.rs`, `ollama_install.rs`, `opencode_bridge.rs` and `local_catalogue.json`, 5,439 lines), `protocol/protocol.json`, `protocol/vectors/noise.json`, and their tests. 24,415 daemon lines in all. |

The gateway and the local-model memory guard are not on `main`. They live on branches. The branch
above is the earliest one that has both. Later branches (`claude/phone-parity` at `f686acc8`,
`codex/continuity-ui-readiness` at `a9c10362`) add swarm, auto routing, voice and an `overseer/`
agent module. Those are out of scope for this RFC.

In scope: the nine crates below, the code they take from Overseer, the order to extract them, and
the plan a later driver follows to put Overseer on the same crates. Out of scope: mote's own lanes,
stores and UI (see [mote](mote.md)), and any change to Overseer before its driver starts.

The crates live in this repository under `crates/`. mote consumes them by path. Overseer consumes
them by path until mote has a remote, then by git dependency pinned to a revision.

## Why

Key points first:

1. Nine crates come out of Overseer: redact, paths, usage, eventlog, ipc, harness, accounts, localmodel, gateway.
2. Most of the source is pure functions over JSON values. The cuts are small except in `daemon.rs` and the gateway.
3. Overseer's gateway, Noise protocol and memory guard exist only on its phone-remote branch, not on main. The audit read them there.
4. Extraction order: redact, paths, usage, eventlog, ipc, harness, accounts, localmodel, then gateway. mote's first criteria need only redact, paths, eventlog and ipc.
5. Overseer migrates in 11 driver steps, each proved by its existing tests.
6. Overseer's manifests say MIT but its root has no LICENSE file. CRATE-17 adds licenses and derivation notices here.
7. Criteria CRATE-01 to CRATE-17 are at the end. The README board holds the checkboxes.


mote needs eight things Overseer already has and has verified: an append-only event log with
cursor replay, an owner-only Unix socket server with the same newline JSON protocol, a WebSocket
gateway with a Noise handshake, harness launch and stream parsing for Claude Code, Codex and
OpenCode, account profiles with per-harness credential folders, secret redaction, usage parsing,
and a memory guard that never loads a local model the machine cannot hold.

Copying the code would fork it. Two daemons would then fix the same parser bug twice. Shared
crates keep one copy. mote takes them now. Overseer moves onto them later, one module at a time,
with its own tests as the proof.

The audit found that most of this code is already written as pure functions over `serde_json`
values with Overseer's own types only at the edges. The cuts are small. The two large modules,
`daemon.rs` and `store.rs`, stay in Overseer except for the parts named below.

## Module audit

Coupling says how much the module knows about Overseer's own concepts: tasks, runs, workspaces,
worktrees, snapshots, VS Code windows, the `OVERSEER_*` environment names. Verdicts: `as-is`
(moves with a rename at most), `after a cut` (moves once the named parts are removed or made
parameters), `overseer-only` (stays).

### `daemon/src` on `main`

| Module | Lines | Responsibility | Depends on | Depended on by | Coupling | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| `redact.rs` | 53 | Regex redaction of API keys, JWTs, bearer tokens, token fields, GitHub and Slack tokens. | `regex` | `daemon.rs` (`emit`, `apply_norm`, `raw_output`, `logout`, `profile_status`) | None. `redact_value` and `fingerprint` live in `daemon.rs` but belong here. | as-is |
| `paths.rs` | 82 | Data, runtime, socket, db, runs, worktrees, profiles and log paths. `OVERSEER_HOME` and `OVERSEER_SOCKET`. XDG on Linux. 0700 directories. Short-socket fallback under `/tmp/overseer-<uid>` keyed by a fingerprint of the data dir. | `libc`, `crate::daemon::fingerprint` | Every module | App name, env prefix, and the `runs`, `worktrees`, `profiles` subdirectories. | after a cut |
| `accounts.rs` | 92 | Provider to harness map. `account_list`, `account_create`, `account_remove` on `Daemon`. | `daemon`, `store`, `paths`, `adapters` | `server.rs` dispatch | `account_list` reads runs for "last used" and "active". `account_remove` refuses while runs are active. The map and the folder confinement check are generic. | after a cut |
| `main.rs` | 108 | `overseerd serve`, `ctl`, `shim`, `socket-path`, `version`. `log()`. `single_instance` flock. Signal setup. | everything | none | The binary. | overseer-only. The flock and log patterns are ten lines each and are rewritten in `moted`. |
| `pr.rs` | 129 | GitHub pull request plan, prepare, record. | `daemon`, `git`, `merge` | `server.rs` | Workspaces, tasks, worktree branches. | overseer-only |
| `usage.rs` | 136 | `codex_limits` reads the newest Codex rollout log for rate limits. `claude_limits` normalizes a `rate_limit_event`. `account_usage` on `Daemon` joins events to runs by profile. | `daemon`, `store` | `server.rs` | Only `account_usage` (the SQL join on `runs`). The two parsers and `window_label` are pure. | after a cut |
| `files.rs` | 151 | Repo file search for mentions, changes since task start, workspace tree. | `daemon`, `git` | `server.rs` | Workspaces, snapshots. | overseer-only |
| `background.rs` | 216 | Counts VS Code and TUI windows, posts an OS notice when the last closes with agents running, `stop_all`. `notify()` runs the bundled notifier app, `osascript`, or `notify-send`. | `daemon`, `shim`, `store` | `server.rs` | VS Code semantics, the notifier app, the `vscode://` URL. | overseer-only. `notify()` (50 lines) is a later `mote-notify` candidate. |
| `merge.rs` | 240 | Merge back: plan, prepare, resolved, complete, abort. Conflict handoff to the run. | `daemon`, `git` | `server.rs`, `pr.rs` | Entirely worktrees and tasks. | overseer-only |
| `git.rs` | 296 | Git plumbing with argv only. Snapshots through a private index and hidden `refs/overseer/` refs. Porcelain v2 status parsing. Worktree add and remove with `overseer/` branch names. | `anyhow`, `uuid` | `daemon`, `files`, `merge`, `pr` | `refs/overseer/`, `overseer.integrationBranch`, the `overseer/` branch prefix, the snapshot identity. | overseer-only for now. mote delegates coding to overseerd. A later `mote-git` is possible after a prefix parameter. |
| `shim.rs` | 305 | Per-run supervisor: reads `launch.json`, spawns the harness with a cleared environment in its own process group, records stdout and stderr into 8 MiB JSON segment files, serves a private control socket (`ping`, `stdin`, `close_stdin`, `signal`), writes `shim.json` and `exit.json`. `peer_uid` for macOS and Linux. | `libc`, `serde` | `daemon.rs`, `server.rs` (`peer_uid_fd`), `background.rs` | None. The entry point is `overseerd shim <dir>`. | as-is |
| `server.rs` | 319 | Owner-only Unix socket. Newline JSON. Peer uid check. 1 MiB request limit. `parse_error`, `invalid_request`, `invalid_params`, `request_too_large` codes. `events.subscribe` replays from the store after a cursor, then streams the broadcast, and sends `resync` on lag. A dispatch table of 46 methods plus `events.subscribe`. `hello` counts UI clients. | `daemon`, `paths`, `shim::peer_uid_fd`, `tokio` | `main.rs` | The dispatch table, `ui_connected` on `hello`, `daemon.shutdown` exiting the process, `daemon.store` and `daemon.events` for replay. | after a cut |
| `store.rs` | 567 | SQLite with WAL. Tables `meta`, `workspaces`, `tasks`, `runs`, `turns`, `snapshots`, `profiles`, `events`. `events_after`, `max_seq`, `oldest_retained`, `prune_run_events`, `search`. | `rusqlite` | everything | Every table except `events`, `meta` and `profiles` is Overseer's. `events` has `task_id` and `run_id` columns. | split: `events` and `meta` to `mote-eventlog`, `profiles` to `mote-accounts`, the rest stays |
| `adapters.rs` | 951 | `Norm` (the normalized event enum). `launch` builds argv, stdin and env for `codex`, `codex-app`, `claude`, `opencode`, `generic`. `base_env` allowlist and PATH. `forbidden_env`. `resolve_program`, `version_of`, `capabilities`. `check_turn_options`. `interrupt_plan`, `follow_up_via_stdin`, `permission_reply`. `classify_error`. `parse` for all four harnesses. `scope_codex_app_child`. `opencode_store_children` reads OpenCode's SQLite read-only. `PARSER_VERSION`. | `serde_json`, `base64`, `uuid`, `rusqlite`, `crate::paths::data_dir` (for `neutral_dir`) | `daemon.rs`, `accounts.rs`, `server.rs` | `OVERSEER_<HARNESS>_PATH`, `OVERSEER_HARNESS_ENV_PASSTHROUGH`, `clientInfo.name = "overseer"`, interrupt ids `overseer-int-`, the deny message "Denied by user in Overseer", `neutral_dir` under the data dir. | after a cut |
| `daemon.rs` | 1,571 | `Daemon` struct. Profiles: `ensure_system_profiles`, `create_profile`, `profile_env`, `login_command`, `logout`, `profile_status`, `codex_identity`. Snapshots. `create_task`, `start_turn`, `spawn_process` (writes `launch.json`, spawns `overseerd shim` with `setsid`), `interrupt` with escalation timers, `answer_permission`. `tail_loop` reads segments, `apply_lines` and `apply_norm` turn `Norm` into events and run state, `TailState`, `finalize`, `mark_ended`, `reconcile`. Comparisons, archive, search, diff, cleanup, `state`, `raw_output`. `redact_value`, `fingerprint`, `strip_ansi`, `run_with_env`. | all | `server.rs` and every `impl Daemon` module | High. Tasks, workspaces, snapshots, the run tree with native children and delayed parents, workspace ownership. | mostly overseer-only. The parts named in the crate sections move: profile functions, the supervisor spawn and segment tail, exit classification, the generic half of `apply_norm` as a tracker, `redact_value`, `fingerprint`. |

### `daemon/src` on the branch only

| Module | Lines | Responsibility | Coupling | Verdict |
| --- | --- | --- | --- | --- |
| `gateway/noise.rs` | 287 | Noise IK and IKpsk1 with `snow`. Prologue, frame kinds, `seal` and `Opener` chunking at 65,000 bytes, key fingerprints, hex. Shared test vectors. | The prologue text `overseer-gateway-v1`. | as-is, prologue becomes a parameter |
| `gateway/base32.rs` | 61 | RFC 4648 base32 without padding. | None. | as-is |
| `gateway/net.rs` | 127 | Loopback, private and link-local ranges. `Cidr`. `local_addresses` via `getifaddrs`. `host_name`. | None. | as-is |
| `gateway/pairing.rs` | 110 | One secret, one use, two minutes, five failures. The `OVSR1-` pairing code. | The `OVSR1` code prefix and `OVERSEER_TEST_*` env names. | as-is, prefix becomes a parameter |
| `gateway/classes.rs` | 217 | Method classes `self`, `read`, `control`, `mac_only` from `protocol/protocol.json`. `allowed(method, scope)`. `undeclared_params`. | `include_str!("../../../protocol/protocol.json")` and a test that greps Overseer's dispatch tables. | after a cut: the table is passed in |
| `gateway/devices.rs` | 167 | Tables `devices` and `remote_requests`. Counter replay check. `request_claim` with `New`, `Done`, `Interrupted`. | `impl Store`. | after a cut: a `DeviceStore` trait with a SQLite impl |
| `gateway/mod.rs` | 878 | `Gateway` state, identity key file (0600), `enable`, `disable`, listener on `0.0.0.0:47810`, WebSocket upgrade on `/v1` only, first frame, session and pairing handshakes, revoked notice, session loop with idle timeout, `once` (exactly-once per device and request id), mDNS via `dns-sd` or `avahi-publish`, the fuzz test. | `Daemon`, `d.store`, `d.emit`, `crate::server::PROTOCOL_VERSION`, `focus` map for Mac windows, `power::watch`, `push::watch`. | after a cut: a `GatewayHost` trait |
| `gateway/remote.rs` | 206 | Per-request class and scope check, undeclared parameter refusal, `request_id` rule for control methods, `remote_command` event, `with_actor`, the device's own methods (`hello`, `ping`, `device.notifications`). | `server::dispatch`, `server::subscribe`, `ProtoError`, `d.repo_is_known` in `guard`. | after a cut: `guard` becomes a host hook |
| `gateway/local.rs` | 161 | Mac-only management: `gateway.status`, `enable`, `disable`, `settings`, `pair_start`, `pair_cancel`, `pair_confirm`, `devices`, `device_revoke`, `device_scope`, `device_rename`. | `Daemon`, `power.is_held`. | after a cut |
| `gateway/power.rs` | 143 | macOS `IOPMAssertion` while runs are active. Linux `systemd-inhibit`. | `ACTIVE` run statuses. | overseer-only now. A later `power` feature. |
| `gateway/push.rs` | 273 | APNs payloads and delivery for needs-you events. | Run, task and event shapes. | overseer-only |
| `sys.rs` | 217 | `Memory { total, available, pressure, level, source }`. macOS `hw.memsize`, `host_statistics64`, `kern.memorystatus_*`. Linux `/proc/meminfo`, `/proc/pressure/memory`. `disk_free` via `statvfs`. `OVERSEER_TEST_MEMORY` fixture file. | The env name. | as-is |
| `local.rs` | 1,039 | Ollama over loopback only. `/api/version`, `/api/tags`, `/api/show`, `/api/ps`, `/api/generate`. `Geometry` and the KV cache formula. `Inventory`. Catalogue from `local_catalogue.json` with a local override. `budget`, `estimate`, `pick`, `context_steps`, `derived_tag`, `ensure_tag`, `approve_with`, `must_stop_load`, `load_guarded`, `unload`. `measured` and `loaded_by_overseer` tables. | `crate::paths::data_dir`, `crate::ollama_install::own_program`, `OVERSEER_OLLAMA_URL`, `OVERSEER_OLLAMA_CANDIDATES`, `OVERSEER_TEST_DISK_FREE`, `OVERSEER_TEST_WATCH_MS`, two SQLite tables on `Store`. | after a cut |
| `net.rs` | 407 | System connectivity answer (SystemConfiguration, NetworkManager, default route) and credential-free TLS probes. | `OVERSEER_*` test env. | Not in this set. A later `mote-net` candidate for mote's Offline journey. |
| `continuity.rs`, `handoff.rs`, `downloads.rs`, `ollama_install.rs`, `opencode_bridge.rs` | 3,696 | Connection policy, handoff between runs, model pulls, Ollama install, the `opencode serve` bridge. | Runs, turns, workspaces, settings in `meta`. | overseer-only |

### `tui/src` and the protocol

| Module | Lines | Responsibility | Verdict |
| --- | --- | --- | --- |
| `client.rs` | 237 | Newline JSON client over a Unix socket on a reader thread. `hello` as client `tui`. `events.subscribe` after the cursor on every reconnect. Drops events at or below the cursor. Resubscribes on `resync`. Starts the daemon once per outage. `Requests` trait for fakes. | after a cut: the daemon-spawn step is a callback |
| `model.rs` | 140 | Typed subset of `state`. | overseer-only |
| `feed.rs` | 462 | Events into display items keyed by `seq`. Keeps `tokens_in`, `tokens_out`, `cost_usd` totals from usage payloads (the parsing code past line 150 was not read, unknown). | overseer-only. The usage normalization belongs in `mote-usage`. |
| `locate.rs` | 117 | Finds `overseerd`, asks it for `socket-path`, spawns `serve` detached. | overseer-only |
| `daemon/tests/common/mod.rs` | 249 | Black-box daemon harness: isolated `OVERSEER_HOME`, harness overrides pointed at `/nonexistent`, `call`, `raw`, `wait_done`, `events`, real git repos. | The pattern is reused in mote's own tests. |
| `daemon/tests/common/phone.rs` (branch) | 354 | A reference phone: pairing, session, `ask`, `act`, `reply_to`, a byte tap for the packet capture test. | carried into `mote-gateway` tests |
| `protocol/protocol.json` (branch) | 161 methods: 90 `mac_only`, 39 `read`, 28 `control`, 4 `self` | Method classes, parameter shapes, result and event shapes. | The shape and the checker move. The table is Overseer's. mote writes its own. |

## Crate proposal

Nine crates. The starting set in the brief is confirmed. No crate is added and none is removed.
`mote-paths` is consumed by binaries only. Every other crate takes directories as arguments, so it
has no dependency on `mote-paths`.

Dependency graph, as a list, each line names what the crate depends on inside this repository:

```
mote-redact       (nothing)
mote-paths        (nothing)
mote-usage        (nothing; feature sqlite adds rusqlite)
mote-localmodel   (nothing; feature sqlite adds rusqlite)
mote-eventlog     mote-redact
mote-harness      mote-redact
mote-accounts     mote-harness
mote-ipc          mote-eventlog
mote-gateway      mote-ipc, mote-eventlog
```

External dependencies carried from Overseer at the versions it pins: `tokio 1`, `serde 1`,
`serde_json 1`, `rusqlite 0.32` (bundled), `anyhow 1`, `uuid 1`, `libc 0.2`, `regex 1`,
`base64 0.22`, `sha2 0.10`, `snow 0.10`, `tokio-tungstenite 0.30`, `futures-util 0.3`,
`getrandom 0.3`, `ureq 2`. Dev: `tempfile 3`.

Every crate is `edition = "2021"`, `license = "MIT"`, `publish = false`.

### `mote-redact`

Takes `redact.rs` whole and `redact_value` and `fingerprint` from `daemon.rs`.

```rust
/// Replaces API keys, JWTs, bearer tokens, token-like JSON fields, GitHub and Slack tokens.
pub fn redact(text: &str) -> String;
/// Walks a JSON value: strings are redacted, and keys named token, access_token, refresh_token,
/// id_token, oauth_token, api_key, apikey, authorization, password, secret, client_secret or
/// cookie have their value replaced whole.
pub fn redact_value(value: serde_json::Value) -> serde_json::Value;
/// The first 16 hex characters of SHA-256. An identity without the secret.
pub fn fingerprint(value: &str) -> String;
```

Must change: nothing in the patterns. `fingerprint` moves in from `daemon.rs` line 1533.
`redact_value` moves in from `daemon.rs` line 1516.

Tests to carry: `redacts_common_secrets` (`redact.rs`). Add one test for `redact_value` key
replacement. The protocol test `ac10_redaction_of_secrets_in_output` stays in Overseer as proof of
the integration.

Risk: none. 60 lines, no state.

### `mote-paths`

Takes `paths.rs` with the app identity as a value instead of literals.

```rust
pub struct AppId {
    /// "Overseer" or "mote": the macOS Application Support folder name.
    pub app_dir: &'static str,
    /// "overseer" or "mote": the XDG folder name and the /tmp fallback prefix.
    pub slug: &'static str,
    /// "OVERSEER" or "MOTE": the prefix of <PREFIX>_HOME and <PREFIX>_SOCKET.
    pub env_prefix: &'static str,
    pub socket_name: &'static str,
    pub db_name: &'static str,
    pub log_name: &'static str,
}

pub struct Paths { id: AppId }

impl Paths {
    pub const fn new(id: AppId) -> Self;
    pub fn data_dir(&self) -> PathBuf;
    pub fn runtime_dir(&self) -> PathBuf;
    /// Under 100 bytes, or a per-user /tmp directory keyed by a fingerprint of the data dir.
    pub fn short_socket(&self, name: &str) -> PathBuf;
    pub fn socket_path(&self) -> PathBuf;
    pub fn db_path(&self) -> PathBuf;
    pub fn log_path(&self) -> PathBuf;
    /// data_dir().join(name). Overseer uses it for runs, worktrees and profiles.
    pub fn sub(&self, name: &str) -> PathBuf;
}

pub fn ensure_private_dir(path: &Path) -> std::io::Result<()>;
```

Must change: `runs_dir`, `worktrees_dir`, `profiles_dir` become `sub("runs")` and so on in
Overseer. The fingerprint comes from `sha2` directly, so this crate does not depend on
`mote-redact`.

Tests to carry: none exist as unit tests. Add: `OVERSEER_HOME` override, XDG on Linux, the short
socket fallback with a 120-byte home, 0700 on `ensure_private_dir`. The protocol test
`ac08_socket_is_owner_only_and_requests_are_not_shell` stays in Overseer.

Risk: low. The socket path must come out byte for byte the same for Overseer, or every running
client loses the daemon. CRATE-03 checks it.

### `mote-eventlog`

Takes the `events` and `meta` parts of `store.rs` (lines 498 to 566 and the `meta` statements),
the `broadcast` channel from `Daemon`, and the replay logic that `server.rs` `subscribe` needs.

The record is generic. Overseer scopes an event by run and groups it by task. mote scopes by
thread and has no group. Both need one scope id to filter, replay and prune by.

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Event {
    pub seq: i64,
    pub ts: i64,
    /// Replay, filter and retention key. A run in Overseer, a thread in mote.
    #[cfg_attr(feature = "overseer-wire", serde(rename = "run_id"))]
    pub scope_id: Option<String>,
    /// A task in Overseer. None in mote.
    #[cfg_attr(feature = "overseer-wire", serde(rename = "task_id"))]
    pub group_id: Option<String>,
    pub kind: String,
    pub source: String,
    pub confidence: String,
    pub payload: Value,
}

/// Functions over a borrowed connection, so a consumer can append inside its own transaction.
pub fn migrate(conn: &Connection) -> Result<()>;
/// Redacts the payload with mote_redact::redact_value before the insert.
pub fn append(conn: &Connection, ts: i64, scope: Option<&str>, group: Option<&str>, kind: &str, source: &str, confidence: &str, payload: &Value) -> Result<Event>;
pub fn after(conn: &Connection, cursor: i64, scope: Option<&str>, limit: i64) -> Result<Vec<Event>>;
pub fn max_seq(conn: &Connection) -> Result<i64>;
pub fn min_seq(conn: &Connection) -> Result<Option<i64>>;
pub fn oldest_retained(conn: &Connection, scope: &str) -> Result<Option<i64>>;
/// Deletes a scope's oldest events past `keep`, never kind "retention". Returns the highest deleted seq.
pub fn prune_scope(conn: &Connection, scope: &str, keep: i64) -> Result<Option<i64>>;
pub fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>>;
pub fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<()>;

/// Live fan-out. Capacity 4096 in Overseer.
pub struct Bus(broadcast::Sender<Event>);
impl Bus {
    pub fn new(capacity: usize) -> Self;
    pub fn publish(&self, event: Event);
    pub fn subscribe(&self) -> broadcast::Receiver<Event>;
}

/// What a replaying subscriber needs. mote-ipc and mote-gateway take this.
pub trait EventSource: Send + Sync {
    fn events_after(&self, cursor: i64, scope: Option<&str>, limit: i64) -> Result<Vec<Event>>;
    fn min_seq(&self) -> Option<i64>;
    fn subscribe(&self) -> broadcast::Receiver<Event>;
}

/// An owning log for consumers without their own Store. One connection, one bus.
pub struct EventLog { conn: Mutex<Connection>, bus: Bus }
impl EventLog {
    pub fn open(path: &Path) -> Result<Self>;
    pub fn emit(&self, scope: Option<&str>, group: Option<&str>, kind: &str, source: &str, confidence: &str, payload: Value) -> Result<Event>;
    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T>;
}
impl EventSource for EventLog { ... }
```

Must change: column names `task_id` and `run_id` become `group_id` and `scope_id`. The
`overseer-wire` feature keeps the wire names Overseer's TUI, extension and phone app read. The
`retention` marker, the 5,000 per scope default and the `events_run` index stay. `Event` gains
`Deserialize`.

Tests to carry: none are unit tests today. Port from `protocol.rs`:
`ac10_replay_cursor_burst_and_retention` as an in-process test over `EventLog` (12,000 appends,
prune, replay from a mid cursor, no gaps, no duplicates) and a lag test with a 1-slot bus that
expects `min_seq` and a resync.

Risk: medium. The column rename is a schema migration on Overseer's live database. The migration
runs `ALTER TABLE events RENAME COLUMN` once when the old names exist. SQLite has supported that
since 3.25 and rusqlite 0.32 bundles a newer one.

### `mote-ipc`

Takes the accept loop, framing, limits, error codes, `subscribe` and the actor thread-local from
`server.rs`, `peer_uid` from `shim.rs`, `ProtoError` and `error_value` from the branch's
`server.rs`, and the TUI client from `tui/src/client.rs`. The dispatch table stays with each
daemon.

```rust
pub const MAX_REQUEST_BYTES: u64 = 1024 * 1024;

pub struct ProtoError { pub code: &'static str, pub message: String, pub data: Value }
impl ProtoError { pub fn new(code: &'static str, message: impl Into<String>) -> Self; pub fn with_data(self, data: Value) -> Self; }
/// {"code","message"} or {"code","message","data"}; a plain anyhow error becomes code "failed".
pub fn error_value(e: &anyhow::Error) -> Value;

pub type ConnId = u64;

pub trait Handler: Send + Sync + 'static {
    /// Runs on a blocking thread. One request at a time per call, many in flight per connection.
    fn dispatch(&self, method: &str, params: &Value) -> Result<Value>;
    /// A request that needs the connection, for example a per-window focus. Default: dispatch.
    fn dispatch_on(&self, conn: ConnId, method: &str, params: &Value) -> Result<Value> { self.dispatch(method, params) }
    /// Called when a connection sends "hello", with its params. Overseer counts UI clients here.
    fn hello(&self, _conn: ConnId, _params: &Value) {}
    fn disconnected(&self, _conn: ConnId) {}
    /// After the reply to this method the process exits. Overseer: daemon.shutdown, daemon.stop_all.
    fn exits_after(&self, _method: &str) -> bool { false }
}

pub struct ServerConfig {
    pub socket: PathBuf,
    /// Test only: a peer must still be this process's uid, and also this one. Can only refuse more.
    pub expect_uid: Option<u32>,
    pub log: Arc<dyn Fn(&str) + Send + Sync>,
}

/// Binds the socket at 0600 inside a 0700 directory, refuses another listener, checks the peer uid,
/// then serves until the process exits.
pub async fn serve(cfg: ServerConfig, handler: Arc<dyn Handler>, events: Arc<dyn EventSource>) -> Result<()>;
/// Replay after the cursor, "replayed", then live. "resync" on lag. Public so the gateway reuses it.
pub fn subscribe(events: Arc<dyn EventSource>, id: Value, params: Value, tx: mpsc::Sender<Value>);
pub fn peer_uid(fd: RawFd) -> Option<u32>;
/// Who is acting on this thread: None is the local user, Some("phone:<name>") a device.
pub fn with_actor<T>(actor: Option<String>, f: impl FnOnce() -> T) -> T;
pub fn actor() -> Option<String>;

pub mod client {
    pub enum Msg { Connected, Disconnected(String), Event(Event), Replayed, Reply { id: u64, result: Result<Value, RpcError> } }
    pub struct RpcError { pub code: String, pub message: String, pub data: Value }
    /// Async client for a tokio daemon that talks to another daemon (mote to overseerd).
    pub struct Client;
    impl Client {
        pub async fn connect(socket: &Path, hello: Value) -> Result<Self>;
        pub async fn request(&self, method: &str, params: Value) -> Result<Value, RpcError>;
        /// Subscribes after the cursor; replays and live events arrive on the receiver, never twice.
        pub fn subscribe(&self, after: i64, scope: Option<&str>) -> mpsc::Receiver<Msg>;
        pub fn cursor(&self) -> i64;
    }
}

#[cfg(feature = "blocking")]
pub mod blocking {
    /// The TUI client as it is: a reader thread, std::sync::mpsc, reconnects, resubscribes on resync.
    pub trait Requests: Send + Sync { fn request(&self, method: &str, params: Value) -> u64; fn connected(&self) -> bool; fn set_cursor_if_unset(&self, cursor: i64); fn subscribe(&self); fn set_stopped(&self, _stopped: bool) {} }
    pub struct Client;
    impl Client {
        /// `spawn` runs once per outage when the socket is gone (the TUI starts overseerd here).
        pub fn start(socket: PathBuf, hello: Value, spawn: Option<Box<dyn Fn() + Send + Sync>>, tx: Sender<Msg>) -> Client;
        pub fn cursor(&self) -> i64;
        pub fn drop_connection(&self);
        pub fn close(&self);
    }
    impl Requests for Client { ... }
}
```

Must change: `hello` no longer reports `data_dir`, `socket`, `pid` or `version` from the crate.
The consumer's `Handler::dispatch("hello")` builds that reply. The server intercepts `hello`
only to call `Handler::hello`. `events.subscribe` is intercepted and served by the crate. The
`OVERSEER_TEST_EXPECT_UID` env read moves to the consumer. `Msg::Event` carries an `Event`, not
a `Value`.

Tests to carry: port `ac08_socket_is_owner_only_and_requests_are_not_shell` (permissions and the
four error codes) and `ac08_connections_from_a_foreign_uid_are_rejected_and_logged` against a
test `Handler`. Add a reconnect test against the async client. The TUI's `tests/live.rs` may already cover the blocking client. Its content was not read in this audit, unknown. The
shell-injection half of `ac08` stays in Overseer with `task.create`.

Risk: medium. `subscribe` holds a store lock while reading a batch of 1,000. With `EventSource`
that lock lives in the consumer. The gateway's `session` also sends through the same
`mpsc::Sender<Value>`, so the channel type must stay `Value`.

### `mote-harness`

Takes `adapters.rs` and `shim.rs` whole, and from `daemon.rs`: `spawn_process` without the
store writes, the segment reading half of `tail_loop`, `supervisor_alive`, `pid_alive`,
`control_socket_path`, `enforce_raw_retention`, `finalize` as a pure classification,
`TailState` and the harness-generic half of `apply_norm` as a tracker, `run_with_env`,
`strip_ansi`.

```rust
pub const PARSER_VERSION: &str = "2026-09-24.1";

pub struct HarnessConfig {
    /// "OVERSEER" or "MOTE": <PREFIX>_<HARNESS>_PATH and <PREFIX>_HARNESS_ENV_PASSTHROUGH.
    pub env_prefix: String,
    /// Sent to Codex app-server as clientInfo. "overseer" today.
    pub client_name: String,
    pub client_version: String,
    /// Prefix of interrupt request ids. "overseer-int" today.
    pub request_prefix: String,
    /// The default deny message for a permission reply.
    pub deny_message: String,
    /// Probes (--version, login status) run here, never in a user's repository.
    pub neutral_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Norm {
    Session(String), Running,
    Text { role: String, text: String },
    /// New: a streamed delta from Claude's stream_event. Never also emitted as Text.
    Partial { text: String },
    Tool { name: String, id: Option<String>, summary: String },
    ToolDetail { id: String, input: Option<Value>, output: Option<String>, status: Option<String>, is_error: bool },
    FileChange { paths: Vec<String>, kind: String, confidence: &'static str },
    Child { native_id: String, parent_native: Option<String>, title: Option<String>, status: Option<String>, text: Option<String>, only_if_known: bool, evidence: String },
    Usage(Value),
    Permission { request_id: String, tool: String, input: Value },
    Error { class: String, message: String },
    TurnDone { ok: bool, summary: Option<String> },
    RpcResult { id: String, result: Value, error: Option<Value> },
    Send(String), TurnId(String),
    BackgroundTasks(usize), BackgroundLaunched(String), BackgroundNotified(String),
    Ignored, Unparsed(String),
}

pub struct LaunchReq<'a> {
    pub cwd: &'a Path, pub prompt: &'a str, pub model: Option<&'a str>,
    pub profile_env: BTreeMap<String, String>,
    pub resume_session: Option<&'a str>, pub program_override: Option<&'a str>, pub args_override: Option<&'a [String]>,
    pub extra_args: &'a [String], pub effort: Option<&'a str>, pub permission_mode: Option<&'a str>,
    pub images: &'a [(String, PathBuf)],
    /// New, Claude only. mote passes these; Overseer passes the defaults.
    pub partial_messages: bool, pub bare: bool, pub json_schema: Option<&'a str>,
    pub mcp_config: Option<&'a Path>, pub allowed_tools: &'a [String],
}
pub struct Launch { pub program: String, pub args: Vec<String>, pub env: BTreeMap<String, String>, pub initial_stdin: Option<String>, pub close_stdin: bool }
pub enum InterruptPlan { Signal, StdinThenSignal(String) }

pub fn base_env(cfg: &HarnessConfig, program: &str) -> BTreeMap<String, String>;
pub fn forbidden_env(key: &str) -> bool;
pub fn resolve_program(cfg: &HarnessConfig, harness: &str) -> Option<PathBuf>;
pub fn version_of(cfg: &HarnessConfig, program: &Path) -> Option<String>;
pub fn capabilities(harness: &str) -> Value;
pub fn launch(cfg: &HarnessConfig, harness: &str, req: &LaunchReq) -> Result<Launch>;
pub fn check_turn_options(harness: &str, effort: Option<&str>, mode: Option<&str>, images: usize) -> Result<()>;
pub fn interrupt_plan(cfg: &HarnessConfig, harness: &str) -> InterruptPlan;
pub fn follow_up_via_stdin(harness: &str, prompt: &str) -> Option<String>;
pub fn permission_reply(cfg: &HarnessConfig, harness: &str, request_id: &str, allow: bool, input: &Value, message: &str) -> Option<String>;
pub fn classify_error(message: &str) -> &'static str;
pub fn parse(harness: &str, stream: &str, line: &str) -> Vec<Norm>;
pub fn scope_codex_app_child(thread: &str, norms: Vec<Norm>) -> Vec<Norm>;
#[cfg(feature = "opencode-store")]
pub fn opencode_store_children(db: &Path, root_session: &str) -> Result<Vec<Norm>>;
pub fn run_with_env(cfg: &HarnessConfig, program: &Path, args: &[&str], env: &BTreeMap<String, String>) -> Result<(i32, String)>;
pub fn strip_ansi(s: &str) -> String;

pub mod supervisor {
    pub const SEGMENT_BYTES: u64 = 8 * 1024 * 1024;
    pub const MAX_LINE_BYTES: usize = 256 * 1024;
    pub struct LaunchFile { pub program: String, pub args: Vec<String>, pub cwd: String, pub env: BTreeMap<String, String>, pub initial_stdin: Option<String>, pub close_stdin: bool, pub control_socket: String }
    pub struct ShimInfo { pub shim_pid: u32, pub child_pid: u32, pub started_ms: u64 }
    pub struct ExitInfo { pub code: Option<i32>, pub signal: Option<i32>, pub spawn_error: Option<String>, pub ended_ms: u64 }
    /// The shim process body. The consumer's binary calls this from its own subcommand.
    pub fn run(dir: PathBuf) -> Result<()>;
    /// Writes launch.json at 0600 and starts `<exe> <subcommand> <dir>` in a new session.
    pub fn spawn(exe: &Path, subcommand: &str, dir: &Path, file: &LaunchFile) -> Result<()>;
    pub fn control(socket: &Path, msg: &Value) -> Result<Value>;
    pub fn segment_path(dir: &Path, n: u64) -> PathBuf;
    pub fn alive(dir: &Path) -> bool;
    pub fn pid_alive(pid: u32) -> bool;
    pub fn exit_info(dir: &Path) -> Option<ExitInfo>;
    pub fn now_ms() -> u64;
}

pub mod tail {
    pub struct Cursor { pub segment: u64, pub offset: u64 }
    pub struct Record { pub t: u64, pub stream: String, pub data: String, pub truncated: bool }
    /// Reads whole lines after the cursor from the current segment, up to 1 MiB.
    pub fn read(dir: &Path, cursor: Cursor) -> Result<(Vec<Record>, Cursor)>;
    pub fn next_segment_exists(dir: &Path, cursor: &Cursor) -> bool;
    /// Removes segments older than `keep` below `current`. Returns what was removed.
    pub fn enforce_retention(dir: &Path, current: u64, keep: u64) -> Result<Vec<u64>>;
    /// Last `max_bytes` of raw records across segments, newest segment first, redacted.
    pub fn raw_output(dir: &Path, max_bytes: usize) -> Result<(Vec<Value>, bool)>;
}

pub mod turn {
    /// The Codex app-server start sequence needs these after "initialize" answers.
    pub struct AppServerPlan { pub prompt: String, pub cwd: String, pub model: Option<String>, pub resume: Option<String>, pub approval: String }
    /// What the consumer must do with its own state. Order matters.
    pub enum Effect {
        Session(String), Running,
        Event { kind: &'static str, source: &'static str, confidence: &'static str, payload: Value },
        Child(Norm), Permission { request_id: String, tool: String, input: Value },
        TurnDone { ok: bool, summary: Option<String>, interrupted_hint: bool },
        InterimResult(String), Send(String), CloseStdin, WriteTurnId(String), Error { class: String, message: String },
    }
    /// TailState plus the harness-generic half of Daemon::apply_norm.
    pub struct Tracker { ... }
    impl Tracker {
        pub fn new(harness: &str, app_server: Option<AppServerPlan>) -> Self;
        pub fn apply(&mut self, norm: Norm, interrupted: bool) -> Vec<Effect>;
        pub fn turn_done(&self) -> Option<bool>;
        pub fn last_error(&self) -> Option<&(String, String)>;
        pub fn session(&self) -> Option<&str>;
    }
    /// Overseer's finalize classification: status and reason from the exit record.
    pub fn exit_outcome(harness: &str, exit: &ExitInfo, turn_done: Option<bool>, interrupted: bool, last_error: Option<&(String, String)>) -> (&'static str, String);
}
```

Must change: every `OVERSEER_` literal in `adapters.rs` reads `cfg.env_prefix`. `neutral_dir`
comes from `cfg`. `clientInfo`, the interrupt id prefix and the deny message come from `cfg`.
`parse_claude` handles `stream_event` (`content_block_delta` with `text_delta`) as
`Norm::Partial` when the launch asked for partial messages, and keeps `Ignored` otherwise.
`launch` for `claude` adds `--include-partial-messages`, `--json-schema`,
`--mcp-config` and `--allowedTools` from the new fields. mote verified those flags against
`claude --help` on 2026-10-08. The opencode SQLite read goes behind `opencode-store`. The
`Child` handling (run tree, delayed parents, reparenting) stays in Overseer: the tracker returns
`Effect::Child(norm)` and the consumer decides. The `FileChange` path canonicalization against a
workspace root stays in Overseer. The run-dir layout (`runs/<id>/p<generation>`) is the
consumer's choice. The crate takes a directory.

Tests to carry: `classify_errors`, `forbidden_keys`, `base_env_drops_keys`,
`codex_turn_options_become_cli_arguments`, `turn_options_are_checked_per_harness`. New golden
tests replay `fixtures/transcripts/*.jsonl` (four files: Claude 2.1.246 auth expired live, Codex
0.155 exec subagent live, OpenCode 1.15.13 task child and write mock) through `parse` and compare
with a recorded `Norm` JSON. Port the fixture harness scripts `fixtures/fake-harness/claude-fixture.js`,
`codex-app-fixture.js`, `replay.js` and `account-cli.js` into `crates/mote-harness/fixtures/`
for the supervisor and tracker tests (ports of `ac16_fixture_*`, `ac14_fixture_claude_background_*`,
`ac19_fixture_codex_app_child_threads_*`, `ac43_fixture_tool_calls_*`). The live-account tests stay
in Overseer.

Risk: high. This is the largest cut and `apply_norm` is interleaved with the store. The tracker
split must keep the Claude background-task bookkeeping (`expected_turns`, `backgrounded`) and the
`close_stdin` after `TurnDone` for `claude` and `codex-app`. The golden transcript tests are the
guard. `PARSER_VERSION` does not change unless parsing does.

### `mote-accounts`

Takes `accounts.rs` lines 13 to 47 (the maps), the `profiles` table from `store.rs`, and from
`daemon.rs`: `ensure_system_profiles`, `create_profile`, `profile_env`, `login_command`,
`logout`, `profile_status`, `codex_identity`. The run-aware parts of `account_list` and
`account_remove` stay in Overseer.

```rust
pub struct Profile { pub id: String, pub name: String, pub harness: String, pub home: Option<PathBuf>, pub is_system: bool, pub created_ms: i64 }

pub fn provider_of(harness: &str) -> &'static str;          // "openai", "anthropic", "local", "none"
pub fn harness_for(provider: &str) -> Option<&'static str>;
pub fn profile_harness(harness: &str) -> &str;              // codex-app shares codex logins
/// The providers table with "available" from resolve_program.
pub fn providers(cfg: &HarnessConfig) -> Value;

/// CODEX_HOME, CLAUDE_CONFIG_DIR, or the four XDG dirs for opencode, created at 0700 under the
/// profile's home. `system_home` is the test override for desktop-linked logins.
pub fn profile_env(profile: &Profile, system_home: Option<&Path>) -> BTreeMap<String, String>;
pub struct LoginCommand { pub program: PathBuf, pub args: Vec<String>, pub env: BTreeMap<String, String> }
pub fn login_command(cfg: &HarnessConfig, profile: &Profile, device: bool) -> Result<LoginCommand>;
pub fn logout(cfg: &HarnessConfig, profile: &Profile) -> Result<(i32, String)>;
pub struct AccountStatus { pub installed: bool, pub program: Option<PathBuf>, pub version: Option<String>, pub logged_in: bool, pub method: Option<String>, pub identity: Option<Value>, pub credentials: Option<i64>, pub detail: String }
/// codex login status, claude auth status (JSON), opencode auth list. API-key logins report not usable.
pub fn status(cfg: &HarnessConfig, profile: &Profile) -> Result<AccountStatus>;
/// Plan and one-way fingerprints from a Codex auth.json. Never a token.
pub fn codex_identity(auth: &Path) -> Option<Value>;
/// Refuses a home outside `root` or equal to it, then removes it.
pub fn remove_home(root: &Path, profile: &Profile) -> Result<()>;

pub trait ProfileStore {
    fn profiles(&self) -> Result<Vec<Profile>>;
    fn profile(&self, id: &str) -> Result<Option<Profile>>;
    fn insert(&self, p: &Profile) -> Result<()>;
    fn delete(&self, id: &str) -> Result<()>;
    fn rename(&self, id: &str, name: &str) -> Result<()>;
}
pub fn ensure_system_profiles(store: &dyn ProfileStore, now_ms: i64) -> Result<()>;
pub fn new_profile(name: &str, harness: &str, profiles_root: &Path, now_ms: i64) -> Result<Profile>;
#[cfg(feature = "sqlite")]
pub mod sqlite { pub fn migrate(conn: &Connection) -> Result<()>; impl ProfileStore for Connection { ... } }
```

Must change: `OVERSEER_TEST_SYSTEM_HOME` becomes the `system_home` argument. The messages that
say "Overseer requires ChatGPT account login" lose the product name. The `profile` event emits
stay in the consumer.

Tests to carry: none are unit tests. Port the fixture `account-cli.js` based parts of
`ac46_accounts_by_provider_fixed_vs_desktop_linked_and_isolated_resign_in_and_removal` and
`ac13_isolated_profiles_have_separate_homes_and_no_keys` (folder isolation, no forbidden keys in
the env, API key reported not usable). Add a `codex_identity` test with a fake `auth.json`.

Risk: low. 250 lines of pure functions plus one table.

### `mote-usage`

Takes the two parsers from `usage.rs` and adds the ledger mote needs. Overseer has no ledger
table today. Its per-turn usage is an event of kind `usage` with the harness's raw payload.

```rust
pub struct Window { pub label: String, pub used: f64, pub resets_at_ms: Option<i64> }
pub struct Limits { pub reported: bool, pub source: String, pub plan: Value, pub windows: Vec<Window>, pub observed_ms: Option<i64>, pub status: Value, pub limited: bool }
/// The newest rollout-*.jsonl under <codex_home>/sessions. Reads token_count lines only.
pub fn codex_limits(codex_home: &Path) -> Option<Limits>;
pub fn claude_limits(rate_limit_info: &Value, observed_ms: i64) -> Limits;
pub fn window_label(minutes: i64) -> String;

/// One turn's usage, normalized from a Norm::Usage payload of any harness. Unknown stays None.
pub struct TurnUsage { pub tokens_in: Option<u64>, pub tokens_out: Option<u64>, pub cache_read: Option<u64>, pub cost_usd: Option<f64>, pub raw: Value }
pub fn turn_usage(harness: &str, payload: &Value) -> TurnUsage;

pub struct LedgerRow {
    pub id: i64, pub ts: i64, pub scope_id: Option<String>, pub lane: String, pub provider: String, pub model: String,
    pub call_kind: String, pub pack_ids: Vec<String>, pub tokens_in: Option<u64>, pub tokens_out: Option<u64>,
    pub cost_usd: Option<f64>, pub latency_ms: Option<u64>, pub note: Option<String>,
}
#[cfg(feature = "sqlite")]
pub mod ledger {
    pub fn migrate(conn: &Connection) -> Result<()>;
    pub fn record(conn: &Connection, row: &LedgerRow) -> Result<i64>;
    pub fn rows(conn: &Connection, scope: Option<&str>, since_ms: i64, limit: i64) -> Result<Vec<LedgerRow>>;
    pub struct Totals { pub calls: u64, pub tokens_in: u64, pub tokens_out: u64, pub cost_usd: f64 }
    pub fn totals(conn: &Connection, since_ms: i64) -> Result<Totals>;
}
```

Must change: `account_usage` stays in Overseer and calls the two parsers. `turn_usage` is new.
Its four payload shapes come from `adapters.rs`: Codex `usage` from `turn.completed`, Codex
app-server `tokenUsage`, Claude `{usage, total_cost_usd, num_turns}` from `result`, OpenCode
`{tokens, cost, reason}` from `step_finish`. The TUI's `feed.rs` already sums these into
`tokens_in`, `tokens_out` and `cost_usd`. Its exact field paths were not read in this audit
(unknown) and must be copied from `feed.rs` when the crate is written.

Tests to carry: `codex_session_log_limits_are_read_from_the_newest_token_count`,
`claude_rate_limit_info_is_normalized`. Add `turn_usage` tests from the four transcript fixtures
and a ledger round trip.

Risk: low.

### `mote-localmodel`

Takes `sys.rs` and `local.rs` from the branch. Adds a streaming chat call for mote's talk lane.

```rust
pub const GIB: u64 = 1 << 30;
pub enum Pressure { Normal, Warn, Critical, Unknown }
pub struct Memory { pub total: u64, pub available: u64, pub pressure: Pressure, pub level: Option<i64>, pub source: String }
/// The kernel on macOS, /proc on Linux. `fixture` names a JSON file read instead (tests).
pub fn memory(fixture: Option<&Path>) -> Result<Memory>;
pub fn parse_meminfo(text: &str) -> Result<(u64, u64)>;
pub fn parse_psi(text: &str) -> Pressure;
pub fn disk_free(path: &Path, fixture: Option<u64>) -> Option<u64>;
pub fn gib(bytes: u64) -> f64;

pub struct Ollama { base_url: String }
impl Ollama {
    /// Refuses anything but 127.0.0.1, localhost or ::1 over plain http.
    pub fn new(url: &str) -> Result<Self>;
    pub fn version(&self) -> Result<Value>;
    pub fn tags(&self) -> Result<Vec<Model>>;
    pub fn show(&self, tag: &str) -> Result<Value>;
    pub fn ps(&self) -> Result<Vec<Loaded>>;
    pub fn unload(&self, tag: &str) -> Result<()>;
    pub fn create_derived(&self, base: &str, context: u64) -> Result<String>;
    /// New for the talk lane: /api/chat with stream true. Blocking; run it on a blocking thread.
    pub fn chat_stream(&self, req: &ChatRequest) -> Result<Box<dyn Iterator<Item = Result<ChatChunk>> + Send>>;
}
pub struct ChatRequest { pub model: String, pub messages: Vec<Value>, pub options: Value, pub keep_alive: Option<String>, pub format: Option<Value> }
pub struct ChatChunk { pub text: String, pub done: bool, pub eval_count: Option<u64>, pub prompt_eval_count: Option<u64>, pub total_duration_ns: Option<u64> }

pub struct Geometry { pub kv_heads_per_layer: Vec<u64>, pub key_length: u64, pub value_length: u64 }
impl Geometry { pub fn kv_bytes_per_token(&self) -> u64; pub fn from_model_info(info: &Value) -> Option<Self>; }
pub struct Model { pub tag: String, pub base: String, pub size: u64, pub family: Option<String>, pub parameter_size: Option<String>, pub parameters: Option<u64>, pub quantization: Option<String>, pub max_context: Option<u64>, pub configured_context: Option<u64>, pub capabilities: Vec<String>, pub geometry: Option<Geometry> }
pub struct Loaded { pub tag: String, pub size: u64, pub size_vram: Option<u64>, pub context: Option<u64>, pub expires_at: Option<String> }
pub struct Inventory { pub memory: Option<Memory>, pub memory_error: Option<String>, pub ollama: OllamaStatus, pub models: Vec<Model>, pub loaded: Vec<Loaded>, pub models_dir: String, pub disk_free: Option<u64> }
pub fn inventory(ollama: &Ollama, program_candidates: &[PathBuf], memory_fixture: Option<&Path>) -> Inventory;

pub struct Entry { pub tag: String, pub tier: u64, pub parameters: Option<u64>, pub disk_bytes: u64, pub max_context: u64, pub geometry: Option<Geometry>, pub verified: Value }
pub struct Catalogue(Vec<Entry>);
impl Catalogue {
    pub fn parse(json: &str) -> Result<Self>;
    /// A second file of the same shape whose `verified` entries win.
    pub fn with_overrides(self, json: &str) -> Self;
}

pub struct PickOptions { pub ceiling_percent: u64, pub headroom: Option<u64>, pub context_target: u64, pub context_floor: u64, pub preferred: Vec<String>, pub allow_unverified: bool, pub harness: String, pub may_download: bool }
pub const MAX_CEILING_PERCENT: u64 = 50;
pub struct Budget { pub total: u64, pub available: u64, pub headroom: u64, pub ceiling_percent: u64, pub ceiling_share: u64, pub ceiling_now: u64, pub ceiling_level: Option<u64>, pub level: Option<i64>, pub level_floor: i64, pub budget: u64, pub pressure: Pressure, pub note: Option<String> }
pub fn headroom(total: u64, configured: Option<u64>) -> u64;
/// min(total x ceiling, available - headroom, the system's own level above 45%). Zero at critical pressure.
pub fn budget(mem: &Memory, opts: &PickOptions, reclaimable: u64) -> Budget;
/// weights + KV cache(context) + 1 GiB. None when the geometry is unknown.
pub fn estimate(weights: u64, geometry: Option<&Geometry>, context: u64) -> Option<u64>;
pub type Measured = HashMap<(String, u64), u64>;
pub struct Choice { pub tag: String, pub run_tag: String, pub run_tag_exists: bool, pub context: u64, pub bytes: u64, pub measured: bool, pub tier: u64, pub installed: bool, pub download_bytes: Option<u64>, pub verified: bool, pub budget: Budget }
pub struct Pick { pub chosen: Option<Choice>, pub alternatives: Vec<Choice>, pub rejected: Vec<Rejected>, pub budget: Budget }
/// Pure. Everything it decides from is passed in.
pub fn pick(mem: &Memory, models: &[Model], loaded: &[Loaded], catalogue: &Catalogue, measured: &Measured, opts: &PickOptions) -> Pick;
pub fn context_steps(target: u64, floor: u64, max: u64) -> Vec<u64>;
pub fn derived_tag(prefix: &str, base: &str, context: u64) -> String;   // "overseer/qwen3-coder-30b:64k" with prefix "overseer"
pub fn canonical(tag: &str) -> String;
/// The guard: refuses a tag and context whose measured or estimated size is above a fresh budget.
pub fn approve(mem: &Memory, tag: &str, context: u64, models: &[Model], loaded: &[Loaded], catalogue: &Catalogue, measured: &Measured, opts: &PickOptions) -> Result<Choice>;
pub fn must_stop_load(mem: &Memory, headroom: u64) -> Option<String>;
/// Loads while sampling memory every `watch`. Cancels and unloads when must_stop_load says so.
pub fn load_guarded(ollama: &Ollama, tag: &str, context: u64, headroom: u64, keep_alive: &str, watch: Duration, memory_fixture: Option<&Path>) -> Result<Value>;
/// The simple rule mote's talk lane states: available >= factor x size.
pub fn fits_simple(mem: &Memory, model_bytes: u64, factor: f64) -> bool;

#[cfg(feature = "sqlite")]
pub mod store {
    pub fn migrate(conn: &Connection) -> Result<()>;                       // local_models_measured, local_models_loaded
    pub fn measured(conn: &Connection) -> Measured;
    pub fn record_measured(conn: &Connection, models: &[Model], loaded: &[Loaded], version: Option<&str>, now: i64) -> Result<Vec<(String, u64, u64)>>;
    pub fn note_loaded(conn: &Connection, tag: &str, at: i64);
    pub fn note_unloaded(conn: &Connection, tag: &str);
    pub fn loaded_by_us(conn: &Connection) -> HashSet<String>;
}
```

Must change: `OVERSEER_OLLAMA_URL`, `OVERSEER_OLLAMA_CANDIDATES`, `OVERSEER_TEST_MEMORY`,
`OVERSEER_TEST_DISK_FREE` and `OVERSEER_TEST_WATCH_MS` become arguments. `crate::paths::data_dir`
and `crate::ollama_install::own_program` go away: the catalogue override and the program
candidates are passed in. The derived tag prefix `overseer/` becomes a parameter. The catalogue
JSON ships with the crate as `catalogue/qwen-coders.json` and the consumer may pass another.

Tests to carry: `sys.rs`: `linux_meminfo_is_read_in_bytes`, `linux_pressure_levels`,
`a_fixture_describes_another_machine`, `macos_numbers_match_the_systems_own_tools`,
`disk_free_is_reported_for_a_path_that_does_not_exist_yet`. `local.rs`: all 16 unit tests
(the budget over 16, 32, 64 and 128 GiB profiles, the estimate against the measured 23.7 GiB,
`context_steps`, `derived_tag`, `canonical`, the pick ordering, the guard). Port the fixture
Ollama from `daemon/tests/common/ollama.rs` (312 lines) and the in-process parts of
`ac85_inventory_reports_the_machine_and_ollama_without_guessing`,
`ac86_the_pick_follows_the_machines_memory` and
`ac140_no_model_over_the_budget_is_loaded_by_any_path`.

Risk: medium. The source is a branch, not `main`. The `measured` table is read by the guard, so
the SQLite feature is needed for Overseer's path. `chat_stream` is new code without an Overseer
test.

### `mote-gateway`

Takes `gateway/noise.rs`, `base32.rs`, `net.rs`, `pairing.rs`, `classes.rs`, `devices.rs`,
`mod.rs`, `remote.rs` and `local.rs` from the branch, behind one host trait. `power.rs` and
`push.rs` stay in Overseer.

```rust
pub struct GatewayConfig {
    /// b"overseer-gateway-v1" for Overseer. b"mote-gateway-v1" for mote.
    pub prologue: Vec<u8>,
    /// "OVSR1" for Overseer. The pairing code prefix.
    pub code_prefix: String,
    pub default_port: u16,          // 47810
    pub ws_path: String,            // "/v1"
    pub protocol_version: i64,
    /// Where identity.key lives, 0600 inside a 0700 directory.
    pub identity_dir: PathBuf,
    /// "_overseer._tcp". None turns advertising off.
    pub mdns_service: Option<String>,
    pub host_name: String,
    /// The method class table, the protocol.json text. Parsed once.
    pub classes: classes::Classes,
    pub idle: Duration, pub pairing_window: Duration, pub confirm_wait: Duration,
}

pub trait GatewayHost: Send + Sync + 'static {
    fn setting(&self, key: &str) -> Option<String>;
    fn set_setting(&self, key: &str, value: &str) -> Result<()>;
    fn devices(&self) -> &dyn devices::DeviceStore;
    fn emit(&self, kind: &str, source: &str, scope: Option<&str>, payload: Value) -> Result<()>;
    fn handler(&self) -> Arc<dyn mote_ipc::Handler>;
    fn events(&self) -> Arc<dyn EventSource>;
    /// Extra refusals for devices. Overseer refuses task.create with a program or an unknown repo.
    fn guard(&self, device: &DeviceCtx, method: &str, params: &Value) -> Result<()> { Ok(()) }
    fn log(&self, line: &str);
    fn now_ms(&self) -> i64;
}

pub struct DeviceCtx { pub device_id: String, pub device_name: String }

pub struct Gateway { ... }
impl Gateway {
    pub fn new(cfg: GatewayConfig) -> Self;
    /// Once, on the runtime: restores "enabled", prunes stored requests, starts if it was on.
    pub fn start(self: &Arc<Self>, host: Arc<dyn GatewayHost>, rt: tokio::runtime::Handle);
    pub fn enable(&self, port: Option<u16>) -> Result<Value>;
    pub fn disable(&self) -> Result<Value>;
    pub fn port(&self) -> Option<u16>;
    pub fn identity(&self) -> Result<Arc<Identity>>;
    pub fn session_count(&self) -> usize;
    pub fn end_sessions(&self, which: impl Fn(&Session) -> bool, state: &str) -> usize;
    /// Exactly once per device and request id, with the stored outcome on a retry.
    pub fn once(&self, device: &str, request_id: &str, method: &str, run: impl FnOnce() -> Value) -> Value;
}

pub mod manage {
    /// gateway.status, enable, disable, settings, pair_start, pair_cancel, pair_confirm, devices,
    /// device_revoke, device_scope, device_rename. Refused when mote_ipc::actor() is a device.
    pub fn dispatch(gw: &Arc<Gateway>, method: &str, params: &Value) -> Result<Value>;
}

pub mod noise {
    pub const CHUNK: usize = 65_000;
    pub enum Kind { Session, Pairing }
    pub struct Keypair { pub private: Vec<u8>, pub public: Vec<u8> }
    pub fn generate_keypair() -> Result<Keypair>;
    pub fn public_of(private: &[u8]) -> Vec<u8>;
    pub fn fingerprint(public: &[u8]) -> String;
    pub fn psk_from_secret(secret: &[u8]) -> [u8; 32];
    pub fn responder(prologue: &[u8], kind: Kind, static_private: &[u8], psk: Option<&[u8; 32]>) -> Result<HandshakeState>;
    pub fn initiator(prologue: &[u8], kind: Kind, static_private: &[u8], gateway_public: &[u8], psk: Option<&[u8; 32]>) -> Result<HandshakeState>;
    pub fn seal(transport: &mut TransportState, message: &[u8]) -> Result<Vec<Vec<u8>>>;
    pub struct Opener;  impl Opener { pub fn new(limit: usize) -> Self; pub fn open(&mut self, transport: &mut TransportState, frame: &[u8]) -> Result<Option<Vec<u8>>>; }
    pub fn first_frame(frame: &[u8]) -> Option<(Kind, &[u8])>;
    pub fn hex(bytes: &[u8]) -> String;  pub fn unhex(text: &str) -> Result<Vec<u8>>;
}
pub mod net { pub fn is_local_range(ip: IpAddr) -> bool; pub struct Cidr; pub fn is_allowed(ip: IpAddr, extra: &[Cidr]) -> bool; pub fn local_addresses() -> Vec<String>; pub fn host_name() -> String; }
pub mod pairing { pub struct Pairing; pub struct Pending; pub const MAX_FAILURES: u32 = 5; pub fn code(prefix: &str, gateway_public: &[u8], secret: &[u8; 16], port: u16, addresses: &[String]) -> String; }
pub mod base32 { pub fn encode(bytes: &[u8]) -> String; pub fn decode(text: &str) -> Option<Vec<u8>>; }
pub mod classes {
    pub enum Class { Own, Read, Control, MacOnly }
    pub enum Scope { Full, Watch }
    pub struct Classes;
    impl Classes {
        pub fn parse(protocol_json: &str) -> Result<Self>;
        pub fn class_of(&self, method: &str) -> Option<Class>;
        pub fn allowed(&self, method: &str, scope: Scope) -> Result<Class, &'static str>;
        pub fn undeclared_params(&self, method: &str, params: &Value) -> Vec<String>;
        pub fn methods(&self) -> Vec<(String, Class)>;
        pub fn planned(&self) -> Vec<String>;
    }
}
pub mod devices {
    pub const REQUEST_RETENTION_MS: i64 = 7 * 24 * 3600 * 1000;
    pub struct Device { pub id: String, pub name: String, pub platform: String, pub public_key: String, pub scope: String, pub paired_ms: i64, pub last_seen_ms: Option<i64>, pub last_addr: Option<String>, pub last_counter: i64, pub revoked_ms: Option<i64>, pub notifications: Value, pub app: Option<String> }
    pub enum Earlier { New, Done(Value), Interrupted }
    pub trait DeviceStore: Send + Sync {
        fn insert(&self, d: &Device) -> Result<()>;
        fn devices(&self) -> Result<Vec<Device>>;
        fn device(&self, id: &str) -> Result<Option<Device>>;
        fn device_by_key(&self, public_key_hex: &str) -> Result<Option<Device>>;
        fn accept_counter(&self, id: &str, counter: i64, now: i64, addr: &str, app: Option<&str>) -> Result<bool>;
        fn touch(&self, id: &str, now: i64) -> Result<()>;
        fn revoke(&self, id: &str, now: i64) -> Result<bool>;
        fn set_scope(&self, id: &str, scope: &str) -> Result<bool>;
        fn rename(&self, id: &str, name: &str) -> Result<bool>;
        fn set_notifications(&self, id: &str, value: &Value) -> Result<()>;
        fn request_claim(&self, device: &str, request_id: &str, method: &str, now: i64) -> Result<Earlier>;
        fn request_store(&self, device: &str, request_id: &str, reply: &Value) -> Result<()>;
        fn request_method(&self, device: &str, request_id: &str) -> Result<Option<String>>;
        fn requests_prune(&self, now: i64) -> Result<usize>;
    }
    #[cfg(feature = "sqlite")] pub fn migrate(conn: &Connection) -> Result<()>;
    #[cfg(feature = "sqlite")] pub struct SqliteDevices(pub Arc<Mutex<Connection>>);
}
```

Must change: `include_str!` of `protocol/protocol.json` becomes `Classes::parse`. The test that
greps Overseer's dispatch tables for method names stays in Overseer. `d.repo_is_known` moves into
`GatewayHost::guard`. The `focus` map (which agent each Mac window shows) leaves the gateway and
stays in Overseer's server. `power::watch` and `push::watch` are called by Overseer after
`start`. `crate::server::PROTOCOL_VERSION` is `cfg.protocol_version`. Every `OVERSEER_TEST_*`
duration becomes a config field. The `mdns` advertiser keeps shelling out to `dns-sd` and
`avahi-publish`.

Tests to carry: `noise.rs`: `noise_vectors_match_the_shared_file` (with
`protocol/vectors/noise.json` copied to `crates/mote-gateway/tests/vectors/overseer-noise.json`
and a second file generated for the mote prologue), `a_wrong_pairing_secret_fails_in_the_first_message`,
`large_messages_are_chunked_and_joined`. `net.rs`: `only_local_ranges_are_allowed`,
`the_mac_offers_at_least_loopback`. `pairing.rs`: both tests. `base32.rs`: the RFC vectors.
`classes.rs`: `scopes_follow_the_classes`, `every_method_a_phone_may_call_describes_its_parameters`
(against a test table). `mod.rs`: the seeded fuzz test. From `daemon/tests/gateway.rs`, against
a test host: `ac116_nothing_answers_before_a_session_is_authenticated`,
`ac116_peers_outside_the_local_network_are_refused`, `ac117_pairing_needs_the_mac`,
`ac117_an_expired_code_and_an_unanswered_request_pair_nothing`,
`ac118_sessions_are_encrypted_and_both_sides_are_authenticated`,
`ac119_devices_scopes_and_revoking`, `ac121_a_stream_cut_at_a_hundred_random_points_arrives_once_and_in_order`,
`ac121_an_empty_frame_in_a_session_keeps_the_session`, `ac122_a_request_sent_again_acts_once`,
`ac141_a_phone_pairs_once`, `ac130_a_phone_sends_only_what_the_protocol_describes`. The reference
phone `tests/common/phone.rs` moves to `crates/mote-gateway/tests/phone.rs`. The power, push,
mDNS-on-the-wire and two-surfaces tests stay in Overseer.

Risk: high. The largest extraction, from a branch that is still moving. The existing phone app
depends on the prologue, the `OVSR1` prefix and the vectors, so Overseer's config keeps all three
unchanged. The fuzz test takes minutes in release mode.

## Extraction order

Least coupled first, each step leaving `cargo test --workspace` green in this repository:

1. `mote-redact`. Unblocks everything.
2. `mote-paths`. Unblocks `moted` startup (MOTE-01).
3. `mote-usage`. Pure parsers and a new table. No dependents.
4. `mote-eventlog`. Unblocks MOTE-02.
5. `mote-ipc`. Unblocks the socket half of MOTE-01 and the work lane's client (MOTE-12).
6. `mote-harness`. Unblocks the think lane (MOTE-09, MOTE-11).
7. `mote-accounts`. Needs 6. Unblocks the sign-in card (MOTE-11).
8. `mote-localmodel`. Independent of 1 to 7. Unblocks the talk lane (MOTE-04). Can run in
   parallel with 4 to 7.
9. `mote-gateway`. Needs 4 and 5. No MOTE criterion in v0 needs it. Last.

The mote critical path is 1, 2, 4, 5 for the daemon core, then 6, 7, 8 for the lanes.

## Overseer migration plan

For the driver that later moves Overseer onto the crates. Each step is one pull request in
Overseer. Each step's proof is the named tests green plus the acceptance criteria they support
still verified at the new commit, recorded in `docs/verification/` through
`docs/verification/records.py` as Overseer's rules require. The crates are consumed by path from
`../mote/crates/<name>` until mote has a remote, then by `git = "<url>", rev = "<sha>"`.

Steps 1 to 8 apply to `main`. Steps 9 and 10 apply to the branch that carries the gateway and
Continuity, or to `main` after that branch merges.

1. **Add the dependencies.** In `daemon/Cargo.toml` and `tui/Cargo.toml` add the crates by
   path. Change nothing else. **Proof:** `cargo build --workspace` and `cargo test --workspace`
   green.
2. **Redact.** Replace the body of `daemon/src/redact.rs` with `pub use mote_redact::{redact, redact_value, fingerprint};`.
   Delete `redact_value` and `fingerprint` from `daemon.rs` and fix the two `use` lines.
   **Proof:** `cargo test -p overseerd ac10_redaction_of_secrets_in_output` and the full suite.
   AC-10 and AC-13 (no credential leakage) still verified.
3. **Paths.** `daemon/src/paths.rs` becomes a `static` `mote_paths::Paths` with Overseer's
   `AppId` and three wrappers for `runs_dir`, `worktrees_dir`, `profiles_dir`. Before the change,
   record `overseerd socket-path` for an empty `OVERSEER_HOME`, a long `OVERSEER_HOME` over 100
   bytes, and no `OVERSEER_HOME`. **Proof:** the three outputs are byte for byte the same after
   the change, `ac08_socket_is_owner_only_and_requests_are_not_shell` green, AC-04 and AC-08 still
   verified.
4. **Event log.** `Store` keeps its connection. The `events` and `meta` functions in `store.rs`
   call `mote_eventlog` functions on `self.conn`. `Store::migrate` calls `mote_eventlog::migrate`,
   which renames `task_id` and `run_id` to `group_id` and `scope_id` once. `Daemon.events` becomes
   `mote_eventlog::Bus`. The daemon enables the `overseer-wire` feature. `Daemon::emit` and
   `apply_norm` keep their signatures. **Proof:** `ac10_replay_cursor_burst_and_retention`,
   `ac05_ac07_state_survives_daemon_crash_and_reattaches`,
   `ac18_fixture_recursive_tree_duplicates_and_delayed_parent` green. An upgrade test opens a copy
   of a database written by the previous `overseerd`, runs `state` and `events.list`, and gets the
   same rows. The TUI's `tests/control.rs` and `tests/look.rs` green. The extension's packaged
   UI scenarios (Overseer's `test/ui`) green with the same event JSON. AC-05, AC-07, AC-10 still
   verified.
5. **Usage.** `daemon/src/usage.rs` keeps `account_usage` and calls `mote_usage::codex_limits`
   and `claude_limits`. **Proof:** `ac62_account_usage_is_what_the_harness_reports` green. AC-62
   still verified.
6. **Harness.** Delete `adapters.rs` and `shim.rs`. `main.rs` `shim` calls
   `mote_harness::supervisor::run`. `Daemon::spawn_process` calls `supervisor::spawn` with
   `"shim"`. `tail_loop` uses `tail::read`, `next_segment_exists`, `exit_info`, `alive`.
   `apply_norm` becomes: run `Tracker::apply`, then apply each `Effect` to the store exactly as
   today (`Effect::Child` goes to the existing child logic, `Effect::Event` to `insert_event`,
   `Send` and `CloseStdin` to the control socket after the lock). `finalize` calls
   `turn::exit_outcome`. The `HarnessConfig` has `env_prefix: "OVERSEER"`, `client_name:
   "overseer"`, `request_prefix: "overseer-int"`, the existing deny message, and `neutral_dir:
   data_dir().join("tmp")`. **Proof:** every `fixture` test in `protocol.rs` green
   (`ac16_*`, `ac18_*`, `ac19_*`, `ac20_*`, `ac14_fixture_*`, `ac43_*`, `ac60_*`, `ac15_*`,
   `ac06_*`, `ac09_*`). `state` reports the same `parser_version`. One live Codex turn and one
   live Claude turn with the tiny prompts AC-14 allows, recorded. AC-06, AC-09, AC-14, AC-15,
   AC-16, AC-18, AC-19, AC-20, AC-43, AC-60 still verified.
7. **Accounts.** `daemon.rs` profile functions call `mote_accounts`. `store.rs` profile
   functions call `mote_accounts::sqlite`. `accounts.rs` keeps `account_list` and
   `account_remove` and calls `remove_home`. **Proof:** `ac13_*`, `ac46_*`, `ac62_*` green.
   AC-11, AC-13, AC-46 still verified.
8. **IPC.** `server.rs` keeps `dispatch` and implements `mote_ipc::Handler` on `Daemon`
   (`hello` counts UI clients, `exits_after` for `daemon.shutdown` and `daemon.stop_all`,
   `dispatch_on` for `ui.focus` on the branch). `main.rs` `serve` calls `mote_ipc::serve`.
   `tui/src/client.rs` becomes `pub use mote_ipc::blocking::{Client, Msg, Requests};` with the
   daemon-spawn closure from `locate.rs`. **Proof:** `ac08_*`, `t11_an_attached_tui_counts_as_a_watching_ui`,
   `ac45_*` green. `overseerd ctl hello` output unchanged. The TUI's
   `tests/live.rs`, `interact.rs`, `control.rs` green. AC-08, AC-45 still verified.
9. **Local models** (branch). Delete `sys.rs`. `local.rs` keeps `ollama_program` with the
   `own_program` candidate and calls `mote_localmodel` for everything else, passing
   `data_dir().join("local_catalogue.json")` as the override. `continuity.rs` passes the
   `OVERSEER_*` test env values into the arguments. **Proof:** `tests/continuity.rs` `ac85_*`,
   `ac86_*`, `ac140_*`, `ac94_*` and `tests/local_runs.rs` `ac140_*` green,
   `macos_numbers_match_the_systems_own_tools` green on the owner's Mac. AC-85, AC-86, AC-140
   still verified.
10. **Gateway** (branch). Delete `gateway/noise.rs`, `base32.rs`, `net.rs`, `pairing.rs`,
    `classes.rs`, `devices.rs`, `remote.rs`, `local.rs` and most of `mod.rs`. Keep `power.rs`,
    `push.rs`, the `focus` map (moved to `server.rs`), and a `GatewayHost` impl on `Daemon`
    whose `guard` holds today's `task.create`, `repo.inspect` and `repo.files` refusals.
    `GatewayConfig` keeps `overseer-gateway-v1`, `OVSR1`, `_overseer._tcp`, port 47810 and
    `include_str!("../../../protocol/protocol.json")` as the classes text. **Proof:** every test
    in `tests/gateway.rs` and `tests/phone_methods.rs` green, the fuzz test green in release
    mode, `noise_vectors_match_the_shared_file` green against the unchanged
    `protocol/vectors/noise.json`, the phone app's simulator scenario (AC-132) green with the
    unchanged app. AC-116 to AC-122, AC-130, AC-141 still verified.
11. **Close.** Remove dead code and the `OVERSEER_*` literals the crates replaced. Update
    `docs/compatibility.md` and the README to name the crates and their revision. Record every
    step's commit in the ledger.

What stays Overseer-only after the plan: `daemon.rs` (tasks, workspaces, snapshots, the run tree,
reconcile, comparisons, archive, search, diff, cleanup, `state`), `store.rs` (every table except
`events`, `meta`, `profiles`), `git.rs`, `merge.rs`, `pr.rs`, `files.rs`, `background.rs`,
`main.rs`, the `dispatch` table, `gateway/power.rs`, `gateway/push.rs`, `continuity.rs`,
`handoff.rs`, `downloads.rs`, `ollama_install.rs`, `opencode_bridge.rs`, `net.rs`,
`device_login.rs`, `review.rs`, the TUI's views, the VS Code extension, `protocol/protocol.json`.

## License and notices

- Overseer's `daemon/Cargo.toml` and `tui/Cargo.toml` declare `license = "MIT"`. No `LICENSE`
  file exists at the Overseer repository root. Whether one exists elsewhere is unknown. The
  owner of mote is the author of Overseer, so no third party is involved in the transfer.
- This repository adds a `LICENSE` file with the MIT text and the owner's name before the first
  crate lands. Every crate manifest carries `license = "MIT"`.
- Every crate README states: "Portions derived from Overseer (`daemon/src/<files>`) at
  `a7a5cac0` on `main` and `1488b4be` on `claude/phone-remote-vscode-control-b48a34`, MIT."
  The copied files `protocol/vectors/noise.json`, the four transcript fixtures, the four fake
  harness scripts, `local_catalogue.json` and `tests/common/phone.rs` carry the same line in a
  `NOTICE` next to them.
- The transcript fixtures are redacted harness output. They are checked with
  `mote_redact::redact` in a test before they land, and the test asserts no change.
- Third-party licenses are not asserted here. CRATE-17 records the output of `cargo license`
  for the workspace. `snow`, `tokio-tungstenite`, `ureq`, `rusqlite`, `regex`, `base64`, `sha2`,
  `tokio`, `serde` and `uuid` are expected to be MIT or Apache-2.0 dual. Unknown until recorded.

## Open decisions with recommended defaults

1. **How Overseer consumes the crates.** Default: by path (`../mote/crates/<name>`) until mote
   has a remote, then `git` with a pinned `rev`. No crates.io publication.
2. **Event scope names.** Default: columns `scope_id` and `group_id`, wire names the same, and an
   `overseer-wire` feature that renames them to `run_id` and `task_id` for Overseer's clients.
   Overseer's migration renames the columns once.
3. **Environment prefixes.** Default: a runtime config value per crate (`AppId.env_prefix`,
   `HarnessConfig.env_prefix`), never a compile-time feature.
4. **Gateway prologue and pairing prefix.** Default: both are config. Overseer keeps
   `overseer-gateway-v1` and `OVSR1` so the existing phone app keeps working. mote uses
   `mote-gateway-v1` and `MOTE1`. Vectors exist per prologue.
5. **Method class table.** Default: the consumer passes the JSON text. mote writes its own
   `protocol/protocol.json` with its methods when the gateway lands. The checker in
   `tests/protocol_shapes.rs` is ported as a reusable test helper then.
6. **Claude partial messages.** Default: `LaunchReq.partial_messages` off, and `Norm::Partial`
   is emitted only when it is on. Overseer's parser output is unchanged at `PARSER_VERSION
   2026-09-24.1`.
7. **IPC client shape.** Default: an async client written new in `mote-ipc`, and the TUI client
   carried unchanged behind the `blocking` feature. Overseer's TUI keeps the blocking one.
8. **Crate name prefix.** Default: `mote-` for all nine, even though Overseer consumes them. A
   rename later is mechanical.
9. **rusqlite version.** Default: `0.32` with `bundled`, the version both daemons use.
10. **The talk lane's memory rule.** mote's MOTE-04 says 1.25 times the model size. Overseer's
    guard uses the budget formula. Default: `mote-localmodel` offers both. The talk lane calls
    `fits_simple(mem, size, 1.25)` for the check MOTE-04 names and `approve` before any load, so
    the guard still holds.
11. **OpenCode session store reading.** Default: behind the `opencode-store` feature, on for
    Overseer, off for mote.
12. **Later candidates.** `mote-git` (snapshots and status with a ref prefix), `mote-net`
    (connectivity and probes for mote's Offline journey), `mote-notify` (`notify()` from
    `background.rs`). Default: not now. Each gets its own short RFC when a MOTE criterion needs it.
13. **Where the shim subcommand lives.** Default: each daemon binary keeps its own `shim`
    subcommand that calls `mote_harness::supervisor::run`. No separate shim binary.

## Acceptance criteria

One sentence each, plus a Verify clause. Checkboxes live in the root README under Ready for Go.
CRATE-00 there is the owner's approval of this RFC.

- **CRATE-01** The workspace has `crates/mote-redact`, `mote-paths`, `mote-usage`, `mote-eventlog`, `mote-ipc`, `mote-harness`, `mote-accounts`, `mote-localmodel` and `mote-gateway`, each with a README that names its Overseer source files and revision, and each builds alone. **Verify:** `for c in crates/*; do cargo build -p $(basename $c) || exit 1; done` and `cargo clippy --workspace -- -D warnings` exit 0.
- **CRATE-02** `mote-redact` redacts the same seven patterns as Overseer's `redact.rs` and replaces the same twelve secret keys as `redact_value`. **Verify:** `cargo test -p mote-redact` green including the carried `redacts_common_secrets`, and `diff <(grep -o 'r"[^"]*"' crates/mote-redact/src/lib.rs) <(git -C ~/projects/overseer show a7a5cac0:daemon/src/redact.rs | grep -o 'r"[^"]*"')` prints nothing.
- **CRATE-03** `mote-paths` returns Overseer's socket path byte for byte for an Overseer `AppId`. **Verify:** `cargo test -p mote-paths` green, and a test runs `overseerd socket-path` from `~/projects/overseer/target/debug` with `OVERSEER_HOME` unset, set short, and set to a 120-byte path, and compares each with `Paths::new(OVERSEER).socket_path()`.
- **CRATE-04** `mote-eventlog` appends with strictly increasing `seq`, replays after any cursor with no gap and no duplicate, prunes a scope to 5,000 with a `retention` marker, and reports the oldest retained seq. **Verify:** `cargo test -p mote-eventlog` green including a 12,000 append burst test ported from `ac10_replay_cursor_burst_and_retention`.
- **CRATE-05** A subscriber that lags the bus receives a resync signal and can resume from the durable log without loss. **Verify:** `cargo test -p mote-eventlog lag` with a capacity-1 bus asserts the receiver sees `Lagged`, resumes with `after(cursor)`, and ends with every seq once.
- **CRATE-06** `mote-ipc::serve` binds a 0600 socket in a 0700 directory, refuses a peer with another uid, refuses a request over 1 MiB, answers `parse_error`, `invalid_request` and `invalid_params` with those codes, and serves `events.subscribe` with replay, `replayed` and live events. **Verify:** `cargo test -p mote-ipc server` green with a test `Handler`, ported from `ac08_socket_is_owner_only_and_requests_are_not_shell` and `ac08_connections_from_a_foreign_uid_are_rejected_and_logged`.
- **CRATE-07** The async client and the blocking client both reconnect after a dropped connection and resubscribe from their cursor so no event is delivered twice. **Verify:** `cargo test -p mote-ipc client` cuts the connection mid-stream at ten points and compares the received seqs with the log.
- **CRATE-08** `mote-harness::launch` builds the same argv, stdin and env as Overseer for `codex`, `codex-app`, `claude`, `opencode` and `generic` when the new Claude fields are off, and never forwards a forbidden key. **Verify:** `cargo test -p mote-harness launch` green including the five carried unit tests.
- **CRATE-09** `mote-harness::parse` turns the four transcript fixtures into the same `Norm` sequence Overseer produces at `PARSER_VERSION` `2026-09-24.1`. **Verify:** `cargo test -p mote-harness golden` compares against `tests/golden/*.norm.json` recorded from Overseer at `a7a5cac0`.
- **CRATE-10** The supervisor records stdout and stderr into segment files, answers `ping`, `stdin`, `close_stdin` and `signal` on its control socket, writes `shim.json` and `exit.json`, and keeps the harness alive when its parent dies. **Verify:** `cargo test -p mote-harness supervisor` spawns the test binary's `shim` subcommand with `/bin/sh` scripts, kills the parent with SIGKILL, and reads a complete `exit.json`.
- **CRATE-11** With `partial_messages` on, Claude `stream_event` deltas become `Norm::Partial` and the final `assistant` text is still emitted once as `Norm::Text`. **Verify:** `cargo test -p mote-harness partial` with the fake `claude` shim emitting deltas then a result.
- **CRATE-12** `mote-accounts::profile_env` creates 0700 credential folders per harness and never sets a forbidden key, `status` reports an API-key login as not usable, and `codex_identity` returns fingerprints and a plan but never a token. **Verify:** `cargo test -p mote-accounts` green with the carried `account-cli.js` fixture and a fake `auth.json`.
- **CRATE-13** `mote-usage` parses Codex rollout limits and Claude rate-limit events as Overseer does, normalizes the four usage payload shapes, and round-trips ledger rows. **Verify:** `cargo test -p mote-usage` green including the two carried tests and a `totals` test over three rows.
- **CRATE-14** `mote-localmodel` reads memory within 5% of `vm_stat` and `sysctl` on macOS, computes the budget for 16, 32, 64 and 128 GiB profiles as Overseer's tests expect, estimates `qwen3-coder:30b` at 64k within 15% of the measured 23.7 GiB, refuses any load above the budget, and cancels a load when available memory falls under half the headroom. **Verify:** `cargo test -p mote-localmodel` green with the 21 carried unit tests and the fixture Ollama ported from `tests/common/ollama.rs`.
- **CRATE-15** `mote-gateway` reproduces Overseer's Noise vectors under the Overseer prologue, survives the seeded fuzz test, and passes the ported pairing, session, scope, revoke, cut-stream and exactly-once tests against a test host. **Verify:** `cargo test -p mote-gateway` green and `cargo test -p mote-gateway --release fuzz` green, with `tests/vectors/overseer-noise.json` identical to `protocol/vectors/noise.json` at `1488b4be`.
- **CRATE-16** Overseer on `main` consumes `mote-redact`, `mote-paths`, `mote-eventlog`, `mote-ipc`, `mote-harness`, `mote-accounts` and `mote-usage` by path with every protocol and TUI test green and an existing database opening unchanged. **Verify:** in the Overseer checkout `cargo test --workspace` green, `overseerd ctl hello` and `overseerd socket-path` outputs identical before and after, and an upgrade test over a copy of the owner's `overseer.sqlite` returns the same `state` and `events.list` rows.
- **CRATE-17** Every crate carries the MIT license and a derivation notice, the copied fixtures carry a `NOTICE`, the transcript fixtures contain nothing `mote_redact::redact` would change, and the third-party license list is recorded. **Verify:** `cargo license --workspace` output saved in `docs/verification/CRATE-17.md`, `grep -L "derived from Overseer" crates/*/README.md` prints nothing, and `cargo test -p mote-harness fixtures_are_redacted` green.
