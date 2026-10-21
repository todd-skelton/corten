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
2. Acting (built): `corten::agent::Tools` registers each tool with a name, description, input
   JSON Schema written by hand, and a handler that calls the same rule the route calls. Auth is
   the app's: a handler returns `ToolError::Unauthorized` (HTTP 401), `Forbidden` or
   `Invalid` (tool results with `isError`). Deriving schemas from `Form` types (`schemars`)
   is still open.
3. Agent UIs: chat clients that render HTML UI from tools can embed Corten's server-rendered
   fragments and islands as they are, because they are plain HTML and standard custom elements.

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

## What a production marketplace app still needs

Measured against a production marketplace app. Everything else it needs (pages, forms with
patches, islands, live feeds across instances, CSP and CSRF, versioned assets, head tags, PWA,
i18n catalog, sitemap, health, graceful shutdown, agent tools) exists and is tested.

1. Realtime beyond one keyed feed: multi-topic subscriptions on one connection, entity and
   summary streams, and per-user filtering. Today an app opens one `Feed` per topic.
2. Shell navigation built from contributions with permissions and roles. Today the app's shell
   template renders its own navigation.
3. A design-token pipeline (one source, CSS now, Swift and Kotlin later).
4. Web push notifications (subscription storage, VAPID, service worker `push` handler).
5. Form validation helpers: field errors rendered next to inputs from one typed error value.
   Today an action returns `patch_with_status(422, ...)` with a fragment the app writes.
6. Observability beyond `tracing`: request metrics, trace propagation, an OpenTelemetry layer.
7. Number, money and date formatting for locales other than en-US.
8. Authentication and sessions are the app's (only cookie helpers are provided); auth across
   web, native and agents is an open decision below.

## Open decisions

1. Native navigation: Turbo Drive in shell pages versus a Turbo-compatible adapter in `corten.js`.
2. Auth across surfaces: session cookies for web and native shells, tokens for native screens
   and agents, one permission check in handlers.
3. Action schemas for agents: derive from the `Form` types (`schemars`) or declare them by hand.
