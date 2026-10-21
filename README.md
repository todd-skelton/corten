# Corten

Server-rendered web apps in Rust, with the interactivity of a client framework where a page
needs it and none of the weight where it does not.

- Pages are HTML from the server, written in `html!`, a JSX-like macro that compiles to string
  writes. Components are plain functions. A misspelled prop or field is a compile error.
- The browser gets a small runtime (5.5 KB compressed) that swaps pages on navigation, patches
  fragments after form posts and loads islands: custom elements for the parts that need code.
- Live data is numbered patches over server-sent events, resumable after a reconnect or a
  failover. Islands with heavy client state can render with Solid.
- No WASM, no bundler, no JavaScript toolchain in the app. One binary and a `static/`
  directory.

Status: 0.1, not yet on crates.io. License: [MIT](LICENSE).

## Quick start

```sh
cargo install --git https://github.com/todd-skelton/corten corten-cli
corten new my-app
cd my-app
corten dev        # http://localhost:3000, rebuilds on save, open pages reload
```

`corten new` writes a working app: a page, a form that patches part of the page, an island, a
test and a Dockerfile.

| Command | What it does |
|---|---|
| `corten new <dir>` | Creates an app. `--name` sets the crate name, `--corten-path` uses a local checkout. |
| `corten dev` | Builds and runs the app, rebuilds and restarts on every change (a failed build keeps the last good server running), and reloads open pages. `--port` or `PORT`. |
| `corten check` | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`. |
| `corten build` | The release binary. |

The CLI is a convenience over cargo, not a build system: an app is an ordinary crate, and
`cargo run` works too.

## An app

```rust
use corten::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let routes = Router::new().route("/", get(home)).route("/greet", post(greet));
    App::new(routes).shell(shell).serve().await
}

async fn home(req: Req) -> corten::Result {
    req.page("Home", html! {
        <h1>"Welcome"</h1>
        <form method="post" action="/greet" data-patch>
            <input name={GreetForm::FIELDS.name} required>
            <button>"Greet me"</button>
        </form>
        <Greeting name="" />
    })
}

#[derive(serde::Deserialize, Fields)]
struct GreetForm { name: String }

/// With JavaScript the runtime swaps in the patch; without it the form posts and the whole page
/// comes back. One handler serves both.
async fn greet(req: Req, Form(form): Form<GreetForm>) -> corten::Result {
    match req.mode {
        Mode::Action => req.patch(Greeting { name: &form.name }),
        _ => req.page("Home", html! { <Greeting name={&form.name} /> }),
    }
}

#[component]
fn Greeting(name: &str) -> impl Render {
    html! { <p id="greeting" aria-live="polite">if !name.is_empty() { "Hello, "{name}"!" }</p> }
}

fn shell(doc: Document) -> impl Render {
    html! {
        <!DOCTYPE html>
        <html lang={doc.lang()}>
            <head><meta charset="utf-8">{doc.head}<link rel="stylesheet" href={doc.asset("style.css")}></head>
            <body><main>{doc.main}</main></body>
        </html>
    }
}
```

Handlers are plain axum handlers. `App` adds everything a production app needs around them.

## App: defaults and overrides

| Concern | Default | Override |
|---|---|---|
| Address | `HOST` (`127.0.0.1`), `PORT` (`3000`) | environment, or `app.router()` to serve it yourself |
| Shell | a minimal HTML document | `.shell(fn(Document) -> impl Render)`, or `fn(Document, Option<C>)` with a context |
| Who is asking | nothing | `.context(\|headers\| async { Some(viewer) })`: shells, handlers (`req.context::<C>()`) and error pages see it |
| Error pages | 404, 405 and 500 inside the shell, in the request's mode (page, swap or JSON); the error is logged, never shown | `.error_page(\|status\| (title, view))` |
| Security | CSP with a per-request nonce, nosniff, referrer, frame and opener headers, a same-origin check on unsafe methods | `.policy(Policy::new()...)` |
| Static files | `static/` (or `STATIC_DIR`) at content-hashed, immutable URLs; islands in `static/islands/` preloaded per page | `.assets(dir)` |
| Health | `/health/live`, `/health/ready` (always ready) | `.ready(\|\| async { db_ok().await })` |
| Shutdown | Ctrl-C and `SIGTERM` end live streams, then the server | |
| Logs | errors, startup and shutdown to stderr (feature `log`) | set your own `tracing` subscriber first |
| Compression | pages per request (zstd, brotli, gzip), static files once at the highest levels (feature `compress`) | |
| Installable app | off | `.pwa(Pwa::new(manifest))`: manifest, generated service worker, offline page |
| Languages | none | `.locales(locales)`: `req.t()` and `doc.lang()` follow `Accept-Language` |
| Dev mode | when `CORTEN_DEV` is set (`corten dev` sets it): errors shown on the page, open pages reload after a rebuild | `.dev(bool)` |

## Views

`html!` reads like JSX and compiles to `push_str` calls: static markup is merged at compile
time, expressions are escaped as they are written, and the output buffer is sized from the last
render of the same call site.

| Write | Means |
|---|---|
| `"Price"` | text (source whitespace between tags is never output) |
| `{expr}` | an expression, escaped: anything that implements `Render` (strings, numbers, `Option`, other views) or else `Display`. `raw(html)` prints trusted HTML |
| `href={url}`, `href={("/items/", &id)}` | an attribute; a tuple prints its parts without building a string |
| `hidden={flag}`, `class={maybe}` | `bool` prints the bare name or nothing; `None` leaves the attribute out |
| `<input {..errors.aria(field)}>` | spreads `Attrs` |
| `for`, `if`/`else`, `match`, `let` | as in Rust, with markup in the braces |
| `<Card title="x" compact>children</Card>` | a component: a struct with those fields (`compact` is `true`), children in `children` |
| `<>...</>`, `<!DOCTYPE html>`, `<input>` | fragments, the doctype, void elements without a closing tag |

`#[component]` turns a function into a props struct and a `Render` impl, so the same
component renders inside a page and on its own as a patch, and the two cannot drift. A view owns
what it captures and renders once, so it can be returned from a function.

