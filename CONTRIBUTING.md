# Contributing to Corten

Thanks for helping. Open an issue to discuss a change before a large pull request; a small fix
can go straight to a pull request. Every change follows the [values](README.md#values), and
when two conflict, the order given there decides. A change that adds a dependency or a new
concept needs a reason a smaller change cannot meet. The checks below prove a change builds and
its tests pass; the [review](#review) decides whether it should land.

## Layout

```text
crates/
  corten/            The framework on axum: App, Req, html!, components, the runtime (js/corten.js)
  corten-macros/     html!, #[component], #[event], script!, messages!, tokens!, derives (Fields, Route), on proc_macro alone (no syn)
  corten-cli/        The corten command, standard library only
  corten-feed/       Live feeds: rows, numbered patches, replay, resume
  corten-protocol/   Wire types and Replica (the client side), no web or async dependencies
  corten-cluster/    One feed across instances: Postgres outbox, NATS JetStream relay
  corten-ui/         Optional headless components: number field, combobox, tabs, menu button
examples/watchlist/  A stock watchlist: live prices, news, alerts; scripts/smoke.mjs tests the runtime against it
examples/card-market/ A trading-card store (home, categories, item pages) with every production module on; smoke.mjs tests it
examples/seller-desk/ A seller's inventory desk: a client-heavy screen built with handlers
examples/cluster/    A listings feed on corten-cluster, and the failover benchmark
examples/ui-gallery/ corten-ui styled by a small design system; smoke.mjs tests it in a browser
docs/              Guides (docs/guide), design decisions and limits, surfaces (architecture.md)
skills/corten/     The agent skill for building apps on Corten
bench/fullstack/     The same page in fourteen frameworks, and the scripts that compare them
bench/live/          One price feed pushed to many clients in the same frameworks
```

Dependencies of `corten`: `axum`, `futures-util`, `getrandom` (CSP nonces), and from axum's
tree `http-body` (axum's body trait), `tower-layer` and `tower-service` (middleware traits),
`form_urlencoded` (forms and query strings), `zmij` (float digits) and `serde`; `tokio`
(sockets, signals, timers), `tracing`, `corten-macros` (no dependencies). Optional:
`tracing-subscriber` (`log`, on by default), `serde`'s derive and `serde_json` (`json`), `schemars` (`types`, `agent`), `zstd`, `brotli`, `flate2` (`compress`),
`corten-feed` (`feed`). Apps need no JavaScript dependencies; `package.json` here is tooling for
the framework itself.

## Tooling

- Rust (stable). On Windows the tests run in Docker, because the host cannot link `windows-sys`.
- Node 22 or later, for the runtime's type check, its minified build and the browser smoke
  tests: `npm install`, then `npx playwright install chromium firefox webkit` (the core and
  `corten-ui` smoke tests run in all three).
- Docker, for the example images the smoke tests run against.

## Checks

All of these must pass before a pull request is ready. Run each command separately (shown for
PowerShell from the repository root; in another shell use `$PWD` for `${PWD}`).

```powershell
docker run --rm -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-target:/target -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm sh -c "rustup component add clippy rustfmt >/dev/null 2>&1; cargo fmt --all --check && cargo run -q -p corten-cli -- fmt --check crates examples bench && cargo clippy --workspace --all-features --all-targets -- -D warnings && cargo test --workspace --all-features"
docker run --rm -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-target:/target -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm sh -c "rustup component add clippy rustfmt >/dev/null 2>&1; cargo test -p corten --no-default-features && cargo build -q -p corten-cli && /target/debug/corten new /tmp/app --corten-path /src/crates/corten && cd /tmp/app && /target/debug/corten check && cd /src && cargo publish --workspace --dry-run --locked --allow-dirty"
docker run --rm -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-msrv-target:/target -e CARGO_TARGET_DIR=/target -w /src rust:1.88-bookworm cargo check --locked --workspace --all-features --all-targets
npm run check
docker build -f examples/watchlist/Dockerfile -t corten-watchlist .
docker run -d --name corten-watchlist -p 18480:8080 corten-watchlist
npm run smoke -- http://localhost:18480
docker rm -f corten-watchlist
docker run -d --name corten-watchlist-dev -p 18483:8080 -e CORTEN_DEV=1 corten-watchlist
node scripts/smoke-dev.mjs http://localhost:18483 corten-watchlist-dev
docker rm -f corten-watchlist-dev
docker build -f examples/card-market/Dockerfile -t corten-card-market .
docker run -d --name corten-card-market -p 18481:8080 -e ITEM_SOURCE=fixture -e FIXTURE_PATH=/fixtures/smoke.json -e AGENT_ORIGINS=http://127.0.0.1:18481 -v "${PWD}/examples/card-market/fixtures:/fixtures:ro" corten-card-market
node examples/card-market/smoke.mjs http://localhost:18481
docker rm -f corten-card-market
docker build -f examples/ui-gallery/Dockerfile -t corten-ui-gallery .
docker run -d --name corten-ui-gallery -p 18482:8080 corten-ui-gallery
node examples/ui-gallery/smoke.mjs http://localhost:18482
docker rm -f corten-ui-gallery
```

The first command also compiles every `rust` code block in `README.md` and `docs/guide/` as a
doctest. Mark a fragment that cannot compile on its own `rust,ignore`, and a complete program
that serves forever `rust,no_run`. The second ends by packaging every published crate and
building each from its `.crate` file, as crates.io will (`cargo publish --dry-run`); the third
builds on the oldest supported Rust, `rust-version` in the root `Cargo.toml`. The `CI` workflow
(`.github/workflows/ci.yml`) runs the same checks on every pull request and push to `main`.

## Cluster integration tests

`corten-cluster`'s tests run against real Postgres and NATS JetStream when two variables are
set, and are skipped otherwise:

```powershell
docker network create corten-test
docker run -d --name corten-test-pg --network corten-test -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=feeds postgres:16-alpine
docker run -d --name corten-test-nats --network corten-test nats:2-alpine -js
docker run --rm --network corten-test -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-target:/target -e CARGO_TARGET_DIR=/target -e CORTEN_TEST_DATABASE_URL=postgres://postgres:postgres@corten-test-pg/feeds -e CORTEN_TEST_NATS_URL=nats://corten-test-nats:4222 -w /src rust:1-bookworm cargo test -p corten-cluster -- --test-threads=1
docker rm -f corten-test-pg corten-test-nats
docker network rm corten-test
```

The failover benchmark is `node examples/cluster/bench/run.mjs`; see its header.

## Pull requests

1. One change per pull request, with a test for each behavior: a Rust test, a `compile_fail`
   doctest for a compile-time guarantee, an `@ts-expect-error` line in
   `scripts/typecheck-fixture.js` for a typing guarantee, and a browser smoke check for anything
   the runtime does.
2. A change to rendering, the runtime or the request path says what it measured before and after
   (`render_cost`, bytes shipped, nanoseconds per request).
3. Update the guide in `docs/guide/` that describes the behavior, and add a line to
   `CHANGELOG.md` under Unreleased.
4. Commit messages say what changed and why, in plain sentences.
5. The description says what changed and why, the checks run and their results, and the
   measurements from 2. It merges after its [review](#review).

## Review

Every pull request is reviewed against the [values](README.md#values) before it merges, by
someone who did not write it: another person, or, when an agent wrote the change, a new agent
that starts from the diff and these documents rather than from the author's reasoning. The
reviewer reads the change as an adversary and looks for the way it breaks a value; where values
conflict, the order in the README decides. What to look for:

- Correctness and reliability: an input, failure, cancellation, retry or concurrent request it
   gets wrong; a behavior with no test, or a test that would still pass if the behavior broke.
- Performance at the user's end: a cost added to every request, render or page (instructions,
   allocation, bytes shipped, a round trip) with no before-and-after number, or a number taken
   with a measurement that cannot see the change.
- Compile-time safety and one way to do things: a mistake left to startup or to a request that
   the compiler could catch, a guarantee with no `compile_fail` doctest or `@ts-expect-error`
   line, a string at a seam (a URL, a ref, a class, a message key), or a second way to do
   something that already has one ([design rules](docs/design.md#rules)).
- The other values: a dependency or concept a smaller change would avoid, something the
   browser or HTTP already provides, a page that stops working without JavaScript, a default
   that is not production-ready, a guide or changelog that no longer matches.

Findings come first, the most severe first, each with its file and line and the value it
breaks. The author fixes each one or answers on the pull request why it stands. The pull
request merges when every finding is resolved and the checks pass. A review that finds nothing
says so, and names what it could not check.

## Releasing

Every published crate (all of `crates/`) shares the version in `[workspace.package]` and is
released together; the examples have `publish = false`. Pushing a tag `vX.Y.Z` runs the
`Release` workflow (`.github/workflows/release.yml`): the checks, a `corten` binary for each
platform, `cargo publish --workspace` (each crate after the ones it depends on), then a GitHub
release with the binaries and the version's `CHANGELOG.md` section as its notes. It stops before
publishing when the tag, the workspace version and the changelog disagree.

1. In one pull request, set `version` in `[workspace.package]` and the same version on every
   `corten*` line of `[workspace.dependencies]`, then rename `## Unreleased` in `CHANGELOG.md`
   to `## X.Y.Z (YYYY-MM-DD)` and start a new empty `## Unreleased` above it. A release before
   1.0 that changes the API bumps the minor version.
2. Merge it, then tag that commit on `main` and push the tag:
   `git tag -a vX.Y.Z -m "Corten X.Y.Z"` and `git push origin vX.Y.Z`.
3. Approve the `release` environment when the workflow asks.

A published version cannot be replaced, only yanked (`cargo yank --version X.Y.Z <crate>`), so
a fix is a new patch release.

### Before the first release

crates.io only lets a workflow publish (trusted publishing) for a crate that already exists, so
the first release uses an API token once:

1. Sign in to crates.io with the GitHub account that owns the repository.
2. In the repository settings, create the environment `release` with yourself as a required
   reviewer, and add a secret `CARGO_REGISTRY_TOKEN` to it: a crates.io token with the
   `publish-new` and `publish-update` scopes, expiring in a few days.
3. Release as above. Then, for each crate, open its Settings on crates.io, add a trusted
   publisher (GitHub, repository `todd-skelton/corten`, workflow `release.yml`, environment
   `release`), delete the secret and revoke the token. Later releases authenticate through
   `rust-lang/crates-io-auth-action` with no stored token.
