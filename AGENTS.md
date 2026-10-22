# Working on Corten

Read `README.md` (rules, request modes) and `docs/architecture.md` (surfaces) before changing
anything. Keep the framework small: a change that adds a dependency or a new concept needs a
reason that a smaller change cannot meet.

## Where things go

1. Wire format and client-side feed logic: `crates/corten-protocol`. No async or web
   dependencies there; it must build for native and agent clients.
2. Server feed (replay, live streams, followers): `crates/corten-feed`.
3. Pages, request modes, patches, islands, the runtime, `App` defaults: `crates/corten`.
   Markup is `html!` and `#[component]`, links are `#[derive(Route)]` structs, behavior is an
   `#[island]` (its script next to it; `ref={Name}` and `on:event={Action}` in its markup) and
   text is `messages!` (`crates/corten-macros`, no dependencies: keep it on `proc_macro` alone).
   Never write a URL, `data-ref`, `data-on` or message key as a string in a view. Islands have
   one pattern, `island(tag, { state, connect, actions })`; do not add built-in client behaviors.
   Cleanup is `ctx.signal` alone (an `AbortSignal`): runtime APIs that start lasting work take
   `{ signal }` and return nothing, and nothing hands back a stop function. On the server, work
   that outlives the handler's future takes `corten::Signal`.
   Styles are a CSS file per component (`#[component(css)]`, scoped at compile time, `class={Name}`)
   and tokens from `tokens!`; never write one of a component's classes as a string, and keep
   inline `style=` out (the CSP blocks it).
   A compile-time guarantee gets a `compile_fail` doctest next to the feature. A change to what `html!` generates needs the item page's
   `render_cost` test before and after (`CORTEN_FIXTURE=bench/fullstack/fixtures/items.json`,
   release build) so rendering does not get slower unnoticed.
4. The `corten` command: `crates/corten-cli`, standard library only. `template/` is the app
   `corten new` writes; it must pass `corten check` with rustfmt's default settings.
5. Edit the runtime in `crates/corten/js/corten.js` and `solid.js`; the crate embeds their
   minified builds in `js/dist/`, which only `npm run build:js` writes (`npm run check` fails
   when they are stale).
6. Every behavior the runtime has is covered by `scripts/smoke.mjs` against `examples/hello`,
   and production modules (security, assets, PWA, head tags, compression) by
   `examples/item-page/smoke.mjs`. Add the check with the behavior.
7. Types across seams: never hand-write a type the server already defines. Islands are checked
   against the declarations `corten::types` generates (`examples/*/types/app.d.ts`, rewritten by
   the `island_types_match_the_server` test); a typing guarantee gets an `@ts-expect-error` line in
   `scripts/typecheck-fixture.js`.
8. Widgets the browser lacks go in `crates/corten-ui`: headless `#[island]`s (no CSS; parts are
   styled by role and ARIA state), labels from the app, links as typed routes (`Link`), working as
   plain HTML before the script loads. Add one only when an app needs it and the platform has no
   element for it; remove it when the platform does. Each is tested in a browser by
   `examples/ui-gallery/smoke.mjs`.
9. `bench/` is not part of the workspace. Its apps are built only by `bench/fullstack` scripts.

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
docker build -f examples/ui-gallery/Dockerfile -t corten-ui-gallery .
docker run -d --name corten-ui-gallery -p 18482:8080 corten-ui-gallery
node examples/ui-gallery/smoke.mjs http://localhost:18482
docker rm -f corten-ui-gallery
```

Rust tests run in Docker because the Windows host cannot link `windows-sys`. Run each command
separately.
