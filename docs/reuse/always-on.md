# Reuse: always-on with approval gates

What mote reuses to stay running, wake on schedules and mentions, and never act outward
without the owner. Covers MOTE-20 and MOTE-21. Researched 2026-10-08.

## Key points

1. `moted install` writes `~/Library/LaunchAgents/com.mote.moted.plist` and loads it with `launchctl bootstrap gui/$UID`. launchd restarts the daemon after a crash and at login.
2. On the Pi, `moted install` writes `~/.config/systemd/user/moted.service` with `Restart=always`, `Type=notify` and a watchdog, and prints the `loginctl enable-linger` command.
3. The cron watcher is `croner` plus a tokio sleep loop over a `schedules` table. `tokio-cron-scheduler` is skipped: one maintainer, last release a year ago, persistence only for Postgres and NATS.
4. Slack uses Socket Mode through `slack-morphism`. No public URL is needed. It needs an app-level `xapp-` token and a bot `xoxb-` token. Without them the watcher reports itself disabled.
5. GitHub is polled with `gh api notifications` at the `X-Poll-Interval` the server returns. Webhooks need a public URL and wait for the relay.
6. Every outward action is a row in an `approvals` table. The window shows it as a card. Nothing posts until the row is `approved`. Rows expire after 24 h.
7. launchd supervises moted. moted supervises its own children and watcher tasks with `backon` backoff. Nothing else sits in between.
8. Watchers are read-only. Each one wakes the router with an `utterance` event whose source is `watcher:<name>`.

## What the concept needs technically

launchd agent. `moted install` writes this file with the `plist` crate and mode 0644, then runs `launchctl bootstrap gui/$UID <plist>`. `moted uninstall` runs `launchctl bootout gui/$UID/com.mote.moted` and removes the file.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.mote.moted</string>
  <key>ProgramArguments</key><array><string>/Users/bilal/.cargo/bin/moted</string><string>serve</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
  <key>ThrottleInterval</key><integer>5</integer>
  <key>ProcessType</key><string>Background</string>
  <key>WorkingDirectory</key><string>/Users/bilal/Library/Application Support/mote</string>
  <key>StandardOutPath</key><string>/Users/bilal/Library/Application Support/mote/moted.out.log</string>
  <key>StandardErrorPath</key><string>/Users/bilal/Library/Application Support/mote/moted.err.log</string>
  <key>EnvironmentVariables</key><dict><key>PATH</key><string>/Users/bilal/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin</string></dict>
</dict></plist>
```

`KeepAlive` with `SuccessfulExit false` restarts the daemon only after a non-zero exit. `moted shutdown` exits 0 and stays down until login or `launchctl kickstart -k gui/$UID/com.mote.moted`. `PATH` must name `claude`, `ollama`, `gh` and `overseerd` because launchd does not read the shell profile. `launchctl print gui/$UID/com.mote.moted` shows state and last exit status.

systemd user unit (Pi):

```ini
[Unit]
Description=mote daemon
After=network-online.target

[Service]
Type=notify
ExecStart=%h/.cargo/bin/moted serve
Restart=always
RestartSec=2
WatchdogSec=30
Environment=PATH=%h/.local/bin:/usr/local/bin:/usr/bin:/bin

