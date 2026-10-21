# Surfaces: one server for web, native and agents

Corten starts with the web, but nothing in it assumes the client is a browser. The server owns
state, rules and user-facing text. A surface is only a way of showing a screen and sending
actions back. Every surface speaks plain HTTP, JSON or HTML, and server-sent events, which
every platform has built in.

## Three contracts

| Contract | What it is | Web (today) | Native screen | Agent |
|---|---|---|---|---|
| Screen | a route whose page struct is its view model | HTML from the askama template | `Accept: application/json`, `{"title","data"}` from `req.screen` | the same JSON, as a read tool or resource |
| Action | a `POST` route with typed form input | `<form data-patch>`, patch or redirect | the same `POST`, JSON screen back | the same `POST`, as a tool call |
| Feed | keyed rows: snapshot plus numbered patches | `subscribe()` in `corten.js` | `corten_protocol::Replica` through UniFFI | `Replica`, or the raw SSE stream |

One handler answers every surface. It branches on `req.mode` only to choose the response shape,
never to change what the action does. A new surface therefore adds a renderer, not a second
backend.

## What exists now

1. `Mode::Data` and `Req::screen` (feature `json`): the same URL is HTML for browsers and JSON
   for clients that ask for `application/json`. Responses carry `Vary: corten, accept`, so caches
   keep the two apart. The runtime's own `corten` header always wins over `Accept`.
2. `corten-protocol`: the feed's wire types and `Replica`, the client half of the protocol in
   Rust. It depends only on `serde` and `serde_json`, so it builds for iOS, Android, WebAssembly
   and agent processes. `corten-feed` uses the same `check_seq` and `apply_ops` rules, so the
   server follower and every client apply patches the same way.
3. The example's `watch` handler serves a browser with and without JavaScript, and a data
   client, from one function. `examples/hello/src/app.rs` tests all three.
4. `corten::agent` (feature `agent`): actions as Model Context Protocol tools over HTTP POST.
   `examples/item-page` exposes `items.get` (a read tool returning the screen JSON) and
   `alerts.create` (the same rule the form uses), behind bearer tokens.
5. PWA: a manifest and a generated service worker (`corten::pwa`) make the web app installable
   with an offline fallback page; the item-page smoke test checks both in Chromium.

## Native apps (next, not built)

1. Web-like screens run in a native shell around the server-rendered pages (Hotwire Native or
   similar). Open decision: Hotwire Native drives pages through Turbo's JavaScript adapter.
   Either the mobile shell pages load Turbo Drive, or `corten.js` implements that adapter.
   Prototype both before choosing; it changes only the runtime, not apps.
2. Native screens (camera scanning, offline seller work) read `req.screen` JSON, post the same
   actions, and follow feeds with `Replica`. A `corten-native` crate would wrap `Replica`, an
   HTTP/SSE client and an offline outbox (SQLite on the device, an idempotency key per write)
   behind UniFFI, giving Swift and Kotlin one tested core.
3. Design tokens come from one source and generate CSS, Swift and Kotlin. Only the components
   native screens use are mirrored.

## Agents

1. Reading: an agent fetches any screen as JSON. Because rule 7 keeps every displayed value in
   the view model, the agent sees what a person sees, and nothing is scraped from HTML.
2. Acting (built): `corten::agent::Tools` registers each tool with a name, a description and a
   handler `(caller, Input) -> Result<Output, ToolError>`. The `inputSchema` and `outputSchema`
   agents see are generated from `Input` and `Output` (`schemars`), so they cannot drift from
   the code, and bad arguments are refused before the handler runs. `Tools::auth` checks every
   call once and hands the proven caller to the tool; `ToolError::Unauthorized` is HTTP 401,
   `Forbidden` and `Invalid` are tool results with `isError`.
3. Agent UIs: chat clients that render HTML UI from tools can embed Corten's server-rendered
   fragments and islands as they are, because they are plain HTML and standard custom elements.

## Types across seams

Every place two languages or two processes meet is checked automatically, in CI, from one Rust
definition:

| Seam | Source of truth | Checked by |
|---|---|---|
| Rust to template | page struct | askama, at compile time |
| Template to island props | the props struct (`corten::Props`) | `corten::types` writes `types/app.d.ts`; `Island<P>` makes `this.props()` a `P`; `tsc --checkJs` (strict) |
| Feed rows to island | the row struct | the same declarations; `subscribe<Row>` types the snapshot and every patch |
| Form fields to handler | the input struct | `#[derive(Fields)]`: templates print `Input::FIELDS.email` and errors use it, so a renamed field is a compile error |
| Agent arguments and results | the tool's input and output structs | schemas generated from them; arguments are deserialized before the handler runs |
| Screen JSON to native and agents | the page struct | `Types::json_schema` gives JSON Schema for code generators |

