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
4. **Plain functions, not a client framework, by default.** A handler is an exported function the
   markup calls with the elements it uses, run by one delegated listener per event type; there
   is no component instance, lifecycle class or hydration in the browser. Rows a script changes
   often are views: generated classes that read and write the server-rendered elements.
5. **Handlers are checked where they are called.** The compiler reads each script's JSDoc, so a
   call with a ref on the wrong element, an event of the wrong type, a value of the wrong type or
   a handler the script does not export fails to compile at the call. No generated declarations
   are needed between markup and script, and none can be stale.
6. **Plain CSS files, scoped by the compiler, not utility classes or CSS-in-Rust.** Utility
   frameworks put unchecked strings in every view and need a scanner and a build step; CSS in
   Rust needs inline styles the CSP blocks, or a runtime. A CSS file per component keeps the
   language designers know, and the macro makes its class names typed. Tokens cover the
   one-off values (spacing, direction per breakpoint) utilities are good at.
7. **A page is sent whole, not streamed in parts.** Streaming a slow section later in the same
   response sends the status and headers before that section exists, so a failure in it can no
   longer become an error page or a redirect; placing it out of order needs a script, so
   without JavaScript it lands at the end of the page; and with the data fetched together
   (`tokio::join!`) and rendering in microseconds, the most it saves is the slowest source's
   wait. A part that is slow and optional is a `<corten-live>` whose stream sends it when it is
   ready and ends.
8. **Static file names are checked when the shell renders, not when it compiles.** The static
   directory is chosen at run time (`App::assets`, `STATIC_DIR`), so a compile-time check would
   look in the wrong place for some apps, and an app names only a few files, all in its shell.
   `doc.asset("style.css")` panics on a missing file in debug builds, so `corten dev` and the
   page test `corten new` writes find a typo on the first render.

## Rules

1. HTML comes from the server, in `html!`. Scripts never build markup from strings; they move,
   show or hide server-rendered nodes. Extra rows ship in a `<template ref={rest}>`. (One
   exception: `ComboBox` mirrors the server's `<option>`s into its listbox, so each option is
   sent once.)
2. A fragment that can be patched is a component, rendered by the page and by the patch, so the
   two cannot drift.
3. A handler reaches the page only through its arguments: refs, `event`, `target`, `signal`,
   `scope` and values. State that outlives an event is the DOM itself or `state(el, init)`.
4. User-facing text lives in views or the view model, never in scripts.
5. Everything works without JavaScript: links are links and forms post.
6. A page another surface reads derives `Serialize`, and its fields are its whole view model
   (formatted text included). Then `req.screen` gives every surface the same data.

## Limits

1. Client navigation uses the Navigation API (Chrome, Edge, Firefox 147+, Safari 26.2+); older
   browsers do full page loads, and forms, patches and handlers still work.
2. Patches replace whole elements: the server's markup wins, except for what the user is doing.
   A field they edited keeps its value (for the form that was sent, edited after it was sent),
   and focus and the caret move to the matching element (same id; else, inside the nearest
   element with an id, the same name; focus also follows an element without either to the same
   place). A field without an id or a unique name takes the server's value. Other browser
   state inside a replaced element (an open `<details>`, scroll position) resets, so keep
   targets small.
3. `<script>` tags inside swapped pages do not run. Put behavior in a component's script.
4. Handlers run from one listener per event type on the document, so a third-party listener that
   stops an event before it reaches the document stops its handlers too, as in any delegating
   framework. Markup the runtime inserts (a navigation, a patch) waits for the modules its
   handlers name, so those handlers always run while their event is dispatched. On the first
   page, an event that arrives before its module (a click while the page still loads) gets the
   browser's default and no handler, as it would without JavaScript; the page loads its
   modules as module scripts that hand their exports to the runtime as they run, so their handlers are
   in before `DOMContentLoaded`, and this needs a person faster than the network.
5. Offline is a fallback page plus cached versioned files, not offline data.
6. Hover and touch prefetch reuses a fetched page for up to 10 seconds (cleared after any form
   post). It keeps the 8 newest and cancels the bodies of the rest, so hovering does not hold
   pages for the whole session; with the browser's data saver on it fetches nothing.
7. A feed's write rate is bounded by Postgres commit latency on its feed row. Split hot data into
   several feeds.
8. `html!` views render once and own what they capture; read from `req` before building the view.
   A value borrowed from one the view owns is computed inside the markup:
   `html! { let shown = saved.as_deref().unwrap_or(typed); <input value={shown}> }`.
9. Component stylesheets support plain CSS with nesting and `@media`, `@supports`, `@container`,
   `@layer`, `@keyframes` and `@font-face`; `@import` and escaped class names are not supported.
   Keyframe names are not scoped. Every listed component's CSS is in the one file, whether or not
   a page uses it: the item page's `components.css` (tokens, two themes, utilities and five
   components) is 3.2 KB, 822 bytes brotli (measured). A large library adds to every first load
   until the file is split per route, which is not built.
10. `corten-ui` is tested in Chromium only. Its popups are placed by the design system's CSS with
   anchor positioning (Baseline since January 2026); a browser without it shows them where
   `popover` puts them by default, centered. Tabs are horizontal.
    Its names in the page are prefixed (`ui.combo_box.js`, `data-scope="ui-combo-box"`) through
    `[package.metadata.corten] scripts = "ui"`, so an app's own components never collide with
    them. Two of an app's own scripts with one file name are a startup error that names both.
