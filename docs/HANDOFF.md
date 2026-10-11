# Handoff: client cursor durability

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `1fd84b1`. Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

The client cursor is isolated in `mote-client/src/cursor.rs`. Loading accepts only nonnegative
`i64` values. Missing files start from zero silently. Empty, malformed, negative, overflowing and
unreadable files start from zero with a generic warning that does not echo contents.

Storing creates a unique same-directory temporary file with `create_new`. It writes and syncs the
file, atomically renames it over `client_cursor` and syncs the parent directory. Pre-rename failure
removes only the owned temporary file. Rename failure preserves the prior committed cursor.
Directory sync failure remains visible to the caller even though the rename may already be visible.

Event reduction still happens before cursor persistence. A persistence failure keeps the
just-reduced event visible once and records its pipeline and display-latency state. It then latches a
dedicated visible cursor issue and halts later queued reductions. Pipeline, configuration, turn and
UI-event worker results cannot clear the cursor issue.

The real Unix-socket reopen test stores sequence 10, drops the first app, loads 10, subscribes from
10 on a new connection, ignores a stale replay of 10 and displays and stores sequence 11 once.
Stream filtering and reducer semantics are unchanged.

V0-43 remains open. It still requires an unlocked native daemon-connected screenshot and manual
close and reopen inspection of the cursor with no duplicate display. UI-40 remains open. Its focused
headless latency checks pass, but native release measurements with at least 20 Refresh actions and
round trips are still required.
