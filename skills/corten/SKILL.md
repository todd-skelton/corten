---
name: corten
description: Build or change web apps on the Corten Rust framework (html! views, typed routes and forms, #[component(js)] handlers in a script next to the component, scoped component CSS, <corten-live> and feeds). Use when a project depends on the corten crate or the task mentions Corten.
---

# Corten

Corten is built for speed at the user's end. Rust renders the first page on the server with
`html!`; in the browser, the handler scripts the page names do everything the page can do with
what it already has, with no request. The server answers writes and data the page lacks, and
live data arrives as patches over server-sent events. One request handler answers a full page,
a client navigation, a form patch and a JSON data request.

## Rules

These are what the framework is built around. Code that breaks them fights the framework.

1. **No strings at a seam.** URLs are `#[derive(Route)]` types, form field names come from
   `#[derive(Fields)]`, classes are constants from the component's CSS file, refs come from
   `let (a, b) = refs();`, handlers are `on:event={js::name(..)}` calls checked against the
   script's JSDoc, and user-facing text comes from `messages!`. If you are typing a path, field name, class or key as a string, stop and find
   the typed form.
2. **Markup is Rust.** Every element comes from `html!`. Scripts never build HTML from strings:
   they clone markup the server rendered (`<template ref={rest}>`, a view's `rows(..).push(..)`)
   and move, show, hide or fill what is on the page.
3. **A patched fragment is a component** with an `id`, rendered by the page and by the patch, so
   the two cannot drift.
4. **Work goes where it is fastest.** Sorting, filtering, selecting, totals, showing and hiding
   what the page already has are handlers, not round trips. Writes and data the page does not
   have go to the server. Do not default to a server patch for something the browser can do.
5. **Pages and actions work without JavaScript.** Links are links, and forms post and get the
   whole page back. A handler that makes a server action instant keeps the form that does it
   without; a feature only a browser can provide (drag and drop, a canvas) needs JavaScript.
6. **Cleanup is `signal`.** A handler that takes `signal` gets an `AbortSignal` that aborts
   when its element leaves the page; pass it to `addEventListener`, `fetch`, `subscribe` and
   `mount`. On the server, spawned work watches `req.signal()`.
7. **Text lives in views or the view model**, never in scripts. A value every surface
   shows (a formatted price) is a field the server fills.
8. **Prefer the platform.** Use `<dialog>`, `popover`, `<details>`, form validation attributes
   and CSS before writing a handler.

## Decision table

| You need | Use | Recipe |
|---|---|---|
| a page or a link to one | a route in the `#[corten::routes]` module, its handler returning `req.page(<Layout ..>)` | [pages-and-routes.md](references/pages-and-routes.md) |
| text in more than one language | `messages!` and `req.t()` | [pages-and-routes.md](references/pages-and-routes.md) |
| to sort, filter, select or total what the page already shows | a handler, with `#[component(view)]` rows when there are many; no request | [scripts.md](references/scripts.md) |
| a form with a file input | `Upload<T>` with a `Files` field (`next_file`, `bytes_up_to` or `chunk`, then `fields`), `upload.limit(..)` for the body; never `Form` or a raw body | [forms.md](references/forms.md) |
| a write that updates part of the page | `data-patch` plus `req.patch_or(view, page)`, `req.invalid` for errors | [forms.md](references/forms.md) |
| behavior in the browser | `#[component(js)]`, exported functions in a script next to it | [scripts.md](references/scripts.md) |
| a child that reports to its parent | a `Handler` prop, or an `#[event]` | [scripts.md](references/scripts.md) |
| a fragment the server keeps current | `<corten-live src={Route}>` plus `corten::live` | [scripts.md](references/scripts.md) |
| live rows a script updates | a `Feed` plus `subscribe` in an `on:connect` handler (feature `feed`) | [scripts.md](references/scripts.md) |
| many rows a script reads or changes (a grid) | `#[component(view)]` rows, `rows(body, Row)` in the handler | [scripts.md](references/scripts.md) |
| a number field, combobox, tabs, menu button or toast | `corten-ui` first, the platform otherwise | [scripts.md](references/scripts.md) |
| styles for a component | `#[component(css)]`, a CSS file next to it, design tokens | [styles.md](references/styles.md) |
| JSON for a native client or an agent | `req.screen(&data, doc)` | [pages-and-routes.md](references/pages-and-routes.md) |
| a route with no screen (webhook, public API, service call) | handlers return `JsonResult<T>` (every failure is JSON), mounted under `App::api` when they share a prefix | [pages-and-routes.md](references/pages-and-routes.md) |
| routes that need sign-in or a permission | the handler takes `Caller<Viewer>` (`Viewer: Identify`), and an area's routes module says `caller = Viewer` so none can forget it; `Option<Caller<Viewer>>` for a page that shows something either way | [pages-and-routes.md](references/pages-and-routes.md) |
| data from another service's JSON | structs of only the fields you read (`#[derive(Deserialize, FromJson)]`, read with `corten::json::from_slice`), not `serde_json::Value` | [pages-and-routes.md](references/pages-and-routes.md) |
| a page or JSON a CDN may share | return `Public<HtmlResult>` (`Public::new(max_age, page)`); its handler cannot take a `Caller` and its page cannot set a cookie | [pages-and-routes.md](references/pages-and-routes.md) |
| a test | `app().router().oneshot(..)` with the `corten` mode header | [testing.md](references/testing.md) |
| deploying behind a domain | set `HOSTS=shop.example` (any other `Host` but `localhost` and IP addresses gets 421; never `HOSTS=*` to silence it) | the app guide, Host names |
| the visitor's IP address (an API's per-client limit, a rate limit) | the handler takes `ClientIp`; behind a proxy set `PROXIES` to the proxy's addresses (never read `X-Forwarded-For` yourself) | the app guide, The client's address |
| routes slower than pages (a slow upstream, a report, uploads) | `.deadline(Duration)` on their `Area` or `Api` (`None` for none), never raising `App::deadline` for every page | the cancellation guide |
| HTTPS on the app itself | `corten::serve_wrapped(corten::listeners(3000).await?, move \|tcp\| acceptor.accept(tcp), app)` (the TLS library and its config are the app's; ALPN `http/1.1`), never a listener that runs the handshake itself, which is outside `max_connections` and `header_timeout` | the app guide, Your own listeners |
| A Unix socket, a socket handed over | `corten::serve_on(listeners, app)` (`listeners(port)` alone for another default port), never `axum::serve`, which loses the shutdown | the app guide, Your own listeners |
| work on a timer or a child process, outside requests | `App::task("name", \|signal\| ..)`: return when `signal.aborted()` resolves (shutdown), within `App::shutdown_grace` | the app guide, Tasks |

Read only the recipe the task needs. For concepts, read the guides in the Corten repository's
`docs/guide/`; for exact signatures, read the crate's rustdoc.

## Finish

1. List every component with a script or styles, and every `#[event]`, once in `App::components` (an unlisted one is an
   error under `corten dev`).
2. Run `corten fmt`, then `corten check`: `cargo fmt --check`, `corten fmt --check` (the markup
   in `html!`), clippy with warnings denied, `tsc` over the scripts in `src/` and `static/`, and the
   tests. The compiler checks each `js::` call against the script's JSDoc; `tsc` checks the
   function bodies (install TypeScript; without it `corten check` prints that they were not
   checked). At a workspace root it checks each member that depends on `corten`.
3. When behavior changed, load the page in a browser both with JavaScript and without it.
