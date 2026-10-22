# Design decisions and limits

[Back to the README](../README.md)

The [values](../README.md#values) decide what goes in. This file records how they turned into the
framework's shape, and what it does not do yet.

## Choices

1. **Markup is a Rust macro, not TSX.** Rendering TSX needs a JavaScript runtime on the server
   or a second toolchain in the app; `html!` keeps one language and compiles to the same string
   writes a hand-written renderer would do. Measured on the item page, `html!` renders as fast
   as the compiled templates it replaced (askama).
2. **No WASM.** Hydrating a WASM app costs hundreds of kilobytes before the page responds; the
   server already has the HTML, so the browser only needs behavior where there is behavior.
3. **No virtual DOM on the server.** Frameworks that build a view tree per request and then
   serialize it pay for it in throughput (Leptos in the
   [README benchmark](../README.md#performance)). `html!` writes straight to the
   response buffer.
4. **Custom elements, not a client framework, by default.** Islands use the platform; Solid is
   opt-in for the screens that need fine-grained reactive state.
5. **Plain CSS files, scoped by the compiler, not utility classes or CSS-in-Rust.** Utility
   frameworks put unchecked strings in every view and need a scanner and a build step; CSS in
   Rust needs inline styles the CSP blocks, or a runtime. A CSS file per component keeps the
   language designers know, and the macro makes its class names typed. Tokens cover the
   one-off values (spacing, direction per breakpoint) utilities are good at.

## Rules

1. HTML comes from the server, in `html!`. Islands never build markup from strings; they move,
   show or hide server-rendered nodes. Extra rows ship in a `<template ref={Rest}>`. (One
   exception: `ComboBox` mirrors the server's `<option>`s into its listbox, so each option is
   sent once.)
2. A fragment that can be patched is a component, rendered by the page and by the patch, so the
   two cannot drift.
3. An island's script reaches the page only through the names its markup gives it: `refs`,
   `all` and actions run by `on:event`. Per-element state is `state()` or the DOM itself.
4. User-facing text lives in views or the view model, never in island code.
5. Everything works without JavaScript: links are links and forms post.
6. A page another surface reads derives `Serialize`, and its fields are its whole view model
   (formatted text included). Then `req.screen` gives every surface the same data.

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
8. Component stylesheets support plain CSS with nesting and `@media`, `@supports`, `@container`,
   `@layer`, `@keyframes` and `@font-face`; `@import` and escaped class names are not supported.
   Keyframe names are not scoped. Every listed component's CSS is in the one file, whether or not
   a page uses it: the item page's `components.css` (tokens, two themes, utilities and five
   components) is 3.2 KB, 822 bytes brotli (measured). A large library adds to every first load
   until the file is split per route, which is not built.
9. `corten-ui` is tested in Chromium only. Its popups are placed by the design system's CSS with
   anchor positioning (Baseline since January 2026); a browser without it shows them where
   `popover` puts them by default, centered. Tabs are horizontal.
   Its tags are not prefixed (`<combo-box>`, `<tab-group>`), so an app island with the same
   name is a startup error.
