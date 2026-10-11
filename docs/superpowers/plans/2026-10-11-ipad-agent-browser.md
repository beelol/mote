# iPad Inferred UI and Browser Approval Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver IPAD-04 and IPAD-05 by enabling gated host-side Matrix inference and a Mac-hosted browser fixture whose outward actions require durable approval from the iPad Simulator.

**Architecture:** Finish the existing fail-closed inferred-UI path before adding browser work. Put Chrome for Testing and CDP behavior in a new `mote-browser` crate, generalize approvals into host-bound typed intents, and keep orchestration in `moted`. The iPad receives existing validated approval surfaces and returns typed decisions through the paired host transport.

**Tech Stack:** Rust 2021, existing Ollama client and Matrix compiler, Chrome for Testing 155.0.8059.39, chromiumoxide 0.9.1, Tokio/CDP, SQLite, existing Mote event log and Matrix renderer.

**Spec:** `docs/superpowers/specs/2026-10-11-ipad-host.md`

## Global Constraints

- Keep `models.local_only = true`; the only currently permitted surface model is `qwen3:4b`.
- Automatic surface inference remains disabled unless the fixed 200-prompt live gate reaches at least 190 first-try valid surfaces.
- Every model attempt uses the existing pack, ledger and redacted event boundary.
- Browser use is local Mac mode only with one separate profile per character under Mote's data directory.
- Navigation and browser effects are restricted to an explicit approved-origin allowlist. Redirects
  outside it fail closed.
- Pin Chrome for Testing to 155.0.8059.39 and chromiumoxide to 0.9.1.
- Never open or inspect the default Chrome profile or the Chrome Safe Storage keychain item.
- Submit, purchase, message, download, OAuth, non-GET and malformed or unknown outward actions are commit effects and remain inert until approval.
- Page text is untrusted tool output and cannot authorize actions or change the task.
- Live purchases, messages and external sites are excluded. Acceptance uses local fixtures only.
- Run `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before every commit.

## Review Focus

- An inference rejection or transport failure must preserve the prior committed surface and ordinary prose. Task 1 pins all terminal paths through the production turn.
- A live validity score below 95 percent must leave automatic inference disabled without blocking fixture-driven browser work. Task 2 records the score and gate state separately.
- A crash or repeated approval must not execute a fixture side effect twice. Task 4 pins claim, restart and duplicate-decision behavior.
- A stale node reference or changed page state must invalidate the pending action. Tasks 3 and 5 pin navigation invalidation and approval-time revalidation.
- Page prompt injection must remain quoted evidence and must never cause a browser commit or tool authorization. Task 5 pins the injection fixture end to end.
- Browser commits must be stopped at the network boundary before JavaScript, navigation, download or
  request dispatch can produce an effect. Tasks 3 and 5 pin this with CDP Fetch interception.

---

### Task 1: Production inferred-surface integration

**Files:**
- Modify: `crates/mote-testkit/src/lib.rs`
- Modify: `moted/src/main.rs`
- Modify: `moted/src/inferred_ui.rs`
- Create: `moted/src/surface_activation.rs`
- Test: `moted/src/main.rs` approval and turn tests

**Interfaces:**
- Consumes: existing `infer_surface_after_answer`, `CandidateStream`, retry budget, model boundary and Matrix commit path.
- Produces: one post-answer inference call site that receives request text, completed prose, prior
  canonical source and diagnostics after the context-store lock is released, plus a fail-closed
  `SurfaceActivationStore` read boundary.

- [ ] **Step 1: Add queued structured replies to the scripted model**

Allow deterministic response queues per model without changing existing fixture behavior.

- [ ] **Step 2: Write failing production-turn tests**

Test disabled-by-default, valid activation, failed activation and stale model, digest, corpus or
contract attestation. Then test prose before one surface, `{kind:none}` with no surface, invalid then
valid with one commit, two invalid replies preserving prior UI, transport failure preserving prose,
exact pack and ledger counts, stale event rejection and invented action rejection.

- [ ] **Step 3: Run the focused tests and confirm failure**

Run: `cargo test -p moted inferred_`
Expected: FAIL because `handle_turn` does not call the inference path.

- [ ] **Step 4: Call inference after a completed answer**

Release the shared `ContextStore` guard first. Carry the original request, completed prose and turn
sequence into the existing inference finalizer. Call it only when a durable attestation exactly
matches the selected model name and digest, corpus hash and compiler contract version. Do not call on
incomplete, failed or ineligible turns.

- [ ] **Step 5: Run inferred-UI and turn tests**

Run: `cargo test -p moted inferred_ && cargo test -p moted completed_`
Expected: PASS with one ledger row per attempt and no prior-surface mutation on failure.

- [ ] **Step 6: Commit the dormant production integration**

Keep runtime activation behind the new fail-closed attestation boundary. Run workspace test and
clippy, then commit `feat(ui): integrate inferred surfaces after replies`.

### Task 2: Live 200-prompt activation gate

**Files:**
- Modify: `moted/src/bench.rs` only if the existing output lacks an explicit activation decision
- Modify: `moted/src/surface_activation.rs`
- Create: `docs/verification/IPAD-04.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: the fixed 200-prompt corpus and `qwen3:4b` surface model.
- Produces: recorded first-try, retry and terminal validity counts plus a durable activation
  attestation bound to model name, model digest, corpus hash and compiler contract version.

