# Approval file-write closure plan

## Goal

Make file-write containment and completion fail closed without changing the approval card, thread,
deny or expiry contract. Keep V0-52 open until its required native screenshot exists.

## Task 1: Resolve write targets before containment decisions

**Files:**
- Modify: `moted/src/watchers.rs`

**Interfaces:**
- Add an approval error that carries database, filesystem and payload failures.
- Make `request_file_write`, `approve` and `approve_for_thread` return that error.
- Resolve an existing target by canonicalizing it.
- Resolve a nonexistent target by canonicalizing its immediate parent and appending its filename.
- Never classify an unresolved path with lexical prefix matching.

- [ ] **Step 1: Add failing containment tests**

Cover a nonexistent `<data>/../escape/file` target and a nonexistent child below an in-data symlink
whose destination is outside the data directory. Both requests must remain pending and write
nothing.

- [ ] **Step 2: Implement resolved containment**

Canonicalize the data directory and resolved target before comparing them. Propagate resolution
errors. Preserve the existing 24-hour pending approval, thread binding and card behavior for outside
targets.

## Task 2: Report execution only after a successful write

**Files:**
- Modify: `moted/src/watchers.rs`
- Modify: `moted/src/main.rs`

- [ ] **Step 1: Add failing write-error tests**

Use a directory as the direct inside write target. Assert an I/O error, no `Executed` decision and
no approval row. Use an outside directory as an approved write target. Assert the write error leaves
the row pending and it can still be denied. Insert malformed pending payload JSON and prove approval
fails while the row stays pending.

- [ ] **Step 2: Make status follow filesystem success**

Propagate direct write errors. Parse approved payloads into a typed path and content value. Perform
the write before changing the database status to approved. Preserve `Ok(false)` for an unknown,
wrong-thread, expired or non-pending approval.

- [ ] **Step 3: Pin public dispatcher behavior**

Through `file.write`, prove a failed inside write returns an internal RPC error instead of an
executed decision. Through an outside pending request and its trusted approval event, prove a failed
approved write returns an internal RPC error, appends no accepted approval event, keeps the row
pending and still allows denial.

## Task 3: Record honest V0-52 evidence

**Files:**
- Modify: `docs/verification/V0-52.md`
- Replace: `docs/HANDOFF.md`

- [ ] **Step 1: Document code evidence**

Record canonical containment for traversal and symlinked-parent cases, propagated write errors and
pending preservation. Keep the criterion partial because the native approval-card screenshot is not
available.

- [ ] **Step 2: Verify and commit**

Run:

```text
cargo test -p moted watchers::tests -- --nocapture
cargo test -p moted approval_dispatch_tests -- --nocapture
cargo test --workspace
cargo clippy --workspace -- -D warnings
git diff --check
```

Commit: `fix(approvals): report file write failures`
