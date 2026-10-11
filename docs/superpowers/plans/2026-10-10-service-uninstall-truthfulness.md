# Service uninstall truthfulness plan

## Goal

Preserve the launchd plist and return an error whenever `launchctl unload -w` cannot run or exits
nonzero. Print removal success only after unload succeeds and the plist is removed. Keep V0-53 open
for literal logout/login acceptance.

## Task 1: Make unload result authoritative

**Files:**
- Modify: `moted/src/service.rs`
- Modify: `docs/verification/V0-53.md`
- Replace: `docs/HANDOFF.md`

**Interfaces:**
- Add a private command result carrying success and stderr.
- Add a private uninstall helper with an injectable one-shot command runner.
- Production invokes exactly `launchctl unload -w <plist>` through `std::process::Command`.

- [ ] **Step 1: Add failing tests**

Cover nonzero exit with stderr, nonzero exit without stderr, process launch error and successful
unload. Assert exact program/arguments. Every failure must preserve the plist and return meaningful
nonempty error text. Success removes the plist.

- [ ] **Step 2: Implement truthful ordering**

Run unload first. On spawn or nonzero failure, return an error and do not remove the plist or print
success. Only after successful unload remove the plist if present, then print
`removed com.mote.moted`. Preserve idempotent successful behavior when the plist is absent.

- [ ] **Step 3: Record partial V0-53 evidence**

Keep prior successful live launchctl evidence. Add the four fake-runner tests and state that unload
failure now preserves recovery state. Keep V0-53 open because literal logout/login remains untested.

## Verification

```text
cargo test -p moted service::tests::uninstall_ -- --nocapture
cargo test --workspace
cargo clippy --workspace -- -D warnings
git diff --check
```

Commit: `fix(service): preserve plist when unload fails`