- [ ] **Step 1: Run the fixed live corpus through the real boundary**

Run: `MOTE_LIVE=1 cargo run -p moted -- bench surface`

Use the exact schema, manifest, parser, resolver, lowerer and ledger path. The JSON result is appended
to the active data directory's `bench.jsonl`.

- [ ] **Step 2: Evaluate the activation predicate**

Write an active attestation only when first-try valid count is at least 190 of 200. If it is lower,
record the measured result, remove or supersede any stale attestation, keep IPAD-04 open and keep
production inference disabled. Any model, digest, corpus or contract change invalidates the prior
attestation.

- [ ] **Step 3: Capture Simulator success and failure states**

With scripted replies, capture success, one-retry recovery, prior-surface preservation and final prose fallback in the iPad Simulator. Live-model success does not replace these deterministic failure checks.

- [ ] **Step 4: Verify and conditionally close IPAD-04**

Record model digest, Ollama version, prompt corpus, counts, ledger queries, Simulator runtime and screenshots in `docs/verification/IPAD-04.md`. Update the checkbox only if every clause passes.

- [ ] **Step 5: Run verification and commit**

Run workspace test and clippy. Commit `test(ui): measure inferred Matrix validity` whether the gate passes or remains open.

### Task 3: Browser runtime and local fixtures

