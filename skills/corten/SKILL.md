---
name: corten
description: Build or change web apps on the Corten Rust framework (html! views, typed routes and forms, #[island] components with a script next to them, scoped component CSS, <corten-live> and feeds). Use when a project depends on the corten crate or the task mentions Corten.
---

# Corten

Corten renders every page on the server with `html!`. The browser gets HTML, a small runtime and
custom-element islands, and live data arrives as patches over server-sent events. One handler
answers a full page, a client navigation, a form patch and a JSON data request.

## Rules

These are what the framework is built around. Code that breaks them fights the framework.

1. **No strings at a seam.** URLs are `#[derive(Route)]` types, form field names come from
   `#[derive(Fields)]`, classes are constants from the component's CSS file, island refs and
   actions are `ref={Name}` and `on:event={Name}`, and user-facing text comes from
   `messages!`. If you are typing a path, field name, class or key as a string, stop and find
   the typed form.
2. **Markup is Rust.** Every element comes from `html!`. Island scripts move, show, hide or fill
   server-rendered nodes; they never build HTML from strings. Extra rows ship inside
   `<template ref={Rest}>`.
3. **A patched fragment is a component** with an `id`, rendered by the page and by the patch, so
   the two cannot drift.
4. **Everything works without JavaScript.** Links are links, and forms post and get the whole
   page back. `data-patch` and islands only make the page better.
5. **Cleanup is `ctx.signal`.** Pass it to `addEventListener`, `fetch`, `subscribe` and `mount`;
   `connect` returns nothing. On the server, spawned work watches `req.signal()`.
6. **Text lives in views or the view model**, never in island scripts. A value every surface
   shows (a formatted price) is a field the server fills.
7. **Prefer the platform.** Use `<dialog>`, `popover`, `<details>`, form validation attributes
   and CSS before writing an island.

## Decision table

| You need | Use | Recipe |
|---|---|---|
| a page or a link to one | a handler returning `req.page`, a `Route` type | [pages-and-routes.md](references/pages-and-routes.md) |
| text in more than one language | `messages!` and `req.t()` | [pages-and-routes.md](references/pages-and-routes.md) |
| a form that updates part of the page | `data-patch` plus `req.patch`, `req.invalid` for errors | [forms.md](references/forms.md) |
| behavior in the browser | an `#[island]` with a script next to it | [islands.md](references/islands.md) |
| a fragment the server keeps current | `<corten-live src={Route}>` plus `corten::live` | [islands.md](references/islands.md) |
| live rows an island updates | a `Feed` plus `subscribe` (feature `feed`) | [islands.md](references/islands.md) |
| a screen with heavy client state | Solid in an island (feature `solid`) | [islands.md](references/islands.md) |
| a number field, combobox, tabs or menu button | `corten-ui` first, the platform otherwise | [islands.md](references/islands.md) |
| styles for a component | `#[component(css)]`, a CSS file next to it, design tokens | [styles.md](references/styles.md) |
| JSON for a native client or an agent | `req.screen(title, &data, view)` | [pages-and-routes.md](references/pages-and-routes.md) |
| a test | `app().router().oneshot(..)` with the `corten` mode header | [testing.md](references/testing.md) |

Read only the recipe the task needs. For concepts, read the guides in the Corten repository's
`docs/guide/`; for exact signatures, read the crate's rustdoc.

## Finish

1. List every island and styled component once in `App::components` (an unlisted one is an
   error under `corten dev`).
2. Run `corten check`: `cargo fmt --check`, clippy with warnings denied, and the tests. Island
   scripts start with `// @ts-check`; if the app generates declarations (see testing), run
   `tsc --noEmit` or check them in the editor.
3. When behavior changed, load the page in a browser both with JavaScript and without it.
