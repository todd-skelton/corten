# Changelog

Notable changes to Corten. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Corten is pre-1.0, so any minor version may change the API.

## Unreleased

### Added

1. `App::api(prefix, routes)`: routes with no screen (webhooks, public APIs, the MCP endpoint)
   answer every failure as JSON whatever the client accepts, including unknown URLs, wrong
   methods and the deadline, with no shell, nonce or context and a `default-src 'none'` CSP.
   `Api::allow_origin` lets one site's pages call them from the browser (CORS preflight and
   headers, writes without cookies let through for those routes only); `Api::context` runs
   `App::context` for them.
2. `Error::with_status`, for a 4xx with a message written for the client.
3. `Errors` is a response (feature `json`): 422 with the JSON error and its fields.
4. `Nonce` is an extractor, for a handler that writes HTML with an inline script itself.
5. Handlers replace islands. `#[component(js)]` reads the script next to the component and gives
   it a module `js` with one function per `export function`; the markup calls them,
   `on:click={js::confirm(event, dialog)}`, and each argument is checked against the handler's
   JSDoc when the crate compiles. `let (save, dialog) = refs();` names elements (typed by the tag
   each is put on), `event`, `target`, `signal` and `scope` are the browser's, and other
   arguments are JSON values. A component takes a handler as a prop (`on_remove: Handler`, set
   with `on:remove={..}`). `#[event]` declares a typed DOM event (`on:RowToggled={..}`, the class
   imported from `corten/events`), `script!` a module several components share, and `state(el,
   init)` (from `corten`) state kept with an element. The runtime runs every handler from one
   listener per event type on the document and runs `on:connect` when markup joins the page.
6. `examples/seller-desk`: a client-heavy screen (selection with ranges, filter, sort, inline
   edits, a confirm dialog, live stock levels) on handlers, with a browser test and timings.

### Removed

1. `#[island]`, `#[prop]`, `island()` and the custom element each island was. A component with a
   script renders its own root element, marked `data-scope`; `corten-ui`'s components are
   `#[component(js)]` with a `div` root. `Types::components` declares typed events instead of
   `CortenIslands`, and `corten::find_islands` is gone (the page preloads the modules its
   handlers named while it rendered).

### Changed

1. Every JSON failure has one shape, `{"error":{"status":404,"message":"Not Found"}}`. A data
   request's error now carries `message`, and an extractor's rejection is that JSON instead of
   plain text. `req.invalid` answers a data request with
   `{"error":{"status":422,"message":...,"fields":{...}}}` instead of `{"errors":{...}}`.
2. The CSP nonce is made only when a document asks for it; other responses get the same policy
   without a nonce source. `Policy::apply` no longer puts a `Nonce` in the request extensions:
   take `nonce: Nonce` as an extractor instead of `Extension<Nonce>`.
3. `App::context` no longer runs for static files, the runtime, the PWA files and the health
   checks.
4. `App` is one middleware instead of three (security, compression, errors). Measured with
   callgrind, a small JSON handler costs 27,584 instructions a request instead of 36,324, a
   small page 31,742 instead of 41,280, and a JSON 404 19,892 instead of 26,930
   (`docs/guide/app.md`, the request path).
5. `examples/item-page` mounts its MCP endpoint with `App::api`; its smoke test calls it from a
   page on another origin.

### Documentation

1. The README is now an overview: highlights, quick start, one compiled example, values,
   benchmarks and an index.
2. The detail moved to guides in `docs/guide/`, and the design choices, rules and limits to
   `docs/design.md`.
3. `cargo test` compiles the README's and the guides' Rust examples, so they cannot drift from
   the API.
4. New `CONTRIBUTING.md`, `SECURITY.md` and this changelog.
5. New agent skill in `skills/corten/`: rules, a decision table and recipes for common tasks. The
   `corten new` template's `AGENTS.md` points to it.

## 0.1.0 (not tagged; `main` as of 2026-10-21)

The first public version.

### Added

1. Server rendering with `html!`: components, typed routes and links, typed messages per locale,
   form field names from `#[derive(Fields)]`, and `compile_fail` tests for each typed seam.
2. One handler per route for four request modes: full documents, client navigation, form patches
   and JSON data screens.
3. `App` defaults: security headers with a CSP nonce, a same-origin write check, error pages,
   versioned static files, health checks, compression, PWA manifest and offline page, and live
   reload in dev.
4. Islands: `#[island]` components with `ref` and `on:event`, the script next to the component,
   typed props and TypeScript declarations generated from Rust types. Solid is opt-in.
5. Scoped component stylesheets with design tokens and themes.
6. Cancellation on web standards: an `AbortSignal` for every island and request, and a request
   signal and deadline on the server.
7. Live data: `<corten-live>` fragments, and feeds (a snapshot plus numbered patches over
   server-sent events) that clients resume after a disconnect.
8. `corten-cluster`: one feed across many instances through a Postgres outbox, a leased relay to
   NATS JetStream and per-instance followers.
9. `corten-ui`: headless number field, combobox, tabs and menu button.
10. `corten::agent`: an app's actions as Model Context Protocol tools.
11. The `corten` CLI: `new`, `dev`, `build` and `check`.
12. Benchmarks against Leptos, SvelteKit, React Router, TanStack Start and Next.js in
    `bench/fullstack`.
