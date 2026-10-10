# Handoff: Mote closeout driver

Updated 2026-10-10 00:42 PDT. Replace this file, never append. Delete it when nothing needs
transfer. Nothing here assumes chat history.

## Where

- Branch: `main`. Remote: `origin` at `bc9cc4b` when this snapshot was written.
- Contract: `docs/rfcs/v0.md`. Driver goal: `docs/goal-close-out.md`. Matrix plan:
  `matrix/CLOSEOUT.md`.
- The owner directed this session to drive Mote and delegate the Matrix UI work. Do not ask the
  owner questions before 09:00 America/Los_Angeles. Make conservative choices and defer anything
  that truly needs the owner.

## State

- The v0 board is 46 of 55 checked. The remaining items are GUI live verification plus V0-53's
  literal logout/login check.
- Matrix closeout Tasks 1 and 2 are already committed and pushed: design coverage at `f93b87e`
  and the Vector default theme at `bc9cc4b`.
- Subagent `/root/matrix_ui` owns Matrix Task 3 only. Its goal is to freeze the grammar, resolve
  the open design questions, reconcile examples, measure source size, commit, and push without
  touching production code. Persistent task state is in `.superpowers/sdd/CLOSEOUT/`.

## Driver work in progress

The prior driver left an uncommitted `surface.render` seam. Current owned changes are:

- `moted/Cargo.toml` and `Cargo.lock`: add the existing `mote-ui` workspace dependency.
- `moted/src/main.rs`: validate a requested surface fail-closed, append `ui.surface` on the chosen
  thread, return whether the original tree was valid, and cover validation plus event emission
  with three tests.
- `docs/goal-close-out.md`: record Mac-native first and the two-layer inferred UI boundary.

Targeted evidence is green:

- `cargo test -p moted surface_tests`: 3 passed, 0 failed.
- `cargo clippy --workspace -- -D warnings`: exit 0.
- `git diff --check`: exit 0 before this handoff update.

The full suite cannot be honestly reported inside the current filesystem sandbox because the
scripted test server cannot bind `127.0.0.1:0` (`Operation not permitted`). This is an environment
limit, not a test assertion failure. Do not commit until `cargo test --workspace` runs outside that
sandbox and passes. The owner requested no prompts before 09:00, so do not request elevated test
permission before then.

## Exact next actions

1. Review the Matrix Task 3 report and diff when `/root/matrix_ui` finishes. Dispatch an independent
   task reviewer before accepting it. Preserve unrelated daemon changes while the subagent pushes.
2. At or after 09:00 PDT, run `cargo test --workspace` with the permission needed for loopback test
   servers. Re-run `cargo clippy --workspace -- -D warnings` and `git diff --check`.
3. If green, commit the daemon seam and goal update as a small Conventional Commit, then push main.
4. Continue Matrix Task 4 only after Task 3 review is clean. Task 4 is the minimal Mac-native
   consumer runtime. Keep UI-00 unchecked.

## Rules that still bite

- `models.local_only` stays true. Never start Claude or Codex. Pull no unapproved model.
- Every model call uses the ledger. Redaction runs before storage. Structured verdicts parse
  fail-closed. No API keys.
- Use Conventional Commits with no AI attribution. Verify before every commit. Push accepted work.
