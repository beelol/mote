# Reuse: browser use in two modes

Promise 18 in `docs/concepts.md`. Decision row "Browser use" in `docs/rfcs/mote.md`. Researched
2026-10-08 against Chrome 154 on the owner's Mac, Claude Code 2.1.289 and the crates named below.
Cloud mode runs inside mote-cloud, the engine, on the Mac today and a server later. Local mode is
lent by mote-client on the owner's Mac behind an owner switch. mote-ui draws what the browser found.

## Key points

1. One Rust crate, `mote-browser`, drives Chrome over the DevTools Protocol with `chromiumoxide` 0.9.1. The same crate runs inside mote-cloud (cloud mode) and inside mote-client (local mode).
2. The browser binary is Chrome for Testing, pinned by version, downloaded by `moted browser install`. Since Chrome 136 the owner's normal Chrome ignores `--remote-debugging-port` on its default profile, so an agent needs its own `--user-data-dir` or the owner's consent through `chrome://inspect`.
3. A character reads a page as an accessibility snapshot with refs (`e12 button "Place order"`) and acts on refs. Screenshots are the fallback. Every leading agent (Playwright MCP, Claude Code's browser pane, agent-browser, browser-use) works this way.
4. Sessions live in a dedicated profile the owner signs into once per site. mote never reads the Chrome default profile. The cloud profile gets only sites the owner lends by explicit cookie export, never a silent sync.
5. Every irreversible click (submit, send, buy, delete, post, consent, download, OAuth grant) becomes an `approvals` row from `always-on.md`. The gate is Rust code. Page content cannot reach it.
6. The think lane classifies each click with a structured `{"effect": "read" | "fill" | "commit"}` verdict parsed fail-closed to `commit`. Structural facts (a submit button inside a form, a POST from the page, a download event) force `commit` whatever the verdict says.
7. A browser task is a record in the thread's working set. When mote-client connects or disconnects, the task moves hosts. The record moves. Cookies never move.
8. A coverage map tells the owner which sites each mode can reach as the owner. The window and the phone show it and mote says plainly when a site is out of reach.
9. Measured anchors: Online-Mind2Web with human grading put Operator at 61.3%. Self-reported WebVoyager scores above 85% are inflated, per the Online-Mind2Web paper. Anthropic's browser red team went from 23.6% to 11.2% attack success with mitigations, and to 0% on four browser-specific attacks.

## What browser use needs technically

**Modes and names.** mote-cloud owns every browser task. Cloud mode runs `mote-browser` in the mote-cloud process with a headless Chrome for Testing and a per-character profile under the data directory. Local mode runs `mote-browser` in mote-client on the Mac with a headed Chrome for Testing and the owner's lent sessions. mote-client advertises `browser.local` over the gateway (`hardware.md`) only while the owner's switch is on. Each step is a request from mote-cloud to the host that owns the task. The think lane sees one tool set either way.

**Driver.** `chromiumoxide::Browser::launch(config)` starts Chrome with `--user-data-dir=<data_dir>/browser/<character>/<mode>` and a loopback debugging port. `Browser::connect(ws_url)` attaches to a running endpoint, which is how a hosted browser or the owner's own Chrome is reached. The crate is tokio only and ships `get_cookies` and `set_cookies` on the browser. The CDP calls behind each tool: `Accessibility.getFullAXTree` for the snapshot, `DOM.scrollIntoViewIfNeeded` plus `DOM.getBoxModel` plus `Input.dispatchMouseEvent` for a click on a ref, `Input.insertText` for typing, `Page.captureScreenshot` for pixels, `Page.navigate` and `Page.navigateToHistoryEntry` for moving, `Runtime.evaluate` for a same-origin `fetch` in the page context, `Browser.setDownloadBehavior` with `eventsEnabled` plus `Browser.downloadProgress` for downloads, and `Network.*` events for the HAR.

**Reading pages.** The snapshot is one line per node: `<ref> <role> "<name>" [value] [state]`. Refs map to `backendDOMNodeId` and die on navigation. The interactive-only filter is the default. The full tree is a flag. `browser_find(query)` returns up to 20 refs whose role or name contains the query. `browser_text()` returns main-content text for reading, not for acting. A screenshot is taken when the snapshot has fewer than three interactive nodes, when the think lane asks, or before and after every `commit` step as evidence. Claude counts an image as `ceil(w/28) * ceil(h/28)` tokens, so a 1280 by 800 screenshot costs about 1,334 tokens. Published snapshot sizes range from 3,000 to 8,000 tokens on ordinary pages and pass 100,000 on dense ones. The interactive filter and a per-call cap therefore matter more than the choice between text and pixels. No controlled head-to-head measurement exists. mote measures its own.

**Actions and the gate.** Read actions (`open`, `snapshot`, `find`, `text`, `screenshot`, `back`, `scroll`) never ask. Fill actions (`type`, `select`, `click` with effect `fill`) run on sites the owner allowed once or always, like Claude Code's `Allow once` and `Always allow`. Commit actions insert an `approvals` row with kind `browser_<effect>` and the target URL, the ref line, and the before screenshot, and wait. The driver forces `commit` when the ref is a submit control inside a `form`, when the URL host is a known OAuth or payment authority (`accounts.google.com`, `/oauth/authorize`, `checkout.`), when a download event fires, or when the verdict fails to parse. The owner can mark a site `always_ask` and then every click waits.

**Sessions.** Local: the owner opens the headed profile from the window, signs in to a site once, and the session stays in that profile. Attaching to the owner's real Chrome instead needs Chrome 144 or later, the `chrome://inspect/#remote-debugging` toggle, a consent dialog on every connection and a banner during the session. One report says the JSON discovery endpoint returns 404 in that mode. How a client finds the WebSocket URL there is unknown. Cloud: the owner signs in through a screencast card (`Page.startScreencast` frames drawn by mote-ui, input forwarded) or lends a site by name. Lending reads `Storage.getCookies` for that domain from the local profile, sends them over the Noise gateway, and calls `Storage.setCookies` in the cloud profile. Each lend is an approval row with the domain and the cookie count. The Chrome default profile is never read. Its cookies sit behind the keychain item "Chrome Safe Storage" and reading it is an infostealer pattern.

**Downloads and same-origin calls.** A download is a `commit` action. In local mode the file lands under `~/Downloads/mote/<task>` on the Mac. In cloud mode it lands under the cloud data directory and the window offers Fetch. `browser_fetch(url, method)` runs `fetch` inside the page so the site's cookies apply. `GET` and `HEAD` are read actions. Other methods are `commit`. The response body goes through `mote-redact` before it enters a tool result.

**Evidence.** Every step writes a `browser_step` event with task id, host, mode, action, ref line, URL, latency and token estimate. Every `commit` keeps a before and an after screenshot. Every task keeps one HAR built from `Network` events with the `har` crate types, with cookies, authorization headers and form bodies redacted. The ledger row for each tool call carries the snapshot token count.

**Approval on the phone.** When the Mac is off the approval row must reach the owner. Target: the phone client from `hardware.md` over the gateway with push, behind DEF-02. Interim: a Slack direct message with Approve and Deny buttons through the Socket Mode client in `always-on.md`, allowed only for kinds the owner listed under `approvals.slack_kinds`, never for `buy` or `send`. Unanswered rows expire after 24 h as today.

**Handoff.** The record is `browser_task { id, thread_id, goal, site, mode, host_id, url, step_no, form_state, pending_approval_id, evidence_ids, coverage_state }`. `form_state` holds field label, ref line and a redacted value per field the character filled. When mote-client connects, mote-cloud moves tasks whose site is `signed_in` only locally to the local host. When it disconnects mid-task, the task goes to `rehosting`, the cloud host opens `url`, takes a snapshot, and the think lane refills from `form_state`. A pending approval survives the move. Open tabs, cookies, local storage and downloads never move. The owner's DoorDash question runs this way. The cloud coverage map says `doordash.com signed_out`. The local map says `signed_in`. The task waits for mote-client while the switch is off. The local host opens the orders page. The think lane reads the order status from the snapshot. mote-ui draws a map card, a provider widget or a food bag with the minutes underneath.

**Coverage map.** Table `site_coverage(site, mode, state, checked_ms, reason)` with states `signed_in`, `signed_out`, `blocked`, `unknown`. A probe opens the site's configured account URL. A redirect whose final URL matches the site's configured login URL is `signed_out`. Otherwise the think lane emits `{"state": "...", "reason": "..."}` and anything malformed is `unknown`. The window shows the map per mode and the character says "I cannot see your DoorDash orders from the cloud. Turn on the Mac switch or sign in to DoorDash in the cloud profile."

## Candidates

| name | repo | language | license | last release | verdict | reason | pin |
| --- | --- | --- | --- | --- | --- | --- | --- |
| chromiumoxide | github.com/mattsse/chromiumoxide | Rust | MIT or Apache-2.0 | 0.9.1, 2026-02-25 | embed | Async CDP, tokio only, `launch` and `connect`, cookies, full protocol bindings. | 0.9.1 |
| headless_chrome | github.com/rust-headless-chrome/rust-headless-chrome | Rust | MIT | 1.0.22, 2026-06-11 | skip | Sync API on threads. Has `connect(ws_url)`. Keep as fallback if chromiumoxide stalls. | none |
| fantoccini | github.com/jonhoo/fantoccini | Rust | MIT or Apache-2.0 | 0.22.1, 2026-02-28 | skip | WebDriver classic over chromedriver. No CDP events, no BiDi. | none |
| thirtyfour | github.com/stevepryde/thirtyfour | Rust | MIT or Apache-2.0 | 0.37.7, 2026-10-08 | later | WebDriver with a BiDi module. The Firefox path when needed. | 0.37.7 |
| rustenium | github.com/dashn9/rustenium | Rust | unknown | 1.1.14, 2026-09-19 | skip | BiDi plus CDP, young, one author. | none |
| Playwright via Node sidecar | github.com/microsoft/playwright | TypeScript | Apache-2.0 | rolling | skip | Needs Node and a sidecar for what CDP gives directly. | none |
| Chrome for Testing | googlechromelabs.github.io/chrome-for-testing | binary | Chrome terms | 155.0.8059.39 stable | embed | Keeps the pre-136 debugging behaviour. Builds for mac-arm64, linux64 and linux-arm64. | 155.0.8059.39 |
| Owner's Chrome via `chrome://inspect` | developer.chrome.com | binary | Chrome terms | Chrome 154 installed | option behind switch | Chrome 144+ toggle, consent dialog per connection, banner. Endpoint discovery unknown. | n/a |
| Claude in Chrome through `claude --chrome` | claude.com/chrome | extension | Anthropic terms | installed | option, unverified | Real sessions plus Anthropic's classifiers. Behaviour under `claude -p` is unknown. | n/a |
| Claude Code built-in browser pane | claude.com/docs/third-party/claude-desktop/browser | app | Anthropic terms | 2.1.289 | port the idea | `read_page`, `find`, `get_page_text`, refs, Allow once and Always allow, Keep cookies. | n/a |
| chrome-devtools-mcp | github.com/ChromeDevTools/chrome-devtools-mcp | TypeScript | Apache-2.0 | v1.10.1, 2026-09-23 | port the idea | Google's own uid snapshots, `--autoConnect`, `--isolated`, security notes. | none |
| Playwright MCP | github.com/microsoft/playwright-mcp | TypeScript | Apache-2.0 | v0.0.83, 2026-09-28 | port the idea | ARIA snapshot with refs. Measured 12.9k to 14.1k input tokens per call in one study. | none |
| agent-browser | github.com/vercel-labs/agent-browser | Rust | Apache-2.0 | v0.38.2, 2026-10-01 | port the idea | Native Rust CDP daemon, `snapshot -i`, `@e1` refs, `--delta`, `DevToolsActivePort` auto-connect, 7 MB install. | none |
| browser-use | github.com/browser-use/browser-use | Python | MIT | 0.13.11, 2026-10-07 | port the idea | DOM distillation to indexed elements. 89.1% WebVoyager self-reported on 586 tasks. | none |
| Stagehand | github.com/browserbase/stagehand | TypeScript | MIT | 3.7.3, 2026-08-28 | skip | `act`, `extract`, `observe` over Playwright. 6.9k tokens per call measured. No public benchmark row. | none |
| Skyvern | github.com/Skyvern-AI/skyvern | Python | AGPL-3.0 | v1.0.55, 2026-10-01 | skip | 85.85% WebVoyager self-reported. AGPL. | none |
| Magnitude | github.com/magnitudedev/magnitude | TypeScript | Apache-2.0 | cli 0.2.6, 2026-10-05 | skip | 93.9% WebVoyager self-reported. Vision-first. | none |
| Notte | github.com/nottelabs/notte | Python | unknown | v1.9.7, 2026-10-08 | skip | 86.2% WebVoyager self-reported. Hosted. | none |
| Agent-E | github.com/EmergenceAI/Agent-E | Python | MIT | no release, pushed 2026-05 | port the idea | DOM distillation, 73.1% WebVoyager and 73.0% WebArena. Dormant. | none |
| Lightpanda | github.com/lightpanda-io/browser | Zig | AGPL-3.0 | nightly | skip | No rendering, no screenshots, partial CDP, beta. | none |
| Steel self-hosted | github.com/steel-dev/steel-browser | TypeScript | Apache-2.0 | v0.5.4-beta, 2026-08-25 | cloud option | CDP endpoint in Docker, profiles. Cloud tiers about $0.05 to $0.10 per browser hour. | none |
| Browserless | github.com/browserless/browserless | TypeScript | SSPL-1.0 or commercial | v2.57.0, 2026-09-30 | skip | SSPL. | none |
| Browserbase, Hyperbrowser, Anchor, Kernel | vendor sites | hosted | commercial | 2026 | hosted option | Persistent profiles. Overage about $0.10 to $0.12, $0.10, $0.05 plus $0.01 per start, and unknown per hour. | none |
| har | github.com/mandrean/har-rs | Rust | MIT or Apache-2.0 | 0.9.0, 2026-03-22 | embed | Serde types for the HAR evidence file. | 0.9.0 |
| Online-Mind2Web, OSWorld-Verified, BrowserGym | github.com/OSU-NLP-Group/Online-Mind2Web | Python | MIT, Apache-2.0 | 2026 | port the idea | WebJudge grading at about 85% human agreement. OSWorld-Verified leaders near 85%. Measure mote the same way. | none |

## Recommendation

```toml
# crates/mote-browser/Cargo.toml
[dependencies]
chromiumoxide = { version = "=0.9.1", default-features = false, features = ["tokio-runtime"] }
har = "=0.9.0"
tokio = { version = "1.53", features = ["process", "sync", "time", "fs", "macros"] }
serde = { version = "=1.0.228", features = ["derive"] }
serde_json = "=1.0.151"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }
mote-redact = { path = "../mote-redact" }
```

Chrome launch, both modes. Cloud adds `--headless=new`:

```text
<data_dir>/browser/bin/chrome-155.0.8059.39/chrome \
  --user-data-dir=<data_dir>/browser/<character>/<mode> \
  --remote-debugging-port=0 --remote-debugging-address=127.0.0.1 \
  --no-first-run --no-default-browser-check --window-size=1280,800
```

Commands: `moted browser install` fetches Chrome for Testing from the last-known-good JSON and records the version. `moted browser signin <site>` opens the headed profile at the site's login URL. `moted browser lend <site>` exports that site's cookies to the cloud profile behind an approval. `moted browser coverage` prints the map per mode. `moted bench browser` prints snapshot tokens and step latency over the seed sites.

Tools in the single `moted mcp` server from `think.md`, registered only when a browser host is online: `browser_open`, `browser_snapshot`, `browser_find`, `browser_text`, `browser_click`, `browser_type`, `browser_select`, `browser_screenshot`, `browser_fetch`, `browser_download`, `browser_back`, `browser_coverage`. `browser_click` takes `{ref, effect, reason}`. The server routes each call to the task's host over the gateway and writes a ledger row and a `browser_step` event. Claude Code's own browser tools are not reused in v0. The think lane runs headless under `claude -p` and the pane belongs to the desktop app.

Verdict parsing follows `router.md`:

```rust
#[derive(serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect { Read, Fill, Commit }

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClickVerdict { pub r#ref: String, pub effect: Effect, pub reason: String }

pub fn effect_of(text: &str, structural_commit: bool) -> Effect {
    if structural_commit { return Effect::Commit; }
    serde_json::from_str::<ClickVerdict>(text).map(|v| v.effect).unwrap_or(Effect::Commit)
}
```

Profile layout: `<data_dir>/browser/<character>/local` on the Mac, `<data_dir>/browser/<character>/cloud` on mote-cloud, mode 0700, one Chrome per profile, Chrome closed after 10 idle minutes and relaunched on demand.

## Safety rules

1. mote never reads the Chrome default profile, its cookie database or the "Chrome Safe Storage" keychain item.
2. mote never creates accounts, never types a password, a card number or a government id, and never solves or bypasses a CAPTCHA. The character says so and asks the owner to do it in the headed profile.
3. Every `commit` effect waits for an `approvals` row. A watcher, a page, a tool result or a Slack message can never write `approved`.
4. Submit controls inside forms, OAuth and payment hosts, non-GET `browser_fetch`, downloads and consent banners are `commit` regardless of the verdict. A malformed verdict is `commit`.
5. Page text enters a model call only inside a tool result wrapped with a per-turn random boundary marker and labelled as data. Instructions found in page text are quoted back to the owner, never acted on.
6. Allowlist edits, `always_ask` edits, lends and the local switch happen only in the window or the phone client, never through a tool.
7. Consent banners get the most private option when the owner has allowed the site, and otherwise wait.
8. Lends are per site, per approval, with the cookie count shown, and expire with the cookies.
9. Every screenshot, HAR and tool result passes `mote-redact` before storage. Form values in `form_state` are redacted for fields whose ref is a password or has a secret-like label from the structural `type` attribute, never from a word list.
10. A site the coverage map marks `blocked` or `signed_out` yields a plain "cannot reach" card, never a retry with another profile.

## Efficiency targets and what to measure

| Metric | Target | How measured |
| --- | --- | --- |
| Snapshot tokens, interactive filter, 10 seed sites | p50 at most 4,000, p95 at most 12,000 | `moted bench browser`, ledger token estimate |
| Screenshot tokens at 1280 by 800 | about 1,334 per image | `ceil(w/28) * ceil(h/28)` |
| Snapshot latency after load | p50 at most 300 ms | `browser_step` latency |
| Step latency, click or type to new snapshot | p50 at most 1.5 s | same |
| Warm launch to first page | at most 2 s on the Mac, unknown on the server | bench |
| Chrome RSS with 3 tabs | at most 600 MB on the Mac, unknown on a Pi | `ps -o rss` over the process tree |
| Hosted browser cost | $0 self-hosted, at most $0.12 per hour hosted | vendor invoice |
| Approval round trip, window or phone online | at most 10 s | approval row timestamps |
| Handoff, client connect to first snapshot on the new host | at most 5 s | `browser_task` event timestamps |
| Task success on an owner-written 20-task set | at least 70% with human grading | WebJudge-style rubric in the evidence file |

## Open decisions with defaults

1. Local attach path. Default: the dedicated headed profile with one sign-in per site. Attaching to the owner's running Chrome waits until endpoint discovery under `chrome://inspect` is verified.
2. `claude --chrome` in the think lane. Default: off. Verify whether `claude -p --chrome` exposes the Claude in Chrome tools before any use.
3. Cloud browser. Default: Chrome for Testing headless inside mote-cloud. A hosted service only when the server cannot run Chrome.
4. Cloud sign-in. Default: screencast card. Cookie lending for sites the owner names.
5. Phone approval transport. Default: phone client over the gateway behind DEF-02. Slack buttons as the interim for kinds in `approvals.slack_kinds`, empty by default.
6. Snapshot shape. Default: one line per node with refs, interactive filter on, 12,000 token cap with a truncation notice.
7. Screenshot policy. Default: before and after every `commit`, otherwise on request or when the snapshot is nearly empty.
8. Allowlist default. Default: `allow` on every site except a blocked list, with `always_ask` for bank, mail and recovery sites the owner names.
9. Firefox and WebKit. Default: none. Firefox dropped CDP in 141 and Safari has no BiDi, so both wait for `thirtyfour` BiDi.
10. Pi 5 as a browser host. Default: no. Chrome for Testing has a linux-arm64 build, and memory on the Pi is unmeasured.

## Acceptance criteria

- **BROWSE-01** `moted browser install` downloads the pinned Chrome for Testing for the host platform, verifies the version string, and `mote-browser` launches it with a per-character, per-mode `--user-data-dir` on a loopback port. **Verify:** `cargo test -p mote-browser launch::` with argv golden list, plus a live launch recorded in the evidence file.
- **BROWSE-02** `browser_snapshot` returns one line per node with stable refs, the interactive filter on by default, and a 12,000 token cap with a notice. **Verify:** fixture pages served locally, snapshot compared to golden files, token count asserted.
- **BROWSE-03** `browser_click`, `browser_type` and `browser_select` act on refs, a stale ref after navigation returns an error that names re-snapshot, and no action runs on a page without a current snapshot. **Verify:** fixture form test with navigation between steps.
- **BROWSE-04** A click whose verdict is `commit`, a malformed verdict, a submit control inside a form, a non-GET fetch, a download and an OAuth host each insert an `approvals` row and nothing happens until it is `approved`. **Verify:** one test per trigger with a fixture site, the row inspected, the page unchanged.
- **BROWSE-05** Read actions never insert approvals and fill actions run only on allowed sites. **Verify:** test with an allowed site, a blocked site and an `always_ask` site.
- **BROWSE-06** mote never opens the Chrome default profile path and never reads the keychain. **Verify:** a test asserts every `--user-data-dir` is under the data directory, and `log stream` during a live run shows no keychain prompt.
- **BROWSE-07** Page text reaches the think lane only inside a boundary-marked tool result, and an injected instruction on a fixture page is quoted in the card and not executed. **Verify:** fixture page with hidden text ordering a form submit, event log shows no `commit` row.
- **BROWSE-08** Every step writes a `browser_step` event and a ledger row with token estimate, every `commit` keeps before and after screenshots, and every task keeps a redacted HAR. **Verify:** `SELECT` after a fixture task, files present, `grep` finds no cookie or authorization header values.
- **BROWSE-09** The local headed profile keeps a sign-in across Chrome restarts and the cloud profile holds only lent sites. **Verify:** sign in to a fixture site, restart, snapshot shows the signed-in state, cloud profile cookie list contains only the lent domain.
- **BROWSE-10** `moted browser lend <site>` exports only that domain's cookies, needs an approval, and the row shows domain and cookie count. **Verify:** test over the gateway with a mock cloud host and a `SELECT` over approvals.
- **BROWSE-11** The coverage map records `signed_in`, `signed_out`, `blocked` or `unknown` per site and mode, a redirect to the configured login URL yields `signed_out`, and a malformed verdict yields `unknown`. **Verify:** fixture sites with and without a login redirect, map rows inspected.
- **BROWSE-12** A character asked about a site the current mode cannot reach replies with a card that names the mode and the action the owner can take. **Verify:** scripted model test with `doordash.com signed_out` in cloud and `signed_in` locally.
- **BROWSE-13** When mote-client connects, tasks whose site is reachable only locally move to the local host, and when it disconnects mid-task the cloud host reopens the URL, re-snapshots and refills from `form_state` within 5 s. **Verify:** test with two in-process hosts and a gateway cut at step 3 of a fixture form.
- **BROWSE-14** A pending approval survives a handoff and is answerable from the window, the phone client or Slack for listed kinds, and `buy` and `send` never accept Slack. **Verify:** approval created on one host, answered after handoff, Slack path refused for `buy`.
- **BROWSE-15** Cloud downloads land under the cloud data directory and the window offers Fetch, local downloads land under `~/Downloads/mote/<task>`. **Verify:** fixture download in each mode, paths asserted.
- **BROWSE-16** `browser_fetch` with GET runs in the page context with the site's cookies and non-GET methods need approval. **Verify:** fixture API that echoes cookies and method.
- **BROWSE-17** The DoorDash journey ends in a UI tree card with a map, a provider widget or a food bag and the minutes underneath, rendered by mote-ui from a fixture order page. **Verify:** screenshot of the card and the tree validated by `mote-ui`.
- **BROWSE-18** Snapshot p50 tokens are at most 4,000 and step p50 latency at most 1.5 s on the seed sites. **Verify:** `moted bench browser` output in the evidence file.

## Sources

- Chrome 136 remote debugging change: https://developer.chrome.com/blog/remote-debugging-port
- Chrome 144 `chrome://inspect` consent flow: https://developer.chrome.com/blog/chrome-devtools-mcp-debug-your-browser-session
- Default profile port report and 404 discovery: https://github.com/trycua/cua/issues/2916
- Chrome for Testing downloads: https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json and https://developer.chrome.com/docs/automation-and-testing/download-test-binaries
- chromiumoxide: https://github.com/mattsse/chromiumoxide and https://crates.io/crates/chromiumoxide
- headless_chrome: https://github.com/rust-headless-chrome/rust-headless-chrome
- fantoccini: https://crates.io/crates/fantoccini and thirtyfour BiDi: https://docs.rs/crate/thirtyfour/0.37.7
- WebDriver BiDi support table: https://caniuse.com/wf-webdriver-bidi and Firefox CDP removal: https://fxdx.dev/cdp-retirement-in-firefox/
- safaridriver: https://support.circleci.com/hc/en-us/articles/360056461992-Enabling-safaridriver-support-on-macOS
- Claude Code built-in browser and Claude in Chrome: https://claude.com/docs/third-party/claude-desktop/browser and https://code.claude.com/docs/en/whats-new/2026-w28
- Claude in Chrome safety and red team: https://www.anthropic.com/news/claude-for-chrome and https://support.claude.com/en/articles/12902428-using-claude-in-chrome-safely
- Brave on Comet injection: https://brave.com/blog/comet-prompt-injection/ and https://brave.com/blog/unseeable-prompt-injections/
- Spotlighting: https://arxiv.org/abs/2403.14720 and Prismata: https://arxiv.org/abs/2607.08147
- chrome-devtools-mcp: https://github.com/ChromeDevTools/chrome-devtools-mcp
- Playwright MCP snapshots: https://playwright.dev/mcp/introduction and token study: https://itnext.io/why-does-playwright-mcp-use-so-many-tokens-da56b6dbd2db
- Snapshot size estimates: https://prophetchrome.com/blog/accessibility-tree-vs-screenshots-browser-ai and https://morphllm.com/playwright-mcp
- agent-browser: https://github.com/vercel-labs/agent-browser and https://agent-browser.dev/native-mode
- Claude image tokens: https://platform.claude.com/docs/en/build-with-claude/vision
- Set-of-Mark: https://arxiv.org/abs/2310.11441
- WebVoyager: https://arxiv.org/abs/2401.13919 and leaderboard: https://leaderboard.steel.dev/leaderboards/webvoyager
- Online-Mind2Web paper and leaderboard: https://arxiv.org/abs/2504.01382 and https://leaderboard.steel.dev/leaderboards/online-mind2web
- HAL cost-aware board: https://hal.cs.princeton.edu/online_mind2web
- OSWorld-Verified: https://yutori.com/leaderboards/osworld-verified.md and https://leaderboard.steel.dev/leaderboards/osworld
- WebArena and BrowserGym: https://arxiv.org/abs/2307.13854 and https://arxiv.org/abs/2412.05467
- Operator launch numbers: https://en.wikipedia.org/wiki/OpenAI_Operator
- Gemini 2.5 Computer Use results: https://siliconangle.com/2025/10/07/googles-gemini-2-5-computer-use-model-can-navigate-web-like-human/
- browser-use, Skyvern, Magnitude, Notte, Agent-E, Stagehand: https://github.com/browser-use/browser-use, https://www.skyvern.com/blog/skyvern-2-0-webvoyager-benchmark-results/, https://github.com/magnitudedev/magnitude, https://github.com/nottelabs/notte, https://github.com/EmergenceAI/Agent-E, https://github.com/browserbase/stagehand
- Lightpanda: https://github.com/lightpanda-io/browser
- Steel: https://github.com/steel-dev/steel-browser and https://llms.steel.dev/articles/browserbase-vs-steel/
- Browserless licence: https://docs.browserless.io/enterprise/open-source
- Browserbase contexts and pricing: https://docs.browserbase.com/platform/browser/core-features/contexts.md and https://www.tinyfish.ai/blog/browserbase-pricing
- Hyperbrowser pricing: https://docs.hyperbrowser.ai/reference and Anchor: https://llms.anchorbrowser.io/anchor-browser-vs-browserbase
- Kernel: https://github.com/onkernel/kernel-images and https://steel.dev/blog/steel-vs-kernel
- CDP downloads and cookies: https://chromedevtools.github.io/devtools-protocol/tot/Browser/ and https://chromedevtools.github.io/devtools-protocol/tot/Storage/
- Chrome Safe Storage keychain item: https://github.com/kawakatz/macCookies
- har crate: https://crates.io/crates/har
- OpenAI dots approvals: https://help.openai.com/en/articles/20001530
- Local checks on 2026-10-08: `claude --help` shows `--chrome`, Chrome 154.0.8037.98 installed, crates.io versions, GitHub release tags.
