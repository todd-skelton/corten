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
5. `Cache` (`corten::cache`, in the prelude): a handler returns `(Cache::public(max_age), response)`
   for a page or JSON that is the same for every visitor, with `.stale_while_revalidate(..)`;
   `Cache::no_store()` keeps a response out of every cache. `App` sends a public response as
   `private, no-cache` when the request has a `Cookie` or `Authorization`, the context found
   someone, the response sets a cookie or the HTML has the request's nonce, and as `private` with
   the same lifetime for a client navigation, a form answer or data from a page route.
   `examples/item-page` marks its home and category pages public; its smoke test checks the
   headers and that a page is restored from the browser's back/forward cache (in Playwright's
   full Chromium; its default headless shell never restores a page).
6. `examples/item-page` serves the item page's data as JSON at `/api/items/{id}` under
   `App::api`, the same view model the page renders, shared with the cache by `Arc`.
7. Handlers replace islands. `#[component(js)]` reads the script next to the component and gives
   it a module `js` with one function per `export function`; the markup calls them,
   `on:click={js::confirm(event, dialog)}`, and each argument is checked against the handler's
   JSDoc when the crate compiles. `let (save, dialog) = refs();` names elements (typed by the tag
   each is put on), `event`, `target`, `signal` and `scope` are the browser's, and other
   arguments are JSON values. A component takes a handler as a prop (`on_remove: Handler`, set
   with `on:remove={..}`). `#[event]` declares a typed DOM event (`on:RowToggled={..}`, the class
   imported from `corten/events`), `script!` a module several components share, and `state(el,
   init)` (from `corten`) state kept with an element. The runtime runs every handler from one
   listener per event type on the document and runs `on:connect` when markup joins the page.
8. `examples/seller-desk`: a client-heavy screen (selection with ranges, filter, sort, inline
   edits, a confirm dialog, live stock levels) on handlers, with a browser test and timings.
9. For lists and large components: `ref.closest()` (the element with the ref around the event's
   target, so one handler on a container serves every row), groups (a `@typedef` of elements in
   the script becomes a struct `js::Name { .. }` passed as one argument), keyed refs
   (`key={item.id}` and a `Map<string, HTMLElement>` parameter), and handlers take up to 12
   arguments. `corten::script::el` is in the prelude.
10. A handler parameter typed with a class imported from `"corten/events"` is checked against the
   `#[event]` type when the crate compiles.
11. `corten::view::Remove(id)`: a patch that removes an element.
12. `[package.metadata.corten] scripts = "prefix"`: a library's scripts and component roots are
    named `prefix.name` and `prefix-name`. `corten-ui` uses `ui`.
13. `#[derive(Id)]` (in the prelude): an element id as a type, `struct RowId<'a>(&'a str)` is
    `row-{key}`, so the markup and the patch that replaces or removes the element cannot drift.
    `Remove` takes one. Two ids with the same name in one program fail to build (two versions
    of one library excepted).
14. `corten fmt [--check] [path ..]` lays out the markup inside `html!` (rustfmt leaves macro
    bodies alone) at rustfmt's `max_width`. It never joins lines the author split and only changes
    whitespace: a body whose tokens would change is left as written, with a warning.
    `corten check` runs `corten fmt --check`.
