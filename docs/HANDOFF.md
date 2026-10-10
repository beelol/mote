# Handoff: Inferred Matrix surface call boundary

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `092a2cb`.
Requested subject: `fix(models): redact and account for surface calls`.
Fix Round 1 subject: `fix(models): preserve strict surface accounting`.
Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

Task 1 of the inferred Matrix surfaces plan is implemented. It is not connected to ordinary talk
or think turns. It does not finalize Matrix candidates and does not retry rejected surfaces.

`mote-localmodel` now has a structured reply helper that returns the parsed value and the exact raw
assistant content. The prior structured helper delegates through it and keeps its existing API.

The inactive surface lane accepts only `none` or `matrix` with a bounded source. It uses only local
`qwen3:4b` with a 32,768 context, thinking disabled, temperature zero and an Ollama JSON schema.
The prompt names only the current host bindings and the lossless v0 Matrix subset. It defaults to
no surface unless a surface materially helps or the request asks for a dashboard, form, list,
table or controls.

Request text, delivered prose, prior canonical source and diagnostics are redacted before pack
construction and before the HTTP request. Each of the five stored pack parts is now the exact
content of one outbound message. Dynamic values are JSON serialized with their labels before token
accounting, so quotes, backslashes and fixed prompt text are included. Oversized redacted values
are reduced before serialization until the complete message content fits within 32,768 tokens.

Malformed structured replies retain their raw assistant content. A failed surface call records its
real completion-token count instead of zero. The strict decision visitor rejects duplicate `kind`
and `source` fields before JSON values can collapse them. Every invoked surface call records one
five-part pack, one `surface` ledger row, one redacted `model.request` event and one
`model.response` event on both success and failure.

Shared request timing and ledger construction now live in `moted/src/model_call.rs`. Existing talk,
router and think behavior remains unchanged. Tests inspect the real scripted HTTP bodies for talk
and router, and the exact instrumented system and turn payload for think.

Task 2 remains next. It must share normal Matrix candidate finalization and add the bounded retry.
Production activation remains disabled until the fixed 200-prompt UI-24 gate passes. No v0
criterion closes from this task alone. No remote write or push occurred.
