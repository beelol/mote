# Pipeline config client closure plan

## Goal

Make the Pipeline Config tab submit only dirty typed values off the render thread and verify every
five-stage by four-tab production-shell state. Keep V0-47 and V0-48 open for their required native
screenshot and live click evidence.

## Task 1: Verify the complete production Pipeline shell

**Files:**
- Modify: `mote-client/src/app.rs`
- Modify: `mote-client/src/observation.rs`

- [ ] **Step 1: Add a populated 20-state shell test**

Install one snapshot with distinct events, numbers and requests for talk, router, think, memory and
ledger. Render through `MoteApp::draw_shell` at 1280 by 800. Drive every stage and Now, Config,
Numbers and Requests tab through real egui input.

For all 20 states, require production navigation, all five stage labels, all four tab labels, the
selected stage's distinct content, all nine configuration keys and populated metrics or requests.

## Task 2: Apply dirty configuration asynchronously

**Files:**
- Modify: `mote-client/src/app.rs`

**Interfaces:**
- Add typed per-key `ConfigSetRequest` and per-request outcome values.
- Add a background configuration worker using request and result channels.
- Parse booleans, model strings and integers by known key. Never submit invalid local text.
- Track dirty keys, edit revisions, pending batches and a visible configuration error.

- [ ] **Step 1: Prove the worker cannot block rendering**

Use a blocking test sender and prove the calling thread remains free until the worker result arrives.

- [ ] **Step 2: Submit only changed typed keys**

Seed all nine values. Change only `models.talk`; edit and revert another key. Apply must queue exactly
one typed `config.set` and repeated Apply while pending must queue nothing.

- [ ] **Step 3: Preserve failures and concurrent edits**

A failed request keeps the edit dirty and visible, re-enables Apply, shows the daemon error and does
not refresh solely for that failure. A mixed batch refreshes successful keys but preserves failed or
subsequently edited keys.

- [ ] **Step 4: Refresh only after the batch completes**

Process requests in fixed configuration-key order. A successful result clears a key only when its
submitted revision still matches. Request one snapshot after the batch finishes. Clean keys adopt
the snapshot; dirty keys do not. Snapshot success must not hide a configuration error.

## Task 3: Record partial V0-47 and V0-48 evidence

**Files:**
- Create: `docs/verification/V0-47.md`
- Modify: `docs/verification/V0-48.md`
- Modify: `README.md`
- Replace: `docs/HANDOFF.md`

Record the 20 production-shell combinations and dirty-only background request behavior. Preserve the
existing daemon/ledger evidence. Keep both README boxes unchecked. V0-47 still needs its live-turn
screenshot. V0-48 still needs the native Config click correlated with the next-turn ledger change.

## Verification

Run:

```text
cargo test -p mote-client
cargo test --workspace
cargo clippy --workspace -- -D warnings
git diff --check
```

Commit: `fix(client): apply pipeline config asynchronously`
