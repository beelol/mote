# Working on mote

Rules for every agent. The owner set them.

## Documents
- The build contract is `docs/rfcs/v0.md`. It decides every default. The current scope is the v0 list in the root README under Ready for Go. Everything else from planning is `docs/backlog.md` and is out of scope until the owner moves it.
- Reference, never overriding `v0.md`: `docs/rfcs/mote.md` (MOTE-NN), `docs/rfcs/shared-crates.md` (CRATE-NN), `docs/rfcs/ui-language.md` (UI-NN), `docs/rfcs/context-layer.md` (CTXL-NN), `docs/reuse/*.md`.
- A criterion is verified only when its Verify clause is fully covered. Evidence goes in `docs/verification/<ID>.md` with the command, the output, the environment and the date.
- The handoff file is `docs/HANDOFF.md`. Replace it, never append. Delete it when nothing needs transfer.
- Do not write `knowledge/` fragments. Put durable facts in the RFCs or the README.

## Code
- Rust 2021, `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` green before every commit.
- No API keys for models. `models.local_only` is true: local Ollama models only, and exactly the three named in `v0.md` section 0 may be pulled. Subscription logins (`claude -p`, Codex) wait for the owner to flip the flag.
- Every model call goes through the ledger. Every event goes through redaction before storage.
- Model verdicts are structured JSON parsed fail-closed. No regex over natural language.
- No emojis. Comments only where the code cannot speak for itself. No ticket IDs in code.

## Commits
- Conventional Commits, imperative subject, 72 characters max, no AI attribution trailers.
- Small verified changes, merged to `main`. There is no remote until the owner adds one.

## Prose
- Plain declarative sentences. One idea per sentence. No em dashes.
