# Changelog

Notable changes to Corten. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Corten is pre-1.0, so any minor version may change the API.

## Unreleased
### Added

1. `App::api::<ApiRoot>(|api| api.route::<ItemJson>(get(item_json)))`: routes with no screen (webhooks, public APIs, the MCP endpoint)
   answer every failure as JSON whatever the client accepts, including unknown URLs, wrong
   methods and the deadline, with no shell, nonce or context and a `default-src 'none'` CSP.
   `Api::allow_origin` lets one site's pages call them from the browser (CORS preflight and
   headers, writes without cookies let through for those routes only); `Api::context` runs
   `App::context` for them, and `Api::guard` decides who may call them. A route given to an API
   outside its prefix does not compile (item 30).
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
21. `Area::components(&[..])`: components only an area's pages render (see the `Area` item
    below) get a stylesheet of their own, `sections/{prefix}.css`, linked from those pages only,
    so a large part of the app most visitors never open stops adding to every first load. A
    client navigation into the area loads the stylesheet before it swaps the page in (a full
    load if it fails), and keeps it. A component with a stylesheet listed in two places, or
    tokens in an area, is a startup error; under `corten dev` a page outside the area that
    renders one of its components is an error naming both. Measured with callgrind (release build, in
    process, instructions in `App`'s middleware per item page): without sections a navigation
    took 87,249-87,331 against 87,252-87,257 before (three runs each), and a full document
    100,101-103,892 against 100,176-103,616 (ten runs each, medians 103,270 and 102,652; the same
    build lands near 100,200 or 103,600 from run to run). With the item page's four item
    components in an `/items` section, a navigation took 88,341-88,589: writing the links is
    about 120 of those, and most of the rest is in serde_json, which the section code does not
    call, and the allocator. A full document took 100,766-102,327. The home page's
    component CSS is then 593 bytes brotli instead of 1,047 and an item page's 1,141 in two
    files. The runtime is 126 bytes gzip larger. `examples/hello` puts the about page's note
    in an area, and the smoke test checks the stylesheet loads before the swap, stays, is not
    linked twice, and that a failed load falls back to a full page load.
22. `corten-ui`'s `Toaster` and `Toast`: a persistent `role="status"` region an action's patch
    fills by id (`#ui-toast`), hidden after `seconds` but kept while the pointer or focus is on
    it or the page is hidden (WCAG 2.2.1), with a close button the script reveals. Without
    JavaScript the message is in the page the post answers with. `examples/ui-gallery` saves a
    draft with one.
23. `scripts/smoke.mjs` and `examples/ui-gallery/smoke.mjs` run in Chromium, Firefox and WebKit
    (`node smoke.mjs [url] [engines...]`), and CI installs all three. Checks a browser cannot
    make are skipped with the reason: Playwright's WebKit on Windows has no Navigation API
    (navigations are full loads there) and does not send a `Secure` cookie over http. Its Linux
    build has the API, but Playwright then waits forever after the first navigation the runtime
    intercepts, so the smoke test checks WebKit's swap from inside the page and runs the rest
    without the API.
24. Guards: `Area::guard(|context, req| Access::..)` for an area, `App::guard` for the whole app
    and `Api::guard` for an API. Every route under the prefix (including unknown URLs and wrong
    methods there) needs the guard's `Access::ALLOW`, so a handler cannot forget a sign-in or
    permission check. The check gets the request as
    `Guarded`: `req.writes()` (every method but GET, HEAD and OPTIONS) gives reads and writes at
    one URL different rules, `req.is::<Route>()` names one route, `req.uri()` builds a sign-in
    link back. `App::guard` denies by default; `App::unguarded::<SignIn>()` opens
    one route, and Corten's own routes and the offline page are never guarded. A sign-in answer
    that a guard would send to sign in again is a 500 that says so, not a redirect loop. The app decides who passes; Corten
    answers before the handler and its extractors run, in the request's mode:
    `Access::sign_in(&route)` is a 303 for a page or a form post without JavaScript,
    `corten-location` for a form patch, a JSON 401 for a data client or an `App::api` route,
    and `Access::refuse(error)` is the error page or the JSON error. Guards nest (outer first),
    run the context for API routes under them, and read the app's context type (a guard of
    another type does not compile). Under `corten dev` a refusal names its guard. `Access` is in
    the prelude. `examples/hello` guards an account page; the smoke test checks the redirect
    signed out and the page signed in. Measured with callgrind (item page, full document,
    release build, instructions in Corten's middleware itself, the same in each of 3 runs):
    1,951 before, 2,006 with no guard, 2,392 with a context and a guard on `/items`, and 2,443
    with a context, a guard on `/` and one unguarded route, of about 405,000 for the whole
    request. The answer a guard gives
    is boxed: carried inline, it made every request's future larger, and `memcpy` alone rose
    from 17,935-18,544 instructions to 19,073-19,518.
25. `App<C>`: the context type is part of the app's type. `App::context` turns an `App` (an
    `App<NoContext>`) into an `App<Viewer>`, so a shell or guard that reads another type, a
    guard on an app without a context, and a second `App::context` do not compile (each was a
    startup panic or not caught), and `App::guard`'s closure needs no parameter types.
    `compile_fail` doctests cover each.
