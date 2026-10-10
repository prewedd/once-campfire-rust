# once-campfire-rust

Campfire in Rust. It started as a port that had to be indistinguishable from the Rails app in
`reference/` (a submodule pinned to the SHA it was matched against), and that parity is done: see
`README.md` and `plans/rust-conversion.md`. The app may now diverge from Rails where that makes it
faster or better.

- The Rails app is still the reference for anything not deliberately changed: when in doubt about
  existing behavior, read the Ruby.
- Stay compatible with existing installs unless told otherwise: the SQLite schema, the storage
  layout, and signed/encrypted cookies (so people stay signed in across an upgrade).
- List every deliberate divergence under "Known differences" in `README.md`, and update the tests
  and parity masks it affects.
- Port-owned frontend changes go in `crates/assets/overrides/`, which shadows the reference's assets
  by logical path. Don't edit `reference/`.

## Layout

| Path | Package | What |
|---|---|---|
| `crates/rails_compat` | `rails_compat` | Rails signing/encryption/serialization contracts, verified by `vectors/` |
| `crates/kit` | `campfire_kit` | Axum adapter, `Ctx`, params, cookies, session, forgery protection, flash, formats, responses, gzip, and the front server (TLS, ACME, HTTP/2, response cache) |
| `crates/routes` | `campfire_routes` | Path helpers mirroring `config/routes.rb` |
| `crates/db` | `campfire_db` | rusqlite over the existing schema, models, queries, fixtures loader |
| `crates/richtext` | `campfire_richtext` | Action Text content pipeline: sanitize, attachments, autolink, plain text |
| `crates/storage` | `campfire_storage` | Active Storage-compatible blobs, disk service, variants (libvips), previews (ffmpeg) |
| `crates/cable` | `campfire_cable` | Action Cable protocol server, its WebSocket implementation, and in-process pub/sub |
| `crates/assets` | `campfire_assets` | Propshaft-compatible digesting, importmap, vendored JS/CSS, port-owned overrides |
| `crates/views` | `campfire_views` | Askama templates (at the ERB file's relative path) and view helpers |
| `crates/campfire` | `campfire` (bin) | Controllers, router wiring, channels, jobs, integrations |
| `parity/` | — | Playwright parity harness, screen inventory, reference Docker setup |
| `reference-tools/` | — | Ruby scripts run inside the reference container to produce `vectors/` |
| `bench/` | — | Load generator, benchmark scripts and recorded results |

## Working rules

- Rust comes from mise if it isn't on the PATH: `mise exec rust@1.98.1 -- cargo ...` (the version
  in `Dockerfile`). The `reference/` submodule must be checked out for `crates/assets` to build.
- `cargo test --workspace --exclude html5ever` runs everything. The app's integration tests need the
  seed data (`parity/bin/seed build`, which needs Docker); without it they skip silently, so say so
  when reporting results.
- `cargo clippy --workspace --exclude html5ever --all-targets` should stay clean. (`html5ever` is a
  vendored copy with one backported fix, kept identical to upstream otherwise.)
- Put shared dependency versions in the root `[workspace.dependencies]`, and reference them with
  `foo.workspace = true`.
- When matching existing behavior, read the reference's source. When it depends on Rails or gem
  internals, read the gem source inside the reference image
  (`docker run --rm campfire-reference bundle show <gem>`), not docs or memory.
- Write code that reads like the surrounding code: small, clearly named functions, and comments
  only where the behavior is non-obvious. Cite the reference file (`reference/app/...`) when
  matching Rails, and say why when deliberately diverging from it.
- Tests live beside the code. Golden-vector tests read `vectors/*.json`.
- Performance changes come with before-and-after measurements, recorded under `bench/results/`.

## Reference container

`parity/` builds the reference image as `campfire-reference` from `reference/Dockerfile` and runs it
in production mode with a fixed `SECRET_KEY_BASE` (see `parity/.env.reference`) so that golden
vectors, seeds and screenshots are reproducible. Those keys are for tests only.

## Iterating fast

The smallest build, the fewest tests that cover the change, the fastest deploy.

### The loop