**Files:**
- Create: `crates/mote-browser/Cargo.toml`
- Create: `crates/mote-browser/src/lib.rs`
- Create: `crates/mote-browser/src/launch.rs`
- Create: `crates/mote-browser/src/snapshot.rs`
- Create: `crates/mote-browser/src/policy.rs`
- Create: `crates/mote-browser/src/evidence.rs`
- Create: `crates/mote-browser/tests/runtime.rs`
- Create: `crates/mote-testkit/src/browser_fixture.rs`
- Create: `crates/mote-testkit/fixtures/browser/**`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/mote-testkit/Cargo.toml`
- Modify: `crates/mote-testkit/src/lib.rs`

**Interfaces:**
- Produces: `BrowserHost`, `BrowserSession`, `BrowserCommand`, `BrowserResult`, `BrowserSnapshot`,
  `NodeRef`, `BrowserEffect::{Read, Fill, Commit}`, `ApprovedOrigins`, `BrowserPolicy` and
  `BrowserEvidence`.

- [ ] **Step 1: Write failing launch and policy tests**

Assert a headed launch with profile path `<data>/browser/<character>/local`, loopback CDP,
`--use-mock-keychain`, exact pinned version and refusal of default-profile or arbitrary attach paths.
Capture and assert the exact process arguments. Table-test unapproved origins, off-origin redirects,
submit, purchase, message, download, OAuth, non-GET and malformed verdicts as blocked or `Commit`.

- [ ] **Step 2: Write failing snapshot and stale-ref tests**

Assert stable refs within one document, interactive filtering, token cap notice, read and fill behavior, ref invalidation after navigation, and refusal to act without a current snapshot.

- [ ] **Step 3: Run the focused tests and confirm failure**

Run: `cargo test -p mote-browser`
Expected: FAIL because the crate does not exist.

- [ ] **Step 4: Implement the browser worker boundary**

Run headed Chrome for Testing through chromiumoxide 0.9.1 on one dedicated Tokio worker. Expose
synchronous commands across channels so `moted` does not own CDP futures. Install and verify Chrome
for Testing 155.0.8059.39 from the official availability manifest. Never attach to a running browser.

- [ ] **Step 5: Implement snapshots, actions and evidence capture**

Use accessibility-tree refs. Invalidate the map on navigation. Use CDP Fetch and download
interception so protected requests pause before dispatch or JavaScript-visible completion. Continue
only after policy and approval-time revalidation. Capture before and after commit screenshots and a
HAR with cookies, authorization headers and form bodies redacted before persistence.

- [ ] **Step 6: Build the local fixture server**

Provide counters and pages for read, fill, submit, purchase, message, download, OAuth, non-GET,
navigation, off-origin redirect, stale refs and prompt injection. No fixture contacts an external
host.

- [ ] **Step 7: Run browser tests and commit**

Run `cargo test -p mote-browser`, workspace test and clippy. Commit `feat(browser): add isolated local browser runtime`.

### Task 4: Generic durable host-bound approvals

**Files:**
- Create: `moted/src/approvals.rs`
- Modify: `moted/src/watchers.rs`
- Modify: `moted/src/main.rs`
- Modify: `crates/mote-context/src/lib.rs`
- Test: `moted/src/approvals.rs`

**Interfaces:**
- Produces: `ApprovalIntent`, `ApprovalRecord`, `ApprovalDecision`, `ApprovalExecutor`, `ApprovalStore::create`, `claim`, `complete`, `deny`, `expire` and `pending_for_host`.
- Adds: `tool_ledger` rows for non-model browser steps without changing the invariant that each model call creates exactly one model-ledger row.

- [ ] **Step 1: Write migration and compatibility tests**

Open a database with existing file-write approvals, migrate it, and prove all V0-52 approve, deny, expiry and failure behavior remains unchanged.

- [ ] **Step 2: Write failing host-bound approval tests**

Test cross-host and cross-thread rejection, changed action hash, changed page-state hash, expiry, deny, concurrent approval, restart after claim, repeated approval and exactly-once fixture execution.

- [ ] **Step 3: Run focused tests and confirm failure**

Run: `cargo test -p moted approvals::`
Expected: FAIL because the generic store is absent.

- [ ] **Step 4: Implement the durable state machine**

Canonicalize and hash the exact intent in memory, then discard it. Persist host, thread, origin,
node ref, request fingerprint, kind, separately redacted display payload, action hash, page-state
hash, status, expiry and timestamps. Atomically claim pending work before execution. Finalize success
or failure. Repeated decisions return the stored state and never invoke the executor again.

- [ ] **Step 5: Add browser approval cards and tool accounting**

Cards show site, effect and expiry but contain no secret or executable client contract. Add a separate `tool_ledger` schema for browser action name, redacted input/output token estimates, latency and status.

- [ ] **Step 6: Run approval and V0-52 tests**

Run: `cargo test -p moted approval && cargo test -p mote-context`
Expected: PASS with existing file-write behavior preserved.

- [ ] **Step 7: Commit the approval foundation**

Run workspace test and clippy, then commit `feat(approval): bind durable decisions to host actions`.

### Task 5: Browser coordinator, tools and evidence

**Files:**
- Create: `moted/src/browser.rs`
- Modify: `moted/src/main.rs`
- Modify: `moted/src/mcp.rs`
- Modify: `moted/src/model_call.rs` only if shared timing helpers are needed
- Modify: `moted/Cargo.toml`
- Create: `moted/tests/browser_fixture.rs`

**Interfaces:**
- Consumes: `mote-browser`, generic approvals and host actor context.
- Produces: daemon-owned browser task coordinator and `browser.open`, `snapshot`, `text`, `find`, `click`, `type`, `select`, `back`, `scroll`, `screenshot` and `fetch` tools.

- [ ] **Step 1: Write failing end-to-end fixture tests**

For every outward trigger, assert CDP interception pauses it, a pending approval exists and the
fixture counter, page and download directory remain unchanged. Assert unapproved origins, off-origin
redirects, deny and expiry are inert. Approve one action, revalidate origin, node ref, request
fingerprint and page-state hash, then assert exactly one effect. Assert stale page state refuses
execution.

- [ ] **Step 2: Write the prompt-injection and evidence tests**

Assert injected page instructions remain boundary-marked text, produce no commit, and appear only in a redacted card. Assert one `browser_step` event and one `tool_ledger` row per step, redacted HAR, and before and after commit screenshots.

- [ ] **Step 3: Run the fixture tests and confirm failure**

Run: `cargo test -p moted --test browser_fixture`
Expected: FAIL because browser coordination is absent.

- [ ] **Step 4: Implement daemon coordination**

The daemon owns session, task, snapshot and approval state. Force commit classification for protected triggers regardless of model verdict. Recheck current URL, ref and page-state hash immediately before execution.

- [ ] **Step 5: Add MCP tools through daemon IPC**

The separate MCP process proxies to the running daemon and never owns Chrome state. Wrap all page content as untrusted tool output. Do not expose a raw CDP or JavaScript execution tool.

- [ ] **Step 6: Produce the final Matrix result**

Compile the fixture result through the existing host-owned Matrix candidate path. The browser never sends executable Matrix source directly to the iPad.

- [ ] **Step 7: Run browser, daemon and security tests**

Run: `cargo test -p mote-browser && cargo test -p moted --test browser_fixture && cargo test -p moted approval`
Expected: PASS with no external network and no default-profile access.

- [ ] **Step 8: Commit the coordinator**

Run workspace test and clippy, then commit `feat(browser): gate outward actions with approvals`.

### Task 6: iPad Simulator approval journey

**Files:**
- Modify: `mote-ios/src/live.rs`
- Modify: `crates/mote-client-core/src/state.rs`
- Modify: `crates/mote-client-core/src/surface.rs`
- Create: `mote-ios/tests/browser_approval.rs`
- Create: `docs/verification/IPAD-05.md`
- Modify: `README.md`
- Modify: `docs/HANDOFF.md`

**Interfaces:**
- Consumes: paired localhost transport, host-owned approval cards and browser coordinator.
- Produces: terminal approval-card replacement and the final validated Matrix result in the same Simulator client.

- [ ] **Step 1: Write failing client journey tests**

Stream a pending approval, reconnect, deny it and assert no effect. Stream another, approve it, assert one effect and replace or disable the terminal card. Repeated taps remain server-inert.

- [ ] **Step 2: Run the client journey test and confirm failure**

Run: `cargo test -p mote-ios --test browser_approval`
Expected: FAIL until terminal approval updates are reduced.

- [ ] **Step 3: Implement terminal approval reduction**

Keep the host authoritative. The client renders pending, approved, denied, expired and failed states and sends only a typed decision carrying the host-bound record from the selected host.

- [ ] **Step 4: Drive the live Simulator fixture**

Browse, read and fill automatically on the approved fixture. Confirm submit blocks. Approve from the Simulator, observe exactly one fixture effect and render the final Matrix result. Repeat with deny and expiry.

- [ ] **Step 5: Verify and close IPAD-05**

Record Chrome version, exact headed process arguments, profile path, fixture URLs, events, ledger
rows, redacted HAR, screenshots, page-state checks and Simulator captures in
`docs/verification/IPAD-05.md`. Prove the arguments select only the Mote profile, loopback debugging
and mock keychain, and that no code path attaches to the default Chrome process.

- [ ] **Step 6: Final verification and commit**

Run workspace test and clippy, update the README checkbox and Simulator parent if every active child is checked, update the handoff, and commit `test(ipad): verify browser approval in Simulator`.
