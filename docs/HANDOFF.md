# Handoff: truthful service uninstall

Updated 2026-10-10. Replace this file, never append.

Branch: `main`. Base: `8848f8e`. Preserve untracked `.DS_Store` and `.superpowers/`. Do not push.

Service uninstall now treats the `launchctl unload -w <plist>` result as authoritative. A process
launch error or nonzero exit returns a meaningful error and leaves the plist intact. This preserves
the recovery state needed to inspect or retry the unload.

Only a successful unload removes the plist and prints `removed com.mote.moted`. Successful unload
remains idempotent when the plist is already absent. Tests use an injected one-shot runner and never
invoke real launchctl. They cover exact command arguments, nonzero exits with and without stderr, a
process launch error, successful removal and successful absence.

V0-53 remains open. The existing live install, KeepAlive restart and successful uninstall evidence
is retained in `docs/verification/V0-53.md`. Literal logout and login survival still requires the
owner's session.