| You changed | See it | Test it | Ship it |
|---|---|---|---|
| CSS / JS in `crates/assets/overrides/` (editing an existing file) | reload the dev page: nothing to build | `e2e/run --affected` (smoke + the suites for those files) | `/root/campfire/deploy-assets.sh` (~1 s, no build) |
| Rust, templates, a new asset file | `script/dev/server` (~17 s after a one-line change) | `cargo test -p <crate you touched>` + `e2e/run --affected` | commit, `/root/campfire/deploy.sh` |
| Nothing visible (docs, scripts) | — | — | commit; no deploy |

- **Dev server**: `script/dev/server` runs a debug build (line tables only, mold, via the toolchain
  image's `RUSTFLAGS`) on http://127.0.0.1:3090 with the overrides
  served from disk, a staging copy of production (`script/dev/staging`: no push subscriptions, no
  webhooks) and its own LiveKit. Restart it only after Rust/template changes, or after adding an
  asset file (the asset manifest is built in).
- **Type-check before building**: `cargo check -p campfire` (or the crate you touched) catches
  most mistakes in seconds; don't wait for a full build or a test run to find a typo.
- Rust comes from the toolchain image: `docker run --rm -v $PWD:/src -w /src -v
  campfire-cargo-registry:/usr/local/cargo/registry -v campfire-cargo-target:/src/target
  campfire-toolchain:local cargo …` (that's what `script/dev/server` uses).

### Tests: run what the change can break, nothing more

- `e2e/run --affected` runs `smoke` (~20 s) plus the suites mapped to the files changed since the
  deployed revision, in parallel. Name suites to run just those: `e2e/run huddle schedule`.
- `e2e/run --all` (~5 min, threads is the slowest) only before risky deploys: changes to the
  composer, the message list, Turbo/Stimulus plumbing, cable channels, or many files at once.
- Rust: `cargo test -p <crate>` for the crate touched; `-p campfire_views` whenever templates or
  the sidebar/composer markup change (the parity goldens filter this app's own markup: add a
  marker in `crates/views/tests/messages_support/mod.rs` and `parity_a.rs` for new markup).
  `cargo test --workspace --exclude html5ever` only before a release or a schema change.
- In the toolchain container, pass `--ulimit nofile=65536:65536` for workspace-wide runs: without it
  dozens of app tests fail with "Too many open files" / "unable to open database file".
- Known, not regressions: two Rust timing tests (`parses_pathological_pages_quickly`,
  `deeply_nested_content_attachments_render_quickly`) fail in debug builds on a busy machine.
- After many suite runs the dev LiveKit degrades: `screen`'s "stopping removes it" and huddle
  checks start failing even alone. `docker restart livekit-dev` (dev only) and rerun.
- Never edit an existing asset's imports to a module the running dev server doesn't know (a new
  file) while suites run: restart the dev server first.
- A new feature gets a suite in `e2e/suites/<name>.e2e.js` (copy `smoke`'s shape: `check()`,
  exit non-zero on failure) and a line in `affected()` in `e2e/run`.

### Deploys (team.prewedd.com)

- `/root/campfire/deploy-assets.sh`: edits to existing CSS/JS only. Copies the overrides to
  `/root/campfire/assets`, which production serves live (`CAMPFIRE_LIVE_ASSETS`). It refuses, and
  says so, when anything else changed.
- `/root/campfire/deploy.sh`: everything else. Needs a clean tree (it tags the image with the
  commit). Compiles incrementally in the toolchain image (`profile.deploy`: this workspace
  unoptimized, every dependency at opt-level 3 and cached; a one-line Rust or template change
  compiles in ~16 s, a cold build ~4 min), wraps the binary in the cached runtime image
  (`Dockerfile.prebuilt`), smoke-tests it on a copy of production, backs up the database, switches,
  and rolls back if it isn't healthy within a minute. The TLIC apps' health is printed at the end.
- The Dockerfile's own full build (`docker build .`) stays for releases and CI; deploys don't use it.
- Huddles: never restart the `livekit` container while people are in a huddle (it drops them for a
  few seconds); `docker logs livekit` shows who is.

### Don't

- Don't run the full matrix "to be safe" after a CSS tweak, or rebuild images to check a template:
  use the dev server.
- Don't run two cargo builds against the same target directory at once (they queue on its lock).
- Don't leave work only in a session's scratch directory: suites, scripts and data belong in the
  repo (`e2e/`, `script/dev/`) or `/root/campfire-dev`.
