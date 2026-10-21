# Corten

A small full-stack framework for Rust: the server renders every screen, the browser gets HTML
plus a ~200-line runtime and custom-element islands, and live data flows as numbered patches
over server-sent events. The same server is built to answer native screens and AI agents from
the same routes (see [docs/architecture.md](docs/architecture.md)). Web is the first surface.

Status: 0.1, not on crates.io or npm.

License: not chosen yet. Until a license file is added, all rights are reserved.

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
    js/corten.js     Browser runtime, served at /corten/corten.js, imported as "corten" (Island, subscribe)
    js/solid.js      SolidIsland, h, when, each; bundled into js/dist/solid.js (feature "solid")
  corten-feed/       Server side of a live feed: rows, numbered patches, replay, resume, follow a log
  corten-protocol/   Wire types and Replica (the client side), no web or async dependencies
examples/hello/      One screen that uses every piece; scripts/smoke.mjs tests it in a browser
docs/architecture.md How web, native and agent surfaces share one server
scripts/             build-solid.mjs (regenerates js/dist/solid.js), smoke.mjs
```

Dependencies: `askama`, `axum`, `futures-util`. Feature `json` adds `serde` and `serde_json`;
`feed` adds `corten-feed` (and `tokio`); `solid` adds nothing on the server. Apps need no
JavaScript dependencies and no bundler; `package.json` is tooling for the framework itself.

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

## Limits

1. Client navigation needs the Navigation API (Chrome, Edge, Firefox 147+, Safari 26.2+); older
   browsers do full page loads, and forms, patches and islands still work.
2. Patches replace whole elements, so focus inside a replaced element is lost. Keep targets small.
3. Scripts inside swapped pages do not run. Put behavior in islands.
4. No offline mode and no client-side routing. A screen that needs offline or a camera is a
   native screen (see the architecture doc).
5. One process per feed unless the app follows a shared log; the Postgres outbox to NATS relay
   that does this was proven in the benchmark branch and is not extracted yet.
