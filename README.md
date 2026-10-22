# Corten

**Server-rendered web apps in Rust, with interactivity only where a page needs it.**

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Corten renders every page on the server with `html!`, a JSX-like Rust macro. The browser gets
HTML, a small runtime that swaps pages and patches forms, and *islands*: components whose
behavior is a short script next to their Rust markup. Live data arrives as numbered patches over
server-sent events. One handler serves the browser, native apps and AI agents.

- **Fast on the server.** 23,034 rendered pages a second on one CPU, 4.7x ASP.NET Core, the
  fastest of thirteen other frameworks, and 1.5 MiB of memory after the first request
  (measured, [below](#performance)).
- **Light in the browser.** A 3.3 KB (brotli) runtime; the benchmark item page loads 4.4 KB of
  JavaScript (gzip). No WASM, no virtual DOM, no hydration.
- **Typed end to end.** Routes, form fields, CSS classes, island refs and actions, and messages
  are Rust types. TypeScript types for islands are generated from Rust.
- **Works without JavaScript.** Links are links and forms post; scripts improve a page but never
  hold it up.
- **Production defaults.** A strict CSP, a cross-site write check, error pages, versioned
  assets, cache headers that never share one user's page with another, compression, health
  checks, graceful shutdown and an installable PWA, from `App::new(routes).serve()`. Routes
  with no screen go under `.api("/api", routes)` and answer only JSON.
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

The same item page (listings table, offers, a form that patches) and the same item as JSON in
fourteen frameworks, each with its usual production settings on one CPU and 1 GiB, measured on one
DigitalOcean c-8 droplet (Xeon Platinum 8280) on 2026-10-22, one run. Requests a second are at 64
users; "failed" means the app could not serve the fixed rate (or start) without tuning. Method,
every table and the caveats are in [bench/fullstack](bench/fullstack/README.md).

| | Pages/s, no database | JSON/s, no database | Pages/s, item API | Process start to first page | Memory after first request | JavaScript, gzip |
|---|---|---|---|---|---|---|
| **Corten** | **23,034** | **16,014** | **1,290** | **6.6 ms** | **1.477 MiB** | **4.4 KB** |
| ASP.NET Core 10 | 4,915 | 16,810 | 1,278 | 702 ms | 30.56 MiB | 17.0 KB |
| Leptos 0.8 | 1,790 | 22,847 | 915 | 7.4 ms | 1.473 MiB | 7.0 KB + 287 KB WASM |
| Phoenix 1.8 | 1,986 | 3,225 | 481 | 1.2 s | 126 MiB | 46.2 KB |
| Astro 7 | 1,441 | 2,227 | 492 | 311 ms | 36.78 MiB | 9.2 KB |
| Spring Boot 4.1 | 1,289 | 10,072 | 610 | 7.9 s | 144.4 MiB | 17.0 KB |
| SvelteKit 3 | 1,104 | 3,873 | 442 | 159 ms | 23.38 MiB | 37.0 KB |
| TanStack Start | 755 | 4,269 | 374 | 195 ms | 26.51 MiB | 110.9 KB |
| React Router 7 | 602 | 2,530 | 327 | 275 ms | 32.01 MiB | 98.6 KB |
| Rails 8.1 | 573 | 1,171 | 440 | 1.0 s | 63.87 MiB | 40.6 KB |
| Django 6.1 | 394 | 1,514 | 221 | 1.0 s | 63.31 MiB | 17.0 KB |
| Nuxt 4 | 343 | 8,561 | 224 | 294 ms | 35.28 MiB | 73.7 KB |
| Next.js 16 | failed (197) | 1,221 | failed (135) | 627 ms | 51.45 MiB | 136.1 KB |
| Laravel 13 | failed | failed | failed (125) | 757 ms | 95.41 MiB | 84.3 KB |

With the item API every request fetches and parses about 55 KB of JSON, which bounds every app;
Corten and ASP.NET Core are level there. Leptos serves JSON from memory faster than Corten
(22,847 against 16,014 a second); why Corten's JSON route costs more than its HTML page has not
been investigated. Pushing live updates is measured in [bench/live](bench/live/README.md): at
10,000 open connections on one core, only Corten and Leptos delivered all 100,000 messages a
second.

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
