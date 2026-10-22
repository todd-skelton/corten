# Corten

**Server-rendered web apps in Rust, with interactivity only where a page needs it.**

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Corten renders every page on the server with `html!`, a JSX-like Rust macro. The browser gets
HTML, a small runtime that swaps pages and patches forms, and *islands*: components whose
behavior is a short script next to their Rust markup. Live data arrives as numbered patches over
server-sent events. One handler serves the browser, native apps and AI agents.

- **Fast on the server.** 3,355 rendered pages a second where Next.js 16 does 52, and a 184 ms
  cold start (measured, [below](#performance)).
- **Light in the browser.** A 3.3 KB (brotli) runtime; the benchmark item page loads 4.5 KB of
  JavaScript (gzip). No WASM, no virtual DOM, no hydration.
- **Typed end to end.** Routes, form fields, CSS classes, island refs and actions, and messages
  are Rust types. TypeScript types for islands are generated from Rust.
- **Works without JavaScript.** Links are links and forms post; scripts improve a page but never
  hold it up.
- **Production defaults.** CSP with nonces, a cross-site write check, error pages, versioned
  assets, compression, health checks, graceful shutdown and an installable PWA, from
  `App::new(routes).serve()`.
- **No JavaScript toolchain in the app.** One binary and a `static/` directory.

## Status

Corten is 0.1: usable, tested, and not yet on crates.io. Install it from this repository. The
API will change before 1.0; [CHANGELOG.md](CHANGELOG.md) lists what changed. `corten-ui` is
tested in Chromium only.

## Quick start

```sh
cargo install --git https://github.com/todd-skelton/corten corten-cli
corten new my-app
cd my-app
corten dev    # serves http://localhost:3000 and reloads on save
```

`corten new` writes a working app: a page, a form that patches part of the page, an island, a
test and a Dockerfile. See [Getting started](docs/guide/getting-started.md).

## Example

A page with a form. With JavaScript, submitting it swaps in the greeting; without JavaScript it
posts and the whole page comes back. One handler serves both.

```rust,no_run
use corten::prelude::*;

#[derive(Route)]
#[route("/")]
struct Home;

#[derive(Route)]
#[route("/greet")]
struct Greet;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let routes = Router::new()
        .route(Home::PATH, get(home))
        .route(Greet::PATH, post(greet));

    App::new(routes).serve().await
}

async fn home(req: Req) -> corten::Result {
    req.page("Home", page(""))
}

async fn greet(req: Req, Form(form): Form<GreetForm>) -> corten::Result {
    match req.mode {
        // With JavaScript: send only the greeting, swapped in by its id.
        Mode::Action => req.patch(Greeting { name: &form.name }),
        // Without: the whole page.
        _ => req.page("Home", page(&form.name)),
    }
}

#[derive(serde::Deserialize, Fields)]
struct GreetForm {
    name: String,
}

fn page(name: &str) -> impl Render + '_ {
    let f = GreetForm::FIELDS;
    html! {
        <h1>"Welcome"</h1>
        <form method="post" action={Greet} data-patch>
            <input name={f.name} required>
            <button>"Greet me"</button>
        </form>
        <Greeting name={name} />
    }
}

/// Rendered by the page and by the patch, so the two cannot drift.
#[component]
fn Greeting(name: &str) -> impl Render {
    html! {
        <p id="greeting" aria-live="polite">
            if !name.is_empty() {
                "Hello, " {name} "!"
            }
        </p>
    }
}
```

A misspelled route, field or prop does not compile. Behavior lives in an island: the markup
names what the script may touch, and the script sits in the file next to it.

```rust,ignore
// src/click_counter.rs
#[island]
pub fn ClickCounter() -> impl Render {
    html! {
        // on:click runs the script's `count` action;
        // ref={Total} is the element it reads as `refs.total`.
        <button type="button" on:click={Count}>
            "Clicks: "
            <output ref={Total}>"0"</output>
        </button>
    }
}
```

```js
// src/click_counter.js
import { island } from "corten";

export default island("click-counter", {
  state: () => ({ count: 0 }),
  actions: {
    count({ state, refs }) {
      state.count += 1;
      refs.total.value = String(state.count);
    },
  },
});
```

## When to use it

Corten fits apps whose screens are server data: stores, marketplaces, dashboards, admin tools,
content with live parts. It suits teams that want one language on the server and the platform in
the browser.

Look elsewhere for an offline-first app whose state lives on the device, for server-side TSX or
React components, or for a pure static site (a static site generator is simpler).

## Values

1. **Fast, and proven fast.** Speed is a feature, and every claim has a measured number behind
   it. A change to rendering, the runtime or the request path is measured before and after
   (`render_cost`, bytes shipped, nanoseconds per request). We say no to features that cost every
   user to help a few, like view transitions on every swap.
2. **The platform first.** Use what the browser and HTTP already provide: custom elements,
   `FormData`, `AbortSignal`, the Navigation API, popover, server-sent events, native validation.
   Build only what the platform lacks, and remove it once the platform catches up. One idea
   should work the same everywhere, so cancellation is `AbortSignal` in the browser and on the
   server.
3. **HTML that works on its own.** Every page renders on the server, and links and forms work
   without JavaScript. Scripts improve a page but never hold it up.
4. **Mistakes fail at compile time.** No strings at the seams between server and client: routes,
   refs, actions, class names, form fields and messages are all typed. TypeScript types are
   generated from Rust, never written by hand. If a guarantee exists, a test proves it fails to
   compile when broken.
5. **One way to do each thing.** Islands have one pattern, cleanup has one hook, and a handler
   serves every mode. Fewer concepts make large codebases readable for people and for agents.
   The behavior sits next to its markup, and the markup names everything the script can touch.
6. **Small surface, deliberate dependencies.** A dependency or a new concept has to clearly earn
   its place. The framework owns what nearly every app needs, like security headers,
   cancellation, error pages and assets. Domain logic and things only some apps use belong to the
   app or platform.
7. **Good defaults, easy overrides.** `App::new(routes).serve()` should be production-ready.
   Every default can be replaced in one line, and the common case stays short.
8. **One server, every surface.** The same handler and data serve the browser, native apps and AI
   agents. The server is the source of truth, and clients render it.
9. **Verified, not assumed.** Every runtime behavior has a browser smoke test. Reliability work,
   like failover and lost-patch counts, is benchmarked rather than argued.

## Performance

The same item page (listings table, offers, a form that patches) in six frameworks, one CPU per
app, measured on one machine. Method, every table and the caveats are in
[bench/fullstack](bench/fullstack/README.md).

| | Corten | Leptos 0.8 | SvelteKit 3 | React Router 7 | TanStack Start | Next.js 16 |
|---|---|---|---|---|---|---|
| Rendered pages per second, no database, 64 users | **3,355** | 440 | 224 | 173 | 137 | 52 |
| Cold start to first page | **184 ms** | 220 ms | 425 ms | 1,035 ms | 496 ms | 1,679 ms |
| Memory after first request | **1.6 MiB** | 1.7 MiB | 23.5 MiB | 33.0 MiB | 26.4 MiB | 49.2 MiB |
| JavaScript the page loads, gzip | **4.5 KB** | 7.0 KB + 287 KB WASM | 37.0 KB | 99.2 KB | 108.6 KB | 136.1 KB |

With a database fetch on every request both Rust apps are bound by parsing the API response and
measure about equal, around 3x the fastest JavaScript framework. The JavaScript figure was
re-measured on 2026-10-21; the other rows are from the full run.

## Documentation

| Guide | What it covers |
|---|---|
| [Getting started](docs/guide/getting-started.md) | the `corten` command and the parts of an app |
| [The app](docs/guide/app.md) | every default `App` puts around your routes, and how to replace each |
| [Views](docs/guide/views.md) | `html!`, components, typed links and messages |
| [Requests and forms](docs/guide/requests-and-forms.md) | request modes, patches, validation, redirects |
| [Islands](docs/guide/islands.md) | behavior in the browser: refs, actions, props, Solid |
| [Live data](docs/guide/live-data.md) | `<corten-live>`, feeds, running a feed across instances |
| [Cancellation](docs/guide/cancellation.md) | `AbortSignal` in the browser and on the server, deadlines |
| [Styles](docs/guide/styles.md) | a scoped CSS file per component, design tokens, themes |
| [Headless components](docs/guide/ui.md) | `corten-ui`: number field, combobox, tabs, menu button |
| [Type safety](docs/guide/type-safety.md) | every seam between server and client, and how it is checked |
| [Design decisions and limits](docs/design.md) | why it is built this way, and what it does not do |
| [Surfaces](docs/architecture.md) | one server for web, native apps and agents |

API reference: `cargo doc --open -p corten`. Building an app with an AI agent? The
[corten skill](skills/corten/SKILL.md) gives it the rules and recipes.

## Crates

| Crate | What it is |
|---|---|
| [`corten`](crates/corten) | the framework, on axum: `App`, `Req`, `html!`, islands, the browser runtime |
| [`corten-cli`](crates/corten-cli) | the `corten` command: `new`, `dev`, `check`, `build` |
| [`corten-macros`](crates/corten-macros) | the macros, on `proc_macro` alone (no `syn`) |
| [`corten-ui`](crates/corten-ui) | optional headless islands for widgets the browser lacks |
| [`corten-feed`](crates/corten-feed) | live feeds: rows, numbered patches, replay, resume |
| [`corten-cluster`](crates/corten-cluster) | one feed across instances: Postgres outbox, NATS JetStream |
| [`corten-protocol`](crates/corten-protocol) | the feed wire format and a client replica, for native and agent clients |

Examples: [hello](examples/hello) (every piece on one screen), [item-page](examples/item-page)
(every production module), [ui-gallery](examples/ui-gallery) (`corten-ui` styled), and
[cluster](examples/cluster) (failover across instances).

## Contributing

Issues and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) has the layout and the
checks; report security issues as [SECURITY.md](SECURITY.md) describes.

## License

[MIT](LICENSE).

## Name

Cor-ten is weathering steel: it is meant to rust, and the rust layer is what protects it. Rust
underneath, a thin protective surface on top. "COR-TEN" is a U.S. Steel trademark for steel.
