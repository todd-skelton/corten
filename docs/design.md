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
8. **Static files are compiled into the program, a constant per file.** `embed!(pub struct
   Static = "static")` includes the directory when the crate compiles, so the binary is the
   whole deploy (no directory beside it to find or keep in step), and a page names a file by its
   constant, `req.asset(Static::STYLE_CSS)`: a typo or a removed file does not compile. A name
   that comes from data (an uploaded picture's) is `req.find_asset(name)`, an `Option`. The
   cost is that a file added with no change to the code is seen at the next build that compiles
   the crate, since a macro cannot ask cargo to watch a directory; `corten dev` reads the
   directory where the macro read it, so it serves the new file at once and restarts without a
   build when only those files changed (a removed file builds, so a page that names it does not
   compile), and the release Dockerfiles touch every source before
   building, so the crate compiles with every file. The PWA's icons and worker script are
   named by their constants too; an icon another site serves is a `Uri`.

9. **Work happens where it is fastest for the user.** The server renders the first page, so it
   arrives finished, and answers what only it can: a write, and data the page does not have. What
   the page can do with what it already has (sorting, filtering, selecting, a total, showing and
   hiding) is a handler, since no render is faster than no round trip; a view makes even 1,000
   rows fast to change ([README](../README.md#client-heavy-screens)). Rust renders on the server
   because it is the fastest renderer there, and plain JavaScript runs in the browser because no
   framework or WASM layer is faster than the platform's own DOM calls. The scripts guide has the
   table for choosing ([Server or browser](guide/scripts.md#server-or-browser)).
10. **The document is a component the app owns, not a shell Corten configures.** A page hands
   `req.page` its whole document, the app's layout with the page in it, and the head is markup
   like the rest (`<title>`, the page's tags, `<Corten />` where Corten adds its own), so there is
   no metadata builder, no shell hook and no second way to change the document. The default
   layout and error page live in the `corten new` template, where the app can read and change
   them, not in the crate. `html!` splits a document into the parts each request needs (the
   title for a data client; the title, head tags and `<main>` for a navigation), so a layout
   costs a navigation nothing: on the item page a full document got about 7% faster than with
   the shell and a navigation stayed within noise (`render_cost` medians of alternating runs,
   same bytes).
11. **A request says what it is with a `corten` header, not `Accept` or `Sec-Fetch-*`.** A
   navigation and an action both answer HTML, so no registered media type tells them apart, and
   `Accept` already chooses the format: `application/json` asks for a page's data
   (`Mode::Data`). Carrying the mode there too would need a media type parameter of Corten's own
   (`text/html;corten=nav`), as private as a header but mixed into the format choice that data
   clients, caches and proxies read as content negotiation. `Sec-Fetch-Mode` and `Sec-Fetch-Dest`
   tell a navigation from a `fetch` but not a navigation from an action, a script cannot set
   them, and data clients do not send them. With a header of its own, its absence is the safe
   answer: a browser request without it (and not asking for JSON) gets the whole document, as a
   browser without JavaScript needs. The mode would move to a standard header that says "part of
   a page" and that a script may set.
12. **The runtime prefetches links itself, not with Speculation Rules or `rel="prefetch"`.** A
   navigation the runtime makes asks with `corten: nav` and gets the title, head tags and
   `<main>`, a fraction of the document. A Speculation Rules prefetch fetches the whole document
   into a cache the browser uses only for a document navigation, not for the runtime's `fetch`. A
   `rel="prefetch"` response goes into the HTTP cache, but it is the document, sent without
   `corten: nav`, and answers vary on that header (`Vary: corten, accept`), so the runtime's
   request does not match it. Either would download more and still leave the runtime's
   navigation to fetch again. Speculation Rules would fit where the browser navigates anyway (full
   page loads, browsers without the Navigation API), and the runtime would drop its own once a
   prefetch can be asked with the runtime's headers and answer its `fetch`.
13. **Component CSS is scoped by prefixing its classes, not with `@scope`.** A prefixed class
   (`.offer-list__row`) matches nothing an app's global stylesheet or another component wrote, so
   a component looks the same on every page; with `@scope`, the class stays `.row` and any
   global `.row` rule still applies to it. A donut limit at nested components would also need every
   component's root marked (`data-scope` marks only those with refs, kept state or a handler
   taking `scope`), and a scoped selector competes on specificity and proximity where a prefixed
   class is one class. The typed class constants do not depend on the prefix. The macro would
   emit `@scope` when it gives the same isolation from global rules, at the same specificity.
14. **A feed patch is keyed `upsert` and `remove` of whole rows, not JSON Patch or Merge Patch.**
   A feed is a set of rows by id, and each write replaces rows or removes them. JSON Patch
   (RFC 6902) could say the same over an object keyed by id (`add` at `/<id>` replaces a row,
   `remove` removes it), but only as a subset: a client given JSON Patch is expected to take any
   path, `test`, `move` and `copy`, and ids would need JSON Pointer escaping (`~0`, `~1`), so a
   subscriber either carries a whole JSON Patch engine for two operations or rejects valid JSON
   Patch. Merge Patch (RFC 7396) merges a value into the old one field by field, so a row cannot
   be replaced whole, and its `null` means "delete", so a field cannot be null. Keyed whole rows
   are one encoding every subscriber applies the same way, in one pass, and `corten-protocol`
   holds it so native and agent clients do not write their own diffing
   ([architecture](architecture.md#rules-that-keep-surfaces-from-drifting), Rules 5). A feed would
   move to JSON Patch if subscribers needed to apply it with a stock JSON Patch library: each
   change as `add` or `remove` at `/<id>`, with the same sequence numbers and resume.

## Rules

1. Markup is written in `html!`, never built from strings in a script. A script makes new
   elements by cloning markup the server rendered (a `<template ref={rest}>`, or a view's rows,
   `rows(..).push(items)`, filled through typed holes), and moves, shows, hides or writes the
   ones on the page. (One exception: `ComboBox` mirrors the server's `<option>`s into its
   listbox, so each option is sent once.)
2. A fragment that can be patched is a component, rendered by the page and by the patch, so the
   two cannot drift. It is declared `-> impl Patch`, and its markup's top-level elements each have
   a `#[derive(Id)]` id, which the compiler checks (`view::Patch`), so no patch loses an element in
   the browser.
3. A handler reaches the page only through its arguments: refs, `event`, `target`, `signal`,
   `scope` and values. State that outlives an event is the DOM itself or `keep(init)`, the
   running handler's component's own. (Two exceptions: `CopyButton` copies an element outside
   its component, which it is given as a typed `Id` value and finds by that id, since refs do
   not reach outside a component; and `RelativeTime` keeps the page's relative times in its
   module, so one timer serves them all, with a `visibilitychange` and a `pageshow` listener for
   the life of the page that read the clock again when it comes back.)
4. User-facing text lives in views, the view model or the catalog, never in scripts. Which of
   the first and the last is one rule per app (`docs/guide/views.md`, Text): an app with one
   locale writes fixed text as literals in its views, and the catalog holds only what needs it (a
   count, text with arguments, text a script fills); an app with more than one locale writes all of it through the
   catalog. Text that
   depends on what a script computed ("3 selected") is a catalog message the markup passes
   unfilled (`t.js().rows_selected()`) and the script fills in (`label({ count })`): the
   browser formats it with no round trip, and the compiler checks its arguments against the
   handler's `Message<"count">` (`docs/guide/scripts.md`, Text a script computes).
5. Pages and actions work without JavaScript: links are links and forms post. A feature only a
   browser can provide (drag and drop, a canvas) needs JavaScript; one that makes a server action
   instant (filtering the rows on the page as you type) keeps the form that does it without.
6. A page another surface reads derives `Serialize`, and its fields are its whole view model
   (formatted text included). Then `req.screen` gives every surface the same data.
7. `App` configures the whole app; a part of it under a URL prefix is an `Area` (or an `Api`
   for routes that answer only JSON), which owns its routes. Its prefix and its routes are route
   types (`#[corten::routes(area = Admin)]` or `(api = ApiRoot)` on the module of its routes,
   `.area(admin::area)`), so a route outside the prefix does not compile. A new setting that applies to a prefix goes on `Area` or `Api`, never on
   `App` as a method taking a prefix string. Who is asking is not a setting: a handler takes
   `Caller<T>`, and an area's routes module says `caller = T`, so a handler there that does not
   take it does not compile. Before adding to `App`, check whether the
   setting belongs on one of those, and whether the builder still reads as one list of settings
   with one way to give each; reorganize it when it does not, rather than add a second way.
8. A mistake the compiler can see is a compile error, with a `compile_fail` doctest: a type, a
   trait bound, or a `const` assertion over a route's `PATH`. One it cannot see (two settings
   that clash, a `var(--x)` no stylesheet on its pages declares, a PWA that names a static file in
   an app given no static directory)
   fails at startup, and only what depends on the
   request is checked per request. A check that moves earlier also leaves the request path.
9. A layout's props are what most pages set: the title and the page's own head tags. A page that
   needs a different document (a print view, an embed) uses another layout component; a layout
   does not grow a flag per page.
10. A header, wire format or file format that leaves Corten (read by another client, another
    tool, or a person writing the file) follows the standard for it where one exists: Problem
    Details for JSON failures, CLDR for number and date formats. Where Corten defines its own
    instead, a Choice above names the standard it considered, why it does not fit, and what
    would let Corten move to it (Choices 11 to 14). A format only Corten's own
    server and runtime read (the `data-on` wire format) needs no standard, but stays internal:
    nothing outside the framework may depend on it.

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
   Keyframe names are not scoped, so an area's keyframes share one namespace with the rest.
   Every component listed in `App::components` is in one file, whether or not a page uses it: the
   item page's `components.css` (tokens, two themes, utilities and six components) is 3,223 bytes,
   1,047 brotli (measured). `Area::components` moves the components only some pages render into a file
   linked from those pages; the split is by URL prefix, chosen by the app, not detected from what
   a page renders, which would cost every render.
10. `corten-ui` is tested in Chromium, Firefox and WebKit (Playwright's builds). Its popups are placed by the design system's CSS with
   anchor positioning (Baseline since January 2026); a browser without it shows them where
   `popover` puts them by default, centered. Tabs are horizontal.
    Its names in the page are prefixed (`ui.combo_box.js`, `data-scope="ui-combo-box"`) through
    `[package.metadata.corten] scripts = "ui"`, so an app's own components never collide with
    them. Two of an app's own scripts with one file name are a startup error that names both.
11. Who may use a route is in its handler's type: it takes `Caller<T>`, whose `Identify` finds
    the caller or refuses (sign in, 401, 403, 503), and a routes module's `caller = T` holds
    every handler in it to that, checked at compile time, so a route added later is covered.
    The lookup runs only when a handler or middleware reads it, once per request, and reading
    it makes the answer private. A route's answer type says how it fails (`HtmlResult`: the
    error page; `JsonResult<T>`: JSON) and whether a shared cache may keep it (`Public<..>`,
    whose handler may only take `Anonymous` inputs and whose page cannot set a cookie). A check
    that depends on the record (the author, the organization) stays in its handler. That an
    area's or an API's routes are under its prefix is a `const` assertion, which the compiler
    evaluates when it builds the app (`cargo build`, `cargo test`, `corten check`), not under
    `cargo check` alone. Every route reaches the app through these checks: `App::new` takes the
    `Routes` a
    `#[corten::routes]` module makes, and `Area::route` and `Api::route` take `Methods`, made
    by Corten's `get`, `post` and the others, which check the same things. A plain axum
    `Router` or method router does not fit any of them, so there is no unchecked way in. A
    WebSocket (`Upgrade`) and another server's answer passed on (`Relay`) are such routes too.
12. A request goes through fixed stages in one order (`corten::middleware`); the app's own
    steps are a `Middleware` tuple at one place in it, resolved at compile time, not hooks at
    several. An area's or an API's own steps (`Area::middleware`, `Api::middleware`) run right
    after the app's, for the requests at its prefix; they are boxed, since which run depends on
    the path, and an app with none pays nothing for them.
13. A connection switches protocols (a WebSocket, a relayed tunnel) only over HTTP/1.1 served by
    `corten::serve`; HTTP/2's extended `CONNECT` is not supported. Corten reads the handshake
    and hands over the bytes: frames, message limits, buffering and per-message checks are the
    WebSocket library's and the route's. The work on a switched connection is told when shutdown
    begins and is not waited for.
14. Connections are HTTP/1.1. A request's head must arrive within `App::header_timeout` (30
    seconds), which also closes an idle keep-alive connection; how many may be open is
    `App::max_connections`, unbounded unless set. HTTP/2 and HTTP/3 to browsers come from a proxy
    or CDN in front of `corten::serve`. An app with live streams needs one: over HTTP/1.1 a
    browser opens about six connections to a host, each open stream holds one, and requests queue
    behind them once a page with a few `<corten-live>` elements or feeds is open in a few tabs.