26. `App::area::<Admin>(|admin| admin.route::<AdminUser>(get(user)))`: a part of the app under a
    prefix that owns its routes, with `.components(&[..])` for a stylesheet only its pages load
    and `.guard(..)` for who may use it. It replaces `App::section` and a guard given a prefix
    string: the prefix is written once for both, areas and APIs hold their prefixes to one rule
    (letters, digits, `-`, `_`, `.` and `~` between slashes), and an area under an API prefix
    is a startup error. A library ships an area as a function,
    `fn admin<C>(area: Area<C, Admin>) -> Area<C, Admin>`. `Area` is in the prelude.
27. The app guide's "Your own middleware" section: tower layers on the routes run inside
    Corten's middleware (after the context and guards), and layers around `app.router()` run
    outside it, given to `corten::serve`. `docs/design.md` has a rule for where a new `App`
    setting goes (`Area` or `Api` for a prefix, `App<C>` for the context).

    Measured with callgrind (item page, full document, release build, in process, no
    compression, instructions per request in Corten's middleware and axum's routing, two runs
    each, before and after items 25 and 26): no guard 3,969 and 3,968-3,975; a context 4,061 and
    4,059-4,066; a guard on `/items` (an area after) 4,355 and 4,358; `App::guard` with
    `req.writes()` and one unguarded route 4,399 and 4,384-4,391.
28. A `tokens!` theme follows the system too: `"@media": {"(prefers-color-scheme: dark)":
    ":root:not([data-theme=\"light\"])"}` in a theme writes its values again under that
    condition and selector, so a dark theme that the page can choose and the system can ask for
    is written once. Found porting a Chase Sets admin screen, whose dark mode follows the system
    unless the user chose a mode.
29. `Errors::add_form`, `Errors::form` and `Errors::form_message`: a message about the whole form
    (a rule across fields, or the backend refusing the write), rendered as a `role="alert"`
    paragraph and sent to JSON clients as `form` (and first in `message`) and to agent tools
    first in their error. Before, such a message had no place in `Errors`, so `req.invalid`
    gave data clients and agents a 422 without the reason. Found in the same port: the platform
    API rejects some writes with a message that names no form field.
