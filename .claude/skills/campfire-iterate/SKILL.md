---
name: campfire-iterate
description: Change, test and deploy this Campfire app (team.prewedd.com) in the fastest loop - live CSS/JS, debug dev server, change-aware E2E suites, incremental deploys. Use for any feature, fix or style change in once-campfire-rust.
---

# Iterating on Campfire

Follow "Iterating fast" in `AGENTS.md` (also `CLAUDE.md`, a link to it). In short:

1. **Start**: `script/dev/server` (if http://127.0.0.1:3090/up isn't already answering).
2. **Change**:
   - CSS/JS in `crates/assets/overrides/`: just reload. No build.
   - Rust or templates: `cargo check -p <crate>` first (seconds), then `script/dev/server` (~25 s).
3. **Test only what the change can break**:
   - `e2e/run --affected` (smoke + mapped suites, in parallel), or name suites: `e2e/run ui4`.
   - `cargo test -p <crate>`; `-p campfire_views` when markup changed.
   - `e2e/run --all` only for risky, cross-cutting changes.
4. **Ship**:
   - Only existing CSS/JS edited → `/root/campfire/deploy-assets.sh` (~1 s).
   - Otherwise commit, then `/root/campfire/deploy.sh` (incremental build; smoke, backup,
     health check and rollback built in).
5. **Report**: what changed, which suites ran and their results, deploy outcome and time.

New feature → add `e2e/suites/<name>.e2e.js` (copy `smoke.e2e.js`) and map its files in `e2e/run`.
