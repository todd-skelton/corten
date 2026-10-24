# Corten

**Server-rendered web apps in Rust, with interactivity only where a page needs it.**

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Corten renders every page on the server with `html!`, a JSX-like Rust macro. The browser gets
HTML, a small runtime that swaps pages and patches forms, and *handlers*: plain JavaScript
functions in a script next to a component, which its Rust markup calls by name. Live data arrives as numbered patches over
server-sent events. One handler serves the browser, native apps and AI agents.

- **Fast on the server.** 18,043 rendered pages a second on one CPU, 5.3x ASP.NET Core, the
  fastest of thirteen other frameworks, and 1.1 MiB of memory after the first request
  (measured, [below](#performance)).
- **Light in the browser.** A 5.3 KB (brotli) runtime; the benchmark item page loads 6.9 KB of
  JavaScript (gzip, measured). No WASM, no virtual DOM, no hydration: a script takes over 1,000
  server-rendered rows in 6.0 ms, against 20-84 ms for Solid, Svelte, Vue, React and Remix
  (measured, [below](#client-heavy-screens)).
- **Typed end to end.** Routes, form fields, CSS classes, handler calls and the elements they
  take, events and messages are Rust types. TypeScript types for scripts are generated from Rust.
- **Works without JavaScript.** Links are links and forms post; scripts improve a page but never
  hold it up.
- **Production defaults.** A strict CSP, a cross-site write check, error pages, versioned
  assets, cache headers that never share one user's page with another, compression, health
  checks, graceful shutdown and an installable PWA, from `corten::serve(App::new(routes))`. Routes
  with no screen go under `.api("/api", routes)` and answer only JSON.
- **No JavaScript toolchain in the app.** One binary and a `static/` directory.

## Status

Corten is 0.1: usable and tested, on Rust 1.88 or later. A release publishes every crate at one
version on crates.io and attaches a prebuilt `corten` for Linux, macOS and Windows to the GitHub
release. The API will change before 1.0; [CHANGELOG.md](CHANGELOG.md) lists what changed.
The runtime and `corten-ui` are tested in Chromium, Firefox and WebKit.

## Quick start

```sh
cargo install corten-cli    # or cargo binstall corten-cli, for the prebuilt binary
corten new my-app
cd my-app
corten dev    # serves http://localhost:3000 and reloads on save
```

`corten new` writes a working app: a page, a form that patches part of the page, a
component with a script, a test and a Dockerfile. See [Getting started](docs/guide/getting-started.md).

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

    corten::serve(App::new(routes)).await
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

A misspelled route, field or prop does not compile. Behavior lives in a script next to the
component: plain functions the markup calls with the elements they use.

```rust,ignore
// src/click_counter.rs
#[component(js)]
pub fn ClickCounter() -> impl Render {
    let total = refs();
    html! {
        // A click calls count() in click_counter.js with the <output>.
        <button type="button" on:click={js::count(total)}>
            "Clicks: "
            <output ref={total}>"0"</output>
        </button>
    }
}
```

```js
// src/click_counter.js
/** @param {HTMLOutputElement} total */
export function count(total) {
  total.value = String(Number(total.value) + 1);
}
```

`js::count` exists because the script exports `count`, and it takes `total` because the JSDoc
says an `HTMLOutputElement` and the ref is on an `<output>`. Either mismatch is a compile error.

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
   handler calls, refs, class names, form fields and messages are all typed. TypeScript types are
   generated from Rust, never written by hand. If a guarantee exists, a test proves it fails to
   compile when broken.
5. **One way to do each thing.** Behavior is a function the markup calls, cleanup has one hook,
   and a request handler serves every mode. Fewer concepts make large codebases readable for
   people and for agents. The behavior sits next to its markup, and each call names everything
   the function touches.
6. **Small surface, deliberate dependencies.** A dependency or a new concept has to clearly earn
   its place. The framework owns what nearly every app needs, like security headers,
   cancellation, error pages and assets. Domain logic and things only some apps use belong to the
   app or platform.
7. **Good defaults, easy overrides.** `corten::serve(App::new(routes))` should be production-ready.
   Every default can be replaced in one line, and the common case stays short.
8. **One server, every surface.** The same handler and data serve the browser, native apps and AI
   agents. The server is the source of truth, and clients render it.
9. **Verified, not assumed.** Every runtime behavior has a browser smoke test. Reliability work,
   like failover and lost-patch counts, is benchmarked rather than argued.

## Performance

Measured on DigitalOcean c-8 droplets on 2026-10-23, one run each: the server table on a Xeon
Platinum 8168 (round r4), the browser tables on a Xeon Platinum 8280 (round browser-3). The
method, every table and the caveats are in [bench/fullstack](bench/fullstack/README.md) and
[bench/browser](bench/browser/README.md).

### On the server

The same item page (listings table, offers, a form that patches) and the same item as JSON in
fourteen frameworks, each with its usual production settings on one CPU and 1 GiB. Requests a
second are at 64 users; "failed" means the app could not serve the fixed rate (or start) without
tuning.

| | Pages/s, no database | JSON/s, no database | Pages/s, item API | Process start to first page | Memory after first request |
|---|---|---|---|---|---|
| **Corten** | **18,043** | **18,934** | **2,142** | **7.6 ms** | **1.074 MiB** |
| ASP.NET Core 10 | 3,385 | 11,715 | 915 | 860 ms | 30.82 MiB |
| Phoenix 1.8 | 1,290 | 1,839 | 289 | 1.6 s | 125.3 MiB |
| Leptos 0.8 | 1,282 | 15,952 | 631 | 8.8 ms | 1.48 MiB |
| Astro 7 | 1,097 | 1,802 | 418 | 409 ms | 36.85 MiB |
| Spring Boot 4.1 | 1,050 | 7,338 | 433 | 9.2 s | 134.6 MiB |
| SvelteKit 3 | 757 | 3,088 | 298 | 203 ms | 23.98 MiB |
| TanStack Start | 578 | 3,416 | 283 | 244 ms | 26.41 MiB |
| React Router 7 | 455 | 2,159 | 242 | 346 ms | 31.84 MiB |
| Rails 8.1 | 393 | 829 | 260 | 1.4 s | 63.91 MiB |
| Nuxt 4 | failed (266) | 6,998 | 181 | 348 ms | 35.29 MiB |
| Django 6.1 | failed (228) | 1,126 | 149 | 1.4 s | 62.12 MiB |
| Next.js 16 | failed (156) | 1,025 | failed (107) | 793 ms | 51.23 MiB |
| Laravel 13 | failed | failed | failed (73) | 990 ms | 96.8 MiB |

With the item API every request fetches and parses about 55 KB of JSON, which bounds every app.
Corten reads only the fields the page uses from that response, not a `serde_json::Value`, and
serves 2.3x ASP.NET Core's pages there (2,142 a second against 915) at a quarter of its CPU
([details](bench/fullstack/README.md#cortens-item-api-path)). Corten's JSON route costs about what
its HTML page does (133,000 against 124,000 user-mode instructions a request; Leptos's JSON
205,000). This droplet's CPU was slower than r3's, so every app's requests a second are lower than
in r3; instruction counts held. Pushing live updates
is measured in [bench/live](bench/live/README.md): at 10,000 open connections on one core, only
Corten and Leptos delivered all 100,000 messages a second.

### In the browser

The same item page, loaded cold in Chromium 148 from another site, median of 10 loads each. The
phone is the Lighthouse profile: 4x CPU slowdown, 150 ms round trip, 1.6 Mbps down.
"Interactive" is when the page's own code says it is ready (hydration finished, or handlers
connected); client navigation is the home page to the item page. Every app ran on one core with
items from the item API.

| | Phone: interactive | Phone: largest paint | Phone: client navigation | Desktop: interactive | Desktop: client navigation | JS heap | JavaScript, gzip |
|---|---|---|---|---|---|---|---|
| ASP.NET Core 10 | 428 ms | 400 ms | 240 ms | 42 ms | 15 ms | 1.5 MB | 17.0 KB |
| Django 6.1 | 435 ms | 396 ms | 239 ms | 43 ms | 19 ms | 1.5 MB | 17.0 KB |
| **Corten** | **441 ms** | **444 ms** | **191 ms** | **50 ms** | **12 ms** | **1.4 MB** | **6.9 KB** |
| Spring Boot 4.1 | 607 ms | 408 ms | 238 ms | 57 ms | 36 ms | 1.5 MB | 17.0 KB |
| Astro 7 | 615 ms | 256 ms | 242 ms | 48 ms | 17 ms | 1.4 MB | 9.2 KB |
| SvelteKit 3 | 833 ms | 472 ms | 267 ms | 88 ms | 20 ms | 2.1 MB | 37.0 KB |
| Laravel 13 | 873 ms | 416 ms | 212 ms | 67 ms | 22 ms | 2 MB | 84.3 KB |
| Phoenix 1.8 | 926 ms | 396 ms | 120 ms | 86 ms | 15 ms | 2.1 MB | 46.2 KB |
| React Router 7 | 1,033 ms | 424 ms | 257 ms | 146 ms | 29 ms | 2.6 MB | 98.6 KB |
| Rails 8.1 | 1,147 ms | 412 ms | 247 ms | 54 ms | 25 ms | 1.9 MB | 40.6 KB |
| Next.js 16 | 1,360 ms | 400 ms | 431 ms | 138 ms | 105 ms | 3.1 MB | 136.1 KB |
| Nuxt 4 | 1,469 ms | 424 ms | 275 ms | 87 ms | 24 ms | 2.7 MB | 73.7 KB |
| Leptos 0.8 | 1,819 ms | 408 ms | 239 ms | 91 ms | 23 ms | 1.7 MB | 7.0 KB + 287 KB WASM |
| TanStack Start | 2,289 ms | 408 ms | 336 ms | 105 ms | 26 ms | 2.9 MB | 110.9 KB |

On a phone, Corten is interactive with the apps that ship only htmx (ASP.NET Core, Django), in a
third of Next.js's time and a fifth of TanStack Start's. Its client navigation is the fastest
after Phoenix's, which reuses an open LiveView socket. Its largest paint (444 ms) is the slowest
of those three; Astro's is the fastest (256 ms).

### Client-heavy screens

A table of rows a script changes, in Chromium 148 on Linux. Each framework's app is written the
way its keyed [js-framework-benchmark](https://github.com/krausest/js-framework-benchmark) entry
is, built for production, and checked after every operation, so an app that skips work fails
instead of looking fast. Times run from the click to the DOM being updated (the frames that lay
it out and paint it are in the results): the median of 15, the slower of two rounds, or the
range when they disagree. Takeover runs from the HTML being parsed to the app marking itself
ready, for 1,000 rows the server rendered, less any frame the browser paints in between (the
median of 30 loads that take turns between the apps); every framework kept the server's rows.
Plain JavaScript is the floor: hand-written DOM code.

| | **Corten (views)** | Solid 1.9 | Svelte 5 | Vue 3.5 | React 19 | Remix 3 | Plain JS (floor) |
|---|---|---|---|---|---|---|---|
| Take over 1,000 server-rendered rows | **6.0 ms** | 19.6 ms | 25.6 ms | 28.3 ms | 60.1 ms | 83.5 ms | 1.4 ms |
| First click on a taken-over row | **1.8 ms** | 1.1 ms | 2.9 ms | 2.5 ms | 5.3 ms | 16.3 ms | 0.4 ms |
| Change 1 row of 10,000 | **0.2 ms** | 0.2 ms | 1.2 ms | 5.9 ms | 11.7 ms | 120 ms | 0.1 ms |
| Change 100 rows of 10,000 | **0.9 ms** | 1.0 ms | 1.8 ms | 6.8 ms | 26.4 ms | 112 ms | 0.6 ms |
| Update every 10th of 1,000 | **0.6 ms** | 0.9 ms | 1.0 ms | 1.6 ms | 2.0 ms | 10.0 ms | 0.4 ms |
| Select a row | **0.2 ms** | 0.3 ms | 1.2 ms | 0.7 ms | 1.1 ms | 8.5 ms | 0.1 ms |
| Swap two rows | **0.3 ms** | 1.0 ms | 1.0 ms | 1.0 ms | 10.1 ms | 8.6 ms | 0.2 ms |
| Create 1,000 rows | **6.4 ms** | 5.6 ms | 6.9 ms | 12.2 ms | 14.9 ms | 15.5 ms | 5.2 ms |
| Create 10,000 rows | **68.9 ms** | 59.2 ms | 81.9 ms | 127 ms | 351 ms | 172 ms | 48.9 ms |
| Dashboard: 200 prices tick | **0.6 ms** | 1.0 ms | 1.6 ms | 1.3 ms | 0.9 ms | 17.2 ms | 0.4 ms |
| Memory per row | **101 B** | 927 B | 1,229 B | 1,876 B | 2,036 B | 2,416 B | 165 B |
| JavaScript, gzip | **8.6 KB** | 6.8 KB | 20.6 KB | 26.2 KB | 68.4 KB | 28.4 KB | 1.1 KB |

Corten's rows are server-rendered `#[component(view)]` components, so taking them over is reading
the page rather than rebuilding it, and a row costs 101 bytes of JavaScript memory against
927-2,416 for the frameworks, which keep each row's data and bookkeeping in JavaScript. Creating
thousands of rows in the browser is where it trails Solid and plain JavaScript (68.9 ms against
59.2 and 48.9 ms for 10,000, down from 87.5 ms in the round before). What is left is mostly the
cost of keeping each row's key and refs in the page, which is what makes takeover and memory
small ([profile](bench/browser/README.md#creating-many-rows)).

## Documentation

| Guide | What it covers |
|---|---|
| [Getting started](docs/guide/getting-started.md) | the `corten` command and the parts of an app |
| [The app](docs/guide/app.md) | every default `App` puts around your routes, and how to replace each |
| [Views](docs/guide/views.md) | `html!`, components, typed links and messages |
| [Requests and forms](docs/guide/requests-and-forms.md) | request modes, patches, validation, redirects |
| [Scripts](docs/guide/scripts.md) | behavior in the browser: handlers, refs, typed events, views |
| [Live data](docs/guide/live-data.md) | `<corten-live>`, feeds, running a feed across instances |
| [Cancellation](docs/guide/cancellation.md) | `AbortSignal` in the browser and on the server, deadlines |
| [Styles](docs/guide/styles.md) | a scoped CSS file per component, design tokens, themes |
| [Headless components](docs/guide/ui.md) | `corten-ui`: number field, combobox, tabs, menu button, toaster |
| [Type safety](docs/guide/type-safety.md) | every seam between server and client, and how it is checked |
| [Design decisions and limits](docs/design.md) | why it is built this way, and what it does not do |
| [Surfaces](docs/architecture.md) | one server for web, native apps and agents |

API reference: `cargo doc --open -p corten`. Building an app with an AI agent? The
[corten skill](skills/corten/SKILL.md) gives it the rules and recipes.

## Crates

| Crate | What it is |
|---|---|
| [`corten`](crates/corten) | the framework, on axum: `App`, `Req`, `html!`, handlers, the browser runtime |
| [`corten-cli`](crates/corten-cli) | the `corten` command: `new`, `dev`, `check`, `build` |
| [`corten-macros`](crates/corten-macros) | the macros, on `proc_macro` alone (no `syn`) |
| [`corten-ui`](crates/corten-ui) | optional headless components for widgets the browser lacks |
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