[Install]
WantedBy=default.target
```

`moted serve` calls `sd_notify(READY=1)` after the socket binds and `WATCHDOG=1` every 10 s through the `sd-notify` crate. `loginctl enable-linger $USER` keeps the user manager alive without a login. `systemctl --user enable --now moted` starts it. `journalctl --user -u moted -f` follows the log.

Cron watcher. Table `schedules(id, expr, tz, prompt, enabled, last_fired_ms)`. On start and after every change the watcher computes the next occurrence per row with croner, sleeps until the earliest with `tokio::time::sleep_until`, fires, records `last_fired_ms`, and recomputes. A clock jump (sleep, wake, DST) recomputes from now and fires each schedule at most once.

Slack watcher. `apps.connections.open` with the `xapp-` token returns a WebSocket URL. Each envelope is acknowledged by `envelope_id`. `app_mention` events need `app_mentions:read`. Replies need `chat:write`. Reading a thread needs `channels:history`. Slack sends `disconnect` warnings about 10 s before closing, so the client reconnects on that signal. Each mention becomes an utterance with channel, thread ts, user and text.

GitHub watcher. `gh api notifications` with `If-Modified-Since` from the last `Last-Modified`. A 304 is free. The poll interval is `X-Poll-Interval` or 60 s. Each new notification becomes an utterance. `gh auth token` is never read by mote because `gh` runs as a subprocess with its own login.

Approval gate. Table `approvals(id, thread_id, kind, target, payload, status, created_ms, decided_ms, expires_ms)`. `kind` is `slack_post`, `pr_comment`, `email` or `other`. The window renders a card with Approve, Edit and Deny. The executor runs only rows in `approved` state, writes a ledger row, and moves the row to `done`. A sweeper marks `pending` rows past `expires_ms` as `expired`. A watcher can never write an approval as `approved`.

Supervision. moted keeps one tokio task per watcher. A task that returns an error restarts after backoff from 1 s to 60 s and emits `watcher_restart`. Child processes use `tokio::process::Command` with `kill_on_drop(false)` and `process_group(0)` so they outlive a daemon restart when that is wanted, and `nix::sys::signal::kill` for signals.

## Candidates

| name | repo | language | license | last release | verdict | reason | pin |
| --- | --- | --- | --- | --- | --- | --- | --- |
| launchd | Apple | n/a | n/a | macOS built in | embed | The platform supervisor. `KeepAlive`, logs, restart throttle. | n/a |
| systemd user units | systemd.io | n/a | LGPL-2.1 | Pi OS built in | embed | `Restart=always`, watchdog, journald. | n/a |
| plist | github.com/ebarnard/rust-plist | Rust | MIT | 1.10.1, 2026-09-06 | embed | Serde writer for the agent plist. | 1.10.1 |
| service-manager | github.com/chipsenkbeil/service-manager-rs | Rust | MIT or Apache-2.0 | 0.11.0, 2026-02-18 | skip | Abstracts launchd and systemd but hides the plist keys mote needs to control. | none |
| launchd (crate) | crates.io/crates/launchd | Rust | MIT | 0.3.0, 2023-08-08 | skip | Stale. | none |
| sd-notify | github.com/lnicola/sd-notify | Rust | MIT or Apache-2.0 | 0.5.0, 2026-03-09 | embed on Linux | `READY=1`, `WATCHDOG=1`, `watchdog_enabled()`. libc only. | 0.5.0 |
| croner | github.com/hexagon/croner-rust | Rust | MIT | 4.0.1, 2026-10-02 | embed | Parser and next occurrence, time zones, DST rules documented. | 4.0.1 |
| cron | github.com/zslayton/cron | Rust | MIT or Apache-2.0 | 0.17.0, 2026-06-18 | skip | Seven-field syntax, parser only. croner covers it. | none |
| tokio-cron-scheduler | github.com/mvniekerk/tokio-cron-scheduler | Rust | MIT or Apache-2.0 | 0.15.1, 2025-10-28 | skip | Single maintainer, persistence only for Postgres and NATS, wraps croner anyway. | none |
| slack-morphism | github.com/abdolence/slack-morphism-rust | Rust | Apache-2.0 | 2.30.0, 2026-10-08 | embed | Socket Mode client, Web API, Block Kit, maintained. | 2.30.0 |
| tokio-tungstenite | github.com/snapview/tokio-tungstenite | Rust | MIT | 0.30.0, 2026-07-11 | skip | slack-morphism brings its own WebSocket. | none |
| `gh` CLI | github.com/cli/cli | Go | MIT | installed | embed as subprocess | Logged in already. `gh api` honors the notifications headers. | installed |
| octocrab | github.com/XAMPPRocky/octocrab | Rust | MIT or Apache-2.0 | 0.54.3, 2026-10-08 | skip in v0 | Typed client for later. Would need its own token. | none |
| backon | github.com/Xuanwo/backon | Rust | Apache-2.0 | 1.6.0, 2025-10-18 | embed | Watcher and child restart backoff. | 1.6.0 |
| nix | github.com/nix-rust/nix | Rust | MIT | 0.31.3, 2026-05-11 | embed | `kill`, `setsid`, signal names. | 0.31.3 |
| notify-rust | github.com/hoodie/notify-rust | Rust | MIT or Apache-2.0 | 4.18.2 | skip | mote's own window shows notices. Overseer's notifier app pattern can come later. | none |
| Overseer `background.rs` | ~/projects/overseer | Rust | MIT | a7a5cac0 | port the idea | One notice per quit, grace period, `stop_all` escalation SIGINT, SIGTERM, SIGKILL. | a7a5cac0 |

## Recommendation

```toml
# moted/Cargo.toml
[dependencies]
plist = "1.10.1"
croner = "4.0.1"
backon = "1.6.0"
nix = { version = "0.31.3", features = ["signal", "process"] }
slack-morphism = { version = "2.30.0", features = ["hyper", "socket_mode"], optional = true }

