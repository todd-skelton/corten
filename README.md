# Corten

A small full-stack framework for Rust: the server renders every screen, the browser gets HTML
plus a ~200-line runtime and custom-element islands, and live data flows as numbered patches
over server-sent events. The same server is built to answer native screens and AI agents from
the same routes (see [docs/architecture.md](docs/architecture.md)). Web is the first surface.

Status: 0.1, not on crates.io or npm. A marketplace item page is built
in `examples/item-page` and compared with Next.js, TanStack Start, SvelteKit, React Router and
Leptos in [bench/fullstack](bench/fullstack/README.md): cold start, server rendering with and
without a database, and browser timing on desktop and a throttled phone profile.

License: [MIT](LICENSE).

## Name

Cor-ten is weathering steel: it is meant to rust, and the rust layer is what protects it, so it
needs no paint. Rust underneath, a thin protective surface on top, no build step.

- Rust crates: `corten`, `corten-feed`, `corten-protocol`. The crates.io name `corten` is a 2022
  placeholder whose owner says they will transfer it on request; ask before publishing.
- JavaScript: the runtime is imported as `corten`. The npm name is free.
- "COR-TEN" is a U.S. Steel trademark for steel. Check it before any public release.

## Layout

```text
crates/
  corten/            Web framework on axum: Mode, Page, Shell, Req (page/screen/patch/redirect), live(), routes(), Props
    js/corten.js     Browser runtime, served at runtime_path() (/corten/{version}/corten.js), imported as "corten"
    js/solid.js      SolidIsland, h, when, each; bundled into js/dist/solid.js (feature "solid")
  corten-feed/       Server side of a live feed: rows, numbered patches, replay, resume, follow a log
  corten-protocol/   Wire types and Replica (the client side), no web or async dependencies
  corten-cluster/    One feed across instances: Postgres outbox, leased relay to NATS JetStream, idempotent writes
examples/hello/      One screen that uses every piece; scripts/smoke.mjs tests it in a browser
examples/item-page/  A marketplace item page with every production module on; smoke.mjs tests it
examples/cluster/    A listings feed on corten-cluster, plus the failover benchmark (bench/, results/)
docs/architecture.md How web, native and agent surfaces share one server
scripts/             build-solid.mjs (regenerates js/dist/solid.js), smoke.mjs
bench/fullstack/     The same item page in six frameworks, and the scripts that compare them
```

Dependencies: `askama`, `axum`, `futures-util`, `getrandom` (CSP nonces), `tokio` (signals
only) and `tracing`. Feature `json` adds `serde` and `serde_json`; `feed` adds `corten-feed`;
`agent` needs only `json`; `solid` adds nothing on the server. Apps need no JavaScript
dependencies and no bundler; `package.json` is tooling for the framework itself.

## Production modules

All in the `corten` crate; `examples/item-page/src/app.rs` switches every one on.

