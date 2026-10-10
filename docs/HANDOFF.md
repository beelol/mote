# Handoff: Approval file-write closure

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `29e92ee`.
Requested subject: `fix(approvals): report file write failures`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

File-write containment now compares canonical paths. Existing targets are canonicalized directly.
Nonexistent targets use their canonical immediate parent plus their filename. Lexical fallback was
removed. Traversal and an in-data symlink to an outside parent are held for approval and write
nothing. An existing symlink with an unresolved destination fails closed.

Inside write failures return an approval filesystem error and create no approval. Approved outside
writes parse typed payloads and write before changing status. Filesystem and payload failures leave
the row pending and deniable. The dispatcher maps these failures to internal RPC errors. A failed
approved write appends no accepted approval event.

Focused watcher and dispatcher tests cover the new failure paths and the preserved card, thread,
deny and expiry behavior. Full workspace tests and clippy with warnings denied passed. The final
diff check passed.

V0-52 remains partial. Its native approval-card screenshot is still missing. No remote write or
push occurred.
