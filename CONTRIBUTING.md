# Contributing to Corten

Thanks for helping. Open an issue to discuss a change before a large pull request; a small fix
can go straight to a pull request. Every change follows the [values](README.md#values), in the
priority order [AGENTS.md](AGENTS.md#values) gives, and a change that adds a dependency or a
new concept needs a reason a smaller change cannot meet.

## Layout

```text
crates/
  corten/            The framework on axum: App, Req, html!, components, the runtime (js/corten.js)
  corten-macros/     html!, #[component], #[island], messages!, tokens!, derives (Fields, Route), on proc_macro alone (no syn)
  corten-cli/        The corten command, standard library only
  corten-feed/       Live feeds: rows, numbered patches, replay, resume
  corten-protocol/   Wire types and Replica (the client side), no web or async dependencies
  corten-cluster/    One feed across instances: Postgres outbox, NATS JetStream relay
  corten-ui/         Optional headless islands: number field, combobox, tabs, menu button
examples/hello/      One screen that uses every piece; scripts/smoke.mjs tests it in a browser
examples/item-page/  A marketplace item page with every production module on; smoke.mjs tests it
examples/cluster/    A listings feed on corten-cluster, and the failover benchmark
examples/ui-gallery/ corten-ui styled by a small design system; smoke.mjs tests it in a browser
docs/              Guides (docs/guide), design decisions and limits, surfaces (architecture.md)
skills/corten/     The agent skill for building apps on Corten
bench/fullstack/     The same page in six frameworks, and the scripts that compare them
```

Dependencies of `corten`: `axum`, `futures-util`, `getrandom` (CSP nonces), `memchr`
(already in axum's tree), `http-body` (axum's body trait), `tokio` (sockets, signals, timers),
`tracing`, `corten-macros` (no dependencies). Optional: `tracing-subscriber` (`log`, on by default), `serde` and `serde_json`
(`json`), `schemars` (`types`, `agent`), `zstd`, `brotli`, `flate2` (`compress`),
`corten-feed` (`feed`). Apps need no JavaScript dependencies; `package.json` here is tooling for
the framework itself.

## Tooling

- Rust (stable). On Windows the tests run in Docker, because the host cannot link `windows-sys`.
- Node 22 or later, for the runtime's type check, its minified build and the browser smoke
  tests: `npm install`, then `npx playwright install chromium`.
- Docker, for the example images the smoke tests run against.

## Checks

All of these must pass before a pull request is ready. Run each command separately (shown for
PowerShell from the repository root; in another shell use `$PWD` for `${PWD}`).

```powershell
docker run --rm -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-target:/target -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm sh -c "rustup component add clippy rustfmt >/dev/null 2>&1; cargo fmt --all --check && cargo clippy --workspace --all-features --all-targets -- -D warnings && cargo test --workspace --all-features"
docker run --rm -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-target:/target -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm sh -c "rustup component add clippy rustfmt >/dev/null 2>&1; cargo test -p corten --no-default-features && cargo build -q -p corten-cli && /target/debug/corten new /tmp/app --corten-path /src/crates/corten && cd /tmp/app && /target/debug/corten check"
npm run check
docker build -f examples/hello/Dockerfile -t corten-hello .
docker run -d --name corten-hello -p 18480:8080 corten-hello
npm run smoke -- http://localhost:18480
docker rm -f corten-hello
docker run -d --name corten-hello-dev -p 18483:8080 -e CORTEN_DEV=1 corten-hello
node scripts/smoke-dev.mjs http://localhost:18483 corten-hello-dev
docker rm -f corten-hello-dev
docker build -f examples/item-page/Dockerfile -t corten-item-page .
docker run -d --name corten-item-page -p 18481:8080 -e ITEM_SOURCE=fixture -e FIXTURE_PATH=/fixtures/smoke.json -v "${PWD}/examples/item-page/fixtures:/fixtures:ro" corten-item-page
node examples/item-page/smoke.mjs http://localhost:18481
docker rm -f corten-item-page
docker build -f examples/ui-gallery/Dockerfile -t corten-ui-gallery .
docker run -d --name corten-ui-gallery -p 18482:8080 corten-ui-gallery
node examples/ui-gallery/smoke.mjs http://localhost:18482
docker rm -f corten-ui-gallery
```

The first command also compiles every `rust` code block in `README.md` and `docs/guide/` as a
doctest. Mark a fragment that cannot compile on its own `rust,ignore`, and a complete program
that serves forever `rust,no_run`.

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