The declarations file is generated by a test (`Types::sync`) that rewrites it and fails when it
is stale, so a Rust change without the new file fails CI, and an island that no longer matches
fails `npm run check`. An island or subscription that names no type gets `unknown`, never
`any`, so nothing is read unchecked. `scripts/typecheck-fixture.js` proves each of these with
`@ts-expect-error` lines that must stay errors.

Not checked: `data-ref` names between template and island (the island throws on first use), i18n
keys (missing keys render visibly and `Catalog::missing_from` checks catalogs in a test) and
URLs in templates (see open decisions).

## Rules that keep surfaces from drifting

1. No business logic in a surface. Rules, validation, formatting and permissions live in Rust
   on the server.
2. A screen's page struct is its whole view model (README rule 7). Displayed text, prices
   included, is a field.
3. Actions are idempotent or carry an idempotency key, because native and agent clients retry
   (a 502 can hide a committed write).
4. One feed protocol. Clients never invent their own diffing. Changes go through
   `corten-protocol` and its tests.
5. A new surface ships with a test that drives the example through it, like `scripts/smoke.mjs`
   does for the web.

## What belongs in the framework

The test for each candidate feature: the framework takes it only when the framework creates the
problem (client navigation, generated files, its own transports) or is the only layer that can
see it. Anything that depends on the app's domain, data or vendors stays out, even if every app
needs one, because a framework version would be either partial or a wrapper that hides the real
library. Measured against a production marketplace app:

| Need | Owner | Why |
|---|---|---|
| Pages, patches, islands, feeds across instances, CSP and CSRF, assets, head tags, PWA, catalog, sitemap, health, shutdown, agent tools | Framework | Built and tested earlier. |
| Shell data on client navigation | Framework: `Shell::Context`, shell key, `data-nav` | Client navigation swaps only `<main>`, so only the framework can know the shell went stale. It reloads when the context or deploy changes and keeps `aria-current` right. |
| Which nav items show for which roles | App | Roles and permissions are the app's model; the shell template decides. |
| Field errors | Framework: `Errors`, `req.invalid`, focus on first invalid field | The 422 has to come out right in each request mode (patch, page, JSON) and as an agent error; that is transport the framework owns. |
| Types across the server/browser seam | Framework: `corten::types` (TypeScript from the Rust types), `Island<P>`, `subscribe<Row>`, `#[derive(Fields)]` | The framework defines the seams (props, feeds, form posts, tools), so it is the only layer that can type both ends. |
| Response compression | Framework: `corten::compress` | The framework serves the runtime and assets and knows they never change (compress once, best level) and that pages are generated per request (fastest level that pays). A generic layer re-compresses static files on every request at a level tuned for neither. |
| Validation rules and messages | App | Domain rules; messages come from the app's catalog. |
| Push notifications | Framework: `Pwa::import_script` only | The framework writes the service worker, so the app needs a way in. VAPID, encryption, subscription storage and sending are a library (`web-push`) plus the app's tables. |
| Feed diagnostics | Framework: `tracing` events, target `corten::feed` | Resyncs, limiter refusals and drains happen inside the feed. |
| Metrics export, trace propagation, OpenTelemetry | Platform | A `tracing` subscriber layer chosen per deployment. |
| Entity and summary streams | Framework: idle unload in `corten-cluster` | One feed per entity is the model; unloading unused feeds keeps memory bounded. |
| Several topics on one connection | Not built | HTTP/2 already shares one connection across streams. Multiplexing would add a second protocol and break per-stream resume and backpressure for a saving that only matters on HTTP/1.1. |
| Shared stream per URL across islands in one page | Not built | Tried and reverted: two islands on one URL is rare, and sharing couples their lifecycles and replay (a late joiner needs its own snapshot). Revisit if a page needs it. |
| Per-user filtering of live data | App | A per-user feed plus route authorization; the framework cannot know who may see a row. |
| Number, money, date formatting | App (ICU4X or similar) | Removed from the framework: it covered en-US only and would print wrong output for other locales without failing. |
| Design tokens | Build tooling | One source to CSS, Swift and Kotlin is a build step, not runtime. |
| Sessions and authentication | App | Only cookie helpers and the shell context hook are provided. |

## Open decisions

1. Native navigation: Turbo Drive in shell pages versus a Turbo-compatible adapter in `corten.js`.
2. Auth across surfaces: session cookies for web and native shells, tokens for native screens
   and agents, one permission check in handlers. Whatever is chosen, the app's middleware turns
   it into the `Shell::Context` that shells and handlers read.
3. Typed routes: URLs in templates are strings today. A route table that both axum and templates
   read would make a broken link a compile error; not built until a real app shows the need.
