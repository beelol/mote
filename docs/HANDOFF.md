# Handoff: Approval file-write closure

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `29e92ee`.
Requested subject: `fix(approvals): report file write failures`.
Fix Round 1 subject: `fix(approvals): bind writes to validated directories`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

File-write containment now compares canonical paths. Existing targets are canonicalized directly.
Nonexistent targets use their canonical immediate parent plus their filename. Lexical fallback was
removed. Traversal and an in-data symlink to an outside parent are held for approval and write
nothing. An existing symlink with an unresolved destination fails closed.

Fix Round 1 removes the remaining validation-to-write pathname race. Direct and approved writes
walk the canonical parent from the root with descriptor-relative `openat`. Every directory and the
target use `O_NOFOLLOW`. The parent directory handle is retained through the write. Its device and
inode are compared with a fresh descriptor-relative walk after the synchronization point. A parent
swap fails before the target opens. The write never reopens the original target pathname.

Inside write failures return an approval filesystem error and create no approval. Approved outside
writes parse typed payloads and write before changing status. Filesystem and payload failures leave
the row pending and deniable. The dispatcher maps these failures to internal RPC errors. A failed
approved write appends no accepted approval event.

Focused watcher tests include deterministic direct and approved parent swaps after validation and
before target open. They prove no redirected or parked file is written. A direct failure creates no
approval. An approved failure stays pending and deniable. The dispatcher suite preserves the card,
thread, deny and expiry behavior. Full workspace tests and clippy with warnings denied passed. The
final diff check passed.

V0-52 remains partial. Its native approval-card screenshot is still missing. No remote write or
push occurred.