| Module | What it gives an app |
|---|---|
| `security` | `Policy`: CSP with a per-request nonce (`head.nonce()`), nosniff, referrer, frame and opener headers, optional HSTS, `allow_popups` for sign-in or payment popups, and a same-origin check on unsafe methods (`Sec-Fetch-Site`, then `Origin` against `Host`; `allow_origin` for proxies). `cookie::get`, `cookie::SetCookie` (HttpOnly, Secure, Lax by default). Sessions stay in the app. |
| `assets` | `Assets::load(dir, "/assets")`: files in memory, URLs `/assets/{content-hash}/...` served immutable with an ETag; the islands directory is versioned the same way. |
| `meta` | `Page::head() -> Meta`: description, canonical, noindex, Open Graph, links, JSON-LD. Client navigation swaps these tags too. |
| `pwa` | `Manifest` and `Pwa`: `/manifest.webmanifest` and a generated `/service-worker.js`: precache, network-first navigation with an offline page, cache-first only for immutable versioned files. |
| `i18n` | `Catalog` (JSON messages, missing keys visible), `Locales::negotiate(Accept-Language)`, number, money (integer minor units), percent and date formatting. Formatting is en-US only so far. |
| `seo` | `sitemap`, `sitemap_index`, `robots`. |
| `agent` | `Tools`: the app's actions as Model Context Protocol tools over HTTP POST (`tools/list`, `tools/call`), with the app's own auth. |
| crate root | `error_pages::<Shell, _>` (404/403/500 in the request's mode), `health(ready)` (`/health/live`, `/health/ready`), `shutdown_signal()` (Ctrl-C and SIGTERM; ends live streams so shutdown finishes). |

## How a request works

| Request | Who sends it | Handler returns | Client does |
|---|---|---|---|
| `GET`, no `corten` header | first load, reload, no JavaScript | `req.page(&page)`: the shell around the page | renders HTML |
| `GET` with `corten: nav` | runtime, same-origin link or back/forward | `<title>` and the page only | swaps it into `<main>` |
| `POST` with `corten: action` | runtime, `<form data-patch>` | `req.patch(&fragment)` | replaces the element with each fragment's `id` |
| `POST`, no header | form without JavaScript | `req.page(&page)` or `req.redirect(url)` | renders the page |
| any, `Accept: application/json` | native screen, agent, test | `req.screen(&page)`: `{"title", "data"}` | renders it natively, or reasons over it |
| SSE | `<corten-live src>` | `corten::live(stream_of_fragments)` | applies each fragment like a patch |
| `GET {feed}`, `{feed}/live` | `subscribe()` or `Replica` | `feed.snapshot()`, `feed.live(since)` | snapshot, then ordered patches |

Handlers are plain axum handlers; one handler serves every mode and `req.mode` says which.

```rust
async fn watch(req: Req, With(state): With<State>, Form(form): Form<WatchForm>) -> corten::Result {
    let message = state.watch(&form.id);
    match req.mode {
        Mode::Action => req.patch(&WatchMessage { message: &message }),
        _ => req.screen(&Home::new(&state, message)), // no JavaScript, native, agent
    }
}
```

## Rules

1. HTML comes only from askama templates on the server. Islands never build markup from strings;
   they move, show or hide server-rendered nodes. Extra rows ship in `<template data-ref>`.
2. A fragment that can be patched lives in its own template file and the page includes it, so
   the page and the patch cannot drift (`watch_message.html` in the example).
3. One island per file. The file name is the tag name, the default export is the class, and it
   extends `Island`. Corten finds islands in the rendered page, preloads exactly those modules
   and makes the wrappers `display: contents`. Nothing to register.
4. Islands find elements with `this.ref("name")` (throws when `data-ref="name"` is missing) and
   listen with `this.on("click", "name", handler)`. State lives in the DOM (`data-*`).
5. User-facing text lives in templates or in the view model, never in island code.
6. Everything works without JavaScript: links are links and forms post.
7. A page that another surface reads derives `Serialize`, and its fields are its whole view
   model: anything the template shows is a field (formatted text included), never computed in
   the template. Then `req.screen` gives every surface the same data.

## Islands with state

A screen whose state lives in the browser (a seller grid with thousands of live rows) uses three
opt-in pieces; nothing else changes.

| Piece | Server | Browser |
|---|---|---|
| Typed props | `{{ corten::Props(self.grid())\|safe }}` (feature `json`) | `this.props()` |
| Live rows | `corten::feed::Feed<Row>`: `write(\|tx\| ...)`, `snapshot()`, `live(since)` (feature `feed`) | `this.subscribe(url, { snapshot, patch, status })` |
| Rendering | serves `/corten/solid.js` as `corten/solid` (feature `solid`) | `class X extends SolidIsland { view(server) { ... } }` |

Feed guarantees: every `write` is one numbered patch carrying the final value of each row it
touched, encoded once and sent as the same bytes to every subscriber. A subscriber gets the
snapshot and then each later patch exactly once, in order. Reconnects resume from
`Last-Event-ID`; an unknown or expired cursor, a client too far behind, or a gap ends in a fresh
snapshot. Several server instances can follow one log (`Feed::follow`, `apply`, `backfill`,
`reset`) and accept each other's cursors. Rules for stateful islands: data rules in a plain
`*.model.js` module, shared shapes in a `*.types.js` JSDoc file checked against the Rust struct,
apply patches once per animation frame, render long lists as a window.

## Checks

```powershell
# Rust: tests, lints, format (Docker, because the Windows host cannot link windows-sys)
docker run --rm -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-target:/target -e CARGO_TARGET_DIR=/target -w /src rust:1-bookworm sh -c "rustup component add clippy rustfmt >/dev/null 2>&1; cargo fmt --all --check && cargo clippy --workspace --all-features --all-targets -- -D warnings && cargo test --workspace --all-features"
# JavaScript types, strict, no build
npm install; npm run check
# Browser behavior against the example
docker build -f examples/hello/Dockerfile -t corten-hello .
docker run -d --name corten-hello -p 18480:8080 corten-hello
npm run smoke -- http://localhost:18480
# After editing crates/corten/js/solid.js or upgrading Solid
npm run build:solid
```

`corten-cluster`'s integration tests need Postgres and NATS and are skipped without them:

```powershell
docker network create corten-test
docker run -d --name ct-pg --network corten-test -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=corten postgres:16-alpine
docker run -d --name ct-nats --network corten-test nats:2-alpine -js
docker run --rm --network corten-test -v "${PWD}:/src" -v corten-cargo:/usr/local/cargo/registry -v corten-target:/target -e CARGO_TARGET_DIR=/target -e CORTEN_TEST_DATABASE_URL=postgres://postgres:postgres@ct-pg/corten -e CORTEN_TEST_NATS_URL=nats://ct-nats:4222 -w /src rust:1-bookworm cargo test -p corten-cluster -- --test-threads=1
```

The failover benchmark (two instances behind nginx, each killed and restarted under load, 200
SSE clients plus a real browser): `docker build -f examples/cluster/Dockerfile -t corten-cluster .`
then `node examples/cluster/bench/run.mjs out=failover-r1.json`. Last result (Docker Desktop,
one run, 50 writes/s): 777,600 of 777,600 patches delivered, 0 lost, 0 out of order, 0 resyncs,
relay takeover 215 ms, 0 duplicate commits with idempotent retries. Details in
`examples/cluster/results/`.

## Limits

1. Client navigation needs the Navigation API (Chrome, Edge, Firefox 147+, Safari 26.2+); older
   browsers do full page loads, and forms, patches and islands still work.
2. Patches replace whole elements, so focus inside a replaced element is lost. Keep targets small.
3. Scripts inside swapped pages do not run. Put behavior in islands.
4. Offline is a fallback page plus cached versioned files, not offline data. Pages and actions
   need the network. A screen that must work offline or needs a camera is a native screen (see
   the architecture doc).
5. Hover and touch prefetch reuses a fetched page for up to 10 seconds (cleared after any form
   post). A page that must never be stale should not be reached by a plain link.
6. A feed's write rate is bounded by Postgres commit latency on its feed row (every write takes
   the next sequence number). Split hot data into several feeds.
