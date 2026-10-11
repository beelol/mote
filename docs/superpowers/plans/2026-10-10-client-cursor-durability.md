# Client cursor durability plan

## Goal

Persist the event cursor atomically after reduction, recover safely from malformed cursor files and
make persistence failures visible without allowing later queued events to advance. Keep V0-43 open
for its native screenshot and manual close/reopen acceptance.

## Task 1: Add atomic cursor storage

**Files:**
- Create: `mote-client/src/cursor.rs`
- Modify: `mote-client/src/main.rs`

**Interfaces:**
- `LoadedCursor { seq, warning }`
- `load(path)` returns zero without warning for a missing file; malformed, negative, overflow,
  empty or unreadable files recover from zero with a warning that never echoes contents.
- `store(path, seq)` creates a unique same-directory temp, writes and syncs it, atomically renames it
  over the target, then syncs the parent directory. Remove only the owned temp on pre-rename failure.

- [ ] **Step 1: Pin load and atomic replacement tests**

Cover missing, malformed and nonnegative values; two complete replacements; no leftover owned temp;
and an injected rename failure that preserves the prior committed cursor.

- [ ] **Step 2: Implement file and directory durability**

Use `create_new`, `sync_all`, same-directory rename and parent `sync_all`. Do not add a hardware
flush dependency. Surface a directory-sync failure even though rename may already be visible.

## Task 2: Make persistence failure visible and fail safe

**Files:**
- Modify: `mote-client/src/app.rs`

- [ ] **Step 1: Preserve reduce-before-store ordering**

Apply each event to chat state first, then persist its cursor. Record pipeline and latency behavior
for that reduced event. Existing at-most-once stream filtering remains unchanged.

- [ ] **Step 2: Halt later reduction after store failure**

Latch a dedicated visible cursor issue that unrelated worker success cannot clear. The just-reduced
event remains visible exactly once. Later queued events must not reduce until restart/recovery.

- [ ] **Step 3: Add real close/reopen socket coverage**

Reduce and store seq 10, drop the app, load the cursor, then use the real stream client against a
local Unix listener. Assert subscription starts at 10, stale seq 10 is ignored, fresh seq 11 appears
once and the durable cursor becomes 11.

## Task 3: Record partial V0-43 evidence

**Files:**
- Modify: `docs/verification/V0-43.md`
- Replace: `docs/HANDOFF.md`

Record atomic replacement, malformed recovery, visible failure halt and real close/reopen socket
evidence. Keep V0-43 unchecked. Native daemon-connected screenshot and manual close/reopen cursor
inspection remain open. Re-run display latency focused tests because synchronous cursor persistence
is inside the reduce-to-render path; do not claim native sub-50 ms evidence from headless tests.

## Verification

Run focused cursor/reopen/failure/latency tests, then:

```text
cargo test -p mote-client
cargo test --workspace
cargo clippy --workspace -- -D warnings
git diff --check
```

Commit: `fix(client): persist cursor atomically`