## How a request works

| Request | Sent by | Handler returns | Client does |
|---|---|---|---|
| `GET`, no `corten` header | first load, reload, no JavaScript | `req.page(meta, view)`: the shell around the page | renders HTML |
| `GET` with `corten: nav` | runtime, same-origin link or back/forward | the same call: `<title>`, head tags and the page only | swaps `<main>` |
| `POST` with `corten: action` | runtime, `<form data-patch>` | `req.patch(view)` | replaces each element by `id` |
| `POST`, no header | a form without JavaScript | `req.page(..)` or `req.redirect(url)` | renders the page |
| any, `Accept: application/json` | native screen, agent, test | `req.screen(meta, &data, view)`: `{"title", "data"}` | renders natively, or reasons over it |
| SSE | `<corten-live src>` | `corten::live(stream_of_views)` | applies each like a patch |
| `GET {feed}`, `{feed}/live` | an island's `subscribe()`, or `Replica` | `feed.snapshot()`, `feed.live(since)` | snapshot, then ordered patches |

One handler serves every mode, and `req.mode` says which. The shell renders on full loads only;
every page carries a key hashed from the context and the deploy, and when a navigation answers
with a different key (sign-in, a new release) the runtime loads the whole document instead, so
the shell is never stale.

## Forms

`#[derive(Fields)]` gives a form struct its field names as constants, so markup and validation
cannot misspell them. Validation stays in the app; `Errors` carries the first message per field.

```rust
if let Err(errors) = check(&input) {
    // 422: the form as a patch for the runtime, `{"errors": ...}` for JSON clients, or the whole
    // page for a browser without JavaScript (only then is `page` called).
    return req.invalid(&errors, AlertForm { input: &input, errors: &errors }, page).await;
}
```

In the form, `<input name={f.email} {..errors.aria(f.email)}>` sets `aria-invalid` and
`aria-describedby`, and the runtime focuses the first invalid field.

## Islands

An island is a custom element around server-rendered markup. One file per island in
`static/islands/`: the file name is the tag name and the default export extends `Island`.
Corten finds islands in the rendered page and preloads exactly those modules; there is nothing to
register.

```js
import { Island } from "corten";

export default class ClickCounter extends Island {
  setup() {
    let clicks = 0;
    // The element with data-ref="button" inside this island; a missing ref throws.
    this.on("click", "button", (button) => { button.textContent = `Clicked ${++clicks} times`; });
  }
}
```

For screens whose state lives in the browser (a grid of thousands of live rows):

| Piece | Server | Browser |
|---|---|---|
| Typed props | `{Props(&grid)}` in the markup (feature `json`) | `this.props()` |
| Live rows | `corten::feed::Feed<Row>`: `write(\|tx\| ...)`, `snapshot()`, `live(since)` (feature `feed`) | `this.subscribe(url, { snapshot, patch, status })` |
| Rendering | serves Solid as `corten/solid` (feature `solid`) | `class X extends SolidIsland { view(server) { ... } }` |

Feed guarantees: every `write` is one numbered patch carrying the final value of each row it
touched, encoded once and sent as the same bytes to every subscriber. A subscriber gets the
snapshot and then each later patch exactly once, in order; reconnects resume from
`Last-Event-ID`, and an unknown cursor or a gap ends in a fresh snapshot. `corten-cluster`
runs one feed across instances (Postgres outbox, a leased relay to NATS JetStream): in the
failover benchmark, 777,600 of 777,600 patches arrived with both instances killed and restarted
under load (measured, one run, `examples/cluster/results/`).

## Types across seams

Every value that crosses from Rust to the browser or to an agent is defined once, in Rust, and
checked on the other side.

| Seam | How it is checked |
|---|---|
| Handler to markup | `html!` is Rust: a missing field or prop is a compile error |
| Form to handler | `#[derive(Fields)]` names; serde parses the post |
| Server to island | `corten::types::Types::new().add::<Row>().sync(path)` in a test writes TypeScript declarations and fails while they are stale; `npm run check` then fails in every island that reads an old name |
| Server to agent | tool input and output schemas generated from the structs (feature `agent`) |

