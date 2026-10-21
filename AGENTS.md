# Working on Corten

Read `README.md` (rules, request modes) and `docs/architecture.md` (surfaces) before changing
anything. Keep the framework small: a change that adds a dependency or a new concept needs a
reason that a smaller change cannot meet.

## Where things go

1. Wire format and client-side feed logic: `crates/corten-protocol`. No async or web
   dependencies there; it must build for native and agent clients.
2. Server feed (replay, live streams, followers): `crates/corten-feed`.
3. Pages, request modes, patches, islands, the runtime, `App` defaults: `crates/corten`.
   Markup is `html!` and `#[component]`, links are `#[derive(Route)]` structs, island refs are
   `#[derive(Refs)]` enums and text is `messages!` (`crates/corten-macros`, no dependencies: keep it
   on `proc_macro` alone). Never write a URL, `data-ref` or message key as a string in a view.
   A compile-time guarantee gets a `compile_fail` doctest next to the feature. A change to what `html!` generates needs the item page's
   `render_cost` test before and after (`CORTEN_FIXTURE=bench/fullstack/fixtures/items.json`,
   release build) so rendering does not get slower unnoticed.
4. The `corten` command: `crates/corten-cli`, standard library only. `template/` is the app
   `corten new` writes; it must pass `corten check` with rustfmt's default settings.
5. Never edit `crates/corten/js/dist/solid.js` by hand; run `npm run build:solid`.
6. Every behavior the runtime has is covered by `scripts/smoke.mjs` against `examples/hello`,
   and production modules (security, assets, PWA, head tags, compression) by
   `examples/item-page/smoke.mjs`. Add the check with the behavior.
7. Types across seams: never hand-write a type the server already defines. Islands import the
   declarations `corten::types` generates (`examples/*/types/app.d.ts`, rewritten by the
   `island_types_match_the_server` test); a typing guarantee gets an `@ts-expect-error` line in
   `scripts/typecheck-fixture.js`.
8. `bench/` is not part of the workspace. Its apps are built only by `bench/fullstack` scripts.

## Checks (all must pass before you report done)

```powershell
docker run --rm -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-target:/target -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm sh -c "rustup component add clippy rustfmt >/dev/null 2>&1; cargo fmt --all --check && cargo clippy --workspace --all-features --all-targets -- -D warnings && cargo test --workspace --all-features"
docker run --rm -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-target:/target -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm sh -c "rustup component add clippy rustfmt >/dev/null 2>&1; cargo test -p corten --no-default-features && cargo build -q -p corten-cli && /target/debug/corten new /tmp/app --corten-path /src/crates/corten && cd /tmp/app && /target/debug/corten check"
npm run check
docker build -f examples/hello/Dockerfile -t corten-hello .
docker run -d --name corten-hello -p 18480:8080 corten-hello
npm run smoke -- http://localhost:18480
docker rm -f corten-hello
docker build -f examples/item-page/Dockerfile -t corten-item-page .
docker run -d --name corten-item-page -p 18481:8080 -e ITEM_SOURCE=fixture -e FIXTURE_PATH=/fixtures/smoke.json -v "${PWD}/examples/item-page/fixtures:/fixtures:ro" corten-item-page
node examples/item-page/smoke.mjs http://localhost:18481
docker rm -f corten-item-page
```

Rust tests run in Docker because the Windows host cannot link `windows-sys`. Run each command
separately.