15. `corten::Json` (feature `json`, in the prelude): axum's `Json` extractor with the same
    rejections, and a response written by `serde_json::to_vec` into a `Vec` the body keeps.
    axum's `Json` writes through a `BytesMut` writer that costs more for every small write. On a
    bench droplet (Xeon Platinum 8280, app on one core, 64 users, two rounds), the item's 8.8 KB
    of JSON under `App::api` served 24,330-25,000 requests a second instead of 18,608-19,168
    (Leptos, which uses axum's `Json`: 20,008-20,377), and took 121,882 user instructions a
    request in process instead of 193,668. `examples/item-page` uses it for `/api/items/{id}`.
16. `#[component(view)]`: the component gets a class in `corten/views` whose properties are its
    holes, a parameter that is an element's whole text or an attribute's whole value.
    `row.label = "x"` writes that text, `row.label` reads it back, an `<input>`'s `value` and
    `checked` are its live properties, a root `key={id}` keys it, and `corten::types` declares it
    so `tsc` checks scripts against the Rust parameters. Debug builds stop a number property past
    2^53. Handlers use views through `rows(parent, View, template)` from the same module: one list
    per parent element that follows changes made elsewhere. Corten serves `rows()` and the classes as one module, only to apps with views.
17. `Holds<Row, HTMLTableSectionElement>` in `corten/views` types a handler parameter that holds
    a view's rows; `rows(body, Row)` takes only that, so `tsc` rejects the wrong view or a plain
    element, and the Rust compiler rejects a ref whose element the markup does not fill with `Row`s
    only. `Ref` gained a second type parameter for it (`RowsOf<{ view_id("Row") }>`, defaulting to
    `NoRows`). `rows()` gained `get(key)` and `indexOf(view)`. A view whose key the markup also
    shows outside its holes has no `keyOf`, and its `rows()` cannot `push` or `replace`: the
    server makes those rows. `examples/seller-desk`'s grid rows are an `InventoryRow` view.

18. Releases: every crate in `crates/` can be published to crates.io at the one version in
    `[workspace.package]`, with license, repository, keywords, categories and `rust-version`
    (1.88, checked by CI and the checks in `CONTRIBUTING.md`). Pushing a tag `vX.Y.Z` runs
    `.github/workflows/release.yml`: the checks, a static `corten` binary for Linux (x86_64,
    aarch64), macOS (x86_64, aarch64) and Windows, `cargo publish --workspace`, and a GitHub
    release with the binaries and the changelog section. `cargo binstall corten-cli` downloads
    the binary. `CONTRIBUTING.md` (Releasing) has the steps, including the first release.
19. `.github/workflows/ci.yml` runs the checks in `CONTRIBUTING.md` on every pull request and
    push to `main`, plus `cargo publish --workspace --dry-run` and a build on Rust 1.88.
20. List fields. `corten::Form` and `corten::Query` (in the prelude, replacing axum's) read a key
    sent several times into a `Vec` field, a checkbox's `on` into a `bool`, and a blank field into
    `None`; a scalar sent twice is a 422 (400 for a query) naming the field. `#[derive(Fields)]`
    rejects a `bool` or `Vec` field without `#[serde(default)]`, since a browser sends nothing for
    an unchecked box. A typed route's `Vec` or slice field writes its parameter once per item.
    `corten::form::from_bytes` reads a body or query string directly. The alert form reads in
    673 ns against axum's 871 (release build, best of five). `examples/seller-desk` posts its
    checked rows as `ids` fields (each row's box joins the archive form with `form=`) instead of
    a comma-joined field its script wrote.
21. `App::section(prefix, components)`: components only the pages under a URL prefix render get a
    stylesheet of their own, `sections/{prefix}.css`, linked from those pages only, so a large
    part of the app most visitors never open stops adding to every first load. A client
    navigation into the section loads the stylesheet before it swaps the page in (a full load if
    it fails), and keeps it. A component with a stylesheet listed in two places, or tokens in a
    section, is a startup error; under `corten dev` a page outside the section that renders one
    of its components is an error naming both. Measured with callgrind (release build, in
    process, instructions in `App`'s middleware per item page): without sections a navigation
    took 87,249-87,331 against 87,252-87,257 before (three runs each), and a full document
    100,101-103,892 against 100,176-103,616 (ten runs each, medians 103,270 and 102,652; the same
    build lands near 100,200 or 103,600 from run to run). With the item page's four item
    components in an `/items` section, a navigation took 88,341-88,589: writing the links is
    about 120 of those, and most of the rest is in serde_json, which the section code does not
    call, and the allocator. A full document took 100,766-102,327. The home page's
    component CSS is then 593 bytes brotli instead of 1,047 and an item page's 1,141 in two
    files. The runtime is 126 bytes gzip larger. `examples/hello` puts the about page's note
    in a section, and the smoke test checks the stylesheet loads before the swap, stays, is not
    linked twice, and that a failed load falls back to a full page load.
22. `corten-ui`'s `Toaster` and `Toast`: a persistent `role="status"` region an action's patch
    fills by id (`#ui-toast`), hidden after `seconds` but kept while the pointer or focus is on
    it or the page is hidden (WCAG 2.2.1), with a close button the script reveals. Without
    JavaScript the message is in the page the post answers with. `examples/ui-gallery` saves a
    draft with one.
23. `scripts/smoke.mjs` and `examples/ui-gallery/smoke.mjs` run in Chromium, Firefox and WebKit
    (`node smoke.mjs [url] [engines...]`), and CI installs all three. Checks a browser cannot
    make are skipped with the reason: Playwright's WebKit has no Navigation API (navigations are
    full loads there) and does not send a `Secure` cookie over http.

### Removed
1. `#[island]`, `#[prop]`, `island()` and the custom element each island was. A component with a
   script renders its own root element, marked `data-scope`; `corten-ui`'s components are
   `#[component(js)]` with a `div` root. `Types::components` declares typed events instead of
   `CortenIslands`, and `corten::find_islands` is gone (the page preloads the modules its
   handlers named while it rendered).
2. The `solid` feature, `corten/solid`, `corten::solid_path` and `SOLID_JS`. Rows a script
   changes are views, which measured faster to take over (7-8.5 ms against 18-20 ms for 1,000
   server-rendered rows) and lighter (116 bytes a row against 927), and the framework no longer
   ships a third-party JavaScript bundle. Solid stays in `bench/browser` as a comparison.

### Changed

1. A handler never runs after its event: markup a navigation or patch inserts waits for the
   modules its handlers name, and an event that comes before its module on the first page gets
   the browser's default instead of a late handler. `patch()` returns a promise.
2. Every JSON failure has one shape, `{"error":{"status":404,"message":"Not Found"}}`. A data
   request's error now carries `message`, and an extractor's rejection is that JSON instead of
   plain text. `req.invalid` answers a data request with
   `{"error":{"status":422,"message":...,"fields":{...}}}` instead of `{"errors":{...}}`.
3. The CSP nonce is made only when a document asks for it; other responses get the same policy
   without a nonce source. `Policy::apply` no longer puts a `Nonce` in the request extensions:
   take `nonce: Nonce` as an extractor instead of `Extension<Nonce>`.
4. `App::context` no longer runs for static files, the runtime, the PWA files and the health
   checks.
5. `App` is one middleware instead of three (security, compression, errors). Measured with
   callgrind, a small JSON handler costs 27,584 instructions a request instead of 36,324, a
   small page 31,742 instead of 41,280, and a JSON 404 19,892 instead of 26,930
   (`docs/guide/app.md`, the request path).
6. `examples/item-page` mounts its MCP endpoint with `App::api`; its smoke test calls it from a
   page on another origin.
7. Everything `App` renders that does not set `Cache-Control` is sent `private, no-cache`
   (pages, swaps, patches, JSON, errors, refused writes, CORS preflights); before, it had no
   `Cache-Control` and caches could guess a lifetime. Static files and the runtime are unchanged.
8. The CSP allows Corten's own inline tag (the import map) by its SHA-256 hash, so a document
   no longer has a nonce unless the shell calls `doc.nonce()` or a handler takes `Nonce`.
   `Document::nonce` makes the nonce on first call. A shell that writes its own inline script
   still calls `doc.nonce()`; that page is then never shared by a cache.
9. A full document no longer sends `corten-page`; navigations and form answers still do. A
   document that a cache hands to a client navigation by mistake now makes the runtime load the
   URL as a whole page.
10. With `App::messages`, pages vary on `accept-language` too.
11. Measured with callgrind (median of 4-5 runs): a small JSON handler 28,255 instructions a
    request instead of 27,551, a small page 30,912 instead of 31,757 (`docs/guide/app.md`).
    The item page's document is 82 bytes smaller (9,161 instead of 9,243; 2,178 instead of 2,210
    with zstd), measured with `render_cost`; its times on Docker Desktop varied more between runs
    of the same build than between builds.
12. An MCP tool's output is serialized once: `structuredContent` is written from the JSON text and
    the text block is that text as a JSON string, with no `serde_json::Value` in between. Calling
    `items.get` for one item costs 278,273 instructions instead of 2,020,422 (callgrind, median of
    3). A tool may return `Arc<T>` (serde's `rc` feature) to skip a copy.
13. `bench/fullstack`: every app serves the item as JSON at `/api/items/{id}`, measured like the
    page; cold start is timed from the server's exec to its first 200, without Docker's container
    setup; `count.mjs` counts instructions per request with `perf`; `droplet.mjs` runs the whole
    benchmark on a DigitalOcean droplet and destroys it.
14. `bench/fullstack` compares fourteen frameworks: Django, Rails, Laravel, ASP.NET Core, Spring
    Boot, Phoenix, Astro and Nuxt join the six it had, each built the framework's usual way and
    checked to render the same text and JSON (`check-app.mjs`). Warmups are 30 seconds (the JVM
    and .NET need them); `droplet.mjs` records the droplet's CPU model. Apps run with their usual
    settings in one CPU and 1 GiB; one that cannot start or serve the fixed rate is marked failed
    instead of tuned.
15. `bench/live` (new): one price feed pushed to 1,000 and 10,000 clients in the same fourteen
    frameworks, each with its own transport (server-sent events, Corten's feed, Action Cable,
    Phoenix Channels, Reverb). It reports delivery, order, delay, CPU and memory per client, and
    marks an app that refuses clients, loses messages or stops as failed.
    `droplet.mjs --suite live` runs it on a droplet.
16. HTML escaping skips eight bytes at a time while none of them needs escaping, floats render
    without going through the escaping writer (their text never needs it), and a page's header map
    is allocated once with room for the headers `App` adds. The item page took 110,478-110,898
    user instructions a request in process for a navigation instead of 123,515-123,730, and
    138,127-138,687 for the full document instead of 152,150-153,001 (cycles 5-9% fewer), on the
    bench droplet; under load the difference was inside the run-to-run spread.
17. An MCP tool's reply is joined into one string allocated at its final size, instead of two
    `format!` calls that grew theirs while copying a large result.
18. `App`'s middleware is a tower layer of its own instead of axum's `middleware::from_fn`, which
    boxed the rest of the stack again for every request. Measured with callgrind (release build,
    in process, 4,000 requests): a small JSON handler under `App::api` 16,558 instructions a
    request instead of 17,305, a small page 21,190 instead of 21,880.
19. Floats in views (`{price}`, `data-price={price}`) are written from zmij's digits (already in
    axum's tree through serde_json) instead of through `core::fmt`, with the same text as
    `Display`: checked against it for 300 million values, every bit pattern, short decimals and
    short binary fractions. The few values that could sit exactly halfway between two shortest
    forms, where zmij and `Display` round differently, still go through `Display`, as do `f32`
    values. With the middleware change above, the item page took 100,380 instructions a request
    for a navigation instead of 109,373 and 113,949 for the full document instead of 124,617.
    On a bench droplet (Xeon Platinum 8280, app on one core, 64 users, three rounds alternating
    old and new) the item page served 21,053-22,526 requests a second instead of 17,556-20,084
    (user cycles 88,508-96,341 a request instead of 100,013-116,955), and the item's JSON
    24,565-25,804 instead of 23,314-24,867.
20. `examples/item-page` reads the item API's response into structs of only the fields the page
    uses (`ApiItem`) instead of a `serde_json::Value`, so serde skips the rest of the 55 KB
    without building it. Measured with `perf` on a bench droplet (Xeon Platinum 8280, median of
    3 x 5,000 requests): an item page fetched from the item API costs 1,385,381 user instructions
    instead of 4,395,372, the item's JSON 1,398,989 instead of 4,387,294; from memory both are
    unchanged (123,185 and 133,263). On that droplet, two rounds alternating old and new against
    the item API at 64 users, the item page served 3,128-3,176 requests a second instead of
    1,249-1,270 and the item's JSON 3,349-3,372 instead of 1,337-1,359; at 150 a second the page
    used 6.6-6.7% of its core instead of 13.3% (ASP.NET Core there: 1,344-1,373 pages a second,
    25-27%; `bench/fullstack/results/ab-typed-parse`). A response whose list field is not a list
    now fails the request instead of showing an empty list. `count.mjs` takes `api` as a fifth
    argument to count with items from the item API.
21. The runtime's ref cache survives lists: inserting or removing rows no longer makes the next
    handler query the whole component for its other refs (a click after creating 10,000 rows took
    8.8 ms, now 0.8 ms, then 0.1 ms), and markup joining the page is wired with one query for its
    handlers and one for its refs per insert, instead of two per element.
22. The first page loads its handler modules as `<script type="module">` tags (not preloads the
    runtime then `import()`s), and Corten ends each component's module with lines that hand its
    exports to the runtime (a `corten:module` event it takes only from the module the page's own
    script tag for that name loaded, while the page loads; `"corten"` exports nothing for it).
    Handlers are in when the page's scripts run, before `DOMContentLoaded`: on a page of 1,000
    server-rendered rows the time from the parser finishing to the handlers being ready went from
    75 ms to under 10 ms.
23. `bench/browser` compares Corten's views with React 19, Svelte 5, Vue 3.5, Solid and Remix 3,
    client-rendered, server-rendered and hydrated, and on the dashboard; `run-all.mjs` runs a
    round and `summarize.mjs` prints the README's client table. Takeover leaves out the frames
    Chromium paints between parsing and the app being ready (about 100 ms on some loads and none
    on others, for every app), which had made it bimodal. `droplet.mjs --suite browser` runs
    `bench/fullstack`'s browser pass and then `bench/browser` on a droplet. The README's
    Performance section has a browser table and a client-heavy table from that run
    (`browser-1`).
24. Making many rows is cheaper. When many children join one parent at once, the runtime finds
    the ref names in them with one selector query per name instead of reading every `data-ref`;
    `rows().replace` looks up no keys when the list was empty; and a view's `fill` checks a
    boolean attribute before toggling it. On a droplet, creating 10,000 rows took 79 ms instead of
    90 ms (Solid 64, plain JavaScript 61), a row took 101 bytes of heap instead of 116, and
    takeover was unchanged.
25. The README's browser and client-heavy tables are from round `browser-3` (Creating 10,000 rows
    68.9 ms, against 87.5 in `browser-1`; Solid 59.2). `bench/browser/ssr.mjs` takes the loads of
    every app in turns and `run-all.mjs` takes 30, after one app's takeover median in a round
    moved with a slow stretch of the machine. The README's server highlight now quotes round r4
    (18,043 pages a second, 1.1 MiB), the round its server table is from.

26. `corten new` makes the app depend on the `corten` release on crates.io that matches the
    command's version (`corten = { version = "0.1.0", ... }`) instead of the repository's `main`
    branch, so an app builds against the release it was created for. `--corten-path` still uses
    a local checkout. Install the command with `cargo install corten-cli`.
27. `Cargo.lock` takes tokio 1.54.1; 1.54.0 was yanked.

### Fixed
1. Hover prefetch kept every page a user hovered and never opened until the next form post. It
   now keeps the 8 newest (dropped ones have their bodies cancelled) and fetches nothing when
   the browser's data saver is on.
2. A patch no longer throws away what the user is doing: a field they edited (after its form was
   sent, for the form's own answer) keeps its value, and focus and the caret stay on the
   matching element. An invalid answer focuses its field only if the user has not moved on.
3. An integer handler argument past 2^53 reached the browser as a different number. Debug
   builds and tests now stop on it (pass such a value as a string).
4. A multiple select the user edited kept only its first choice when a patch replaced it; it
   keeps them all.
5. `<corten-live>` reconnected to a stream that had ended, so a finished stream was requested
   again every few seconds. `corten::live` now ends with an `end` event and the element closes.
   Shutdown ends open `corten::live` streams first, as it does feed streams: `docker stop` on the
   hello example with one stream open took 4,059 ms before and 965 ms after (measured, one run
   each; 637 to 929 ms with no stream).
6. `bench/fullstack/browser.mjs` finds Corten's sort button and listing prices by the
   `listing-sort` component's scope (`[data-scope=listing-sort]`). It looked only for the
   `<listing-sort>` island that handlers replaced, so the browser pass stopped at Corten's page.
7. `corten-ui`'s `MenuButton` returned focus to its button in WebKit even when the user had
   clicked elsewhere: WebKit moves focus out of a closing popover before `toggle` fires, so the
   menu now records on `beforetoggle` whether focus was inside it.

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
6. The full-stack benchmark tables in the README and `bench/fullstack` are from round r4
   (2026-10-23, one c-8 droplet, Xeon Platinum 8168), measured with the typed item-API parse;
   `bench/live` is still r3 (Xeon Platinum 8280). On r4's slower CPU Corten serves 18,043 item
   pages and 18,934 JSON responses a second from memory at 64 users (r3: 23,086 and 24,338 on the
   8280), and 2,142 item-API pages against ASP.NET Core's 915 (r3: 1,175 and 1,209). A page costs
   123,693 user instructions and the JSON 133,308 (r3: 123,665 and 133,588). Django and Nuxt
   failed r4's in-memory page test, which they passed in r3 near their limit. The item page
   loads 6.2 KB of JavaScript (gzip).
7. `Form` and `Query` in the prelude are `corten::Form` and `corten::Query` (Added 20). Against
   axum's: a blank field is `None` for an `Option` (axum gave `Some("")`, or an error for a
   number), and a `bool` accepts `on` (a checked checkbox), which axum refused. `serde` (without its derive) is now always a
   dependency of `corten`; it was already in axum's tree.
8. Hover prefetch does nothing in a browser without the Navigation API, where a click is a full
   page load that could not use the prefetched response.
9. The README, `docs/design.md` and `corten-ui` say what is tested in which browsers: the
   runtime and `corten-ui` in Chromium, Firefox and WebKit.

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