Untyped props and rows are `unknown`, not `any`. The full table, and what is not checked, is
in [docs/architecture.md](docs/architecture.md#types-across-seams).

## Performance

The same item page (listings table, offers, a form that patches) in six frameworks, one CPU per
app. Measured on one machine, one run per phase; full tables and method in
[bench/fullstack](bench/fullstack/README.md).

| | Corten | Leptos 0.8 | SvelteKit 3 | React Router 7 | TanStack Start | Next.js 16 |
|---|---|---|---|---|---|---|
| Rendered pages per second, no database, 64 users | **3,355** | 440 | 224 | 173 | 137 | 52 |
| Cold start to first page | **184 ms** | 220 ms | 425 ms | 1,035 ms | 496 ms | 1,679 ms |
| Memory after first request | **1.6 MiB** | 1.7 MiB | 23.5 MiB | 33.0 MiB | 26.4 MiB | 49.2 MiB |
| JavaScript the page loads, gzip | **6.2 KB** | 7.0 KB + 287 KB WASM | 37.0 KB | 99.2 KB | 108.6 KB | 136.1 KB |

With a database fetch on every request both Rust apps are bound by parsing the API response and
measure about equal, around 3x the fastest JavaScript framework.

## Choices

1. **Markup is a Rust macro, not TSX.** Rendering TSX needs a JavaScript runtime on the server
   or a second toolchain in the app; `html!` keeps one language and compiles to the same string
   writes a hand-written renderer would do. Measured on the item page, `html!` renders as fast
   as the compiled templates it replaced (askama).
2. **No WASM.** Hydrating a WASM app costs hundreds of kilobytes before the page responds; the
   server already has the HTML, so the browser only needs behavior where there is behavior.
3. **No virtual DOM on the server.** Frameworks that build a view tree per request and then
   serialize it pay for it in throughput (Leptos above). `html!` writes straight to the
   response buffer.
4. **Custom elements, not a client framework, by default.** Islands use the platform; Solid is
   opt-in for the screens that need fine-grained reactive state.

## Rules

1. HTML comes from the server, in `html!`. Islands never build markup from strings; they move,
   show or hide server-rendered nodes. Extra rows ship in `<template data-ref>`.
2. A fragment that can be patched is a component, rendered by the page and by the patch, so the
   two cannot drift.
3. Islands find elements with `this.ref("name")` and listen with `this.on(event, "name",
   handler)`. State lives in the DOM (`data-*`).
4. User-facing text lives in views or the view model, never in island code.
5. Everything works without JavaScript: links are links and forms post.
6. A page another surface reads derives `Serialize`, and its fields are its whole view model
   (formatted text included). Then `req.screen` gives every surface the same data.

## Layout

```text
crates/
  corten/            The framework on axum: App, Req, html!, components, the runtime (js/corten.js)
  corten-macros/     html!, #[component], #[derive(Fields)], on proc_macro alone (no syn)
  corten-cli/        The corten command, standard library only
  corten-feed/       Live feeds: rows, numbered patches, replay, resume
  corten-protocol/   Wire types and Replica (the client side), no web or async dependencies
  corten-cluster/    One feed across instances: Postgres outbox, NATS JetStream relay
examples/hello/      One screen that uses every piece; scripts/smoke.mjs tests it in a browser
examples/item-page/  A marketplace item page with every production module on; smoke.mjs tests it
examples/cluster/    A listings feed on corten-cluster, and the failover benchmark
docs/architecture.md How web, native and agent surfaces share one server
bench/fullstack/     The same page in six frameworks, and the scripts that compare them
```

Dependencies of `corten`: `axum`, `futures-util`, `getrandom` (CSP nonces), `memchr`
(already in axum's tree), `tokio` (sockets and signals), `tracing`, `corten-macros` (no
dependencies). Optional: `tracing-subscriber` (`log`, on by default), `serde` and `serde_json`
(`json`), `schemars` (`types`, `agent`), `zstd`, `brotli`, `flate2` (`compress`),
`corten-feed` (`feed`). Apps need no JavaScript dependencies; `package.json` here is tooling for
the framework itself.

## Limits

1. Client navigation uses the Navigation API (Chrome, Edge, Firefox 147+, Safari 26.2+); older
   browsers do full page loads, and forms, patches and islands still work.
2. Patches replace whole elements, so focus inside a replaced element is lost. Keep targets small.
3. Scripts inside swapped pages do not run. Put behavior in islands.
4. Offline is a fallback page plus cached versioned files, not offline data.
5. Hover and touch prefetch reuses a fetched page for up to 10 seconds (cleared after any form
   post).
6. A feed's write rate is bounded by Postgres commit latency on its feed row. Split hot data into
   several feeds.
7. `html!` views render once and own what they capture; read from `req` before building the view.

## Working on Corten

[AGENTS.md](AGENTS.md) lists the checks: Rust tests, lints and format (in Docker on Windows),
`npm run check` for the runtime's types, and browser smoke tests against both examples.

## Name

Cor-ten is weathering steel: it is meant to rust, and the rust layer is what protects it. Rust
underneath, a thin protective surface on top. "COR-TEN" is a U.S. Steel trademark for steel.