[target.'cfg(target_os = "linux")'.dependencies]
sd-notify = "0.5.0"

[features]
slack = ["dep:slack-morphism"]
```

Commands the owner runs once:

```bash
moted install                                   # writes the plist, bootstraps it
launchctl print gui/$(id -u)/com.mote.moted     # state, pid, last exit
launchctl kickstart -k gui/$(id -u)/com.mote.moted
moted uninstall                                 # bootout and remove
# Pi
moted install && loginctl enable-linger "$USER" && systemctl --user enable --now moted
```

Slack app setup, once: create the app, enable Socket Mode, add an app-level token with `connections:write`, add bot scopes `app_mentions:read`, `chat:write`, `channels:history`, subscribe to `app_mention`, install to the workspace, invite the bot to the channels. mote reads both tokens from its settings, never from the environment of a model call.

## Efficiency targets and what to measure

- Idle: RSS under 60 MB and CPU under 0.5% over 10 minutes with all watchers on and no local model loaded. `ps -o rss,%cpu` sampled every 30 s.
- Cron: fires within 1 s of the scheduled second.
- Slack: mention to router under 2 s. Reconnect after a Slack `disconnect` under 5 s.
- GitHub: one request per poll interval, never more than 60 per hour per watcher, well inside the 5,000 per hour limit.
- Restart: launchd relaunch within 10 s of a crash. Clients replay from their cursor.
- Approval sweep: once per minute, under 1 ms per row.

## Open decisions with defaults

1. Label. Default: `com.mote.moted`. The owner may rename the product and the label.
2. KeepAlive shape. Default: `SuccessfulExit false`. A clean `moted shutdown` stays down. A crash restarts.
3. Slack delivery. Default: Socket Mode. The Events API waits for the relay (DEF-02).
4. GitHub delivery. Default: polling with `gh`. Webhooks wait for the relay.
5. Approval expiry. Default: 24 h. Expired actions never run and the card says so.
6. Missed cron runs after sleep. Default: fire once if the miss is under 1 h, else skip and log `schedule_skipped`.
7. Pi watchdog. Default: `WatchdogSec=30` with a 10 s heartbeat.
8. Who starts overseerd. Default: not mote. The work lane offers a Start action (see `work.md`).
9. Conversation history polling. Default: none. New non-Marketplace Slack apps get 1 `conversations.history` call per minute with 15 messages, so mote relies on events and reads threads only on demand.
10. Watcher list. Default: cron, Slack mentions, GitHub notifications. Email and calendar are later watchers behind their own criteria.

## Acceptance criteria

- **ALWAYS-01** `moted install` writes the plist with `Label`, `ProgramArguments`, `RunAtLoad`, `KeepAlive {SuccessfulExit: false}`, `ThrottleInterval 5`, `PATH`, and log paths under the data directory, then bootstraps it. **Verify:** `plutil -lint` passes and `launchctl print gui/$UID/com.mote.moted` shows `state = running`.
- **ALWAYS-02** The daemon survives window close and reboot. **Verify:** `launchctl list | grep mote` after a reboot (MOTE-20).
- **ALWAYS-03** `moted uninstall` boots the agent out and removes the plist. **Verify:** `launchctl print` reports not found and the file is gone.
- **ALWAYS-04** After `kill -9` launchd restarts moted within 10 s and the window replays from its cursor with nothing shown twice. **Verify:** kill, watch `launchctl print` for a new pid, compare the window with the log.
- **ALWAYS-05** On Linux `moted install` writes the user unit with `Restart=always`, `Type=notify`, `WatchdogSec=30`, calls `READY=1` after bind, and prints the linger command. **Verify:** `systemd-analyze verify --user moted.service` on the Pi or a Debian container. Marked unverified on macOS.
- **ALWAYS-06** The cron watcher fires each enabled schedule within 1 s of its second, emits `utterance` with source `watcher:cron`, and survives restart with `last_fired_ms`. **Verify:** `cargo test -p moted watchers::cron` under paused tokio time.
- **ALWAYS-07** With tokens, the Slack watcher connects in Socket Mode, acknowledges every envelope, and turns `app_mention` into an utterance with channel, thread ts, user and text. Without tokens it emits `watcher_status disabled` and the window shows it. **Verify:** scripted test against a local WebSocket fixture, plus one live mention when the owner provides tokens (MOTE-21).
- **ALWAYS-08** The GitHub watcher polls `gh api notifications` with `If-Modified-Since`, honors `X-Poll-Interval`, and turns each new notification into an utterance. **Verify:** a fake `gh` on `PATH` returns fixtures and headers, and the test counts calls.
- **ALWAYS-09** Every outward action inserts an `approvals` row, the window shows Approve and Deny, nothing executes before `approved`, and a row past 24 h becomes `expired`. **Verify:** test posts to a wiremock Slack API through the gate for approved, denied and expired paths.
- **ALWAYS-10** Watchers hold no write-capable client. **Verify:** a test lists the Slack and GitHub methods each watcher can call and asserts all are read methods.
- **ALWAYS-11** Idle RSS is under 60 MB and CPU under 0.5% over 10 minutes with watchers on. **Verify:** `ps` samples in the evidence record.
- **ALWAYS-12** After a 2 h clock jump the cron watcher recomputes and fires each schedule at most once. **Verify:** paused-time test advancing 2 h.
- **ALWAYS-13** A watcher task that fails restarts with backoff from 1 s to 60 s and emits `watcher_restart`. **Verify:** test with a watcher that fails three times then succeeds.

## Sources

- launchd.plist keys: https://keith.github.io/xcode-man-pages/launchd.plist.5.html
- launchctl bootstrap, bootout, kickstart, print: https://keith.github.io/xcode-man-pages/launchctl.1.html
- Apple Daemons and Services guide: https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html
- systemd.service Restart, Type, WatchdogSec: https://man7.org/linux/man-pages/man5/systemd.service.5.html
- systemd.unit user paths: https://man7.org/linux/man-pages/man5/systemd.unit.5.html
- loginctl enable-linger: https://man7.org/linux/man-pages/man1/loginctl.1.html
- Raspberry Pi linger note: https://www.raspberrypi.com/documentation/services/connect.html
- sd-notify crate: https://crates.io/crates/sd-notify
- plist crate: https://crates.io/crates/plist
- service-manager crate: https://github.com/chipsenkbeil/service-manager-rs
- croner: https://github.com/hexagon/croner-rust
- cron crate: https://github.com/zslayton/cron
- tokio-cron-scheduler: https://github.com/mvniekerk/tokio-cron-scheduler
- tokio interval: https://docs.rs/tokio/latest/tokio/time/fn.interval.html
- Slack Events API: https://docs.slack.dev/apis/events-api/
- Slack Socket Mode: https://docs.slack.dev/apis/events-api/using-socket-mode
- Slack app_mention: https://docs.slack.dev/reference/events/app_mention
- Slack rate limit change 2025-05-29: https://docs.slack.dev/changelog/2025/05/29/rate-limit-changes-for-non-marketplace-apps
- slack-morphism: https://github.com/abdolence/slack-morphism-rust
- GitHub notifications polling: https://docs.github.com/en/rest/activity/notifications
- GitHub REST rate limits: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
- GitHub webhooks: https://docs.github.com/en/webhooks/about-webhooks
- gh api: https://cli.github.com/manual/gh_api
- octocrab: https://crates.io/crates/octocrab
- Claude Code permission prompt tool and `--permission-prompts`: https://code.claude.com/docs/en/cli-reference
- Agent SDK canUseTool and deferred permissions: https://code.claude.com/docs/en/agent-sdk/user-input
- OpenAI Agents SDK human in the loop: https://openai.github.io/openai-agents-python/human_in_the_loop/
- tokio process Command: https://docs.rs/tokio/latest/tokio/process/struct.Command.html
- nix signal kill: https://docs.rs/nix/latest/nix/sys/signal/fn.kill.html
- backon: https://crates.io/crates/backon
- Overseer `daemon/src/background.rs` at `a7a5cac0` (local, read-only)
