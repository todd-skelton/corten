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

## Agents (next, not built)

1. Reading: an agent fetches any screen as JSON. Because rule 7 keeps every displayed value in
   the view model, the agent sees what a person sees, and nothing is scraped from HTML.
2. Acting: actions are already typed `Form` structs. A `corten-agent` crate would list routes
   with their input types as tools (for example an MCP server) and call the same handlers, under
   the same auth and validation. Generating input schemas likely needs `schemars`; add it then,
   behind a feature.
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

## Open decisions

1. Native navigation: Turbo Drive in shell pages versus a Turbo-compatible adapter in `corten.js`.
2. Auth across surfaces: session cookies for web and native shells, tokens for native screens
   and agents, one permission check in handlers.
3. Action schemas for agents: derive from the `Form` types (`schemars`) or declare them by hand.
4. Multi-instance feeds: extract the Postgres outbox to NATS JetStream relay from the benchmark
   branch into `corten-cluster`, with a relay lease and idempotency keys.