30. An area's and an API's prefix and routes are route types, so the compiler checks what was
    checked per request or not at all: `App::area::<P>` and `App::api::<P>` take `P`, a
    `#[derive(Route)]` whose path is the prefix (a prefix with a parameter or a trailing slash
    does not compile), and a function that fills in the `Area` or `Api` with
    `.route::<R>(method_router)`, where an `R` outside the prefix does not compile (a `const`
    assertion, with `compile_fail` doctests). Before, a route typed wrong answered 500 when
    first requested. The function gets the app's context type, so an area's guard reads the
    context's fields without a type annotation (`|viewer, req| viewer.is_some_and(|v| v.staff)`).
    `Area::new`, `Api::new` and `impl From<Router> for Api` are gone. The per-request check of
    where a route was given left the request path with it: measured with callgrind (item page,
    full document, release build, instructions per request in Corten's middleware and axum's
    routing, two runs each, before and after), no guard 6,841-6,842 and 6,819-6,826, a context
    6,930-6,943 and 6,909, a guard on an `/items` area 7,169-7,176 and 7,159, the item route
    inside that area 7,195 and 7,159, `App::guard` with one unguarded route 7,222-7,229 and
    7,212-7,219. (These count more than item 27's figures, which were taken by another script.)
31. `#[derive(ToJson)]` and `corten::json::ToJson` (feature `json`, in the prelude): a struct or an
    enum written as JSON with the layout decided when the crate compiles, so between two values
    the response copies one byte string (`","count":`) instead of writing each key, quote, colon
    and comma, and string values are tested for characters to escape eight bytes at a time. It
    reads `#[serde(...)]` (`rename`, `rename_all`, `skip`, `skip_serializing_if`, `transparent`, and an
    enum's `tag`, `content` and `untagged`) and writes the bytes serde_json writes; an attribute it
    cannot follow (`flatten`, `serialize_with`, `into` ...) does not compile. A field whose type only
    implements `Serialize` is written by serde_json. `corten::json::to_vec` writes any `ToJson`.
    `examples/item-page` derives it on its view model, and its `render_cost` checks the 500 benchmark
    items against serde_json byte for byte and times the JSON.
32. A `<corten-live>` stream resumes after a reconnect: `corten::live` takes `Fragment`s
    as well as HTML, `Fragment::new(html).cursor(seq)` sends the app's cursor as the event's
    `id`, `<corten-live since={seq}>` gives the first connection the cursor the page was
    rendered at, and the `LiveSince` extractor reads either (`Last-Event-ID` first). A
    handler sends only what changed after it, so a page view or a reconnect costs no extra read
    or render. Asked for by the Chase Sets pilot, whose catalog lists drew every row on each
    connection ([#15 A <corten-live> stream cannot resume after a reconnect](https://github.com/todd-skelton/corten/issues/15)).
33. `corten check` type-checks the app's scripts: TypeScript's `tsc` (from the app's `node_modules`
    or the `PATH`) runs in strict JavaScript mode over every `.js` file in `src/` and `static/`,
    against the DOM's types, the `corten` runtime's JSDoc and the app's generated `types/*.d.ts`,
    with a config it writes under `target/corten/`. The compiler already checked each handler's
    call; this checks its body. TypeScript stays optional: without `tsc` the check passes and says
    how many scripts were not checked. Asked for by the Chase Sets pilot
    ([#20 corten check does not type-check an app's scripts](https://github.com/todd-skelton/corten/issues/20)).
34. `#[derive(FromJson)]`, `corten::json::from_slice` and `from_str` (feature `json`, the derive in the
    prelude): JSON read by Corten's reader, which matches each key against the type's fields, makes
    each value straight from the text and skips (validating) the rest. Where it meets anything it
    does not follow (an escaped key, a missing or duplicate field, a number it does not parse
    itself) serde_json reads the text instead, so the value and every error are serde_json's; it
    needs `Deserialize` next to it for that. It reads `rename`, `rename_all`, `alias`, `default`,
    `skip`, `skip_deserializing`, `deny_unknown_fields` and `transparent`; one it cannot follow, or
    an enum with data, does not compile, and a field of a type with only `Deserialize` is read by
    serde_json from its own text. `examples/item-page` reads its item API's response with it:
    measured in process (`perf_event`, user-mode instructions, release build, the 500 benchmark
    responses of 55 KB), 878,078 instructions a response through serde_json and 698,733 through
    `FromJson` (-20.4%), 913,151 and 733,130 with the conversion into the view model;
    `render_cost` (median of 3) reads one in 304.8 µs through serde_json and 185.5 µs. Its
    `render_cost` checks the 500 responses read the same both ways.
35. Live streams are compressed (feature `compress`). A `corten::live` or feed response to a client
    that accepts zstd or brotli is one compressed stream per connection, flushed after every
    event, with the context kept between events. On a 33-row list (6,126 B an event), measured:
    the first event 425 B with zstd 1 and each redraw 16-23 B, where every event was 6,126 B;
    brotli 4 gives 376 B, then 13-23 B. A stream holds about 470 KiB (zstd, 64 KiB window) or
    800 KiB (brotli) while open, and all of them share a 256 MiB budget past which a new stream is
    sent as it is. Gzip-only clients get streams as they are. Asked for by the Chase Sets pilot
    ([#24 corten::live responses are never compressed](https://github.com/todd-skelton/corten/issues/24)).
36. `SubmitKey` (in the prelude): one submission of a form, so a write whose answer was lost and
    is sent again applies once. A form struct takes a `SubmitKey` field and draws it with
    `SubmitKey::field(f.submit)`, a hidden input with a random UUID v4 (the form most APIs take an
    idempotency key in, [#35 A SubmitKey can only be 32 hex digits, so an API that takes a UUID cannot use it](https://github.com/todd-skelton/corten/issues/35));
    a value that is not a UUID v4 is a 422 naming the field. A reloaded post sends the same key, and the
    runtime keeps it for a `data-patch` form until a submit gets an answer below 500 (a network
    error, a 5xx or an abort keeps it), then renews it. The app stores keys with its writes; Corten
    keeps none. Asked for by the Chase Sets pilot, which wrote about 150 lines for it
    ([#9 Forms have no per-submission idempotency key](https://github.com/todd-skelton/corten/issues/9)).
37. `<corten-live>` shows its connection: a `status` attribute of `connecting`, `live`,
    `reconnecting`, `closed` (the browser gave up) or `ended` (the server finished the stream), and
    `stale-since`, the ISO time a live stream dropped, until it is live again. A page can mark a
    stale part with CSS alone. Asked for by the Chase Sets pilot, whose lists showed a dropped
    stream the same as an unchanged one
    ([#33 A page cannot tell whether a <corten-live> element is connected](https://github.com/todd-skelton/corten/issues/33)).
38. Query sets: `#[derive(Route)] #[route(query)] struct ItemQuery { .. }` declares parameters that
    several routes carry, and `#[route(query)] filters: ItemQuery` on a route's field writes each of
    them as the route's own parameter (`corten::route::Params`). A list's filters are declared once
    for its page, its live rows and its bulk routes, so a route cannot drop one, and making one
    route's URL from another's copies one value. Handlers read `Query<ItemQuery>`. A
    `#[route(query)]` field of another type does not compile. Asked for by the Chase Sets pilot,
    whose 10 item filters were repeated on 5 routes by a `macro_rules!`
    ([#32 A route cannot share a set of query parameters with another route](https://github.com/todd-skelton/corten/issues/32)).
39. `#[derive(ToJson)]` writes a `#[serde(flatten)]` field whose type is a struct that derives
    `ToJson` (or an `Option` or `Box` of one): its members in place, as serde writes them, laid
    out when the crate compiles. The derive implements `corten::json::ToJsonMembers` for every
    struct with named fields; flattening a map, an enum or a `Serialize`-only type still does not
    compile, now with a message that says why. A dimension with 12 options (796 B) is written in
    640-860 ns, where serde_json's flatten took 1,820-2,660 ns (measured, release build). Asked
    for by the Chase Sets pilot, whose view model of an API type plus a few fields wrote its JSON
    through serde_json
    ([#34 #[derive(ToJson)] refuses #[serde(flatten)] of a struct that is itself ToJson](https://github.com/todd-skelton/corten/issues/34)).
40. Text a script computes comes from the catalog: `t.js()` (from `messages!`) has each message
    with arguments unfilled, a handler takes one as `@param {Message<"count">} label` (`Message`
    from `corten`) and shows `label({ count })`. The markup passes it in the page's locale
    (`js::count(total, t.js().rows_selected())`), sent as its text and argument names in
    `data-on` (`%["","count"," selected"]`), so only the messages a page uses are sent and the
    browser formats them with no round trip. A message whose arguments are not the ones the
    JSDoc names does not compile at the call, and `tsc` checks the script fills in each one. The
    runtime grew 177 B (74 B gzip). Asked for by the Chase Sets pilot, whose selection bar drew
    every count from 0 to the page's rows in a `<template>` (1,220 B on a 33-row list, sent again
    in each live event)
    ([#17 Scripts have no typed way to show user-facing text they compute](https://github.com/todd-skelton/corten/issues/17)).

41. `<corten-live>` dispatches a bubbling `corten-status` event each time its `status` changes, so
    a handler hears it with `on:corten-status={js::stale(target, ..)}` and reads `status` and
    `stale-since`, as a feed's `subscribe` calls its `status` callback. Asked for by the Chase
    Sets pilot, whose "Stale since {time}" banner watched the attributes with a `MutationObserver`
    ([#49 A script cannot hear a <corten-live> element's status change](https://github.com/todd-skelton/corten/issues/49)).
42. `t.by_key(key, &args)` (from `messages!`): the message with a catalog key chosen at run time,
    such as one another service answers with, in the request's locale, its arguments taken by
    name from pairs, a map or a JSON object (`corten::i18n::Args`). It is `None` for a key the
    catalog lacks or when an argument is missing, and renders escaped like any message. Asked for
    by the Chase Sets pilot, whose attention queue matched 20 API keys to methods by hand
    ([#54 A page cannot show a catalog message chosen at run time by its key](https://github.com/todd-skelton/corten/issues/54)).
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
3. The `memchr` dependency of `corten`, which no code used.

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
28. `corten::Result` takes a second, defaulted parameter, `Result<T = Response, E = Error>`, so
    `Result<u32, Errors>` works under `use corten::prelude::*` instead of failing with too many
    generic arguments. Existing handlers are unchanged.
29. `FieldName` is in the prelude, next to `Errors`, `Fields` and `Form`.
30. `corten::serve(app)` replaces `App::serve`. It takes anything that becomes a router (`App`
    does), so an app adds its own layers around the whole app,
    `corten::serve(app.router().layer(..))`, without giving up `HOST`, `PORT`, the log
    subscriber and graceful shutdown.
31. Less work per page, with the same bytes out: head tags (`Meta`) and `corten::escape` escape
    through the word-at-a-time `escape_into` straight into the page instead of a character at a
    time through `format!`; a route's text path parameters (`&str`, `String`, `Cow<str>`) are
    percent-encoded from the string, a run of plain bytes at a time, without going through
    `Display`; and each script module's tag is made when the assets load, so a page no longer
    hashes the module's name twice and formats its tag. Measured with callgrind on the item page
    app: 134,682 instructions per item page request before, 120,175 after (-10.8%), and 95,398 on
    the home page (50 item links) before, 85,261 after (-10.6%). The item view's `render_cost` is unchanged within noise (median 20.0 µs before,
    20.2 µs after, 15 runs each).
32. `Json<T>` responses need `T: ToJson` instead of `T: Serialize`: add `#[derive(ToJson)]` next to
    `Serialize` (`serde_json::Value` already is one). A value that cannot be JSON (a map with tuple keys)
    no longer compiles, where it answered 500. Measured on the item page's view model (the
    benchmark's 500 items, 6.6 KB of JSON on average), user-mode instructions per request in process
    (`perf_event`, release build, the lowest of each run's repeated passes): a JSON request 114,157
    before, 58,259 after (-49%); `render_cost` (median of 3) puts the item as JSON at 33.2 µs before and 12.7 µs after,
    and the whole JSON request at 77.4 µs and 32.1 µs.
33. `escape_into` tests a string for characters to escape in one or two reads when it is under
    eight bytes, where it checked each byte (most of a page's strings are short: 188 per item
    page, 11 bytes on average), and integers are written two digits at a time. The item page's
    body went from 56,280 instructions to 51,171 (-9.1%), a document request from 96,918 to
    90,528 (-6.6%) and a navigation from 87,249 to 81,649 (-6.4%), measured as above;
    `render_cost` puts build + render main at 28.9 µs before and 19.8 µs after (median of 3, noisy on
    this machine).
34. A route parameter with nothing to percent-encode (most ids and slugs) is tested with one table
    lookup per byte and no branch, then copied once, where the encoder searched it byte by byte.
    The bytes written are unchanged. Measured in process on the item page (11 route links), user
    instructions (`perf_event`, release build, lowest of 3 runs): the page body 56,275 before and
    55,658 after (-1.1%), a document request 94,815 and 93,953, a navigation 86,966 and 86,362;
    `render_cost` is unchanged within noise (build + render main, median of 3: 14.3 µs before,
    16.0 µs after).
35. The JSON a request reads or answers with goes through Corten's own JSON code where it still
    went through `serde_json`: the `Json` extractor needs `T: FromJson` (derive it next to
    `Deserialize`) and reads an `application/json` body with Corten's reader, leaving any other
    JSON content type, and any body the reader gives up on, to axum, so the value and every
    rejection are axum's; `req.screen`'s data, `Meta::json_ld` and a handler argument of a JSDoc
    type Corten does not check need `ToJson` instead of `Serialize`, with the same bytes out.
    Measured on the benchmark's 500 items (`perf_event`, user-mode instructions, release build,
    lowest of each run's passes, three runs within 0.5%): reading an item API response (55 KB)
    through the extractor 1,616,862 before and 725,531 after (-55%; axum's `Json` tracks the path
    to an error as it reads), `json_ld` of the item's view model 106,164 and 50,265 (-53%), and a
    whole `Accept: application/json` request for a `req.screen` of it 123,664 and 69,200 (-44%).
36. `FromJson` reads with less work per value: skipping a value tests for whitespace with one
    compare where it tested four, a number no longer skips whitespace its caller already skipped,
    and a struct with a container `#[serde(default)]` is read over that default (each field written
    in place) instead of into one `Option` per field that is moved out at the end. The values and
    errors are unchanged. Measured as in item 34 under Added: an item API response 698,733
    instructions before, 684,106 after (-2.1%; -22.1% against serde_json's 878,078), 733,130 and
    718,504 with the conversion into the view model. Found comparing the reader with the pilot it
    came from, on the same types and text: the pilot took 654,700 and the reader 686,650 (4.9%
    more); these three close 14,000 of the 32,000, and the rest is spread thin.

37. Field errors are read within the form's id: `errors.within(PriceId(id))` (a `#[derive(Id)]`,
    the element a refusal replaces) gives `aria(field)` and `message(field)`, and a message's
    id is `{form id}-{field}-error` (`price-s1-price-error`), where it was `{field}-error`. Two
    refused forms with a field of the same name (a price in every row) gave two elements the same
    id, and the second field was described by the first form's message. `Errors::aria` and
    `Errors::message` are gone: write `let e = errors.within(FormId);` and `e.aria(..)`,
    `e.message(..)`. Found by the Chase Sets pilot, whose item page has two forms with a `text`
    field
    ([#45 Field error ids are only the field's name, so two forms on a page share them](https://github.com/todd-skelton/corten/issues/45)).
38. `Feed::snapshot` keeps the body it wrote until the rows or `seq` change (a write that touches
    a row, `apply`, `reset`), so the snapshots between two writes, one for every page that
    calls `subscribe()`, share one serialization instead of cloning and serializing every row
    each time. The bytes are unchanged, and a body written while a write landed is not kept. The
    feed holds one snapshot's JSON in memory for it. Measured with 9-field listing rows
    (`perf_event`, user-mode instructions, release build, lowest of 7 passes, two runs alike): a
    snapshot with no write since the last 444,927 before and 1,272 after at 100 rows (18 KB), and
    4,424,419 and 1,274 at 1,000 rows (187 KB); a snapshot right after a write 458,893 and 461,114
    (+0.5%), 4,433,932 and 4,440,648 (+0.2%); a write alone 9,664 and 9,705.
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
8. A client navigation the server redirected (a page handler answering `req.redirect`, now a
   guard sending someone to sign in) changed the address to where it was sent but left the old
   page on screen: the runtime's `history.replaceState` aborted the navigation, and the body
   read with it. The runtime now reads the page, then navigates to its URL in place of the
   requested one with the page in hand, so it is swapped in without a second request. The
   runtime is 156 bytes larger, 46 gzip. Found by the hello smoke test's guarded page in
   Chromium and Firefox.
9. A component's stylesheet can have a class named like the component: `.card` in `Card`'s
   stylesheet is `class={Card}` in its markup. Before, the class constant hid the component's
   own `Component` constant inside `render`, and the build failed with "expected `&Component`,
   found `&Class`" pointing at `#[component(css)]`. Found porting a Chase Sets admin screen,
   where `Field` with `.field` and `Choices` with `.choices` had to be renamed. The generated
   code only moves the class constants into a block: `render_cost`'s render functions measured
   996,177,186 instructions before and 997,217,167 after under callgrind (one run each, 7,735
   bytes a page both times).
10. Without `HOST`, `corten::serve` listens on both loopback addresses, `127.0.0.1` and `[::1]`,
    on one port (only `127.0.0.1` on a system without IPv6). Before, it took `127.0.0.1` alone,
    and a browser that resolves `localhost` to `::1` first (Chromium on Windows) waited for a
    fallback on every new connection: about 200 ms added to every page and asset, measured
    porting a Chase Sets admin screen, where it made the port look slower than the React app it
    replaced. A `[::1]` port another process holds is now an error at startup, since
    `localhost` would reach that process. `HOST` set (as the Dockerfiles set `0.0.0.0`) binds
    that one address, as before.
11. A navigation started before the first page has fired `load` is a full page load instead of a
    swap. A form a script submitted from `on:connect` made Chromium stop loading the page, and
    the runtime taking that navigation over left it without a `load` event
    ([#11 A navigation taken over before the first page loads stops it loading](https://github.com/todd-skelton/corten/issues/11)).
12. `corten dev` rebuilds when a local package the app's build reads changes (a path
    dependency, a workspace's shared crate), not only on a change in the app's directory. It
    takes the packages from `cargo metadata` after each build
    ([#10 corten dev does not rebuild when a path dependency changes](https://github.com/todd-skelton/corten/issues/10)).

13. A `data-patch` form with a control named like one of the form's properties
    (`<button name="action">`, an `<input name="method">`, `id`, `elements`) posts as it does
    without JavaScript. The runtime read `form.action` and `form.method`, which the DOM makes the
    control, and posted to `/[object HTMLButtonElement]` or threw; it now reads the form's own
    properties. Found in the Chase Sets pilot, where `#[derive(Fields)]` named a field `action`.
14. A component is scoped only when it has refs or a handler in its markup takes `scope`
    (`on:click={js::clear(scope)}`). `#[component]` treated any identifier named `scope` as asking
    for it, so a component with a variable or field of that name and two root elements failed with
    an error about refs it did not have. The error for a scoped component with several roots now
    names both causes. Found in the Chase Sets pilot
    ([#26 A component that names a variable scope is treated as scoped](https://github.com/todd-skelton/corten/issues/26)).
15. An image built from one checkout no longer gets the crates another checkout built. The
    Dockerfiles (the examples', the benchmarks' and the one `corten new` writes) keep `target/` in a
    cache every build on the machine shares, and cargo decides a crate is current by file times,
    so a checkout whose files were older than the last build compiled against that build's
    crates (an item page image failed with `FromJson` missing from `corten`), and two builds at
    once wrote the same `target/` (a test build lost an rlib another was replacing). Each build now
    touches the sources it copied first and takes the cache alone (`sharing=locked`), so builds
    wait for each other; a rebuild with no change compiles the workspace's own crates again (the
    item page: 137 s, where it took 6 s), and the registry's stay cached.
16. `corten check` at a workspace root type-checks the scripts of every member that depends on
    `corten`, each against its own `types/`, as `cargo test` there tests each member; it found
    none before and passed without a word. Without `tsc` the note names the members. The CLI reads
    `cargo metadata` with a small JSON reader instead of searching its text. Found by the Chase
    Sets pilot, whose CI ran `corten check` once at the root of its three-member workspace
    ([#39 corten check at a workspace root type-checks none of the members' scripts](https://github.com/todd-skelton/corten/issues/39)).

17. A form with a control named `id` is patched: the runtime read the patch's `el.id`, which for
    `<form id="x"><input name="id">` is the input, so a refused `data-patch` form's 422 was
    dropped and its field errors never shown. Ids are read from the attribute, in the patch and
    where a patch carries over what the user typed (which lost the text in a form like that
    inside a patched block)
    ([#44 A form with a control named id cannot be patched](https://github.com/todd-skelton/corten/issues/44)).
18. A `@typedef` with a `Message` property is a group (`js::Counted { entity, label: t.js().x() }`),
    as one with an element is: a message is not JSON, so the typedef stayed a JSON value, no
    `js::Counted` existed, and the compiler said only that it could not find it. Found by the
    Chase Sets pilot, which passed a count's texts as three parameters
    ([#50 A @typedef of plain values and a Message is not a group, and the error does not say why](https://github.com/todd-skelton/corten/issues/50)).
19. A catalog message may take an argument named like a Rust keyword (`{type}`, `{match}`,
    `{ref}`): its parameter is the raw `r#type`, and `by_key` and `Message<"type">` keep the
    catalog's name. `messages!` refused 59 names, so a catalog shared with another app failed to
    compile; only `self`, `super`, `crate` and `_` remain, with an error that says to rename
    them. Found by the Chase Sets pilot, whose shared catalog has `{type}`
    ([#56 A catalog message cannot take an argument named like a Rust keyword ({type})](https://github.com/todd-skelton/corten/issues/56)).
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
10. The goal is stated as speed at the user's end, with the work done where it is fastest: the
    README's tagline, introduction, "When to use it" and values 1, 3 and 8, a new design choice
    (9), design rules 1, 4 and 5, `docs/architecture.md`, the agent skill and the `AGENTS.md`
    `corten new` writes. They read as if every interaction should be a server render; sorting,
    filtering, selecting and totalling what a page already has belong in the browser. The scripts
    guide has a new section, Server or browser, with a table for choosing. Pages and actions still
    work without JavaScript; a feature only a browser can provide may need it. The missing piece
    for rich client text is
    [#17 Scripts have no typed way to show user-facing text they compute](https://github.com/todd-skelton/corten/issues/17).

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
