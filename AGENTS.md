# Working on Mote

Rules set by the owner.

- Scope comes from root `README.md` -> `Ready for Go`. `docs/rfcs/v0.md` preserves v0 defaults.
  Linked plans and RFCs define each active criterion. Everything else is deferred.
- Verify the full linked clause before checking a criterion. Record command, output, environment
  and date in `docs/verification/<ID>.md`.
- Replace `docs/HANDOFF.md` after each work batch. Delete it only when nothing needs transfer.
- Use Rust 2021. Before every commit run `cargo test --workspace` and
  `cargo clippy --workspace -- -D warnings`.
- Keep `models.local_only = true`. Use only the three Ollama models named in v0 section 0. Do not
  use API keys or subscription harnesses until the owner changes this rule.
- Ledger every model call. Redact every event before storage. Parse model decisions as structured
  JSON and fail closed. Do not regex natural-language verdicts.
- Preserve unrelated work. Do not create `knowledge/` fragments.
- Commit small verified changes to `main` and push. Use imperative Conventional Commit subjects
  of at most 72 characters without AI attribution.
- Use plain prose, no emojis and no em dashes. Add comments only when the code cannot explain
  itself. Put no criterion IDs in code.
