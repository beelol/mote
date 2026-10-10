# Handoff: Approval file-write closure

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `29e92ee`.
Requested subject: `fix(approvals): report file write failures`.
Fix Round 1 subject: `fix(approvals): bind writes to validated directories`.
Fix Round 2 subject: `fix(approvals): bind approved target identities`.
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

Fix Round 2 binds the final target as well as its parent. An existing target is opened without
truncation. Its device and inode are checked before and after the synchronization point. The write
then truncates and writes through the held descriptor. A target that was missing at validation is
created with `O_CREAT|O_EXCL`. A regular-file replacement, hardlink replacement or new file at the
validated name causes the write to fail without modifying either object.

Pending approvals store the original requested path, the resolved destination and whether the
destination was missing or had a specific device and inode. The card displays the resolved
destination. Approval re-resolves the original path and requires the same destination and identity.
Retargeting a pending symlink from one file to another fails closed. Both files remain unchanged.
The approval stays pending and deniable, and no accepted event is appended.

Inside write failures return an approval filesystem error and create no approval. Approved outside
writes parse typed payloads and write before changing status. Filesystem and payload failures leave
the row pending and deniable. The dispatcher maps these failures to internal RPC errors. A failed
approved write appends no accepted approval event.

Focused watcher tests include deterministic parent swaps, target replacements, a hardlink swap, a
missing-target collision and a pending symlink retarget. A direct failure creates no approval. An
approved failure stays pending and deniable. The dispatcher suite proves a pending symlink retarget
returns an internal error and appends no accepted event. Full workspace tests and clippy with
warnings denied passed. The final diff check passed.

V0-52 remains partial. Its native approval-card screenshot is still missing. No remote write or
push occurred.
