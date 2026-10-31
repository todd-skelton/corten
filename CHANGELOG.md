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
20. List fields. `corten::Form` and a route's `Params` (Added 116) read a key
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
    them as the route's own parameter (`corten::route::QueryParams`). A list's filters are declared once
    for its page, its live rows and its bulk routes, so a route cannot drop one, and making one
    route's URL from another's copies one value. Handlers read it as their route's `Params` field. A
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
43. `Policy::form_action(origin)` adds an origin to the CSP's `form-action`, so a page can send a
    form to a hosted checkout, an identity provider or a sibling app, and a form's handler can
    `req.redirect` there; the rest stay blocked (`form-action 'self'`). Found by the Chase Sets
    pilot, whose cart's Check out form posts to marketplace's origin and was blocked by the
    browser before the request was sent; a form posted straight to another app also needs that app
    to accept this origin, so posting here and redirecting is usually simpler
    ([#64 A page cannot send a form to another origin, and a post cannot redirect to one (form-action is always 'self')](https://github.com/todd-skelton/corten/issues/64)).
44. Numbers, money and dates in the request's locale, on the server: `t.number(n)`,
    `t.money(Minor(123450), USD)` (`$1,234.50` in en, `1 234,50 $US` in fr; or a `Decimal` an API
    sends as text, never a float), `t.date(at, DateStyle::Medium)`, `t.time(at, TimeStyle::Short)`
    and `t.date_time(..)` (`Mar 22, 2026, 12:00 PM UTC`), in UTC unless `.zone(..)`, rendered
    escaped and usable as message arguments. `corten::i18n` adds `Decimal`, `Minor`, `Currency`,
    `DateStyle`, `TimeStyle`, `Zone`, `Utc`, `Offset` and `parse_timestamp`. Each locale's data is
    CLDR's from Intl (`scripts/locale-data.mjs`, 108 locales, checked against Intl on 113,080
    values; the Rust test checks 9,828), and `messages!` puts only the listed locales' into the
    app. A tag with no data of its own uses its language's; a language with none is a build
    error. `number`, `money`, `date`, `time` and `date_time` are no longer catalog keys. Asked for
    by the Chase Sets pilot, which wrote `en` dates and prices by hand
    ([#60 A page cannot write a date and time in the request's locale](https://github.com/todd-skelton/corten/issues/60),
    [#63 A page cannot write an amount of money in the request's locale](https://github.com/todd-skelton/corten/issues/63)).
45. The same formats for a script: `t.js().number()`, `t.js().money(USD)` and
    `t.js().date(DateStyle::Medium)` (`.time`, `.date_time`, `.zone(Offset)`) pass a handler whose
    JSDoc takes a `NumberFormat`, `MoneyFormat` or `DateFormat` (from `"corten"`) a function that
    writes what the server writes (`money(123450)` is `$1,234.50` in en), from the locale data sent
    in `data-on` (`%{..}`); a format of the wrong kind does not compile. `scripts/format.mjs` (in
    `npm run check`) runs the runtime's formatter on all 9,828 cases with the formats the server
    sends (`src/i18n/specs.tsv`, which a Rust test rewrites). The runtime is 777 B larger gzipped
    (6,748 to 7,525 B). hello's live prices use it
    ([#60 A page cannot write a date and time in the request's locale](https://github.com/todd-skelton/corten/issues/60),
    [#63 A page cannot write an amount of money in the request's locale](https://github.com/todd-skelton/corten/issues/63)).
46. `<ReadAgain after={..} />` (`Meta::read_again(after)` before item 47 removed `Meta`), among a
    page's head tags: a page showing a read that has not caught up with a write asks to be read
    again. The runtime fetches the same URL after the delay and swaps it in (history entry
    replaced, scroll kept, focus kept by id, no swap when the answer matches the last read's, a
    hidden tab waits),
    however the page was reached: a full load, a link or a `data-patch` form's redirect. A
    navigation away stops it; without JavaScript it is a `<meta http-equiv="refresh">` in
    `<noscript>`. A `Refresh` header on a page is a 500 in debug builds: Chromium, Firefox and
    WebKit keep a full load's after the runtime navigates away and load that URL again (probed).
    The runtime drops `<noscript>` content from the markup it swaps in, which a template would
    otherwise make live. The runtime is 282 B larger gzipped (7,525 to 7,807 B). hello's
    `/saved` shows it
    ([#69 A page cannot ask to be read again shortly, and the Refresh header only works on a full load](https://github.com/todd-skelton/corten/issues/69)).
47. A typed route can carry query parameters whose names are data: a field marked
    `#[route(prefix = "dimension.")]` (a `BTreeMap`, or a `Vec` or slice of pairs) writes each
    entry as `dimension.<key>=<value>`, in order, both encoded. A `HashMap` (whose order would make
    the same link differ), a prefix with a character a URL escapes, and a prefix that overlaps
    another parameter's name are compile errors. Found by the Chase Sets pilot, whose variant
    links built `dimension.<id>=<option>` by hand
    ([#71 A typed route cannot carry query parameters whose names are data (dimension.<id>=<option>)](https://github.com/todd-skelton/corten/issues/71)).
48. `req.moved(&route)`: a `301 Moved Permanently` for a page that moved for good (an old slug, an
    item asked for by its id), kept by the browser for a day (`private, max-age=86400`); a
    `data-patch` form is sent on as by `redirect`, which stays a 303
    ([#72 A page that moved cannot redirect permanently (Req::redirect is always 303)](https://github.com/todd-skelton/corten/issues/72)).
49. The page is a document made of components. `req.page`, `req.screen` and the error page take a
    `Document`: `html!` that starts with `<html>` (one `<head>` with one `<title>` and one
    `<Corten />`, one `<main>` outside any `for`, `if` or `match`, all checked when it compiles), or
    a component that returns `impl Document`, such as the app's `Layout`. `html!` renders only the
    parts a request needs: the title for a data client; the title, the page's head tags (expressions
    and components in `<head>`, marked `data-corten-head`) and `<main>`'s children for a client
    navigation. `<Corten />` is where Corten adds the runtime and the page's scripts and stylesheets.
    `req.asset`, `req.has_asset`, `req.nav`, `req.is_current`, `req.nonce` and `req.lang` are what
    a layout reads. On the item page (release, `render_cost`, eight alternating runs against the
    commit before, Windows) a full document is about 7% faster (median 57.5 to 53.8 us) and a
    navigation within noise (47.0 to 47.8 us; another eight runs gave 48.4 to 48.3), with the same
    bytes, compressed or not.
50. `#[corten::routes]` on a module of routes: each `#[route("/..", get = home, post = save)]` struct in
    it is a typed route that names its handlers, and `url::router()` puts each at its path for
    `App::new` (`#[corten::routes(state = Db)]` gives `url::router(db)`, and every handler may take
    `State<Db>`). A handler that cannot serve its route points its error at the handler's name; a
    route without a handler (`link` marks one that is only linked to, such as an area's page),
    two routes with the same path, or a method named twice does not compile. `corten new`,
    `examples/hello` and the README use it.
51. `corten new` writes the app's `Layout` and `error_page` in `src/views.rs`, for the app to change.
52. `App::error_page` takes `fn(&Req, Failure) -> impl Document`: `Failure` has the status and, under
    `corten dev` only, the error. Every error page is sent with `X-Robots-Tag: noindex`.
53. `JsonLd { value: &product }` (or `<JsonLd value={&product} />`), feature `json`: structured
    data among a page's head tags, as `<script type="application/ld+json">`, with `<` written
    `\u003c` so no text in it can end the element, as `Meta::json_ld` did before item 49 removed
    `Meta`. An expression, component or block inside `<script>` or `<style>` in `html!` is now a
    compile error: it was escaped as HTML, which neither reads back (JSON with `&quot;` is not
    JSON). Found by the Chase Sets pilot, whose item page wrote its own escaping in front of `raw`
    ([#76 A page cannot write JSON-LD in its head since Meta was removed (an expression in <script> is escaped as HTML)](https://github.com/todd-skelton/corten/issues/76)).
54. `#[corten::routes(area = Admin)] mod admin { .. }`: an area's routes name their handlers too.
    The module's `Handlers` trait has a method per route and `admin::area(area, handlers)` mounts
    each in the area: `.area::<admin::Admin>(|area| admin::area(area, Staff).guard(..))`. A route
    without a handler, two routes with one path, and a route outside the area's prefix do not
    compile. Found by the Chase Sets pilot, which serves every route from an area, so item 50
    covered none of them
    ([#78 An area's routes cannot name their handlers (#[corten::routes] covers only App::new)](https://github.com/todd-skelton/corten/issues/78)).
55. `view::Patch` (in the prelude): a view a `data-patch` answer can send, checked when it compiles.
    `html!` is a patch when each of its top-level elements has `id={..}` with a `#[derive(Id)]` type
    and its other top-level parts (expressions, components, every branch of a `for`, `if` or
    `match`) are patches; a component is one when declared `-> impl Patch`, and then its markup must
    be. Text, an element without an id and an id written as text are compile errors, each with a
    `compile_fail` doctest. Each `html!` returns its shape, zero-sized types, so the check costs
    nothing at run time: seller-desk's `render_cost` (rows rooted at `<tr id={RowId(..)}>`, the
    path the check adds code to) is 1,967,358,212 instructions before and 1,967,337,624 after under
    callgrind (release), with the same bytes; the item page's whole `render_cost` is 34,210,499,634
    and 34,188,947,492 (-0.06%).
56. A component parameter can be `impl Render` (any `impl Trait`), a type parameter of its struct:
    `fn Layout(req: &Req, title: &str, children: impl Render) -> impl Document`. The README's example
    compiles as edition 2024, as apps are, so its functions return `impl Document` without lifetimes.
57. `corten_ui::dialog::modal`: `<dialog open={refused} on:connect={dialog::modal(target)}>` makes a
    dialog the server draws open (a refused form, in a whole page or a patch) modal when it joins
    the page, with focus on the first field marked invalid in it. `open` alone is the platform's
    non-modal state: no focus trap, the page behind not inert, no backdrop. It is a handler module
    in `corten_ui::COMPONENTS` (`script!`), takes `HTMLDialogElement` (on another element it does not
    compile) and is 593 bytes as served (measured), loaded only by pages that use it. `examples/ui-gallery`
    refuses a rename in a dialog and its smoke test checks it is modal, with or without the runtime
    for the post, in Chromium, Firefox and WebKit.
58. `data-keep-place` on a form or link: the navigation it starts keeps the scroll position and
    puts focus back on the control the user changed (by id, else by name, and value for a checkbox
    or radio), with its caret, also across a redirect. A list's filters are a GET form applied on
    `change` (`form.requestSubmit()`), so keyboard and screen-reader users no longer start again
    from the top of the page after every filter. A navigation without it still goes to the top.
    `examples/hello`'s about page has a filter that the smoke test checks in Chromium and Firefox
    ([#92 A GET form that applies a filter on change loses the user's place: its navigation scrolls to the top and moves focus to the body](https://github.com/todd-skelton/corten/issues/92)).
59. `view::Append(RowsId, rows)`: a patch that adds rows at the end of the element with that id
    (`<template id data-append>`), so Load more reads and renders one page instead of every row
    shown and the rows the page has stay as they are. `examples/hello`'s about page loads its
    history a page at a time with a GET `data-patch` form, checked by the smoke test ([#90 An answer cannot add rows to a list (Load more re-reads and re-renders every row shown)](https://github.com/todd-skelton/corten/issues/90)).
60. A route's query parameter is named as serde reads its field: `#[serde(rename_all = "camelCase")]`
    and `#[serde(rename = "..")]` rename it in links too (`?priceMin=`), so addresses already shared
    keep working without a hand-written `Params`. A renamed path parameter, or a name a URL would
    have to escape, does not compile (`compile_fail` doctests) ([#91 A query parameter cannot have a name other than its field's (#[route(query)] has no name =), so camelCase URLs need a hand-written Params](https://github.com/todd-skelton/corten/issues/91)).
61. A component several areas list (one screen at `/search` and `/categories/{slug}`) is in a
    stylesheet those areas share, `sections/search+categories.css`, linked from their pages only,
    instead of a startup error that sent it to `App::components` and every page ([#93 An area has one prefix, so a screen at two addresses (/search, /categories/{slug}) puts its stylesheet on every page](https://github.com/todd-skelton/corten/issues/93)).
    The runtime for 58 and 59 is 287 B larger gzipped (8,075 to 8,362 B, measured).
62. A handler's return type is what its route answers (`corten::answer`): `HtmlResult` (a page, a
    patch or a redirect, `Result<HtmlResponse>`; failures are the app's error page) or
    `JsonResult<T>` (every failure of the route is JSON, a path that does not parse included),
    and `Public<..>` around either for an answer a shared cache may keep (`Public::new(max_age,
    answer)`, `.stale_while_revalidate(..)`; only a success or a redirect is shared). A handler
    named in `#[corten::routes]` that returns `Response` or `impl IntoResponse` does not compile,
    and neither does a `Public` handler that takes a `Caller`, a `HeaderMap` or any input not
    marked `Anonymous`. `corten::live` returns `LiveResponse` and a feed's snapshot and stream
    `FeedResponse`.
63. `Caller<T>` (`corten::caller`): who is asking, found by `T: Identify` (a session cookie, an
    API key) only when a handler or middleware takes one, once per request. `Refused::sign_in`
    sends a browser to sign in (303, or `corten-location` for a patch; a JSON 401 for data and
    JSON routes), `unauthorized` and `forbidden` give the error page for 401 and 403, and
    `unavailable` a 503. Reading one makes the answer private and puts the caller in the page's
    key. `Option<Caller<T>>` runs the handler either way. `#[corten::routes(caller = Staff)]`
    holds every handler in the module to taking `Caller<Staff>` (a compile error at the handler
    otherwise). `examples/hello` reads its viewer this way and its account area says
    `caller = Viewer`.
64. `Middleware` and `App::middleware((RequestId, RateLimit))`: the app's own steps, run in order
    at one fixed place in every request (after the cross-site check, the deadline and the panic
    catch; before the handler, the error page, the security headers and `Cache-Control`). The
    tuple is resolved when the crate compiles; one that only passes the request on costs about
    230 instructions, where axum's `from_fn` costs about 700. `corten::middleware` lists the order.
65. Every route reaches the app through the same compile-time checks. `App::new` takes `Routes`,
    which a `#[corten::routes]` module's `router()` makes (`Routes::new()` for an app whose routes
    are all in areas and APIs), and `Area::route` and `Api::route` take `Methods`, which
    Corten's `get`, `post`, `put`, `delete`, `patch`, `head`, `options` and `trace` make
    (`corten::route`, the first four in the prelude), checking each handler as a routes module
    does: its result is an `Answer` and a `Public` answer's inputs are `Anonymous`. An axum
    `Router` or method router does not compile in any of them, so a route can no longer escape
    the answer, caller and cache checks by being registered by hand.
    The checks are at compile time: a page is 22,954 instructions after and 23,530 before, and a
    JSON 400 under `App::api` 28,268 after and 27,776 before (callgrind medians of three runs;
    the app guide's table has the rest).
66. `NoStore<A>`: an answer nothing may keep (`Cache-Control: no-store`), `NoStore<HtmlResult>`
    for a statement or a one-time code, in place of `Cache::no_store()`. A bare `StatusCode` is
    an answer too (`StatusCode::NO_CONTENT` for a write with nothing to say).
67. `#[derive(Fields)]` takes a struct with lifetimes or type parameters, so a query struct that
    borrows the page's values names its form's controls too, and `#[derive(Route)]` declares
    `#[serde(..)]`, so a link-only struct renames its parameters without a serde derive of its own
    ([#98 #[derive(Fields)] refuses a struct with a lifetime, so a borrowed query struct cannot name its form's controls](https://github.com/todd-skelton/corten/issues/98),
    [#99 #[derive(Route)] reads #[serde(rename_all)] but does not declare it, so a link-only struct must derive a serde trait](https://github.com/todd-skelton/corten/issues/99)).
68. `Route::at(id)`: a route with a place on its page, `/accounts/ada#review-7`, from the
    element's `#[derive(Id)]` type, for `req.redirect`, `req.moved`, `req.is_current` and
    `href={..}`, so a redirect after a write brings the browser back to what the user acted on.
    Text does not compile. A client navigation sent to a place gets `corten-location` (`fetch`
    drops a redirect's fragment), and the runtime fetches the page again when the server sends it
    to a URL that differs only in its `#id`, and scrolls to it (Firefox does not scroll to an
    `#id` the address already had). The runtime is 8,666 B gzipped, from 8,529
    ([#100 A typed route cannot carry a #fragment, so a redirect after a write cannot bring the browser back to the element it acted on](https://github.com/todd-skelton/corten/issues/100)).
69. `MonthStyle::{Short, Long}`: `t.date(at, MonthStyle::Long)` and `t.js().date(..)` write the
    month and year alone (`March 2025`) in each locale's order and words, checked against Intl
    like the other styles; `t.date_time` does not take one
    ([#101 t.date has no month-and-year style, so 'Member since March 2025' cannot be localized](https://github.com/todd-skelton/corten/issues/101)).
70. The error page takes extractors, as a handler does: `App::error_page(page, state)` takes
    `async fn(Failure, Req, ..) -> HtmlResult`, so it can take the caller
    (`Option<Caller<Viewer>>`) and draw the layout for who is asking. The lookup runs only for a
    failure, and an optional caller whose lookup fails is `None` there. A body extractor does not
    compile. Callgrind, per request: a page 21,100 -> 21,201 instructions, with a cookie 21,639 ->
    22,030, an unknown URL's error page 20,650 -> 26,122
    ([#88 An error page cannot show who is asking (App::error_page has no Caller), so a signed-in user's 404 is drawn signed out](https://github.com/todd-skelton/corten/issues/88)).
71. `Failure::Unidentified`: a caller whose lookup failed (`Refused::unavailable`) reaches the
    error page as its own case, not as a handler's 503 (`Failure::Failed`); an error page that
    names every case stops compiling until it says what to show
    ([#89 Refused::unavailable reaches the error page as Failure::Failed (503), the same as a handler's own 503](https://github.com/todd-skelton/corten/issues/89)).
72. `ReadAgain` reads a page at a place (`/saved#saved`, a write redirected to `route.at(..)`) again;
    it stopped after one read, since a navigation to the same address with a `#id` was taken for
    a scroll. The runtime is 8,670 B gzipped
    ([#107 ReadAgain never reads a page whose address has a #fragment, so a write redirected to route.at(..) stays on its catching-up state](https://github.com/todd-skelton/corten/issues/107)).
73. `corten new` writes `static/favicon.svg` (Corten's mark) and links it from the layout; the
    examples link the same icon.
74. `Public::shared(Duration)`: a shared cache's own lifetime (`s-maxage`) apart from the
    browser's, so a CDN can keep a slowly changing page for a day while a browser checks again
    after a minute. A response sent privately (a cookie, a client navigation) drops it with
    `public`
    ([#106 Public has no shared-cache lifetime (s-maxage) apart from the browser's, so a CDN cannot keep a page longer than a browser](https://github.com/todd-skelton/corten/issues/106)).
75. `ReadAgain { after }.from(&route)` in a patch: a `data-patch` write whose read is catching up
    answers in place, and the runtime reads the part again from `route` (`corten: action`) until
    it stops asking, instead of the handler waiting for the read or redirecting to a page only to
    show "saving". The reads stop when the part is replaced or the page changes, or when a read
    fails without a patch (the error page, offline), quietly: a handler that cannot read says so
    as a patch, which is applied whatever its status. `examples/hello`'s saved page has a form
    that does it. The runtime is 8,834 B gzipped
    ([#104 A data-patch answer cannot ask to be read again, so a write whose read is catching up has to navigate](https://github.com/todd-skelton/corten/issues/104)).
76. `App::task("refresh", |signal| async move { .. })`: work that lives as long as the app (a
    refresh loop, a queue consumer, a supervised child), started by `corten::serve`. Its `Signal`
    aborts with the new `Abort::Shutdown` when shutdown begins, and `serve` waits for it, up to
    `App::shutdown_grace` (5 seconds unless set), before it returns; one still running then is
    ended and logged by name, and so is one that panics. `App::router` starts none
    ([#116 Work that lives as long as the app (a refresh loop, a supervised child) has no signal, so shutdown neither stops it nor waits for it](https://github.com/todd-skelton/corten/issues/116)).
77. `#[derive(UrlValue)]` (in the prelude) on an enum of unit variants writes each variant as the
    word serde reads for it (its `#[serde(rename_all)]` and `#[serde(rename)]`), so an enum in a
    route's query or path needs no hand-written `Display` kept in step with serde. A variant with
    fields, one serde does not read, or a serde attribute it does not follow is a compile error; a
    test reads every word back through the query reader under each of serde's eight rules.
    `#[serde(..)]` values passed in by a `macro_rules!` are read by `ToJson` and `FromJson` too
    ([#129 A query value must be Display while it is read with serde, so an enum in an address spells its words twice](https://github.com/todd-skelton/corten/issues/129)).
78. `Area::deadline(..)` and `Api::deadline(..)`: routes slower than pages (an upstream that
    answers in minutes, a report, uploads) get a deadline of their own, or `None`, and every
    other route keeps `App::deadline`. In nested areas the innermost that sets one decides. The
    503 and `Abort::Deadline` are the same at every level
    ([#114 App::deadline is one value for the whole app, so one slow route (an upstream call, an upload) lifts it for every page](https://github.com/todd-skelton/corten/issues/114)).
79. `corten::serve_on(listeners, app)` serves an app on listeners of its own (anything that
    implements `axum::serve::Listener`: HTTPS with the app's TLS library, a Unix socket, a socket
    a supervisor hands over) with `serve`'s shutdown across all of them, and
    `corten::listeners(3000)` binds the ones `serve` uses (`HOST`:`PORT`, the loopback pair without
    `HOST`, the port given without `PORT`) for the app to wrap. `serve` is the two together, and
    an app with another default port serves `listeners(8320)` without setting `PORT` itself. The app guide shows HTTPS on
    loopback with `tokio-rustls`
    ([#115 corten::serve takes no listener, so an app that needs TLS or its own socket loses the loopback pair, the stream drain and graceful shutdown](https://github.com/todd-skelton/corten/issues/115)).
80. `corten::Elsewhere::at(&uri, "fee-calculator")`: a place on a page outside the app, which
    `req.moved` and `req.redirect` take, so a redirect to another site keeps its `#fragment`
    (an `axum::http::Uri` drops one it parses). The id is percent-encoded as the browser decodes
    it; a path in place of a full URL is a debug-build panic, since a place on the app's own page
    is `route.at(Id)`
    ([#123 An address outside the app cannot carry a #fragment, so req.moved and req.redirect drop the anchor of an external URL](https://github.com/todd-skelton/corten/issues/123)).
81. `t.percent(ratio, digits)` writes a ratio as a percentage in the locale (`2.9%`, `2,9 %`,
    `%2,9`), with at most `digits` fraction digits, and `t.js().percent(digits)` gives a script
    the same format (`@param {PercentFormat}`). The locale data carries each locale's percent
    pattern and grouping from CLDR, checked against Intl's `style: "percent"` with
    `maximumFractionDigits` on 40 values per locale (13 kept as the Rust and script tests' cases). The runtime grows by 99 bytes gzipped
    ([#119 i18n writes no percentage, so a rate like 2.9% cannot be localized](https://github.com/todd-skelton/corten/issues/119)).
82. `corten::ClientIp`: the visitor's IP address, for a handler that passes it to an API's
    per-client limit or limits by it. It is the connection's peer, or behind proxies the app lists
    (`Policy::proxy("10.0.0.0/8")`, `PROXIES`) the nearest `X-Forwarded-For` address that is not
    one of them; nothing reads the header otherwise. A `Public` handler cannot take it, and an
    answer that read it is private
    ([#120 A handler cannot read the client's address, so an app that calls an API for a visitor cannot pass it on for rate limiting](https://github.com/todd-skelton/corten/issues/120)).
83. `corten::form::Upload<T>` reads a form posted as `multipart/form-data`, which `Form` answers
    with a 415. It gives the files one at a time as they arrive: `next_file()`, then
    `chunk()` to pass a file on without holding it, or `bytes_up_to(max)` (`None` past `max`).
    Then `fields()` reads the text fields into `T` by `Form`'s rules. A file input is a `Files`
    field of the form struct, so markup, the files and `Errors` share its `FieldName`.
    `upload.limit(bytes)` bounds the whole body (2 MiB unless set; 413 past it). Types, counts
    and sizes per file are the handler's checks, reported as field errors. The parser is
    Corten's own, with no new dependency
    ([#140 A form cannot post a file: Form reads only URL-encoded bodies, so a multipart form is a 415 and nothing reads its files](https://github.com/todd-skelton/corten/issues/140)).
84. `#[corten::routes(api = ApiRoot)]`: an API's routes in a module of their own, each naming its
    handler as a page route does; the module gives `api(routes)` (`api(routes, state)` with
    `state = ..`) for `.api(prices::api)`. Only the prefix's route names no handler; a route with
    none, a route outside the prefix, or `area` and `api` together do not compile.
    `examples/item-page` serves its JSON and MCP endpoints from such modules and `bench/live` its
    publish route. The same JSON route costs the same either way: 21,913 instructions per request
    added by hand, 21,836 from its module (callgrind, median of three; the app guide has the
    ranges).
85. `Caller<T>` dereferences to `T`: `viewer.as_deref()` is the `Option<&Viewer>` a layout takes,
    without `viewer.map(|Caller(v)| v)`.
86. `.area(admin::area)` and `.api(api::api)` infer the prefix from the module's function; the
    `::<Admin>` turbofish is needed only for routes added by hand.
87. `corten::upgrade::Upgrade`: a route that switches its connection, as a WebSocket
    (`upgrade.websocket(run)`, the frames read by a library such as tokio-tungstenite) or a tunnel
    relaying an upstream's 101 (`upgrade.switch(headers, run)`). It is checked like any route
    (`Caller<T>`, the host check, the app's middleware), and its handshake as a write: another
    site's page gets a 403. The work runs on its own `Signal`, aborted at shutdown. Requests that
    do not switch run no new code (callgrind: 26,025,236 instructions for 1,200 requests in
    process, against about 26,030,000 on main). `corten::Relay` passes another server's answer on:
    its status, its headers less the hop-by-hop ones and its body as it arrives, never replaced by
    the error page or the JSON error
    ([#117 A route cannot be a WebSocket or pass an upstream response through](https://github.com/todd-skelton/corten/issues/117)).
88. An area with no page at its prefix: its module's prefix route names no handler
    (`#[route("/docs")] pub struct Docs;`) and is then only a `corten::Prefix`, with no `href`,
    so no link to the unserved URL compiles. `Area` and `Api` take any `Prefix`, which every
    `Route` is
    ([#152 An area with no page at its prefix (/items, /sets) must declare the prefix as a link-only route outside its routes module](https://github.com/todd-skelton/corten/issues/152)).
89. `Api::errors(|status, message| ..)`: the JSON every failure Corten answers under an API is
    written as, a `ToJson` of the app's, for an API that speaks another protocol's error shape
    (an Anthropic-compatible gateway). It covers unknown URLs, wrong methods, cross-site writes,
    the deadline, extractor rejections, `Caller` refusals and handlers' errors, keeping their
    headers; a handler's own JSON failure and a `Relay` pass through
    ([#163 An API cannot shape the failures Corten answers itself (404, 405, cross-site 403, Caller 401), so a gateway that must answer in Anthropic's error shape checks keys outside Caller](https://github.com/todd-skelton/corten/issues/163)).
90. `Option<Upgrade>`: `None` for a request that does not ask to switch, so one URL can be a
    WebSocket and a page (or a gateway's relayed answer); the handler returns `SwitchOr<A>`,
    whose failures are answered as `A`'s are. A request that asks is checked as `Upgrade`
    checks it. Routes that do not take it run no new code.
91. `corten::shutdown()`: the app begins the graceful shutdown a signal begins (tasks told, live
    streams ended, listeners closed, requests in progress finished, the grace), from a route a
    supervisor calls or a task. On Windows, `shutdown_signal` also ends on Ctrl-Break, the console
    closing and system shutdown, not only Ctrl-C
    ([#174 An app cannot begin its own graceful shutdown (a route, a task, a supervisor's request), and on Windows only Ctrl-C starts one](https://github.com/todd-skelton/corten/issues/174)).
92. `App::client_zone()` and `Option<ClientZone>`: a handler knows the visitor's time zone (the
    browser's IANA name, kept by the runtime in a `corten-zone` cookie from the first page it
    loads), so a picked date becomes its instant where the user is with no script per form, and a
    page writes times in their zone with the app's time zone library. `None` until the runtime has
    run; a bare `ClientZone` and a `Public` handler taking it do not compile. That cookie alone does
    not make a public page private. The runtime is 78 B larger (gzip)
    ([#149 A handler cannot know the visitor's time zone, so a picked date needs a script to become an instant and every date is written in UTC](https://github.com/todd-skelton/corten/issues/149)).
93. `Refused::message` and `Refused::too_many`: a `Caller` refusal tells a client that reads the
    failure why ("Invalid API key", in the API's error shape) and can be a 429 with `Retry-After`
    for too many failed attempts, so an API key check stays in `Identify`
    ([#177 A Caller refusal cannot say why (Invalid API key) or be a 429 for too many failed attempts, so an API key check stays outside Caller](https://github.com/todd-skelton/corten/issues/177)).
94. A `var(--x)` without a fallback in a listed stylesheet that no stylesheet on the same pages
    declares (the token file, a component's, a `.css` file in the static directory, an area's for
    its own pages) fails `App::router` and names the file and the property, so a misspelled
    token is not a wrong color found in a browser. Checked at startup, about 0.25 ms for 21 KB of
    CSS, since a component's stylesheet compiles without the theme
    ([#179 A stylesheet can use a token the theme does not define (var(--color-primary-contrast)), and nothing reports it until a browser draws the wrong color](https://github.com/todd-skelton/corten/issues/179)).
95. `Error::not_found()`: the 404 a handler answers for a record that is not there,
    `load(id).await?.ok_or_else(Error::not_found)?`, in place of
    `.ok_or_else(|| Error::status(StatusCode::NOT_FOUND))?` (or an app's own `fn not_found`).
96. `HtmlResponse::with_cookie(SetCookie)`: an answer that sets a cookie,
    `Ok(req.redirect(&url::Home)?.with_cookie(SetCookie::new("session", &token)))`, in place of
    `res.headers_mut().append(header::SET_COOKIE, cookie.header())`. The cookie is built by
    `SetCookie`, so its safe flags are the default.
97. `Req::patch_or(view, page)`: a `data-patch` form's answer both ways, the patch with JavaScript
    and `page()` (the page or a redirect) otherwise, in place of
    `if req.mode == Mode::Action { return req.patch(view); }`. Like `Req::invalid`, the page's
    closure runs only without JavaScript, so a patch never loads what only the page shows.
98. `corten::live_watch(values, since, cursor, view)`: a live stream of one value in a
    `tokio::sync::watch` channel, from where the page is: nothing until the next change when it
    shows the current version, the current value first when it is behind, then the latest after
    each change. It replaces about ten lines of `mark_changed`, `unfold` and `Fragment`.
99. `tokens!` gives each utility value a function named after its class: `mx_auto()` is `mx-auto`
    and `gap_3()` is `gap-3`, in place of `mx(Mx::Auto)` and `gap(Space::_3)`. The utility's
    function stays for a value chosen as the page renders and for one per breakpoint. Two that
    would share a name (utility `border-b`, and `border`'s value `b`) are a compile error that
    names both. The watchlist's home page renders in 1.1% fewer instructions (62,827 to 62,162).
100. A component can take an element id as a prop of any id type (`fn ConfirmDialog<I: Id>(id: I)`)
    and print it with `id={id}` or `commandfor={id}`: `Id` now requires `AttrValue` and `Render`,
    which every `#[derive(Id)]` type has. seller-desk opens its archive dialog with
    `command="show-modal"` and no script, sorts and filters its rows by the row view's fields
    (one handler per column) instead of cell positions, writes its unsaved-prices note from the
    catalog, names the prices' currency in their column, and right-aligns the statement's numbers.
    A row is 7 bytes smaller (603 to 596 at 2,000 rows) and about 30 instructions dearer to render
    (its name is now also a whole attribute value, the hole the sort reads).
101. `SetCookie::try_new(name, value)` returns `Err(InvalidCookie::Name | Value)` for a character a
    cookie cannot carry, where `new` panics: a token another service answered with no longer turns
    a sign-in into a 500 the app did not choose. `new` keeps its panic for values the app makes
    and calls the same rule
    ([#204 SetCookie::new panics on a value it cannot carry, so a token from another service can turn a sign-in into a 500](https://github.com/todd-skelton/corten/issues/204)).
102. A return address from outside the code: `corten::Local::parse(text)` keeps a path on this
     site only (one leading `/`, not `//` or `/\`, printable ASCII without `\`, so a control
     character a browser drops cannot make it another host, no `.` or `..` segment, and no
     percent-escaped letter, digit or `-._~` in the path, which Chromium would send unescaped), and
     `req.redirect(&local)` takes it. `req.serves(&local)` says whether the route the router picks
     for its path (the app's own or an area's, not an API's or Corten's) has a `GET` handler, so a
     sign-in no longer checks a return address against a list of prefixes kept by hand. A `Route`
     written by hand and given to `Area::route` must have a path of the shape `#[derive(Route)]`
     writes (whole-segment parameters, plain text), or the app does not compile
     ([#203 An app cannot ask whether it routes a path, so a sign-in's return address is checked against a hand-kept list of prefixes](https://github.com/todd-skelton/corten/issues/203)).
103. `#[derive(FromRef)]`, in the prelude: each field of the app's shared state gets axum's
    `FromRef`, so a handler takes `State<Market>` without a hand-written impl per part. Built on
    `proc_macro` alone (axum's derive needs its `macros` feature); two fields of one type are a
    compile error until one is `#[from_ref(skip)]`. The watchlist and card-market use it.
104. `req.nav(&route)` and `req.nav_section(&route)`: a nav link from one mention of its route,
    `<a {..req.nav(&url::News)}>"News"</a>`, writing `href`, `data-nav` and
    `aria-current="page"` on its own page. The route's URL is made once, written and compared,
    where `href={..}` and `aria-current={req.current(&..)}` made it twice; a tag that also writes
    one of those attributes, or `data-nav` by hand, does not compile. The watchlist and
    seller-desk use it.
105. `t.parse_money(text, currency)`: money as a person types it in the reader's locale, read
    back into `Minor` units, what `t.money` writes: `$1,299.50` is `Some(Minor(129950))` in
    en and `1 299,50 $US` or `12,50` in fr, with or without the symbol, the code, the group
    separators or the fraction (every amount every locale writes reads back, as it is written
    and without its direction marks and no-break spaces). A sign, a group of the wrong size, a
    grouped number that starts with `0` (`0,500`), another locale's decimal
    separator, more fraction digits than the currency shows (`1.234` dollars, `1.5` yen) or an
    amount over `u64` is `None`. The watchlist's and seller-desk's hand-written cent readers are
    gone, and card-market's price alert no longer reads a float: `12,50` on the French page was
    refused and `1e3` was a thousand dollars, and its confirmation is in the reader's locale
    (`alert.set`)
    ([#208 Money typed into a form is parsed by each app, three different ways, and card-market reads it as f64](https://github.com/todd-skelton/corten/issues/208)).
106. A JSON answer with a status or a cookie of its own, and JSON the app already holds:
    `Json(value).with_status(StatusCode::ACCEPTED)` and `.with_cookie(SetCookie)` make a
    `JsonResponse` (the handler returns `Result<JsonResponse, JsonError>`), whose status and body
    reach the client as written, a `503 {"error":..}` of the app's own included; and
    `corten::json::Raw::new(bytes)` is JSON held as bytes (checked once to be one JSON value, an
    `InvalidJson` error otherwise), which `Json(raw)` sends with no serialization or copy and
    which a value `Json` writes copies in as it is. With `compress`, a `Raw` sent again is
    compressed once (brotli 9 and gzip 9, on one background thread) and later reads
    send those forms: on an 80 KB document, 15 ms once instead of 220 µs (zstd 1) to 2.1 ms
    (brotli 2) on every read, and 5,000 bytes instead of 6,992. Apps no longer write their own
    `Answer` for these. A held 80 KB document sent in its stored form costs about 24,100
    instructions a request (callgrind), where one compressed on each read costs 1.41 million.
    Every other answer reads `Accept-Encoding` once for both codings, as bytes, and costs what it
    did: 22,357 to 22,252 instructions for a small JSON answer, 99,935 to 99,940 for an 80-row
    one, 22,708 to 22,602 for text (two runs each against main)
    ([#201 A JSON route cannot answer with a status other than 200 or send JSON it already holds as bytes, so each app writes its own Answer](https://github.com/todd-skelton/corten/issues/201),
    [#234 A JSON answer cannot set a cookie the way an HTML answer can](https://github.com/todd-skelton/corten/issues/234)).
107. A link to an element on the same page takes its `Id`: `href={FeedbackId.fragment()}` writes
    `#feedback`, encoded as `route.at(..)` encodes another page's place, so a validation
    summary's link and the field it names cannot drift apart. It is an attribute value
    (`view::InPage`), not an address: `req.redirect` to it does not compile, and neither does
    text in place of the id. Nothing `html!` writes changed
    ([#254 A link to an element on the same page cannot take an Id, so each app writes its #fragment by hand](https://github.com/todd-skelton/corten/issues/254)).
108. `corten_ui::dialog::modalClosedBy(target, close, signal)`: a dialog the address opens
    (`?selected=it_1`) is made modal as `dialog::modal` makes one, and a close request on it
    (Escape, Android's Back) goes to its close link's address (a ref; a button there does not
    compile) without a new history entry, back when the entry before is the list, so a reload or
    a redraw no longer opens what the user closed and Back cannot loop through it. A
    `cancel` from inside it (a file input's picker) is left alone, and without a script nothing
    changes. ui-gallery's last-sale sheet is tested in Chromium, Firefox and WebKit. Apps' own
    `cancel` scripts can go
    ([#257 corten-ui's dialog::modal closes a dialog the address opened without following its close link, so Escape leaves ?selected= behind](https://github.com/todd-skelton/corten/issues/257)).
109. `corten::Media`: bytes the app holds, sent as a media type the app names
    (`Media::new("image/png", bytes)`; the type is one the app names, read from the bytes, and
    is never the one a client sent with an upload). `Public::immutable(media)` is for a URL that
    names its content: every cache keeps it for a year, cookies or not, as Corten's versioned
    files are. It takes bytes only (a page or JSON changes with the deploy, so it does not
    compile), and its handler is held to `Public`'s rule, so one that could tell who is asking
    does not compile. Before it, a route could serve bytes it held only as an axum `Response`,
    which a route cannot name.
110. `corten_ui::CopyButton`: a button that copies an element's text (an order number, an API
    key's one-time secret; a read-only field's value). The element is named by its
    `#[derive(Id)]` id (`<code id={SecretId}>`, `<CopyButton text={SecretId} label={..}
    copied={..} />`), so the text is on the page once and a button in a list copies its own row's;
    text in its place does not compile. The button is drawn `hidden` and shown where the browser
    has the Clipboard API (a secure origin); after a write it reads the app's `copied` for two
    seconds in a polite live region, and nothing to copy or a failed write changes nothing. Tested
    in ui-gallery in Chromium, Firefox and WebKit: a copy (the clipboard's contents in Chromium,
    which lets a test read it), a denied write, no Clipboard API, and no script. Apps' own copy scripts can go
    ([#244 corten-ui has no copy-to-clipboard control, so each app ships its own script for Copy](https://github.com/todd-skelton/corten/issues/244)).
111. `corten::serve_wrapped(listeners, move |tcp| acceptor.accept(tcp), app)`: `serve_on` with a
    step of the app's own on each connection before HTTP, a TLS handshake. The step runs in the
    connection's own task, counts against `App::max_connections` from before the TCP accept, and
    is closed at `App::header_timeout` when the client stalls before or after it; shutdown ends
    the steps in progress. Before, an app serving HTTPS ran the handshake in a listener of its own
    (the guide's 45 lines), outside both limits, so clients that never sent a ClientHello held
    sockets and tasks with no cap. The library and its configuration (ALPN `http/1.1`) stay the
    app's; the guide's example is four lines. Checked with rustls: HTTPS answers, ALPN is
    `http/1.1`, and a client that sends nothing is closed at the header timeout. A plain
    connection costs about 340 instructions more to open: 27,528-27,543 to 27,841-27,910 for a
    connection with one request, client included (callgrind, in process, three runs each).
    Keep-alive requests cost what they did: 13,827-13,850 to 13,837-13,838
    ([#263 An app serving HTTPS copies the guide's TLS listener, and its handshakes are outside max_connections and header_timeout](https://github.com/todd-skelton/corten/issues/263)).
112. `Area::policy(|p| p.script_src(..).frame_src(..))`: an area's pages add to the app's
    Content-Security-Policy, so a payment provider's scripts and frames, or style attributes for
    sizes drawn from data, are allowed on the pages that need them and not on every other page.
    It only adds (`security::Sources`), nested areas add up, and each area's header is built
    once at startup. A client navigation into or out of such an area loads the page in full, since
    a document keeps the policy it loaded with; an area that adds only what the app has keeps the
    app's. A request finds its area's policy once, by path, and only when some area adds to the
    policy. Per request (callgrind, in process, 1,000 requests, median of three runs, range in
    parentheses), a page outside the area went from 19,695 (18,936-19,698) to 19,095
    (19,095-19,407) with no area policy and 19,122 (19,020-19,728) with one, and a page in the
    area from 19,792 (19,786-20,696) to 20,042 (20,018-20,042), and 20,511 (20,083-20,733) under
    its policy. The same build moves by up to 760 between runs, so only the last, about 500 to
    700 more for a page under an area's policy, may be more than noise
    ([#275 A policy is the whole app's, so the origins one page's payment SDK needs are allowed on every page](https://github.com/todd-skelton/corten/issues/275),
    [#139 A size or position drawn from data needs style attributes, and allowing them opens style-src-attr 'unsafe-inline' for the whole app](https://github.com/todd-skelton/corten/issues/139)).
113. `corten_ui::Copied` (`ui-copied` in the page): `CopyButton` dispatches it after a write the
    browser made, and only then, so an ancestor counts a copy (a referral link's, a share link's)
    with `on:Copied={js::counted(event)}` instead of keeping a copy script of its own. It bubbles
    and carries nothing, since the text may be a secret. ui-gallery counts its SKU's
    copies: one for a copy, none for a denied write. `copy_button.js` is 2,457 bytes, 904 with
    brotli (was 2,223 and 844), and a page with the button fetches `events.js` with it, preloaded
    beside the script so it adds a request but no round trip (276 bytes, 176 with brotli, in
    ui-gallery)
    ([#294 CopyButton tells the app nothing when it has copied, so a copy that is counted keeps its own copy script](https://github.com/todd-skelton/corten/issues/294)).
114. A time relative to now: `t.relative(at, now, RelativeStyle)` writes "in 10 hours" or "3 hr.
    ago" in the request's locale (each locale's plural forms and words from CLDR, checked against
    Intl), and `corten_ui::RelativeTime` keeps it current in the browser on one timer for the
    page, so a live stream no longer redraws every second or resends its part each minute to move
    a countdown. Each locale an app lists adds about 2.3 KB of pointers and 0.4 to 3 KB of text;
    the script is 1.8 KB brotli (5.3 KB as written; component scripts are not minified), on pages
    that use it
    ([#202 A page cannot show a time relative to now that stays current, so a live stream redraws every second and resends the whole part each minute to move a countdown](https://github.com/todd-skelton/corten/issues/202)).
115. `Public::header::<XRobotsTag>(value)`: a header of the app's on any public answer (text,
    `Json`, `Media`, a page), named by a type (`corten::answer::Header`; Corten names
    `XRobotsTag`, `Link` and `ContentLanguage`). The answer is compressed
    and shared as it would be without it. Before, a shared file that had to stay out of search was
    a `Relay`, never compressed (Chase Sets' developer manifest: 161,815 bytes against 19,737 with
    brotli), or an answer type of the app's own. `set-cookie`, the headers Corten sets itself
    (caching, a CDN's `*-cache-control`, CORS, security), those the answer has (`content-type`,
    `location`, `content-disposition`) and the connection's do not compile. A public answer that adds none costs about 30 instructions more per request,
    18,404-18,417 to 18,434-18,459 for a public text answer (callgrind, in process, 1,000
    requests, three runs each); the headers are held behind an `Option<Box<..>>`, since a `Vec`
    in place cost about 50 (18,450-18,467)
    ([#287 A Public answer that is not a page cannot take a header of the app's, so a noindex file is an uncompressed Relay or the app's own answer type](https://github.com/todd-skelton/corten/issues/287)).
116. `Params<R>`: a route's handler reads the route's own parameters back, `Params<url::Item<'_>>`.
     It has the struct's fields, owned (a `&'a str` or `Cow<'a, str>` is a `String`, a `&'a [T]` a
     `Vec<T>`, any other `&'a T` a `T`; a link's field still needs only `Display`), read
     the way a link writes them: a path parameter percent-decoded, a query parameter by the name
     serde reads (`rename`, `rename_all`; `None`, empty or the field's `#[serde(default)]` when the
     link leaves it out), a `#[route(query)]` set from the whole query, a `#[route(prefix = ..)]`
     map from its prefixed keys. A request whose parameters do not fit is a 400
     (`ParamsRejection`). A route with a serde key links do not follow (`alias`, `flatten`,
     `skip`, `with`, `default` on the struct and the like) or a borrowed `#[route(query)]` set
     still writes links, but taking its `Params` does not compile. In a
     `#[corten::routes]` module, a handler of a route with a parameter in its path that does not
     take its own `Params` (one that reads the path with axum's `Path`, or another route's
     `Params`) does not compile (`compile_fail` doctests). On the request path it costs what
     `Path<String>` did: the item page 82,041 to 81,975 instructions per request (the mean of three
     runs on main, which spread over 425, and two here) and its JSON 57,687 to 57,680 (callgrind,
     release, the benchmark fixture) ([#207 A handler reads its path parameters with Path<String>, so nothing checks them against the route struct that declares them](https://github.com/todd-skelton/corten/issues/207)).
117. `corten_ui::CheckboxGroup`: checkboxes of one name of which at least one must be checked
    (`<CheckboxGroup legend=".." missing="Choose where you ship.">`). While none is, its script
    keeps the first enabled box invalid with the app's message, so the browser stops the submit and says so
    at the group, as for a required text field; HTML's `required` on a checkbox means that one box.
    Without the script the form is sent and the handler refuses it, as before. 914 bytes, 379
    with brotli, on the pages that use it
    ([#272 A group of checkboxes cannot require one to be checked without a script, so a form that needs a choice is sent with none](https://github.com/todd-skelton/corten/issues/272)).
118. `corten_ui::RequiredWhen`: fields required only for some values of another field (a refund
    amount for a partial refund), `<RequiredWhen control={f.kind} values={&["partial"]}>`. The
    control is a `FieldName`, so a misspelled one does not compile, and its values are a
    `string[]` parameter, which a handler call now checks and fills from a slice, an array or a
    `Vec` of strings in every build (before, only as an unchecked JSON value with feature `json`). While the control has one of
    the values every field in the set is `required`; for any other the set is disabled, so it is
    neither checked nor sent; an app hides it with `:has` on the control, which holds before the
    script runs. Without the script the
    fields are optional and the handler checks the rule. 1,906 bytes, 678 with brotli, on the
    pages that use it. ui-gallery's listing shows both (where it ships, and the lowest offer when it
    takes offers), and its handler refuses what the browser would have stopped
    ([#269 A field required only for one choice of another field needs a script, so the browser cannot check it before the form is sent](https://github.com/todd-skelton/corten/issues/269)).
119. corten-ui's `PrintButton` opens the browser's print dialog. It is drawn hidden and its script
    shows it, so without a script there is no button that does nothing, and the browser's Print
    command still prints the page. Its script is 736 bytes (too small to compress), on the pages
    that use it. The styles guide's new Printing section covers paper sizes: named `@page` rules
    in a component's own stylesheet, scoped `@media print` rules, and layout chrome that hides
    itself. **Breaking:** in a component's stylesheet an `@page` with no name (which sets every
    page the app prints) does not compile, and a page's name starts with the component's
    (`@page packing-slip-thermal`), so two components cannot set one page
    ([#311 corten-ui has no Print button, and the guide does not say how a page prints at its paper size](https://github.com/todd-skelton/corten/issues/311)).
120. An area can loosen `Cross-Origin-Opener-Policy` for its pages only: `Sources::allow_popups()`
    (`same-origin-allow-popups`) and `Sources::allow_cross_origin_opener()` (`unsafe-none`, which
    Stripe's Connect embedded components need for their sign-in popup), and
    `Policy::allow_cross_origin_opener()` for the whole app. The rest of the app keeps
    `same-origin`, and a page under another opener policy has its own shell key, so a client
    navigation into or out of the area loads the document in full
    ([#318 An area cannot relax Cross-Origin-Opener-Policy, so a page that embeds Stripe Connect breaks its sign-in](https://github.com/todd-skelton/corten/issues/318)).
121. `req.nav_of([(NavTo::section(&url::Desk), "Seller Desk"), (NavTo::page(&url::Money), "Money"), ..])`
    writes a whole nav's links, each with its item, and marks only the deepest entry that holds the page, for a nav whose entries nest or
    come from data (an entry is a route or a `Local`, whose path is written as the browser asks
    for it). Its links name their group (`data-nav-group`), and the runtime ranks the same links
    after a client navigation wherever the markup puts them; a `req.nav` link is still judged
    alone. `corten.js` grows 80 bytes gzip
    ([#341 A nav built from data, or whose current entry is the deepest that holds the page, keeps the first page's aria-current after a client navigation](https://github.com/todd-skelton/corten/issues/341)).
122. `ReadAgain { after }.without_javascript()` asks only a browser without JavaScript to read the
    page again: for a page whose changing part a `<corten-live>` relays while a job runs, which
    with JavaScript keeps itself fresh, and which reading whole would only make open its stream
    again. It writes the `<noscript>` refresh without the mark the runtime reads; the runtime
    is unchanged
    ([#319 A page relayed by <corten-live> cannot ask a browser without JavaScript to read it again](https://github.com/todd-skelton/corten/issues/319)).
123. A script that imports another script module of its directory (`import { loadStripe } from
     "./stripe.js"`) has it preloaded on the pages that load it, with what that one imports, so the
     browser fetches them together instead of one round trip after the other. The tag is made
     when the app starts, so a page costs nothing more to render: one import over a 100 ms
     round trip ran at 245 ms instead of 342 (Chromium, median of 10 cold loads, a local server
     delaying each answer 100 ms). An import of a module the app does not list is a startup
     error that names both (it was a 404 in the browser). The preload helps a full page load; a
     client navigation still fetches a new module's imports after it. The scripts guide describes
     relative imports
     ([#307 A script's relative import of another script module is not preloaded, so the page waits a round trip for it, and the guide does not describe it](https://github.com/todd-skelton/corten/issues/307)).
124. Middleware for part of the app: `Area::middleware` and `Api::middleware` run after the
     app's middleware for each request at the area's or API's prefix (an outer area's before an
     inner one's, then an API's under it, each in the order given), so a rate limit picks its
     routes by where they are, never by a path the middleware compares. A middleware refuses with
     a `Refused` (`Refused::too_many(wait).into_response()`), answered in the request's mode as a
     `Caller`'s refusal is, `Retry-After` included, and the default error page says "Too many
     requests" for a 429. It reads the client's address with `ClientIp::of(&mut req)` without
     splitting the request. card-market's write limit is now on its item pages' area and its
     partner API, with no path checks
     ([#327 Middleware is app-wide only, so card-market's write limit picks its routes by comparing paths and builds its 429 by hand](https://github.com/todd-skelton/corten/issues/327)).
     Measured per request (callgrind, release profile, 5 runs): an app whose areas and APIs have
     none runs 19,976 to 19,988 instructions against main's 19,696, all of it in glibc's
     allocator: the allocations are the same (count and sizes, DHAT) and Corten's own code is
     within 18 (13,971 against 13,953). A request in an area with a pass-through middleware runs
     21,374 to 21,511 against 20,497 to 20,646 with none (one boxed future), and one outside it
     19,909 to 19,913.
125. `t.change(ratio, digits)` and `t.js().change(digits)`: a ratio as a change (a price's move
    today), with exactly `digits` fraction digits and a sign on a gain as on a loss, none on what
    rounds to zero (`+0.60%`, `-0.30%`, `0.00%`; `+0,60 %` in fr, `+%0,60` in tr), as Intl's
    `signDisplay: "exceptZero"` writes it, so a column of them lines up. Each locale's data gains
    the template for a gain; the script's format is a `PercentFormat` with that third template,
    and `scripts/format.mjs` checks it on the same cases as the server (Intl wrote them). watchlist's
    Today column uses it. `change`, `percent`, `parse_money` and `relative` join the message keys
    `messages!` refuses at the top level, since they are its methods (`button.change` is fine)
    ([#331 Polish: watchlist's phone header wraps, and the Today column changes its decimals](https://github.com/todd-skelton/corten/issues/331)).
126. `Media::attachment(name)` and `Media::inline(name)`: a download names its file on any answer,
    private ones included (a user's export), where only a `Public` answer could before. The name
    is written as `filename="cards.csv"` when it is plain ASCII, and otherwise with
    `filename*=UTF-8''..` (RFC 6266) after a `filename` with `_` for each other character, so a
    quote, a backslash, a `%`, a `?` or a line break in a user's file name cannot end the header,
    add a parameter or be decoded as another name. It is the one way to name a download:
    `Public::header` refuses `content-disposition` when the app builds (Added 115). The guide
    says to link to one with `download` (or `data-reload` for a file the browser shows), so the
    runtime does not fetch it on hover and again on click
    ([#364 A private download cannot name its file: Content-Disposition is only on Public answers](https://github.com/todd-skelton/corten/issues/364)).
    No existing answer changes; naming a file costs one header on that answer.
127. `Signal::finish(|signal| async move { .. })`: work a handler must finish (claim, call,
    release) runs to its end even if the handler is dropped, with a signal of its own that aborts
    when the request's does or shutdown begins, so a slow step stops early and the release after
    it still runs; `serve` waits for it at shutdown within `App::shutdown_grace`. The cancellation
    guide no longer says awaited work needs no thought: a drop cannot run async cleanup
    ([#372 A handler dropped when its client leaves cannot release what it holds, and the guide says work it awaits needs no signal](https://github.com/todd-skelton/corten/issues/372)).
    A request that does not call it runs no new code: the app's `Shared` gains one pointer, last,
    read only by `finish`. A plain GET through the router took 16,673 to 17,022 instructions
    before and 16,864 to 16,866 after (callgrind, release build, 5 runs each); the before runs
    vary more than the two differ.
128. `examples/auction-room`: a live card auction. Signed-in people bid on the same lots at once;
    bids are kept in SQLite and checked in one transaction (refused below the price and the step,
    or after the end, and written once per submit key), every page watching a lot sees it change,
    and the outbid bidder alone is told, on every page they have open. Its `Identify`s refuse with
    `sign_in`, `forbidden` (a bidder at the house's desk, the house bidding) and `unavailable`
    (the store is down); its bid and sign-in limits are areas' middleware refusing with
    `too_many`; it works without JavaScript; and its smoke has two bidders in two browsers bidding
    against each other
    ([#337 No example shows several signed-in users in real time with stored data: a live auction room](https://github.com/todd-skelton/corten/issues/337)).
129. `on:document:keydown={js::shortcut(event, ..)}`: a component's handler for an event anywhere
    on the page while the component is on it, so a screen's shortcuts work wherever focus is
    (the page's background, a button) without `document.addEventListener`. It runs after the
    handlers on the event's path, each as its own listener on the document (one stopping the
    event does not keep it from another's), its arguments are read for each event and typed by
    the event (`KeyboardEvent`), and it ends when its element leaves the page. An event that
    does not bubble to the document (`focus`, `mouseenter`), `connect` and a typed event do not
    compile, so a page-wide handler that would never run is a build error. watchlist pauses its
    prices on `p`
    ([#365 A component cannot take keys on the whole page, so work-screen shortcuts call document.addEventListener outside the handler model](https://github.com/todd-skelton/corten/issues/365)).
    A page without one checks one empty set per bubbling event it listens for.
    The minified runtime grows by 280 bytes (25,133 to 25,413), 84 gzipped (9,165 to 9,249)
    and 61 with brotli (8,287 to 8,348).
130. A feed's `snapshot` handler gets the feed's generation as a third argument,
    `snapshot(rows, seq, epoch)`, so a script that uses `seq` as a version can tell a restarted
    feed's `seq` from its own (`docs/guide/live-data.md`).
131. `examples/deck-builder`: a rich client-side screen. A player builds a 40-card deck from a
    collection of 240 by Add, Add selected, drag and drop or the keyboard (Space to pick up, the
    arrows, Space to drop, Escape to put back, each said aloud), with the count, the cost curve and
    the kinds drawn at once, undo and redo (buttons, Ctrl+Z and Ctrl+Shift+Z through
    `on:document:keydown`), and filter and sort in the page. Every change is saved as it is made,
    one save at a time, with the deck's version (its feed's generation and number): a save the rules refuse is rolled back with the
    reason, a stale one (another tab saved first, which its feed shows) lets the player keep both
    changes or take the other tab's deck, and a save that could not be sent, or whose answer was lost, is sent again with its submit key.
    Without JavaScript the same page posts the deck, and the filter and sort are links and a GET
    form ([#336 No example shows a rich client-side screen: a deck builder](https://github.com/todd-skelton/corten/issues/336)).
132. A navigation marks the link or GET form that started it `aria-busy="true"` for as long as it
    marks `<main>`, as a `data-patch` form already was, so a page shows which control is working
    with CSS alone (`docs/guide/requests-and-forms.md`). Both marks now last until the new page is
    swapped in (`<main>`'s ended when the answer's headers came) and stay through a redirect;
    when another navigation replaces one, the first one's marks go at once and the newer one's stay
    ([#422 A navigation marks only main as busy, not the link or GET form that started it, so a page cannot show which action is on its way without a script](https://github.com/todd-skelton/corten/issues/422)).
    The minified runtime grows by 374 bytes (25,510 to 25,884), 124 gzipped (9,278 to 9,402) and
    115 with brotli (8,381 to 8,496).
133. `t.signed_money(amount, currency)` and `t.js().signed_money(currency)`: an amount with its
    sign, as Intl's `signDisplay: "exceptZero"` writes it (`+$3.20`, `-$1.23`, `$0.00`, `+3,20 $US`),
    for a difference between two amounts, the same on the server and in scripts. It uses the
    locale data already shipped: a gain is the loss's template with the plus sign in place of the
    minus, which `scripts/locale-data.mjs` checks against Intl for every locale and currency
    (`docs/guide/views.md`, `docs/guide/scripts.md`). `examples/watchlist` shows today's move in money
    beside the percent, written by the server and kept current by its script. `t.money` itself is unchanged: about 934
    instructions per amount before and after under callgrind (three runs each of 1,200 less 200
    formats; 931.5 to 934.7 before, 933.8 to 933.9 after). `format.js` grows by 48 bytes
    minified (2,487 to 2,535) and 20 gzipped (1,240 to 1,260)
    ([#419 Money cannot be formatted with its sign (+$1.23), on the server or in scripts, so a difference between amounts hand-builds the sign twice](https://github.com/todd-skelton/corten/issues/419)).
134. `#[derive(Href)]` on an enum whose variants each hold a route (an app's screens) makes it a
    link, `href={screen}`, and a component several pages draw takes where its links and forms go
    as `&dyn Href`, so each page passes its own typed route instead of a `String`; a unit variant
    and text where a `&dyn Href` goes do not compile. A `Local` or `Uri` written this way is
    escaped for the attribute. The views guide says how a shared list takes its links and a
    per-row part. A redirect costs what it did: a redirect to a `Local` measured 19,241-19,260
    instructions before and 19,084-19,228 after, one to a route 18,110-18,277 and 18,258-18,316
    (callgrind, 3 runs each)
    ([#361 A value that is one of several routes (an enum of screens) implements Href but cannot be written as href={..}](https://github.com/todd-skelton/corten/issues/361), [#391 A component several pages render cannot take a route as a prop (Href is not dyn-compatible), so a shared list's form action and column links are Strings](https://github.com/todd-skelton/corten/issues/391)).
135. `Sources::referrer(Referrer::NoReferrer)` (also `StrictOrigin`, `SameOrigin`) makes an area's
    `Referrer-Policy` stricter, so a page whose address carries a secret (a consent token, a reset
    link) does not send it with the requests it makes; `Policy::referrer` does it for the app. There
    is no looser value; nested areas send the least any of them allows, and a changed policy gives
    the area its own shell key, and an `App::api` under the area sends it too. An answer that brings
    a policy of its own (a relayed redirect) sends the least of its and the area's, its lines read
    as the browser reads them. Choosing the header per answer, and reading every line
    of one the answer brought, costs a plain GET about 75 instructions (20,351-20,358 to
    20,427-20,432 under callgrind, 3 runs, about 0.4%)
    ([#390 A page or a relayed answer cannot choose a stricter Referrer-Policy: Corten replaces it on every answer](https://github.com/todd-skelton/corten/issues/390)).
136. `corten::Within` is exported beside `Errors`, so a field helper several forms share takes the
    form's scoped errors without reaching into `corten::form` ([#451 form::Within is not re-exported
    beside Errors, so a field helper shared by several forms must find
    corten::form::Within](https://github.com/todd-skelton/corten/issues/451)).
137. A submit refused for the state it was made against, not its input (a stale revision, a step
    already taken), is `errors.refuse(Refusal::Conflict, message)`: `req.invalid` answers it with
    409 (or 410 for `Refusal::Gone`) in every mode, the form drawn again with what the user typed
    and the message at its top, and data clients get the same status in the Problem Details
    document. `Refusal` is a closed set of 4xx refusals, so a 2xx or a 5xx does not compile ([#438 A
    post refused for a reason that is not a field (a stale revision, a step out of order) has no
    answer, so each screen draws its page and sets a 409 on the raw
    response](https://github.com/todd-skelton/corten/issues/438)).
138. `PageOr<Media>` and `PageOr<Relay>`: a route that sends a file answers the page, or a redirect,
    instead when it refuses (a download posted at a stale revision), each `.into()` the answer,
    private like any page and `Public` with a page made for everyone
    ([#415 A handler that answers a file cannot answer a page or a redirect when the file is refused](https://github.com/todd-skelton/corten/issues/415)).
139. `#[route("/{*rest}", any = relay)]`: one handler for every method of a route, those the app
    never names included (`OPTIONS`, `PROPFIND`), for an app moved onto Corten a slice at a time
    that relays the rest to its old server, which then answers them instead of Corten's 405. It is
    the route's only handler (naming another method beside it does not compile). A route with a
    parameter still holds its handler to taking its `Params`: one relay for several catch-alls is
    generic over the route (`async fn relay<R: FromUrl>(_: Params<R>, req: Request)`), and the
    compiler picks the route at each name, so no wrapper per route with a parameter is needed
    (a route without one has no `Params` and takes a handler without them). A path the app
    routes itself keeps only the methods it names. Routes without `any` route and cost as
    before
    ([#360 A catch-all route's handler must take that route's Params, so one relay cannot serve several routes or every method](https://github.com/todd-skelton/corten/issues/360)).
140. Plural messages: `"cards_one"` and `"cards_other"` (the suffixes i18next uses) are one
    message, `t.cards(count)`, which picks the form by the count's CLDR plural category in the
    locale and writes the count as `t.number` does; `t.js().cards()` picks it in the browser the
    same way (`format.js`, checked against Intl for every locale). A catalog without a form its
    language uses (French's `_many`), with one it never uses (English `_zero`), or without
    `_other` does not compile. Breaking: a key ending in `_zero`, `_one`, `_two`, `_few`,
    `_many` or `_other` is a plural form, so `t.cards_one(..)` and `t.cards_other(..)` become
    `t.cards(count)`. Measured: `corten-format` grows from 2,535 to 3,008 bytes (1,260 to 1,475
    gzipped); a `t.js()` plural sends the locale's plural table with its forms (seller-desk's
    grid, which sends one to two handlers, is 502 bytes more per page; its whole page 474 bytes
    raw, 37 gzipped, 15 brotli); seller-desk's `render_cost` under callgrind is 2,026,886,533 instructions
    on main and 2,029,845,444 here (median of 3, +0.15%), and a plural `ItemCount` 1,648.5 per
    render against 1,636.5; the examples' `count == 1` choices are gone and card-market's French no longer
    writes "1 annonces"
    ([#393 Catalogs have no plural rules, so apps pick a form with count == 1 and card-market shows "1 listings"](https://github.com/todd-skelton/corten/issues/393)).
141. `corten dev`, `fmt` and `check` notice when the app's `Cargo.lock` pins another corten than the
    one they were built from (a git revision or a release) and say so before running, with the
    `cargo install` command for the matching one; they ran with the old behavior silently, and
    `check` failed on `cargo fmt --check` with no reason. The command records the commit it is built
    from (`corten --version` prints it), read from `.git` or a published package's
    `.cargo_vcs_info.json`, standard library only. A path dependency is not compared
    ([#458 The corten CLI does not notice it is older than the corten the app pins, so fmt and check silently use old behavior](https://github.com/todd-skelton/corten/issues/458)).
142. Groups and rows of form fields. `#[fields(group)] ship_to: Address` puts a `Fields` struct in a
    form, its fields posted as `ship_to.street` (`f.ship_to.street`), so a field group is defined
    once and a form can hold two of one type. `#[fields(rows)] #[serde(default)] domains:
    BTreeMap<Domain, DomainLimits>` is one group per row, each field named by the row's key,
    `f.domains.at(&Domain::SellerPortal).max_concurrent`, posted as
    `domains[seller-portal].max_concurrent`: the key, not the row's place, joins a row's values, so
    a row left out shifts nothing and an unchecked checkbox is `false` in its own row, and an error
    on one row's field marks that input only, with its own message id. `at` takes only the map's key
    type (a `RowKey`: text, an integer, or a `#[derive(UrlValue)]` enum, by its word); a key's other
    characters are %-encoded. A group of groups, rows in a group or a row, rows without a default, a
    group that is an `Option`, and a row key an enum reads by more than its word (`alias`, `other`)
    do not compile, and neither does a `UrlValue` enum with two variants written as one word. `Form`
    and `corten::form::from_bytes` read both with Corten's own form reader. `#[fields(map)]
    #[serde(default)] counts: BTreeMap<Sku, u32>` is one plain value per key, `f.counts.at(&sku)`,
    posted as `counts[sku]`, with its own error. Plain forms pay for it: reading a form of three
    plain fields costs 3,332 instructions where it took 3,110, and refusing one whose number does
    not parse 5,401 where it took 5,176 (callgrind, release build, 3 runs each); a row's error is
    found by its key, so many rows' errors do not cost each other
    ([#366 A form with the same fields on each row cannot name a row's field or give it its own error](https://github.com/todd-skelton/corten/issues/366), [#363 #[derive(Fields)] refuses #[serde(flatten)], so a field group shared by forms (an address) is copied into each form](https://github.com/todd-skelton/corten/issues/363)).
143. `Req::invalid_with(&errors, form, page)`: a refusal whose form needs a read of its own reads it
    only for the patch, through a `FormPatch` that draws a patch and nothing else (a page there does
    not compile), and the page side only for a browser without JavaScript; a data client gets the
    Problem Details document with neither, as from `invalid` (feature `json`). An app no longer branches on
    `req.mode` for such a refusal and loses the JSON answer
    ([#468 req.invalid takes the refused form's patch already drawn, so a patch that needs a read is read for the no-JavaScript page too](https://github.com/todd-skelton/corten/issues/468)).
144. `DayStyle` for `t.date` and `t.js().date`: the month and day without the year, `Short` (`Mar
    2`) and `Long` (`March 2`), written as Intl writes `{ month, day }` in each locale's order and
    words (`2 mars`, `2. März`), for a chart's axis, a week's row or a timeline whose year the page
    says once; like `MonthStyle`, `t.date_time` does not take it. The locale data gains two patterns
    per locale (from Node's Intl, checked against it, and in the cases the browser's `format.js` is
    checked with); no other format changes
    ([#473 t.date has no month-and-day style without the year (Intl { month: 'short', day: 'numeric' }), so chart labels hand-build month names](https://github.com/todd-skelton/corten/issues/473)).
145. A handler parameter typed by a declaration `corten::types` generated
    (`@param {import("../types/app").SortBy} by`, or a `@typedef` of it) takes a value of the Rust
    type of that name in the component's module, so `js::sortBy(target, SortBy::Price)` compiles
    and `42` or `"prodcut"` do not: one handler serves every name of a fixed set, and a renamed
    variant fails to build. A call to a handler whose parameter is a string literal union
    (`@param {"name" | "price"} by`) no longer compiles, since it took any value; the error and a
    warning on the call say to make it a Rust enum with a generated declaration, and so does a
    list or a generic of one (`Mode[]`, `Record<Mode, number>`, `[Mode, number]`, `{ m: Mode }`):
    any string literal in the parameter's type counts, outside `import("..")`, `Message<..>`, a key
    (an object's, an indexed type's, the keys and members `Pick`, `Omit` and `Exclude` take) and a
    callback's parameter names; a set in a template literal's `${..}` counts. The script still compiles, so an export only other
    scripts import types its parameters as it needs. A `@typedef` of
    any type but an object or a generic one (`@template`) (`Tab | null`, `Tab[]`, `string`) is
    checked as the type it names where a handler takes it, through any number of typedefs, and a
    generated declaration a handler's parameter holds in a shape the call is not checked against is
    refused rather than taking any value, in any shape of a typedef of plain values that holds one;
    a typedef the script keeps for itself is not checked. Typedefs that refer to each other (a
    recursive `Json`, a `Tree` and `Forest`) are seen through as one, and each typedef is
    written out once, so a recursive one, a long chain or a type with thousands of declarations
    costs the macro no more than another.
    What the page gets is unchanged
    ([#362 A handler argument typed by a generated enum union takes any JSON value at the call](https://github.com/todd-skelton/corten/issues/362)).
146. A route costly to draw says `#[route("/reports/suggested", get = suggested, prefetch = false)]` (`Route::PREFETCH`), and every link to it, `href={..}` with the route or a `&dyn Href` holding it, is written with `data-no-prefetch`, so hovering it starts no work and only a click does; the route says it once instead of each link. The runtime now aborts a prefetch it drops (the 8-newest limit, a stale entry, a form post, a visit left before the prefetched answer is used), so the server sees the client go instead of drawing the page to the end, and the views guide describes prefetch, `data-no-prefetch` and `data-reload`. The minified runtime grows by 83 bytes (26,536 to 26,619), 29 gzipped (9,642 to 9,671), 33 with brotli (8,709 to 8,742). The item page's render runs 0.11% more instructions (median of 3, callgrind, release build: 35,623,327,994 against 35,584,847,328 on its base), the check each link makes of its route's `PREFETCH`.
    ([#421 Hover prefetch starts a costly GET page's work; the data-no-prefetch opt-out is undocumented, per link, and a discarded prefetch never aborts](https://github.com/todd-skelton/corten/issues/421)).
147. `Refusal::Upstream`: a submit refused because a service the handler calls failed or did not
    answer (a carrier, a payment provider, another app), answered by `req.invalid` and
    `req.invalid_with` with 502 in every mode: the form drawn again with the message, the page
    without JavaScript, the Problem Details document for a data client, which got the HTML page
    from a handler that set the status by hand. The retry is the same submission: the runtime now keeps a form's
    `SubmitKey` over the key a patch draws (a 5xx patch used to swap in a fresh one, so a retry of a
    write that did apply upstream applied twice), and `SubmitKey::again` draws the posted key into
    the form a browser without JavaScript gets. The minified runtime grows by 172 bytes (26,619 to
    26,791), 36 gzipped (9,671 to 9,707), 29 with brotli (8,742 to 8,771). An agent tool gets the message as `ToolError::Upstream` ("try again later"),
    not as a refusal of a stale state. Still a closed set: no other 5xx, and no `StatusCode`. An
    app's exhaustive `match` on `Refusal` or `ToolError` gains an arm
    ([#480 A post refused because a service it calls failed has no Refusal (502/503), so it cannot use req.invalid_with and a JSON client gets the HTML page](https://github.com/todd-skelton/corten/issues/480)).
148. `corten::form::keep_query(&at, html! { controls })`: a GET form's controls, then the
    rest of a typed route's query as hidden inputs, for a form that changes some of its fields (a
    GET form sends only its own controls and drops its action's query). Every key of the route's
    query that no control names (a field, a `#[route(query)]` set's key, a `#[route(prefix)]`
    entry) is kept, written as the route's links write it, so there is no list of names: a key is
    neither dropped nor sent twice, and a field the route gains is kept by every such form, where a
    hand-written list silently dropped it. It costs a form about 3.5 µs where hidden inputs
    written by hand cost about 0.5 µs (release build, 200,000 renders, 3 rounds: it writes the
    route's address again and reads it back); nothing else renders differently
    ([#439 A GET form that changes some of a typed route's query fields must write every other field as a hidden input by hand, and a missed or doubled one compiles](https://github.com/todd-skelton/corten/issues/439)).
149. corten-ui's `ChoiceGroup` and `ShownFor`: one choice (a radio group) whose value shows and
    hides parts anywhere on the page, the figures and, below a chart, the list worded from them.
    Both take a `Chosen { id, value }`, the choice's `#[derive(Id)]` id and the value the handler
    read, so a part's values are the choice's type (another enum's word does not compile) and its
    group cannot be text. The server checks the value and draws every part for another `hidden`;
    without JavaScript each option is a typed link to the page with that choice; with it, the arrow
    keys and Space choose in place, the address follows in the keys the options differ in (another
    control's, a tab's, stay), and every part, also one a patch adds later, follows the choice. No
    stylesheet repeats the server's values. `TabGroup`'s address now follows the same way, in the
    keys its tabs differ in, so selecting a tab keeps a choice's key (it took the tab link's whole
    query)
    ([#474 A client-only choice cannot switch parts in several places of the page: TabGroup switches only its own panels](https://github.com/todd-skelton/corten/issues/474)).
150. `req.redirect_with_notice(&to, text)` and `req.notice()`: a redirect after a write says what
    it did ("Batch 3517 deleted.") once, on the page it lands on, without a query field that a
    reload, a copied link or Back says again. The text rides in a `__Host-` cookie for that page's
    path (a minute, cut at 500 bytes, the path hashed with its escapes decoded), which the page's
    answer removes and which a `HEAD` request leaves. The page renders it (`Notice`, escaped,
    corten-ui's `Toast { message: req.notice() }` in the layout) and is private. A navigation's page
    that did not draw it, its layout not run, hands it to the runtime in `corten-notice`, and the
    runtime dispatches `corten:notice` (`detail.message`), which the `Toaster` shows; when the
    navigation loads the page in full instead (its layout changed), the runtime keeps the notice in
    session storage and dispatches it after the load. A debug build warns about a full load that
    did not draw it. `OnSite` (sealed): the links a notice may go to, a route, `route.at(id)`, a
    `Local` and a `#[derive(Href)]` enum whose variants all are; another site's URL does not compile.
    The watchlist's subscribe and the gallery's rename use it, and the gallery's toaster moved into
    its layout. A write forgets what was prefetched while it was on its way, which could predate
    it or use up its notice. The minified runtime grows by 637 bytes (26,791 to 27,428), 240
    gzipped (9,707 to 9,947), 216 with brotli (8,771 to 8,987). The item page's render runs
    0.08% more instructions (median of 3, callgrind, release build: 35,619,673,804 against
    35,592,439,055 on its base), the look for the notice cookie in each request's `Cookie`.
    ([#385 A post that redirects cannot say what it did on the page it lands on, so each screen carries its message in a query value](https://github.com/todd-skelton/corten/issues/385)).
151. `Signal::task()`: a task's signal and its `Shutdown` handle, whose `begin()` aborts the signal
    with `Abort::Shutdown` as `serve` does, so a test of an app's task covers its shutdown path (a
    job released, a loop that returns at once) on a paused clock; a request's signal still cannot
    be aborted by the app (a `compile_fail` doctest). Nothing a request does changes
    ([#489 A task's shutdown path cannot be tested: Signal::default never aborts and abort is crate-private, so the app wraps Signal in a trait of its own](https://github.com/todd-skelton/corten/issues/489)).
152. `#[component(js = Owner)]` (`js = path::to::Owner`): a part of a component, a function of its
    own (a sheet, a row, a field), calls its owner's handlers through `js`, so a large page splits
    into parts without a script per part (one more request for a one-line handler) or the owner
    drawing the part's markup and passing it down as a prop. The page loads the owner's one script;
    the part adds none, and an owner without a script does not compile (a `compile_fail` doctest).
    A component's `js` module is now beside it (`__corten_js_{Name}`, hidden) instead of inside
    its `render`, which is how a part reaches it; what renders is unchanged. The item page's
    `render_cost` (release, callgrind, 3 runs) is 35,602,516,052 instructions against
    35,613,859,907 on main (-0.03%, layout: the page has no part)
    ([#492 A sub-component cannot bind its page's script: #[component] has css = Other but no js = Other](https://github.com/todd-skelton/corten/issues/492)).
153. The `zones` feature: the visitor's time zone with its rules, from Corten. With
    `App::zoned_dates()` (which the app sets: the feature alone, which another crate may turn on,
    changes no app's dates), while Corten renders a request's answer (a page, a patch), `t.date`,
    `t.time` and `t.date_time` write in the zone the browser reported, with its daylight saving
    rules (jiff and its bundled tz database, so an image without `/usr/share/zoneinfo` has them),
    and the answer is private; a live part's fragments too. A value that names a day (a due date
    kept as UTC midnight, a chart's axis) takes `.zone(Utc)`, or it reads a day early west of UTC.
    A `Public` answer, the same for everyone, is written as for an unknown zone (UTC, `LocalTime`
    written again in the browser) and stays shared; the route knows it when the crate compiles.
    No handler takes the zone and no component is given it, where an app added a tz crate, a `Zone` newtype, `Option<ClientZone>` on each handler that draws a date and
    a prop down to it. `.zone(..)` still chooses another; outside a request's render a date is in
    UTC. `req.zone()` is the zone (`corten::i18n::Visitor`, a `Zone`), for a picked date, UTC
    until the browser has reported it. corten-ui's `LocalTime` draws a date the first page shows:
    there the server does not know the zone yet and writes UTC, and its script writes it again in
    the browser's zone with the server's own format (`DateFormat` takes an offset,
    `format(at, { offset })`); later pages are the server's. `When` has `datetime()`, `format()` and
    `zone_unknown()`. A date rendered once and kept across requests takes `.zone(..)`, or it is in
    the zone of the request that rendered it. Each `LocalTime` whose script runs carries its format
    (about 850 bytes before compression against about 70 for the date alone). The minified format
    module grows by 18 bytes (3,008 to 3,026); the runtime is unchanged. The item page's
    `render_cost` (release, callgrind, 3 runs, without the feature, which card-market does not
    turn on) is 35,621,122,132 instructions against 35,603,094,486 on main (+0.05%). A page that
    writes ten dates (callgrind, release, per request) runs 34,952 instructions without the
    feature, 36,363 with it in an app that sets neither setting, and 42,854 with
    `zoned_dates` and a zone cookie (the zone looked up once per request, about 380 per date for
    its offset)
    ([#387 Dates in the visitor's zone need a tz crate, a ClientZone argument on every handler and a prop through every component, and the first page is UTC](https://github.com/todd-skelton/corten/issues/387)).
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
4. `App::shell`, `Meta` (`corten::meta`) and the shell's `Document` struct (`doc.head`, `doc.main`,
   `doc.asset`, `doc.current`, `doc.nonce`): the app's layout component replaces them. Corten
   has no built-in shell; an app without `App::error_page` gets a plain error document.
5. `App::context`, `App<C>`, `Req::context`, `Area::guard`, `App::guard`, `App::unguarded`,
   `Api::guard`, `Api::context` and `Access`: a handler takes `Caller<T>` instead, and an area's
   routes module says `caller = T`. Migrate a context to an `Identify` impl, a guard on an area to
   `caller = ..` on its routes module (with a caller type per role), and `App::guard` to
   `caller = ..` on the app's routes module with the sign-in route in a module of its own.
6. `Cache` from the public API and the prelude, and axum's `Router`, `get`, `post`, `put` and
   `delete` from the prelude (`get` and the others there are Corten's now). Return `Public<..>`
   instead of `(Cache::public(..), page)` and `NoStore<..>` instead of `(Cache::no_store(),
   page)`; build routes in a `#[corten::routes]` module instead of an axum `Router`. A layer on
   one route's method router has no replacement: a route's needs are its handler's extractors
   and answer, and the app's own steps are `Middleware`.
7. `Result` from the prelude, and `corten::Result`'s default `Response`: a handler answers with
   `HtmlResult`, `JsonResult<T>` or another answer, and `corten::Result<T>` (`Result<T,
   corten::Error>`) is for helpers. A bare `corten::Result` names no answer and does not compile.
8. The `link` keyword of `#[route]` in a routes module: a route an API serves goes in the API's
   own `#[corten::routes(api = ..)]` module, with its handler.
9. `Req::current`: `req.nav(&route)` writes the whole nav link, and `req.is_current` answers
   the question elsewhere.
10. `STATIC_DIR`, the default `static/` read from the working directory, `App::assets(dir)`,
    `Req::has_asset`, and `Assets::load`, `Assets::build` and `Assets::url` from the public API:
    static files are compiled in (Changed 73).
11. `corten::Query`, and axum's `Path` from the prelude: a route's parameters are its `Params`,
    whether its handler is named in a routes module or added by hand (`Area::route`), and a
    `GET` form's fields its `Form` ([#207 A handler reads its path parameters with Path<String>, so nothing checks them against the route struct that declares them](https://github.com/todd-skelton/corten/issues/207)).

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
25. The README's client-heavy table is from round `browser-3` (Creating 10,000 rows
    68.9 ms, against 87.5 in `browser-1`; Solid 59.2). `bench/browser/ssr.mjs` takes the loads of
    every app in turns and `run-all.mjs` takes 30, after one app's takeover median in a round
    moved with a slow stretch of the machine. The README's server highlight quotes the round
    its server table is from.

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
39. Escaping text for HTML is cheaper: the eight-byte test for a byte to escape is two compares
    instead of four (`"` and `&` fold into `'`, `<` into `>`; a `#` also matches and is then
    written as it is), the loop over text that has one is a function of its own so text with
    none (most of it) skips saving the loop's registers, and that loop uses the same test.
    Measured with callgrind on the item page (release build, the bench fixture, instructions
    per request): escaping 25,036 to 20,218 (-19%), the whole request 111,932 to 107,267
    (-4.2%); the home page, which escapes little, is unchanged (81,291 and 81,548, the
    difference in the allocator). Wall-clock `render_cost` on a busy host could not tell the two
    apart (24.6 and 24.4 µs best of 8).
40. `req.page(title, view)` is `req.page(doc)` and `req.screen(title, &data, view)` is
    `req.screen(&data, doc)`, the title read from the document's `<title>`. Migrate by writing a
    `Layout` component (the old shell, with `<title>{title}</title>` and `<Corten />` in `<head>`
    and `{children}` in `<main>`) and wrapping each page in it.
41. `App::context` needs the context to be `Hash` as well as `Clone`: any layout may show it, so its hash
    is always in the page key.
42. The error page shows the error under `corten dev` only if it prints `failure.detail` (the default
    and the `corten new` page do); the offline page is the error page for 503 ("Not available").
43. Since item 49, `html!` that is one component is that component, its props evaluated where
    the markup is (so `req={&req}` borrows the handler's `req` and a layout stays a document).
    A prop that borrows a local the view's function made (`let title = format!(..); html! {
    <Dialog title={&title} /> }`) is E0515 there, as it compiled before as a closure. Make it
    in the markup instead, `html! { let title = format!(..); <Dialog title={&title} /> }`: the
    `let`s and the component are a view that owns them, and a document when the component is
    one. Found by the Chase Sets pilot, four of whose views broke this way
    ([#77 A view that is one component cannot pass a reference to its own locals (E0515 since the document change)](https://github.com/todd-skelton/corten/issues/77)).
44. `req.patch`, `req.patch_with_status` and `req.invalid` take an `impl Patch` instead of an
    `impl Render`, and debug builds no longer scan a patch for a top-level element without an id
    (a `corten::live` fragment, which is text, is still scanned). Migrate by declaring each
    component a patch sends `-> impl Patch` and writing its ids as `#[derive(Id)]` types.
45. `Failure` is an enum of every way a request can fail (`NotFound`, `WrongMethod`, `CrossSite`,
    `Refused`, `Rejected`, `Failed`, `Panicked`, `TimedOut`, `Offline`), with `status()` and
    `detail()` in place of the fields, so an error page that names each case stops compiling
    when one is added; `corten new` writes one. A handler's 404 is `NotFound` like an unknown
    URL, and the offline page's `status()` is 200. `req.page`, `req.patch`, `req.redirect` and
    the rest return `HtmlResult`; migrate `-> corten::Result` page handlers to `-> HtmlResult`.
46. A cross-site form post from a browser gets the app's error page (`Failure::CrossSite`, 403)
    instead of a bare status line.
47. The security headers reserve room only for the headers they add, so an error page's header
    map is not reallocated, and the route is called without an `async` layer around it: a page
    is about 500 instructions cheaper (24,021 to 23,525), an HTML 404 about 700 (21,849 to
    21,125) and JSON about 270 (19,579 to 19,307), callgrind medians of three runs (the app
    guide's table). The item page's whole `render_cost` (release, under callgrind) is
    34,215,143,042 instructions before and 34,138,183,828 after (-0.22%).
48. `seo::robots`, `seo::sitemap` and `seo::sitemap_index` return `Public<seo::File>`, an answer
    a route can name (`async fn robots() -> Public<seo::File>`), shared for an hour as before.
    `agent::Tools::route` returns `Methods`, for `Api::route`.
49. Reading a `Caller` makes the answer private even when its `Cache-Control` was written by
    hand (by a middleware, or on an `HtmlResponse`), not only when it came from `Public`.
50. **Breaking:** an app answers only requests whose `Host` is `localhost` (or a name under it),
    an IP address, missing, or a name it lists: `Policy::host("shop.example")` (`.shop.example`
    for its subdomains too) or `HOSTS=shop.example` where the deploy sets `HOST` and `PORT`. Any
    other gets `421 Misdirected Request` before routing, with nothing of the app run, so a
    DNS-rebound page cannot use an app on loopback (or a private network) as its own origin;
    `Policy::any_host()` (`HOSTS=*`) answers every name. The first refused name is logged. A
    deployed app behind a domain adds its names; the bench scripts pass their container names
    ([#113 An app on loopback answers any Host, so a DNS-rebound page can read and write its routes](https://github.com/todd-skelton/corten/issues/113)).
51. **Breaking:** `corten::serve` takes an `App`, not anything that becomes a router, so the
    app's tasks cannot be left behind on the way. A tower layer around the whole app is
    `App::layer(l)` instead of `corten::serve(app.router().layer(l))`.
52. A request carries the app's shared state in its `Signal` rather than as an extension of its
    own, so its extensions fit their first allocation: 207 fewer instructions for a page, 647 for
    a signed-in page and 849 for the error page (callgrind, release build, in-process `oneshot`,
    median of three).
53. CI runs the browser smoke tests in Playwright's image for the `@playwright/test` version in
    `package.json`, which has the three browsers and their system libraries. Installing them on
    the runner (`npx playwright install --with-deps`) fetched about a hundred packages from the
    Ubuntu mirror on every run, and stalled until it was cancelled when the mirror slowed to a few
    KB/s (twice on 2026-10-27, 15 and 5 minutes).
54. `bench/fullstack` measures what a request costs: `run-ssr.mjs` reads each phase's CPU time from
    the container's cgroup counter (`cpu-time.mjs`), and `summarize.mjs` adds requests per
    CPU-second, requests a second per MiB and the item API's CPU to `summary.json`. With items in
    memory the load generator also gets the item API's idle cores: in r6 four could not keep
    Corten's core busy (93% for the page, 88% for the JSON).
55. `bench/fullstack/item-api/server.mjs` runs a server process per core it may use (two on a
    bench droplet), each listening with `SO_REUSEPORT`. In r6 its one process used 39-40% of a core
    for Corten's item-API runs and would have capped an app about 2.6x faster
    ([#166 Corten's item-API path spends six times as long fetching and parsing the item as rendering it, and the single-process item API could soon cap it](https://github.com/todd-skelton/corten/issues/166)).
    On Docker Desktop (shared with other work, so rough), pinned to two cores, it served
    4,400-6,600 items a second to 64 users in three runs against 3,600-4,000 in two with one
    process, using about 1.9 cores. Responses are byte for byte the same.
56. `JsonResult<T>`'s error is `JsonError`: `check(&input)?` answers 422 with the fields
    (`{"error":{..,"fields":{..}}}`), and `?` on any other error is answered as before. Before,
    a JSON handler that checked its input and loaded a record could not `?` both, and the
    `Result<Json<T>, Errors>` the docs named did not compile. On a page `?` on `Errors` still
    does not compile (a `compile_fail` doctest): a page's form answers with `req.invalid`.
    Migration: `return Err(Error::status(..))` in a `JsonResult` handler is `Err(Error::status(..))?`
    (or `.into()`); `?` is unchanged.
57. The runtime no longer carries the code that writes numbers, money and dates for a script
    (`t.js().money(USD)` and the rest): it is a module of its own, `corten-format`, served with the
    app's scripts and loaded as a module script by the pages whose handlers take a format.
    `corten.js` went from 24,514 to 22,834 B (9,102 to 8,380 B gzip, -722 B) on every page; a page
    that takes a format loads the 2,074 B module (1,054 B gzip) with its handlers' modules
    ([#159 The item page's JavaScript grew 32% between benchmark rounds r5 and r6 (7.3 to 9.6 KB gzip)](https://github.com/todd-skelton/corten/issues/159)).
58. `corten::json::from_slice` reads the item API's response in 13% fewer instructions: a field
    the type does not declare that holds a string, number, `true`, `false` or `null` (most of
    them) is skipped without the loop that follows nested arrays and objects, and a key is read
    inline. `examples/card-market`'s `ApiItem`, 20 fixture items averaging 69 KB, release build
    under callgrind: 924,942 to 803,111 instructions an item, the same in each run (serde_json
    reading the same type: 1,376,422). Values and errors are unchanged; the reader's tests compare
    every one with serde_json's
    ([#166 Corten's item-API path spends six times as long fetching and parsing the item as rendering it, and the single-process item API could soon cap it](https://github.com/todd-skelton/corten/issues/166)).
59. corten-ui's text props take the text: `label="Actions"`, `close="Dismiss"`,
    `MenuItem { label: "Help", .. }`, `Tab { label: "Offers", .. }` and
    `Toast { message: Some(text) }`, where each was a `&dyn Render` written `&"Actions"` (and an
    optional message cast `as &dyn Render`). `MenuButton`, `MenuItem`, `Tab`, `TabGroup`,
    `Toaster` and `Toast` take any `Render`; a menu's items and a group's tabs share one label
    type. A tab's panel stays a `&dyn Render`, since each panel is a view of its own type.
    ui-gallery's page renders in the same instructions (callgrind, release: 72,700 to 72,511 a
    request, within noise). Migration: drop the `&` from a label; an empty toast is
    `Toast { message: None::<&str> }`; labels of different types in one menu or group (a message
    with arguments beside one without) are each a reference, `[Tab<&dyn Render>; 2]`
    ([#211 A text prop typed &dyn Render needs &"text", and an optional one needs an `as &dyn Render` cast](https://github.com/todd-skelton/corten/issues/211)).
60. examples/watchlist is a module a screen: `home.rs`, `alerts.rs` and `account.rs` beside
    `news.rs`, with `layout.rs` for the document, `socket.rs` for the WebSocket and each screen's tests next to
    it; `app.rs` keeps the routes, the app and the error page, and its route table says which file
    each route is in. No behavior changed: the same 27 tests pass
    ([#229 watchlist's app.rs is 771 lines of app code for seven screens](https://github.com/todd-skelton/corten/issues/229)).
61. A message's arguments are `corten::i18n::Arg`, not any `Display`: text, an integer, or a value
    `t` formats (`t.number`, `t.money`, `t.date`, another message). An integer is written from
    its digits with no `core::fmt` and no escaping, so card-market's item-API home page (50
    `t.home_item(n)` links) went from 64,425 to 58,505 instructions a request (callgrind,
    release, three runs each), winning back most of what message arguments cost there; the en
    and fr home pages and a category page are the same bytes. A float no longer compiles as an
    argument: its text was the same in every locale (`0.9` on a French page), where
    `t.number(decimal)` writes `0,9`. A type of the app's own implements `Arg` with one method
    (`sink.str(..)`). Migration: pass a float through `t.number`, and a `Display` type of the
    app's own as its text or through an `Arg` impl
    ([#237 The item API's home page got 10% slower in the believable-data change; an integer fast path for message arguments may win it back](https://github.com/todd-skelton/corten/issues/237)).
62. A `Public` answer that sets a cookie does not compile: `HtmlResponse<For>` says who an answer
    is for, `ForOne` (the default, what `HtmlResult` holds) or `ForAll` (inside `Public`), and
    `with_cookie` is only on `ForOne`. The `Req` methods that make a page, a patch or a redirect
    take `For` from where the answer goes (the handler's return type, Corten's `get`, or
    `Public::new`, which finds the `Public`'s type from its answer), so handlers and closures are
    written as before. Before, the cookie compiled and the page was sent `private, no-cache`,
    which a CDN never keeps, with a warning only under `corten dev`; a `Set-Cookie` written by
    hand is still caught that way. No code runs differently (`For` is a type alone; card-market's
    public home page: 33,741 to 33,881 instructions a request, callgrind medians of three, where
    main's own runs spread from 33,701 to 33,933).
    Migration: a helper that makes a page for a `Public` handler returns
    `corten::Result<HtmlResponse<For>>` for any `For` instead of `HtmlResult` (`Public::new`
    says the page "is not an answer made for everyone"); one that wraps a page in `Public` takes
    `corten::Result<HtmlResponse<ForAll>>`, as card-market's `shared` does; a closure given to
    axum's own `get` (tests only: an app cannot mount one) names it, `req.page::<ForOne>(..)`;
    `Public<A>` needs `A: Shareable`, which every answer that can be public implements, and an
    answer type of the app's own implements only when it sets no cookie
    ([#235 A Public answer that sets a cookie compiles, and is only made private when it is sent](https://github.com/todd-skelton/corten/issues/235)).
63. A script keeps state with `keep(() => init)` (from `corten`), which replaces `state(el,
    init)`: the object of the component whose script the running handler is in, one per
    instance, so a handler no longer takes the component's root just to find its state. A
    component whose script imports `keep` is always scoped (`data-scope` on its root, with or
    without refs), and such a component's handler on a child's element names its component
    (`data-on="change:form.save($t)/form"`), so it keeps the parent's state wherever the child
    renders it; markup a parent gives a child keeps the parent's handlers' `scope` too. A
    `script!` module, which components share, cannot import `keep` (a compile error).
    seller-desk's grid handlers lose their `scope` argument (six of them), watchlist's `pause`
    too, and corten-ui's menu button and number field keep theirs per component. Take the object
    first, before an `await` or anything that dispatches an event; outside a handler it throws.
    The runtime is 90 bytes larger gzipped (8,503 to 8,593; brotli 7,663 to 7,742);
    card-market's item page renders as before (58,644 to 58,654 instructions, callgrind).
    Migration: `state(scope, () => init)` is `keep(() => init)`, and a `scope` argument used only
    for it is dropped from the handler and its markup
    ([#238 Runtime state is keyed by an element the handler has to pass: state(root, init)](https://github.com/todd-skelton/corten/issues/238)).
64. examples/watchlist keeps the alerts a browser saves and lists them, each with a Remove
    button. They live in a store whose reads catch up with its writes 1.5 s later, as a replica
    would: a write answers with its receipt (`/alerts?saved=7`), and the page or the patched
    status line and list are read again (`ReadAgain`) until reads have it. Before, the address
    carried the time the alert would land and nothing was kept. A browser's watchlist and alerts
    are kept under one id, now in the `visitor` cookie (`visitor.rs`), which was `list`, and both
    forget the browser used longest ago past 10,000 (`recent.rs`)
    ([#220 watchlist's alerts page never shows the alerts you saved](https://github.com/todd-skelton/corten/issues/220)).
65. `corten-cluster`'s `Cluster::snapshot`, `live` and `live_with` return
    `corten::Result<FeedResponse>`, an answer a Corten route names, instead of an axum
    `Response`: an unknown feed is a 404 and Postgres out of reach a 503, both answered as JSON as
    any failure of a feed's route is. The crate depends on `corten` (feature `feed`) instead of
    axum and `corten-feed`. Migration: a handler that returned the `Response` names
    `corten::Result<FeedResponse>`; outside a Corten app, call `.into_response()` on it.
    `examples/listings-feed` is an `App`: the feed in a routes module, the benchmark's
    writes and reads under `App::api` (`/api/feeds/{seller}/market`, `/seq`, `/api/stats`),
    the instance's name from a middleware (`X-Instance`), Corten's health checks (`/health/ready`
    checks Postgres), and the browser probe a page whose `#[component(js)]` follows the feed
    (`/sellers/{seller}`, a 404 for a seller with no feed), where an inline script would be
    refused by the CSP. The failover benchmark ran again on it (`results/failover-r2.json`)
    ([#217 examples/listings-feed does not use App, so its routes skip Corten's compile-time checks](https://github.com/todd-skelton/corten/issues/217),
    [#233 listings-feed's failover benchmark has not been run since the example was renamed](https://github.com/todd-skelton/corten/issues/233)).
66. examples/watchlist keeps the profile picture Ada uploads and shows it on the account page and
    in the header, from `/pictures/{id}`: an id that is a keyed hash of the bytes, served as a
    `Media` every cache keeps. The upload reads the picture's type from its first bytes (PNG,
    JPEG, GIF or WebP), refuses anything else, keeps at most 256 pictures in memory, and needs
    someone signed in. The file input is styled like the page's buttons
    ([#221 watchlist's profile picture upload never shows the picture](https://github.com/todd-skelton/corten/issues/221)).
67. watchlist's WebSocket is `/quotes`, which a ticker outside the browser opens to follow
    stocks: it sends a ticker and is sent the stock's feed row, then again each time it trades.
    It replaces `/echo`, which answered in capitals and which nothing used. A message longer than a
    ticker closes the socket as too big (1009), a client's close is answered, a send the client
    does not take within 10 seconds ends the socket, and shutdown still closes it with a goodbye
    ([#222 watchlist's WebSocket route is an echo, which a stock app would not have](https://github.com/todd-skelton/corten/issues/222)).
68. seller-desk sells card-market's cards: its rows are the three sets' 540 cards, card-market's
    eleven singles first at the cheapest Near Mint price card-market lists them for (a test
    reads its fixture), the rest priced by rarity. Most start in Near Mint and some played; past
    540 rows each card comes again in its next condition at that condition's share of the price.
    A seller keeps a few copies of a dear card and many of a common, and restocks to that.
    Names are cards' names (no `#1`, `#2`), and the sets are card-market's three
    ([#224 seller-desk and card-market give the same card different prices](https://github.com/todd-skelton/corten/issues/224)).
69. card-market shows card art: a generated SVG for each card of its demo catalog in
    `static/cards/`, served at a versioned URL every cache keeps (and the PWA's worker with
    them, cached the first time each loads). The home and category grids show it sized (`width`,
    `height`), past the first row loaded near the screen (`loading="lazy"`), with `alt=""` since
    the link names the card; the item page shows it first, not deferred, with its text. Only the
    built-in demo catalog has art (another source's `item_3` is another card), so the
    benchmark's item page and the item JSON are unchanged. Measured with the demo
    catalog (release build, over HTTP): Ashen Drake's page is 126 bytes larger (42 with brotli)
    plus its 377-byte picture (brotli); the home page is 1,356 bytes larger (105 with brotli) plus
    the pictures that load.
    ([#223 card-market sells trading cards with no card images](https://github.com/todd-skelton/corten/issues/223)).
70. examples/ui-gallery is one screen: a seller's draft listing for a card, with its quantity,
    price and condition, the card's market in tabs, a menu of actions, a saved toast, a rename
    dialog, the last sale, and its SKU with a copy button
    ([#225 corten-ui is used only by ui-gallery, and the gallery's content does not fit together](https://github.com/todd-skelton/corten/issues/225)).
71. examples/card-market's MCP endpoint answers the failures Corten writes itself (a wrong
    method, a cross-site post, the deadline) as JSON-RPC errors (`Api::errors`), the shape its
    tools' own errors have, and a partner's write without a known key is told to send a valid
    one in `X-Api-Key` (`Refused::message`)
    ([#231 Six public APIs appear in no example](https://github.com/todd-skelton/corten/issues/231)).
72. A typed event (`#[event]`) is served with the script that imports it from `corten/events`,
    so an app lists the script's component and never the event; a script that imports an event its
    component's module does not have does not compile. Before, an app that listed `CopyButton`
    without its event would have had the button's script fail to load. A page whose handlers are
    in such a script fetches `events.js` with it (`<link rel="modulepreload">`, 64 bytes)
    rather than after it, one round trip sooner; seller-desk's price cells had waited for it.
    `Script` has an `events` field, written by `#[component(js)]` and `script!`. And
    `on:Copied={..}` listens for the name the event's `#[event]` gave it: a library's carries its
    prefix (`ui-copied`), which `html!` had dropped, so no listener heard a library's event
    ([#294 CopyButton tells the app nothing when it has copied, so a copy that is counted keeps its own copy script](https://github.com/todd-skelton/corten/issues/294)).
73. **Breaking:** static files are compiled into the program. `corten::assets::embed!(pub struct
    Static = "static")` makes a constant per file (`Static::STYLE_CSS`, `Static::ICONS_192_PNG`)
    and `App::assets(Static)` serves them; `req.asset(Static::STYLE_CSS)` is its versioned URL,
    written without an allocation, and a misspelled or removed file does not compile. A name from
    data is `req.find_asset(name)`, an `Option`, replacing `has_asset`. The binary is the whole
    deploy: the Dockerfiles no longer copy `static/`, and an app no longer writes a lookup next to
    `current_exe()`. A file name a URL cannot carry (a space, `#`, `?`, `%`), two files with
    one constant, or a directory that is not there is a compile error; dotfiles are left out. A file
    added with no change to the code is in the program from the next build that compiles the crate
    (the Dockerfiles touch every source first). Under `corten dev` the app reads the directory from
    disk, and `corten dev` restarts it without compiling when only those files were edited or
    added (a removed file builds, and a failed build is tried again at the next change). A
    release binary holds no path from the build machine. The PWA names its files the same way
    (75). Measured on card-market (release,
    callgrind, three runs each): startup 1,470,715-1,485,822 instructions to 1,381,406-1,387,180,
    the item page 81,556-81,740 to 77,294-77,524 per request (`req.asset` no longer looks a string
    up and allocates the URL), and the binary 6,729,592 to 6,752,216 bytes (+22,624, the files)
    ([#268 Static files are a directory found at run time, so a built program cannot be deployed by itself and each app writes its own lookup](https://github.com/todd-skelton/corten/issues/268)).
74. The trait a `#[route(query)]` set implements is `corten::route::QueryParams` (it was
    `route::Params`), so `Params` is the extractor. The examples read their routes' parameters
    as `Params` (16 handlers); ui-gallery's `HomeQuery` and listings-feed's `MarketQuery` are
    gone, the parameters declared once on the route (`MarketWrite` gains `n`), and watchlist's
    `Alerts` no longer derives `Deserialize` ([#207 A handler reads its path parameters with Path<String>, so nothing checks them against the route struct that declares them](https://github.com/todd-skelton/corten/issues/207)).
75. **Breaking:** the PWA names its files by the constants `embed!` makes, so a misspelled or
    removed icon or worker script does not compile (it was a startup panic):
    `Manifest::icon(Static::ICONS_192_PNG, "192x192")`, `maskable_icon` the same, and
    `Pwa::import_script(Static::SW_PUSH_JS)`. An icon another site serves is a `Uri`
    (`Uri::from_static("https://cdn.example.com/192.png")`), its type read from the path without
    the query. The worker script is always a file of the static directory, so it is same-origin by
    its type; a path (`import_script("/abs/sw.js")`) is no longer taken. A file the app's static
    directory does not serve (none given, or another app's) is still a startup panic, now in
    release builds too
    ([#297 The PWA names its icons and worker script by path, so a misspelled file is a startup panic, not a compile error](https://github.com/todd-skelton/corten/issues/297)).
76. examples/listings-feed is a seller's page, not only a load harness: `/sellers/{seller}` draws
    the first 500 listings in a table (`ListingTable`, rows from a `#[component(view)]`, prices
    in the page's money format) from the feed's read, and one `subscribe()` keeps the counts and
    the rows current as changes arrive from any instance. A form posts a market move
    (`POST /sellers/{seller}/market`, the submit key its idempotency key) and patches its result
    in place; without JavaScript it redirects to the page with the write's sequence
    (`?after=`), which asks to be read again until its instance has the write. The `Listing`
    type the script reads is generated (`types/app.d.ts`); the outputs `bench/run.mjs` reads
    keep their names. `examples/listings-feed/smoke.mjs` checks the page in a browser against
    Postgres and NATS, in CI too
    ([#334 listings-feed is a benchmark harness listed as an example](https://github.com/todd-skelton/corten/issues/334)).
77. **Breaking:** `corten_ui::Choice` and `Picker` carry the type the form field reads:
    `Choice<'a, V> { value: V, label }` and `Picker { value: Option<V>, .. }`, the value written as
    the text its `AsRef<str>` gives. With a `#[derive(UrlValue, Deserialize)]` enum
    (`Choice { value: Condition::NearMint, label: "Near Mint" }` for `condition: Condition`) the
    words the combo box offers are the words the handler reads, a selected value of another type
    does not compile, and a form with any other word is refused (422). `#[derive(UrlValue)]` also
    derives `AsRef<str>`, and `RequiredWhen` takes `values` of any `AsRef<str>`
    (`values={&[Pricing::Offers]}`). ui-gallery reads its condition, pricing, regions, tab and
    sale sheet as enums, where it compared strings
    ([#324 corten-ui choices are strings, so nothing ties a Picker or ComboBox value to the field that reads it](https://github.com/todd-skelton/corten/issues/324)).
78. `t.money`, `t.number`, `t.percent` and `t.change` write a value in about 40% fewer
    instructions: 574 for an amount or a rating in `en` where they took 949, 602 in `fr` where
    they took 1,003 (callgrind, release build, 30 amounts in US dollars and 20 two-place ratings
    per round). A page writes into its `String` without a `dyn Write` call per piece, the digits
    are made in `u64` (a `u128` division is a library call), the template is searched as bytes,
    and the currency's code is compared as a number and written only where the locale writes a
    currency by its code. The text is the same; a test checks that no symbol template has a code
    to place.
79. examples/card-market answers a category nothing is in (a slug from an old link) with
    `X-Robots-Tag: noindex` (`Public::header`), so search engines do not list an empty page; a
    category with cards is unchanged
    ([#335 Some framework features appear in no example: Area::policy, Public::header, Req::signal, Refused::too_many](https://github.com/todd-skelton/corten/issues/335)).
80. A feed's row type reaches the script that follows it. `FeedResponse` is `FeedResponse<Row>`
    (`Feed<Level>::snapshot` answers `FeedResponse<Level>`), a feed's route names its rows
    (`#[route("/feed/levels", get = levels, rows = Level)]`, checked against what its `get`
    answers, through the new `Answer::Rows`), and a handler parameter declared
    `@param {FeedUrl<Level>} feed` (from `corten`) takes only a route whose rows are the Rust
    type `Level` in the component's module; a `FeedUrl` it cannot read is a compile error.
    `subscribe()` takes a `FeedUrl<Row>` and its row type from it, so a script's `snapshot` and
    `patch` are typed with no `@param` of their own, a `Stock` handler given a `Level` feed
    fails `tsc`, and a plain string is not a feed's URL. Each mistake has a `compile_fail` doctest
    or an `@ts-expect-error` line. The page's HTML and the runtime's bytes are unchanged
    ([#326 A feed's row type does not reach subscribe(), so a script's row type is whatever its JSDoc says](https://github.com/todd-skelton/corten/issues/326)).
81. `corten fmt` runs `cargo fmt` first, then lays out `html!` across the package, and again
    until neither changes anything, so the one command fixes what `corten check` reports (a
    check that failed on rustfmt was not fixed by it). It stops after four rounds and names the
    files still changing (rustfmt's `hard_tabs` undoes the layout's spaces), runs `cargo fmt` in
    the package it found from a subdirectory, keeps a CRLF file's line endings, prints each warning
    once, and refuses a flag it does not know. `corten fmt --check` checks both, and
    `corten check` runs it in place of its own `cargo fmt --check`. Given paths, or outside a
    package, `corten fmt` still lays out only the markup
    ([#379 corten fmt does not run cargo fmt, so a check that fails on rustfmt is not fixed by it](https://github.com/todd-skelton/corten/issues/379)).
82. corten-ui's `Choice` writes its value as its `Display`, as a route writes a field, in place of
    its `AsRef<str>`, so a row picked by its numeric id is a `Choice<i32>` and the form's field is
    `line: i32` (as for a route's field, `Display` must write what the field's `Deserialize` reads) ([#376 corten-ui choices must be AsRef<str>, so a row picked by its numeric id cannot use the typed picker](https://github.com/todd-skelton/corten/issues/376)).
    A choice also has `terms`, other words that find it without being shown (a card number, a
    SKU, a grade), and `ComboBox` lists the matches ranked: a label or term typed whole, then one
    that starts with the text, then one that holds it, each kind in the options' order. A choice
    is built with `Choice::new(value, label)` and its terms set with
    `Choice { terms: &["NM"], ..Choice::new(..) }`. examples/ui-gallery finds a condition by its
    grade (`mp`) ([#420 corten-ui ComboBox matches only a substring of the label, so a picker cannot search other terms (a card number) or rank the matches](https://github.com/todd-skelton/corten/issues/420)).
83. **Breaking for JSON clients.** Every JSON failure (under `App::api`, a `JsonResult<T>` route, a
    data client) is an RFC 9457 Problem Details document sent as `application/problem+json`, in
    place of Corten's own `{"error":{"status","message"}}` as `application/json`:
    `{"title":"Not Found","status":404,"detail":"No item 7."}`. `title` is the status's reason,
    `detail` the message a client may be told (left out when it would repeat the title, as for
    every 5xx outside `corten dev`), and `type` is left out, which means `about:blank`.
    Validation errors keep `fields` and `form`, now as extension members beside `detail`. A client
    reads `detail` where it read `error.message`, `status` where it read `error.status`, and
    `fields` where it read `error.fields`. `Api::errors` still writes another protocol's shape,
    sent as `application/json`. Gateways, API tools and clients read the failures without
    knowing Corten (`docs/guide/app.md`, `docs/guide/requests-and-forms.md`)
    ([#392 JSON failures use a shape of our own, {"error":{"status","message"}}, instead of RFC 9457 Problem Details (application/problem+json)](https://github.com/todd-skelton/corten/issues/392)).
84. Breaking: `Href` is implemented with `write_link(&self, out, html)` instead of `with_href`, so
    it can be a `dyn Href`; `with_href` is still there to call. An app's own `impl Href` becomes
    `#[derive(Href)]` on its enum, or `write_link`. `corten_ui::Link` is gone: `MenuItem` and
    `Tab` take their link as `&dyn Href`, the one link trait
    ([#391 A component several pages render cannot take a route as a prop (Href is not dyn-compatible), so a shared list's form action and column links are Strings](https://github.com/todd-skelton/corten/issues/391)).
85. corten-ui's `Choice` takes its terms with `.terms(..)`, from a slice of anything that is
    text: constant `&str`s as before, or the `String`s a row computed, borrowed where they are
    (`Choice::new(c.id, &c.label).terms(&c.terms)`), so terms made per row need no second
    `Vec<Vec<&str>>` kept in step with the options. `Choice` and `Picker` take the term type as
    a third parameter, `&str` by default, so `Choice<Condition>` still names the same type.
    `Choice { terms: .., ..Choice::new(..) }` becomes `Choice::new(..).terms(..)`
    ([#448 corten-ui Choice::terms takes &[&str], so terms computed per row (Vec<String>) need a second Vec<Vec<&str>> before the options](https://github.com/todd-skelton/corten/issues/448)).
86. Values 2 is "Standards first, extended rather than replaced", and following a standard where
    one fits is part of correctness in the conflict order: a header, wire format or file format
    of Corten's own where a standard exists needs a Choice in `docs/design.md` saying why
    (design rule 10), and review looks for one without it (`CONTRIBUTING.md`, Review)
    ([#402 The docs do not say Corten extends standards rather than replacing them, and the formats it defines record no reason](https://github.com/todd-skelton/corten/issues/402)).
87. `ReadAgain::from` takes the route by value as well as by reference (a borrowed route is a
    `Route`, and `ReadAgainFrom<'a, R>` is now `ReadAgainFrom<R>`), so a component that is both part
    of the page and the patch builds the read in its body instead of failing with E0515 or E0597
    ([#440 ReadAgain::from borrows its route, so a component that is both part of the page and the patch cannot build the read in its body (E0515, E0597)](https://github.com/todd-skelton/corten/issues/440)).
88. A `data-patch` form's newer submit no longer aborts a pending write (any method but GET): the
    server may have applied it, and its answer, the page's next state, was dropped. A submit made
    while a write is pending waits, and once the runtime has drawn the write's answer and it took
    the write (2xx) or refused only its input (422), it is sent from the form as it is then (the one
    the answer put in its place, found by id), so a version or count the answer wrote goes with it,
    under the form's new key, without its `on:submit` handlers or the browser's constraint checks
    running a second time. Several presses while the write is out send one submit of where the form
    ended; a refusal of the form's state (409, 410), a 5xx, a write that gets no answer, one the
    runtime does not draw (`corten:error`), a redirect, a reload, a navigation, or a form that left
    the page drops it. A GET submit still replaces an older one. The minified runtime grows by 656
    bytes (25,884 to 26,540), 247 gzipped (9,402 to 9,649) and 219 with brotli (8,496 to 8,715)
    ([#381 A data-patch form's submits replace each other, so a screen where every step counts (a count's + and -) writes its own fetch queue](https://github.com/todd-skelton/corten/issues/381)).
89. `FieldName` names where its field is, `FieldName<P = Root>`: a form's own field is still its
    name alone (`FieldName`, with `as_str`), and a group's or a row's field is a
    `FieldName<Group>`, `FieldName<Row<'k>>` or `FieldName<RowGroup<'k>>`, written, not stored:
    compare it with text with `==` (`f.ship_to.street == "ship_to.street"`) and write it with
    `Display` or in markup. `Errors::iter` yields a `Named` for each name.
    `#[derive(Fields)]` also makes `{Name}GroupFields`, `{Name}RowFields<'k>` and
    `{Name}RowGroupFields<'k>` and implements `corten::form::Fields`.
    `#[serde(flatten)]` stays refused, and its message now points to `#[fields(group)]`: a
    flattened group cannot be told from another of its type, and serde reads a flattened field's
    value only as text
    ([#366 A form with the same fields on each row cannot name a row's field or give it its own error](https://github.com/todd-skelton/corten/issues/366)).
90. A key under a field's name sent without the field itself (`go.x` from an image button, `ids[0]`
    from a PHP-style list) is read as that field's group or rows wherever a form is read (`Form`, a
    `GET` form's query, `Upload::fields`, `form::from_bytes`): a field that is not an `Option` is
    refused (422) with a message that says it was sent only that way, where it was ignored and a
    `#[serde(default)] go: bool` read `false` though the button was pressed, or a defaulted `Vec`
    read empty though values were sent. An `Option` reads as `None`, and a key under a name the
    struct does not have is ignored as before
    ([#363 #[derive(Fields)] refuses #[serde(flatten)], so a field group shared by forms (an address) is copied into each form](https://github.com/todd-skelton/corten/issues/363)).
91. Breaking: corten-ui's `MenuButton` takes its items as children, each the app's own element with
    `role="menuitem"` (a link, with `download` or `target` as it needs; a `<form method="post">`
    with the fields it sends around a `<button role="menuitem">`; a `<button command="show-modal">`
    that opens a confirm dialog), and `MenuItem` is gone: it was only a typed link or a field-less
    post, so a row menu with a download, a post with a field or a confirm dialog was built by hand
    without the menu's keys and focus. The script finds the items in the menu as it needs them (an
    item a patch adds counts) and closes the menu on one click listener. `<MenuButton id="m"
    label="Menu" items={&items} />` becomes `<MenuButton id="m" label="Menu">` with an element per
    item
    ([#413 MenuButton items are only a link or a field-less post, so a row menu with a download, a new-tab link, a post with a field or a confirm dialog is hand-built](https://github.com/todd-skelton/corten/issues/413)).
92. Breaking: a call to a handler whose parameter is a set of names written as string literals
    (`@param {"name" | "price"} by`, also with `null`, optional, through a `@typedef`, or as a
    group's field) no longer compiles, and neither does one to a handler whose parameter holds a
    generated declaration inside another type (`import("../types/app").Tab[]`,
    `.. | null`, a typedef of plain values holding one): each took any value at the call. A
    typedef of a plain type (`@typedef {string} Id`, `@typedef {HTMLElement} Root`) a handler
    takes is now checked as that type, where it took any value. Make the set a Rust enum with a
    generated declaration and type the value with it, alone
    ([#362 A handler argument typed by a generated enum union takes any JSON value at the call](https://github.com/todd-skelton/corten/issues/362)).
93. `tokens!` reads the W3C Design Tokens format, 2025.10, instead of a format of Corten's own:
    token files with `$value`, `$type`, groups, `$root`, `$extends`, `{group.token}` references (a
    reference is `var(--group-token)`) and JSON pointers (URI fragments, as RFC 6901 writes them;
    one into a value is the value, an alias it reaches followed to the value), the Color
    module's color spaces as CSS Color 4, and themes as a resolver document's modifiers
    (`tokens!("tokens/tokens.resolver.json")`; a file with `resolutionOrder` is a resolver, whatever
    its name, and each file it names is found from the file that names it), each context applying under `[data-{modifier}="{context}"]`
    (the default with the default values of what the others change) and declaring again the
    tokens that read one it changes, so a theme works on an element inside the page, a light panel
    on a dark page included. Names are written as CSS writes them (`Brand Primary`
    is `--brand-primary`, `BrandPrimary`), and a top-level token is a custom property with no enum.
    Breakpoints, utilities (`"group"` where they said `"scale"`) and the media condition a
    context follows go under `$extensions."run.corten"`. A context that defines a token the defaults
    lack, or retypes one, does not compile (two `compile_fail` doctests); so do a circle of references, contexts of two modifiers whose CSS together differs from the resolver's answer, two names that are one
    CSS name, breakpoints out of order or a breakpoint or utility whose name starts with a digit, a modifier with no default, and a gradient or typography token,
    which have no one CSS value (a stroke style of dashes and caps is `dashed`, the Format module's
    fallback). The JSON reader stays on `proc_macro` alone
    and now reads arrays and numbers. Migration: move `scales` to typed groups (`"#1d1b26"` becomes
    `{"colorSpace": "srgb", "components": [...], "hex": "#1d1b26"}`, `"4px"` becomes
    `{"value": 4, "unit": "px"}`), each `themes` selector to a modifier context in a resolver, and
    `"@media": {cond: selector}` to the modifier's `"media": {context: cond}`; the system-dark
    selector is now `:where(:root):not([data-theme])` (the page chose no theme, with one attribute's
    specificity so a later modifier's rule wins) rather than
    `:root:not([data-theme="light"])`. The examples' generated CSS adds the default context's rule
    (`[data-theme="light"]`, `[data-density="comfortable"]`) and otherwise differs only in that
    selector, a 6-digit hex where one was written with 3, `0px` for a zero length token and no
    space between layered shadows: card-market's tokens 1,946 to 2,171 bytes (473 to 492
    brotli), the watchlist's 14,713 to 14,952 (1,752 to 1,759 brotli)
    ([#394 tokens! reads a token format of our own instead of the W3C Design Tokens format (DTCG 2025.10)](https://github.com/todd-skelton/corten/issues/394)).

93. Breaking: a form's `FieldName` carries the type its control sends, `FieldName<T = Untyped, P = Root>`
    (`condition: Option<Condition>` is a `FieldName<Condition>`, an `Option` and a `Vec` taken off;
    `&'a str` and `Cow<'a, str>` are `String`), so a control given another field's name does not
    compile: corten-ui's `Picker::new` takes only a field that reads its choices' type,
    `Number::new` only a field that reads a number (`Numeric`), `RequiredWhen` only values of its
    control's type (`ValueOf`: the field's own type, or text for a text field or a checkbox; now
    any `Display`, so a number field's values are numbers), and `SubmitKey::field` and
    `SubmitKey::again` only a `SubmitKey` field. `name={..}`, `Errors` and `Within` take a name
    of any type as before, and a name is still the size of a `&str`, so nothing renders
    differently: the item page's render ran 35,591,033,717 instructions against 35,611,790,177 on
    its base (median of 3, callgrind, release build), within the typed build's own range of 22.5
    million, so no difference. To migrate:
    1. An app's own helper typed `FieldName` (now `FieldName<Untyped>`) takes a type parameter,
       `fn hint<T>(f: FieldName<T>)`, or the type when it draws one kind of control
       (`fn whole_number(f: FieldName<String>)`; the errors point at the calls, the fix is the
       helper's signature), and one typed `FieldName<Group>` for a group's names is now
       `FieldName<T, Group>`: the place moved to the second parameter.
    2. Names of fields of different types are different types: a list of them, or a comparison of
       two, takes `.untyped()` (`[f.email.untyped(), f.price.untyped()]`).
    3. A typed control takes no field whose type it cannot see through: one using its struct's
       generics or a lifetime other than borrowed text, a `Box`, a type alias or a path through
       `$crate` is `Untyped` or its own type, so a `Picker` or `NumberField` on it is written by
       hand or the field's type spelled out
    ([#358 A form's FieldName carries no type, so a Picker<Condition> can name another field](https://github.com/todd-skelton/corten/issues/358)).
94. `bench/fullstack`: `droplet.mjs` turns off the droplet's automatic package updates before
    provisioning, so a reboot comes back on the kernel `perf` was installed for, and `count.mjs`
    writes the running kernel to `count-kernel.txt`, has `perf` count an idle second before any
    load (stopping with the reason when it cannot), and logs when `perf` attaches and when the
    load starts. CI's and CONTRIBUTING's publish dry run builds in a target of its own
    ([#466 Benchmark round r8 has no instruction counts: its droplet locked up twice when count.mjs started counting](https://github.com/todd-skelton/corten/issues/466)).
95. A view lent to a call written in markup (`{keep_query(&at, &html! { .. })}`, a whole argument)
    borrows what it uses, as one lent to a prop does since item 70, so the rest of the page reads
    the same local; it was a `move` closure and a second read was E0382. A view given by value, or
    in a closure of its own (`.map(|g| f(html! { .. }))`), still owns what it uses, and so does one
    in a macro's arguments (`format!(..)`). A callee that needs the view `'static` or `Send` takes
    it by value; a lent view that changes a captured `mut` local now changes the page's one (it
    changed a copy). `{&html! { .. }}` alone is lent too. `keep_query`'s docs and the guide now
    lend the controls, and say to clear the fields a form resets on the route it is given. The item
    page's render runs 0.07% more instructions (median of 3, callgrind, release build:
    35,618,729,963 against 35,592,439,055 on its base), which has no lent call: noise between
    builds of the same markup.
    ([#485 A view lent to a call inside markup ({keep_query(&r, &html! { .. })}) still takes ownership of the locals it uses, so a local also read elsewhere fails with E0382](https://github.com/todd-skelton/corten/issues/485)).
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
20. A catalog argument may be camelCase (`{updatedAt}`): the method's parameter is `updated_at`,
    and `by_key` and `Message<"updatedAt">` use the catalog's name, so an API's params with the
    same names fill it. Two names that make one parameter in a message (`{updatedAt}` and
    `{updated_at}`) are an error naming both. Found by the Chase Sets pilot, whose shared catalogs
    have 365 camelCase arguments
    ([#58 A catalog message cannot take a camelCase argument ({updatedAt})](https://github.com/todd-skelton/corten/issues/58)).
21. A patch with a top-level element the browser would drop (no `id`, or text outside an
    element) is an error in debug builds that names it: `req.patch`, `patch_with_status` and
    `req.invalid`'s form answer 500 instead of a patch, so a router test or the first request in
    development fails, and a `corten::live` fragment panics. Release builds send the patch as
    before, with no check. Found by the Chase Sets pilot, whose refused write-off answered 422
    with the API's message in a `<div>` without an id, which the page never showed
    ([#61 A patch element without an id is dropped in the browser, and nothing says so before](https://github.com/todd-skelton/corten/issues/61)).
22. A handler that panics is answered as a 500 with the error page in the request's mode, and the
    panic is logged, instead of the connection closing with no response.
23. `html!` that is `let`s and then one component is a patch when the component is one, as it is
    a document when the component is one: a component declared `-> impl Patch` whose markup made a
    value and passed it to a patch component did not compile. Found by the Chase Sets pilot, which
    wrapped four views in `match () { () => { .. } }` to get past it
    ([#82 A view that is lets then one patch component is not a patch (One<F> never implements Patch)](https://github.com/todd-skelton/corten/issues/82)).
24. A patch or a `<corten-live>` fragment no longer closes what the user opened: a `<dialog>` stays
    open (`showModal()` again when it was modal, so the page behind stays inert), a `<details>` stays
    unfolded and a shown popover stays shown. One the user did not touch takes the new markup's
    state, so the server can still close a dialog it sent open. The runtime is 268 B larger gzipped
    (7,807 to 8,075 B). Found by the Chase Sets pilot, whose item page's live market book closed a
    reference dialog the user was reading
    ([#84 A patch or live fragment closes a <dialog> the user opened and folds a <details> they unfolded](https://github.com/todd-skelton/corten/issues/84)).
25. A checkbox or radio without a `value` attribute is found by its name when a patch keeps what
    the user changed (its selector asked for `[value="on"]`, which only the attribute matches, so
    it fell back to its place).
26. A `data-keep-place` navigation keeps what the user sees around the control it focuses: an
    element that scrolls on its own (a filter rail) keeps its scroll, and a `<details>` open on
    screen stays open (one the user closed stays closed), so the control is in view and focus is not
    lost to `<body>` when the server draws its group closed. `examples/hello`'s filter is in a
    scrolling rail of two groups, checked by the smoke test
    ([#96 A data-keep-place navigation resets a scrolling container (a filter rail), so the control it focuses is out of view](https://github.com/todd-skelton/corten/issues/96),
    [#97 A data-keep-place navigation folds the <details> the user unfolded, so focus can fall to the body](https://github.com/todd-skelton/corten/issues/97)).
    The runtime notes the elements that scroll as they do, so keeping a place reads only those (0.1 ms on a page of 4,770 elements in Chromium, against 15 ms reading every element's scroll; measured), and is 167 B larger gzipped (8,362 to 8,529 B, measured).
27. A release build of `corten` compiles without a warning: the check that a patch has an id on
    each top-level element, which runs in debug builds only, is no longer compiled into release.
28. The error page keeps the headers of the failure it replaces: a request id, `Retry-After`,
    `WWW-Authenticate` and `Server-Timing` a handler or a middleware set were dropped (only
    `Allow` was kept), on the sign-in redirect and the offline page too. The ones that described
    the old body and `Cache-Control` are the error page's
    ([#126 The error page drops the headers a middleware or handler set on the failed response (Retry-After, a request id, Server-Timing)](https://github.com/todd-skelton/corten/issues/126)).
29. A client navigation to a page in another layout loads it in full: the shell key includes the
    layout (a hash of where its `html!` document is written, a constant), so the first layout's
    header and footer no longer stay around the next page's `<main>`, and links between layouts
    need no `data-reload`. The default error document is a layout of its own too
    ([#122 Client navigation between pages with different layouts keeps the first layout, because the shell key does not include the layout](https://github.com/todd-skelton/corten/issues/122)).
30. A link to a route that redirects to another site (`req.redirect(&uri)` or `req.moved(&uri)`)
    leaves the app in one request: a client navigation gets `corten-location` and the browser
    goes there, where before the runtime's `fetch` followed the redirect, the CSP's `connect-src`
    refused it with a console error (the hover's prefetch too), and the page reloaded to follow it
    again
    ([#131 A client navigation to a route that redirects to another origin is blocked by connect-src, then reloads](https://github.com/todd-skelton/corten/issues/131)).
31. `html!` reads a pattern with braces as Rust does: `if let Shape::Circle { r } = shape { .. }`
    and `for Point { x, y } in points { .. }` took the pattern's braces for the body and did not
    compile. `corten fmt` reads them the same way
    ([#142 html! if let cannot take a struct pattern: the pattern's braces are read as the body](https://github.com/todd-skelton/corten/issues/142)).
32. A patch keeps the files chosen in a file input it replaces, as it keeps typed text, so a
    refused upload redraws its input marked invalid with the files still chosen; a successful
    answer to the form that sent them clears them. The runtime grows by 47 bytes gzipped
    ([#143 A patch drops the files chosen in a file input it replaces, so a refused upload cannot mark its input invalid](https://github.com/todd-skelton/corten/issues/143)).
33. A `data-keep-place` swap keeps the scroll of an element that scrolls on its own (a filter
    rail) when the swap lands before that element's `scroll` event, which comes at the next
    frame: a control focus scrolled into view, or a slow machine. It counts the focused control's
    ancestors as scrolled, and an element whose scroll it restored. The runtime grows by 38 bytes
    gzipped
    ([#130 Firefox sometimes loses the filter rail's own scroll after a data-keep-place swap (smoke: 76 -> 0)](https://github.com/todd-skelton/corten/issues/130)).
34. A link to a new page goes to the top of it (or to its `#id`) in Chromium too. Chromium could leave
    a page the runtime swapped in where the last one was scrolled (seen from About to Home, and on
    a link to the page already open), though Firefox and WebKit went to the top; the runtime now
    scrolls itself, as it already did for a page the server sent to an `#id`. Back and forward
    still return to where the user was. Found when `examples/watchlist`'s taller About page made
    the smoke test's "a plain link still goes to the top" fail. The runtime is 6 B larger gzipped
    (9,018 to 9,024 B, measured).
35. A field with a hint of its own can point at the hint and at its error: `e.aria(field).hint(&HintId)`
    writes one `aria-describedby` naming both. Before, the hint needed a second
    `aria-describedby` on the tag, which the browser dropped, or the error's id written by hand.
    A start tag that writes `aria-describedby` or `aria-invalid` beside `e.aria(..)` no longer
    compiles, since the browser would keep only one of them
    ([#147 A field with its own hint cannot also point at its error: Within::aria writes a second aria-describedby, which the browser drops](https://github.com/todd-skelton/corten/issues/147)).
36. A body over `Upload::limit` can be answered with the form and a message. `Upload`'s reads
    return `UploadError` (`TooLarge`, `Malformed`, `Fields`), which `?` still turns into a 413,
    400 or 422; a handler matches `UploadError::TooLarge` instead of comparing statuses or reading
    `Content-Length`. The rest of an over-limit body is now read past (up to 64 MiB) before the
    answer: before, Chromium (and Firefox, for a 20 MB body) took the closed connection for a
    network error and showed nothing, not even the error page
    ([#150 A handler cannot read an Error's status, so an upload over Upload::limit (413) cannot draw the form again with a message](https://github.com/todd-skelton/corten/issues/150)).
37. A page whose handlers call a script module the app does not serve is an error in debug builds
    (a router test, `cargo run`), naming the module to list; a release build logs it. Before, it
    rendered with a 200 and only the browser found the 404, after which the module's handlers did
    nothing
    ([#157 A page that names a script! module the app does not serve renders fine; only the browser finds the 404, and its handlers silently do nothing](https://github.com/todd-skelton/corten/issues/157)).
38. A `Relay` keeps the upstream's `Content-Security-Policy` in place of the app's, so a relayed
    page's own scripts run. Before, the app's policy replaced it and blocked them
    ([#164 A Relay loses the upstream's Content-Security-Policy, so a relayed page's own scripts are blocked](https://github.com/todd-skelton/corten/issues/164)).
39. A connection that never finished its request head was held open for as long as the client
    liked (measured: 150 s and counting), since `serve` never bounded it. A head now
    has 30 seconds (`App::header_timeout`), after which the connection is closed, and
    `App::max_connections(n)` caps how many are open. Both apply to `serve_on` too: Corten now
    accepts connections itself instead of through `axum::serve`; the check costs a keep-alive
    request about 300 instructions, which the accept loop saves, so a request costs what it did
    ([#170 A client that never finishes its request head holds a connection open forever](https://github.com/todd-skelton/corten/issues/170)).
40. The watchlist smoke's check that controls named `action` or `method` do not change a post
    failed in Firefox about one run in three: it saw the post the check before it sent (whose
    message it already matched), which Firefox reported late, not a second post. That check now
    waits for its own answer, and the next one names each post's stock. The runtime never sent
    a post twice: the route the smoke intercepts saw one per submit
    ([#161 Firefox smoke: the shadowed-controls form is seen posting twice now and then](https://github.com/todd-skelton/corten/issues/161)).
41. `examples/seller-desk`: typing a price threw `unsent.add is not a function` and the
    "prices not saved" note never showed. The grid kept two `state(scope, ..)`s for one element,
    and an element has one, so the second got the first's object. They are one object now, and
    `state`'s docs say an element has one state. The seller-desk smoke test, which caught it, now
    runs in CI, and waits for what it checks instead of fixed timers.
42. In Chromium a submit button's `formmethod` decides how a navigating form goes, as it does in
    Firefox and without the runtime. The runtime read the method from `NavigateEvent.formData`,
    which Chromium sets from the form's own method: a `formmethod="post"` button in a GET form was
    taken over and sent as a GET of its `formaction` with no fields, so the write was dropped
    without a word, and a `formmethod="get"` button in a POST form loaded the page in full instead
    of swapping it. It now reads the method from the submitter, else the form. `corten.js`:
    22,834 to 23,032 B (8,380 to 8,442 B gzip)
    ([#190 In Chromium, a POST submit button in a GET form is sent as a GET with no fields: the runtime reads the method from NavigateEvent.formData](https://github.com/todd-skelton/corten/issues/190)).
43. A `data-patch` form reads its submit button as the browser does: `formmethod="put"` (or any
    method the browser does not know) is a GET, an empty `formmethod` is a GET, an empty
    `formaction` is the page's own address, a relative one resolves against `<base href>`, and a
    `formenctype` of `multipart/form-data` sends multipart. It read the raw method and action attributes and only
    the form's `enctype`, so it sent a PUT where the browser sends a GET, used the form's method
    and action for the empty ones, and sent a `formenctype="multipart/form-data"` button's file
    as text. A form whose method is `dialog` now closes its dialog and sends nothing; before, it
    sent a `DIALOG` request. One function, `sendsAs`, reads the button for both the patch and the
    navigation path. `corten.js`: 23,032 to 23,264 B (8,442 to 8,503 B gzip)
    ([#196 A data-patch form reads its submit button's raw formmethod and formaction, so formmethod=put sends a PUT where the browser sends a GET](https://github.com/todd-skelton/corten/issues/196)).
44. A `Relay`'s answer is passed on as it came with feature `compress` on too, as its rustdoc said
    (the guide said a relayed event stream was compressed; it now says none is): a buffered relayed body was compressed for a client that accepted zstd, brotli or gzip, and a
    relayed event stream (a model's streamed answer through a gateway) for one that accepted zstd
    or brotli, holding a compression context (480 KiB reserved for zstd, about 1 MB for brotli)
    from the budget the app's own live streams share, for as long as it ran, and every compressible
    relay got `Vary: accept-encoding`. A relay that should be compressed forwards the client's
    `Accept-Encoding` to its upstream, with a client that does not decode the body. Every other answer pays
    one look at its extensions: 11 instructions when it has none, 32 when it has one (callgrind)
    ([#205 With compress on, a Relay's answer is re-encoded per connection, so a gateway holds a 480 KiB coder for every relayed model stream and cannot opt out](https://github.com/todd-skelton/corten/issues/205)).
45. `SetCookie::path` refuses a control character (a tab or line break) where it is set, and
    `SetCookie::new`, `path` and `remove` name the caller's line when they panic. Before, such a
    path passed `path` and panicked when the answer was sent
    ([#204 SetCookie::new panics on a value it cannot carry, so a token from another service can turn a sign-in into a 500](https://github.com/todd-skelton/corten/issues/204)).
46. The two smoke checks that failed now and then in CI say why when they fail: the early form
    submit (`scripts/smoke.mjs`) reports the navigation requests, responses, commits, failed loads
    and errors with their times, and ui-gallery's first page names the engine, the scripts that
    loaded and the failed requests instead of throwing a bare timeout. When either smoke throws (a
    page whose `load` never comes, as WebKit's first `goto` in `scripts/smoke.mjs` once did), it
    lists the requests its open pages are still loading, oldest first. None of these reproduced
    locally, so none is fixed yet
    ([#232 WebKit smoke: "a form submitted before the first page loads navigates" fails now and then](https://github.com/todd-skelton/corten/issues/232),
    [#252 ui-gallery smoke: the first page's combo box did not connect within 30 s in Firefox (CI, once)](https://github.com/todd-skelton/corten/issues/252)).
47. `bench/fullstack`'s browser pass stopped in round r7: card-market refuses a price alert on an
    item with no listings (`item_12345`), and the pass waited 30 s for "Alert set". The form
    action is no longer timed on that item, for any app, since the others set an alert Corten
    refuses and the two are different work; the empty item's load, sort and client navigation are
    measured as before. Medians of the action now come from eight of ten runs per profile
    ([#259 Benchmark round r7 has no browser numbers: Corten refuses the price alert on item_12345, which has no listings, and browser.mjs times out waiting for 'Alert set'](https://github.com/todd-skelton/corten/issues/259)).
48. `ReadAgain::from(&route)` in a page's markup starts its read, as in a patch, on a full load
    and after a navigation: only those parts are read again, and what is beside them (a form the
    user is filling in, a payment provider's frame) stays. Before, the runtime acted on it only in
    a patch, so a page had to start the first read with a script of its own. It stops when the
    page is left or an element with an id beside the tag is replaced; with JavaScript a page that
    reads parts again is not read whole, so its head's `ReadAgain` is for a browser without it.
    The watchlist's alerts page reads its status line and list so. The runtime grows by 273 B
    (72 B brotli)
    ([#276 A page cannot ask for one of its parts to be read again, so a page holding a third-party frame needs a script to start the read](https://github.com/todd-skelton/corten/issues/276)).
49. examples/ui-gallery's live regions stay in the page while they are empty: the toaster's root,
    the form's result and the menu's result were `display: none` until a message came, so the
    message was often not announced. The toast's box is drawn only while there is a message, and
    its text is the box's color (it was the page's muted grey, under 4.5:1 contrast on the dark box).
50. A `data-patch` form whose submit an `on:submit` handler cancelled was sent anyway: the runtime
    sent it before the page's handlers ran. A handler that stops the submit's propagation still
    gets the patch ([#199 A data-patch form still sends when an on:submit handler cancels the submit event](https://github.com/todd-skelton/corten/issues/199)).
51. A `data-patch` form's body is the browser's: a file field is the file's name (it was
    `[object File]`), line breaks are CRLF, a `text/plain` form is sent as text, not urlencoded,
    and neither content type names a charset. A file chosen again while the first is on its way is a
    new submit unless it is the same file (name, size and time), not a double click ([#200 A data-patch form's urlencoded body differs from the browser's: a file is [object File], line breaks are not CRLF, and text/plain is sent urlencoded](https://github.com/todd-skelton/corten/issues/200)). With 50, `corten.js` grows 201 bytes gzip.
52. The idle-connection test measured the timeout from its own call, not from the clock's tick the
    connection went idle at, so it failed under load when the clock's thread lagged. It now checks
    what the clock promises, and expiry itself is unchanged
    ([#279 watch test: an idle connection can expire early when the clock's thread lags, and the test fails under load](https://github.com/todd-skelton/corten/issues/279)).
53. A patch that took the focused element away (a row removed, or the last row of a list drawn
    again without it) left focus on the body, so a keyboard or screen reader user was sent to the
    top of the document. Focus now goes to the nearest element around it that is still on the
    page, focusable only for the move; so does focus on an element patched into one that cannot
    take it (a button now disabled). `corten.js` grows 158 bytes gzip ([#267 A patch that removes the focused element drops focus to the body](https://github.com/todd-skelton/corten/issues/267)).
54. CI's browser job could hang for an hour or more when a Playwright call never returned (Firefox,
    after the watchlist smoke closed another site's page), holding the runner with no report of
    where. The job now has a 30 minute limit (a passing one takes 11 to 14), and every browser smoke
    stops itself after five minutes with no check finished, printing the last check it finished and,
    for the runtime's smoke, the requests still loading ([#317 CI: the browser job can hang for an
    hour in the watchlist smoke, and has no timeout](https://github.com/todd-skelton/corten/issues/317)).
55. The ui-gallery smoke failed now and then in WebKit with "due to access control checks" for
    `/?sale=last`: WebKit reports a request to the page's own origin that did not finish (the
    hover prefetch, or a load the next navigation dropped) that way, as a page error, and
    Playwright does not always report that request as failed. Such a message for the gallery's
    own origin, which CORS never refuses, no longer fails the smoke; another origin's still does
    ([#301 ui-gallery smoke: WebKit logged a console error for /?sale=last, due to access control checks (CI, once)](https://github.com/todd-skelton/corten/issues/301)).
56. examples/seller-desk started its sales simulation with `tokio::spawn` in `main`, so it ran on
    through shutdown and outside the app. It is the app's `task` now, started once the server
    listens and stopped by its `Signal` at shutdown, as watchlist's market is; after a stall it
    runs one late tick instead of every missed one back to back
    ([#328 seller-desk starts its sales simulation with tokio::spawn, so it does not stop at shutdown](https://github.com/todd-skelton/corten/issues/328)).
57. corten-ui's `NumberField` could drop a step asked for with Enter or Space on a button: a
    pointer press set a flag that skipped the next click, and cleared it in a timer after the
    release, so a key that came before that timer ran (on a busy page) did nothing. The field now
    records the button each pointer pressed and skips only that press's own click (a key's click
    has `detail` 0 and always steps); a press released off the button, cancelled, or on a button
    disabled at its bound makes no click and is forgotten ([#316 ui-gallery smoke (CI, Chromium): a keyboard
    step on the second number field is lost while the first is held](https://github.com/todd-skelton/corten/issues/316)).
58. ui-gallery's RelativeTime smoke pauses its fake clock, so only the test moves the page's time.
    Under load, a slow read used to add real seconds, and "each second" failed
    ([#314 ui-gallery smoke: RelativeTime's each-second check fails under load because the fake clock still runs in real time](https://github.com/todd-skelton/corten/issues/314)).
59. examples/seller-desk looked like a static table. Price cells look like fields; the sortable
    headers show they sort and which way (`aria-sort`); the rows scroll inside the grid under a
    header that stays, with the item count above them (an archive's answer replaces it, and while
    the filter is on it says how many rows it shows of how many, kept true as an archive removes
    rows); a row at quantity 0 is marked sold out, kept current by the live levels (a `sold_out`
    hole of the row's view); on a phone the table keeps the name, quantity and price (it stays a
    table, so each cell has its header); and the statement has a total row
    ([#332 Polish: seller-desk does not look like the client-heavy screen it is](https://github.com/todd-skelton/corten/issues/332)).
60. examples/watchlist signed a visitor in by trusting a `viewer` cookie that held their name, so
    anyone could set one and be anyone, and signing out only cleared it in that browser. Signing in
    now starts a session the server keeps, named by a random token in an `HttpOnly` `Secure`
    cookie; the profile picture is kept in the session, not a cookie of its own; signing out ends
    the session on the server, and signing in again ends the one the browser had
    ([#322 watchlist signs a visitor in by trusting a plain viewer cookie, so anyone can set it and act as anyone](https://github.com/todd-skelton/corten/issues/322)).
61. examples/card-market's item page left the right half of a desktop screen empty: the picture
    stacked under the title. From 56rem the picture is the left column, beside the title, the
    tags and the price summary; a phone's one column is unchanged. The page gains two wrapping
    `<div>`s, 74 bytes of its 11,435
    ([#330 Polish: card-market's item page leaves the right half empty on desktop](https://github.com/todd-skelton/corten/issues/330)).
62. CI's Firefox stalled now and then in the watchlist smoke ("still loading: nothing"), at a
    different step each time, and once lost a page's script context in the ui-gallery Copy check.
    The likely cause is Playwright 1.60's Firefox driver, which could lose the commit of a new
    page's first navigation to a document sent with `Cross-Origin-Opener-Policy: same-origin`, as
    Corten's pages are by default ([microsoft/playwright#42731 Firefox page.goto never resolves on the first navigation to a page with COOP same-origin](https://github.com/microsoft/playwright/issues/42731),
    fixed in its Firefox build 1551); the race did not reproduce locally. The tooling is on
    Playwright 1.64.1 (Firefox build 1555), and CI's browser image follows it; Firefox still
    applies COOP. Its WebKit 27.2 logs a CSP refusal for the style it sets inside every `<select>`
    (WebKit bug 327212); in that WebKit the smokes set that line aside and report every other CSP
    violation from the page's `securitypolicyviolation` events instead (`scripts/browser-bugs.mjs`)
    ([#350 CI's Firefox stalls after the early form check, closing a page whose route is still registered](https://github.com/todd-skelton/corten/issues/350)).
63. A nav link a patch put on the page kept the `aria-current` the server wrote, judged against
    the request that made the patch (a form's action, a `ReadAgain`'s `data-from`) rather than
    the page. The runtime now marks nav links after a patch that puts one on the page or removes
    one, as after a navigation; a patch without one (a ticker's) only looks through the elements it
    put. `corten.js` grows 65 bytes gzip ([#347 A nav link a patch puts on the page keeps the mark the server judged against the form's action, not the page](https://github.com/todd-skelton/corten/issues/347)).
64. A `#[route(prefix = ..)]` field in a `#[route(query)]` set was written into links but read
    back empty: `Params` read a set with serde's derive, which knows nothing of the prefix. A set
    is now read field by field the way a route reads its own, so a list it leaves out is empty
    (it was a 400 without `#[serde(default)]`), a field's own `#[serde(default)]` fills what the
    query leaves out, and a set needs no `Deserialize`. **Breaking:** a set's serde keys that a
    route's reader does not follow (`alias`, `flatten`, `deny_unknown_fields`, the struct's own
    `default`, ...) make `Params` of a route that includes it not compile, naming the key, as they
    do on the route (a struct's `default` would read a link's `None` back as the default's
    value); and a route whose own query names and its sets' could be read back as each other's
    (one name written twice, which was a 400, or a prefix another name starts with) does not
    compile. Reading a route with a four-parameter set costs 4,107 instructions, up from 3,669,
    the same as those parameters on the route itself (callgrind, release build)
    ([#310 A #[route(prefix)] field inside a #[route(query)] set is written into links but Params reads it back empty](https://github.com/todd-skelton/corten/issues/310)).
65. examples/watchlist's header wrapped to two rows on a phone, and its alerts page used the left
    half of a desktop screen. The header is one row at 390 px: the viewer's name (and picture) is
    the link to their account, where the Account link was, and a visitor has a shorter "Sign in"
    button in its place. The alerts form and list sit side by side from 48rem
    ([#331 Polish: watchlist's phone header wraps, and the Today column changes its decimals](https://github.com/todd-skelton/corten/issues/331)).
66. examples/ui-gallery read as a grid of components rather than one listing screen. The
    listing form sits beside one column of the draft's other cards (Market, Activity, Draft,
    Stock), so cards of different heights leave no holes. Its "Ships to" boxes and its pricing,
    now radios, are styled as the fields above them. The "Manage" card's menu is the Draft card's
    "More actions" button. The SKU's copy count shows only after the first copy ("Copies: 2"), and
    `/help` explains each part of the form
    ([#333 Polish: ui-gallery reads as a grid of components rather than a listing screen](https://github.com/todd-skelton/corten/issues/333)).
67. card-market in French translated its headings but not its prices, ratings, conditions or
    card details. Its item page and cards now write a price in US dollars as the reader's locale
    writes money (`0,79 $US`), a seller's rating with the locale's decimal separator, and the
    conditions and detail labels the catalog names (`Peu joué`, `Rareté`); a value the catalog
    does not name is shown as the item API sent it. The page reads these from fields the JSON
    leaves out (`#[serde(skip)]`: cents, a `Decimal` rating, a `Grade`, a `Detail`), so
    `/api/items/{id}` still answers the benchmark's shared view model, and in English the page
    reads as before (`parity.mjs`). The listing sort compares each row's `data-cents`, the
    cents the server sorts by, where it compared a float that was `NaN` without a price, and
    keeps rows with the same price in the server's order both ways, so a second click restores it, and the
    scripts guide says when a `data-` attribute is the right tool rather than a view. The smoke
    checks a price, a rating and a condition in French
    ([#323 card-market in French translates its headings but not its prices, ratings or card details](https://github.com/todd-skelton/corten/issues/323),
    [#329 card-market sorts listings by a data-price string instead of through the rows' views](https://github.com/todd-skelton/corten/issues/329)).
68. `messages!` with an empty catalog (`{}`, an app that uses `t` only for the locale's formats:
    numbers, money, dates and relative times) failed to compile under `#![deny(warnings)]`: its `by_key` had a `match` whose only arm
    returned, so the line after it was unreachable. A doctest compiles an empty catalog with
    warnings denied.
69. Two components with one name in different modules (each feature's `SettingsForm`)
    with a stylesheet or a script fail to build, at the second one, instead of the app panicking
    when it starts ("both style .settings-form__*", "two scripts named"), and so do two `script!`
    modules with one name: like an id, each class prefix and script name is an exported symbol,
    and the symbols now say what clashed
    (`corten__two_components_style__settings_form`, `corten__two_scripts_are__settings_form`,
    `corten__two_ids_are__settings_form`, which was `corten_id__settings_form`)
    ([#368 Two features' components or ids with the same name pass cargo check and fail at link or at server start](https://github.com/todd-skelton/corten/issues/368)).
70. A `data-patch` form's `SubmitKey` is renewed once the whole answer has arrived (a patch, a
    page, or one the runtime does not draw), not when its headers do: an answer whose body was cut
    off on the way dispatches `corten:error` with status 0 and keeps the key, so the retry is the
    same submission and a committed write does not apply twice. An older submit that a newer one
    of the same form aborted no longer renews the key the newer one was sent with. The minified
    runtime grows by 97 bytes (25,413 to 25,510), 29 gzipped (9,249 to 9,278) and 33 with brotli
    (8,348 to 8,381).
71. `rows()` takes a `#[component(view)]` without a key, as the scripts guide always said it could:
    it gives a `RowList` (`at`, `of`, `indexOf`, `remove`, `swap`, `clear`, iteration), without
    `get`, `push` or `replace`, which need a key. A view no longer needs a key only to satisfy
    `tsc`. `scripts/typecheck-fixture.js` checks both
    ([#418 rows() takes only a view with a key, so a keyless #[component(view)] a script only reads fails tsc (TS2769), though the guide presents key as optional](https://github.com/todd-skelton/corten/issues/418)).
72. A `use` inside a `#[corten::routes]` module that brings in a handler, and a handler written
    inside the module, are a compile error there that says handlers are named from where the module
    is declared, not an unused import and "cannot find value" at each handler. A `use` the module
    names for itself (a field's type, a `pub use`, a `#[cfg(test)]` import) is left alone
    ([#359 A handler imported inside a #[corten::routes] module is not found, and the error does not say handlers are named from where the module is declared](https://github.com/todd-skelton/corten/issues/359)).
73. `corten check` runs the tests before `tsc`, so the test that writes `types/app.d.ts` declares a
    new view or event before a script's import of it is checked, instead of `tsc` refusing the
    import with TS2305 and stopping the check. The scripts guide says one component list goes to
    both `App::components` and the types test
    ([#386 corten check runs tsc before the test that writes types/app.d.ts, so a script using a new view fails with TS2305 until the tests are run by hand](https://github.com/todd-skelton/corten/issues/386)).
74. `#[component(css = path::Owner)]` shares a stylesheet from any module of the crate that can see
    the owner, named by its path, so a screen whose panels are in files of their own has one sheet; it worked only in the
    owner's module, and the error named a hidden module with `cargo add` advice. An owner that
    cannot be found is reported at the path written. The owner's module hands its sheet's path to
    the helper at compile time; nothing changes at run time
    ([#377 css = Owner works only in the owner's module, so a screen's views in several files cannot share its stylesheet, and the error names a hidden module](https://github.com/todd-skelton/corten/issues/377)).
75. A view lent to a prop inside another view (`action={&html! { .. }}`) borrows what it uses
    instead of owning it, so the rest of the markup can read the same local; it was a `move`
    closure and a second read was E0382. A view given by value, and the view at the top of an
    `html!`, still own what they use
    ([#414 A view in a prop (action={&html! {..}}) takes ownership of the locals it uses, so a local also read elsewhere in the page fails with E0382](https://github.com/todd-skelton/corten/issues/414)).
76. An element with the `hidden` attribute stays hidden when a component's class gives it a
    `display`: `components.css` (or, when only areas have stylesheets, each area's file) starts with
    `[hidden]:where(:not([hidden=until-found i])){display:none!important}`, so neither an app nor a
    design system for corten-ui has to know to add it. The rule adds 69 bytes to the stylesheet,
    before compression ([#423 The [hidden] { display: none !important } rule corten-ui requires is
    not in the template and nothing checks it, so a component's display silently shows hidden
    parts](https://github.com/todd-skelton/corten/issues/423)).
77. Two `#[component(view)]`s or two `#[event]` types with one name in one crate fail to compile,
    as two components with one name do, rather than the app when it starts (across crates, the
    linker refuses the pair when the program uses code next to both, and the start-time check
    catches the rest)
    (`corten__two_views_are__Row`, `corten__two_events_are__RowToggled`, and
    `corten__two_events_named__row_toggled` for the event's name in the page)
    ([#371 Two features' #[event] types or views with the same name still fail only when the server starts](https://github.com/todd-skelton/corten/issues/371)).
78. A `for`, `if` or `match` inside a `for` in `html!` compiles, in a patch and in any view: it
    failed with "use of moved value" on the whole `html!`, since the marker that says whether each
    part is a patch was moved on the loop's first pass. A patch can now redraw rows known from data
    in one loop, with a branch per row. A view refused as a patch now names the kind of part that
    is not one (`ComponentOrExpressionNotAPatch`: a component declared `-> impl Render`;
    `IdNotADeriveIdType`; `TextOrElementWithoutId`), though not its place in the view. The markers are zero-sized and the macro's output is otherwise the same, so rendering
    is unchanged
    ([#383 An if or match inside a for in html! fails with E0382 (use of moved value), and a patch's E0277 does not name the part that is not a patch](https://github.com/todd-skelton/corten/issues/383)).
79. During a pin bump (`Cargo.toml` moved to another corten revision, `Cargo.lock` not resolved
    yet), `corten dev`, `fmt` and `check` told the user to install the corten the app was leaving:
    the note now compares the command with the revision `Cargo.toml` asks for when it is not the one
    the lock was written for, and says nothing for a tag or branch whose commit the lock does not
    know yet
    ([#475 During a pin bump, the CLI version note reads the stale Cargo.lock and tells you to install the older corten](https://github.com/todd-skelton/corten/issues/475)).
80. A `Box<dyn Href>` (a link a shared component's function prop returns, when one enum of every
    page's route would join separate feature modules) is written as `href={boxed}`, as the `dyn
    Href` it holds; it was an `Href` but not an attribute value. A value that is neither text nor a
    link nor an attribute value is now an error that says so ("this value cannot be written in an
    attribute", naming what an attribute takes) instead of one about a private `__attr` method. The
    item page's `render_cost` (release, callgrind, 3 runs) is 35,567,415,208 instructions against
    35,548,560,642 on main (+0.05%; the page writes no boxed link, so this is layout)
    ([#457 Box<dyn Href> is an Href but not an attribute value, so href={boxed} fails with an error naming a private method](https://github.com/todd-skelton/corten/issues/457)).
81. A query word an enum parameter does not have (`?state=due_now`, an old bookmark or an address
    edited by hand) reads as missing, `None` or the field's `#[serde(default)]`, so `Params` shows
    the page with its filter, sort or tab at its default where it answered 400 with serde's text, and
    a `#[route(prefix = ..)]` entry holding one is left out of the map. A
    parameter that must have a value, a value that is not a word (`?page=two`), a list holding one
    and a form's body are still refused. Links write what they did
    ([#447 A typed query filter whose address holds an unknown value refuses the whole page (400) where it should read as unset](https://github.com/todd-skelton/corten/issues/447)).
82. A `corten-macros` script test bounded three parses on the wall clock (under 2 or 3 s), so check 1
    failed on a busy machine with nothing wrong. Each case now passes in one run under a second, and
    otherwise is checked by its growth (at a size about four times as long as at a quarter of it,
    not sixteen), so load slows both runs alike
    ([#495 A corten-macros script test's wall-clock bound (under 2 s) fails when the machine is busy, so check 1 goes red without a cause](https://github.com/todd-skelton/corten/issues/495)).
84. The browser smokes retry a page's `goto` or `reload` once on `net::ERR_NETWORK_CHANGED`
    alone, which Chromium raises when Docker changes the machine's network interfaces mid-run (twice
    in CI in a week, each blocking a merge until a rerun); any other failure, a second one included,
    still fails. One helper, `scripts/steady.mjs`, wraps each smoke's browser at launch. Test code
    only
    ([#497 Browser CI fails at random on net::ERR_NETWORK_CHANGED during a smoke navigation, blocking merges until a rerun](https://github.com/todd-skelton/corten/issues/497)).
85. `req.redirect_with_notice` from a write on a plain `http://` origin that is not loopback (a LAN
    tool at `http://automation.lan:3001`) reaches its page: a browser refuses the `Secure`
    `__Host-corten-notice` cookie there, so the notice was dropped without a trace. Corten reads
    the write (an `Origin` of this host over `http://`, no `Sec-Fetch-Site`) and sets a plain
    `corten-notice` cookie instead (`HttpOnly`, `SameSite=Lax`, a minute), which a page reads
    only from a request without `Sec-Fetch-Site`, so on HTTPS another site on the domain cannot
    plant one in a current browser; an app with `Policy::hsts()` never sets or reads it, and
    `localhost` and `127.0.0.1` keep `__Host-`. The ui-gallery smoke renames from the machine's
    LAN address in each engine (it fails on the image before this change). The item page's
    `render_cost` (release, callgrind, 3 runs) is 35,612,123,545 instructions against
    35,603,094,486 on main (+0.03%, within the runs' spread: a page's GET only compares the
    method and looks at its cookies once, as before)
    ([#498 redirect_with_notice is lost without a trace on a plain-HTTP LAN deployment: __Host- cookies need a secure origin](https://github.com/todd-skelton/corten/issues/498)).
86. A task (`App::task`) that returns before shutdown began is logged with its name, as one that
    panics is (`corten: task "pricing worker" returned before shutdown began`): a task that could
    not start or gave up ended without a trace, so a log could not tell it from one that runs. A
    task that did its work once and returned is now logged as well: the app guide has it wait for
    the signal when done, and a task the configuration turns off not registered (a `match` on the
    configuration in the builder chain). Nothing a request does changes
    ([#503 A task that returns before shutdown is silent: a task the configuration turns off, or one that cannot start, ends without a trace](https://github.com/todd-skelton/corten/issues/503)).
87. A release build of an app no longer warns that `Assets::module_tag` is never used: its only
    caller is the debug-build check that a page's handlers call a module the app serves, so it is
    built in debug builds only. Nothing a page renders changes
    ([#500 A release build warns that Assets::module_tag is never used: its only caller is a debug-build check](https://github.com/todd-skelton/corten/issues/500)).

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
6. The full-stack benchmark tables in the README and `bench/fullstack`, server and browser, are
   from round r8 (2026-10-30, one c-8 droplet, Xeon Platinum 8168, the tree at `2976f15`); its
   instruction counts are r7's (2026-10-28, Xeon Platinum 8280, `a9b1163`), since r8's droplet
   locked up at the counting step; `bench/live` is still r3 (Xeon Platinum 8280). r7's results are kept without a
   browser pass. Corten serves 15,747 item pages and 24,652 JSON responses a second from memory at
   64 users (r7: 18,758 and 24,976), and 3,616 item-API pages against ASP.NET Core's 1,065 (r7:
   3,540 and 1,157). Its page from memory fell 16.1% while the other apps moved by a median of
   -2.0%, after a 19.9% fall from r6 to r7. A page costs 95,949 user instructions and the JSON
   69,572 in r7. A page from memory costs 63 µs of CPU (15,890 per CPU-second; ASP.NET Core 4,184),
   300 pages a second take 4.7% of a core (ASP.NET Core 25%), and each MiB serves 960 pages a
   second (ASP.NET Core 48). Next.js and Laravel failed as before. The item page loads 10.2 KB of
   JavaScript (gzip; r7 9.3 KB).
7. `Form` in the prelude is `corten::Form` (Added 20), and a route's parameters its `Params`. Against
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
11. The README's example names its view `home_page` (it was `page`), so `home` serving it reads as
    `req.page(home_page(&req, ""))` rather than `req.page(page(..))`; the forms recipe in the agent
    skill follows.
12. The brand pack is in `docs/brand/` (mark, lockups, favicon, palette, traced from
    `corten-weathering-concept.png`), and the README opens with the lockup, in its light version
    when the reader's system is in dark mode.
13. The app guide says why a JSON 400 under `App::api` costs about 500 instructions more since
    every route goes through the checks (Added 65): the checked `get` marks a JSON route's
    failed response with an extension, profiled under callgrind in place of "most likely".
14. The examples use the patterns they lacked. `examples/item-page`: a moved address (`req.moved`),
    `Policy::new().hsts()` behind `HTTPS`, `.deadline`, `.ready` (the item API answers), a
    category page shared longer by CDNs (`.shared(..)`) with its breadcrumbs as `JsonLd`, and an
    alert on an item with no listings answered with a whole-form error (`Errors::add_form`).
    `examples/seller-desk`: a `Middleware` (`Server-Timing`), an archive made idempotent by
    `SubmitKey`, a `NoStore` statement page, a nav from `req.nav`, and a typed event
    (`#[event] PriceEdited`) a price cell sends the grid, which counts the prices not saved.
15. The examples are named for what they are: `examples/hello` is `examples/watchlist` (a stock
    watchlist) and `examples/item-page` is `examples/card-market` (a trading-card store), with
    their packages, images and CI steps. Every example has a designed layout in light and dark.
    The watchlist is styled utility-first from its token file's utilities (`display`, `gap`,
    `p`, `bg`, `columns`, ...), with the repeated combinations named in `src/theme.rs`, and the
    styles guide compares that with Tailwind.
16. Every pull request is reviewed against the values before it merges, by someone who did not
    write it, who looks for the way the change breaks a value (CONTRIBUTING.md, Review). The
    order that decides between conflicting values now sits with the values in the README
    rather than in AGENTS.md, which has an agent start a new agent for the review, and a pull
    request template asks for the checks, the measurements and the findings.
17. `examples/watchlist` is a stock watchlist whose pages each do a job a user has. You watch
    and remove stocks from a market of eight; each browser's list is kept on the server under a
    random id in a `list` cookie (the 10,000 used most recently) and changed under a lock, so two
    removals sent at once both land. Each row's Remove button submits the watch form (`formaction`), and the list and the
    form are patched in place. Each row shows today's change, from the server (`t.percent`) and
    from the script (`t.js().percent(2)`). The market moves from a task of the app's
    (`App::task`, which stops at shutdown) rather than `tokio::spawn`. The About page is now
    News: one query struct (`#[route(query)] filter: NewsQuery`) for the sector filter, whose
    sectors are `#[derive(UrlValue)]`, the "only stocks I watch" filter and Load more; a briefing
    signup checked on the server (field errors, `req.invalid`) that sends the browser back to its
    form (`route.at(Id)`); and stories that link to a place on the publisher's page
    (`Elsewhere::at`). The saved page is now Alerts, a price alert checked on the server and
    answered while its read catches up (`ReadAgain`), and the avatar upload has a form on the
    Account page, with sign out. The smoke test follows the new pages: a list a patch replaces
    opens one new stream and closes the old one, the list survives a reload, the change cells
    follow the feed, a sector's box lists its stories, a refused price comes back marked with
    focus on it, and a story's place stays in the address.
18. `examples/card-market` shows a partner API and a write limit. `POST /api/alerts` sets an
    alert for a partner's user: the handler takes `Caller<Partner>`, found by the `X-Api-Key`
    header (`API_KEYS`, no partner without it), so a request without a known key is a JSON 401,
    and the form's rules answer with the fields. A middleware of the app's (`WriteLimit`)
    allows about 20 writes a minute from one client (`ClientIp`, an IPv6 address by its /64;
    `WRITES_PER_MINUTE` to change it) and answers the next with a 429 and `Retry-After`; a
    partner's alerts are counted by the partner, whatever address it sends from; reads pass
    straight through, so pages a CDN keeps stay public, and agents answer to their token, which
    no longer defaults to `dev-token` (`AGENT_TOKENS`). The smoke test runs the container with
    Docker's networks as its proxies (`PROXIES`), so each forwarded address has its own count.
19. The examples show believable data and fit a phone. card-market, run with no item source
    (`API_BASE` or `ITEM_SOURCE`), serves a demo catalog built into it (`fixtures/demo.json`):
    original cards in three sets, sellers, offers and a sealed box with its contents. Its home
    page lists the source's items as cards (name, set, lowest price) and a category page lists
    its items; against the item API, which has no list, the home page links the first ids as
    before. Tables too wide for a phone scroll inside their panel (the item page) or their box
    (seller-desk's grid). seller-desk's statement writes its amounts with `t.money` and its
    words from a catalog, not a hand-written `$`.
20. `examples/cluster` is `examples/listings-feed`, named for what it is (a seller's listings
    feed served by several instances), like the other examples; the crate it shows is still
    `corten-cluster`. Its package and image are `listings-feed` and `corten-listings-feed`.
21. The styles guide says where a page whose path is the prefix of other areas (`/account` above
    `/account/cart`) puts its CSS: in `App::components`, as the home page does, with the pilot's
    measurement
    ([#239 A page whose path is the prefix of other areas (/account above /account/cart) cannot have a stylesheet only it links](https://github.com/todd-skelton/corten/issues/239)).
22. The examples write no seam as text: card-market's header is a component with scoped
    classes (`SiteHeader`, lined up with the page below it) instead of `class="site"`, and the
    watchlist's `LivePrices` passes its feed's route to the script (`url::Stocks`) instead of
    taking `url::Stocks::PATH` as a `&str`. The watchlist's titles end in "| Watchlist" (its home
    is "Stocks") and its header fits a phone on two lines; card-market's error page links home,
    its item page's tables and price summary fit a phone, its alert form keeps less room under
    it, and its demo cards name their rarity twice (the chips and the details) instead of four
    times: the subtitle is the set and number, and no tag repeats a field or a chip.
23. Each example has a README: what the app is, how to run it (`corten dev` and Docker) with
    the environment it reads, and which file holds each piece. The main README lists the examples
    in the order to read them, watchlist first
    ([#230 No example has a README that says how to run it and where each feature is](https://github.com/todd-skelton/corten/issues/230)).
24. Work on a schedule is a task with tokio's intervals: `App::task`'s documentation and the
    guide (app.md, "On a schedule") show a job hourly at :15 and one every five minutes in one
    loop, so they never overlap, keeping each job's last run for a status page, run on a paused
    tokio clock in a doctest; the guide says what is left to the app (the monotonic clock's jumps,
    the next run's time, local wall times). The guide's task example and the watchlist's market
    task now use `MissedTickBehavior::Skip` (an overrun is followed by one late run; tokio's
    default, `Burst`, runs once for each missed mark, back to back) and poll the signal first
    (`biased`), so no run starts once shutdown has begun. Corten adds no scheduler. Corten's own
    tests build tokio with `test-util` for the paused clock: each tokio clock read in them checks
    one flag more; apps are not affected
    ([#278 An app cannot run a job on a schedule (hourly at :15, every 5 minutes, daily at 03:30), so each writes the clock, the missed-mark skip and the no-overlap rule itself](https://github.com/todd-skelton/corten/issues/278)).
25. The app guide (Areas) and the skill say how a page in an area that another permission opens
    (an order's review form under `/account/purchases`) is declared: a second routes module in the
    same area with its own `caller`, passed in one call,
    `.area(|area| reviews::area(orders::area(area)))`, rather than an `identify` that picks the
    permission from the path. A test holds two modules' callers apart in one area
    ([#273 A page under an area cannot take its own caller, so a nested form that needs another permission checks it by path or by hand](https://github.com/todd-skelton/corten/issues/273)).
26. CONTRIBUTING's check 2 unpacks sources into a throwaway `--tmpfs`, so its publish dry run builds
    against Corten's crates as they are now, not a copy an earlier run left in the shared volume
    ([#300 Check 2's publish dry run builds corten-ui against a stale corten unpacked in the shared registry volume](https://github.com/todd-skelton/corten/issues/300)).
27. `corten_ui::NumberField` says it is for counts and measures (a quantity, a weight), not
    money, and the UI guide says a price is a text input with `inputmode="decimal"` read by
    `t.parse_money`: a number input takes only the browser's decimal syntax, and its value is an
    `f64`. ui-gallery's price and lowest offer are such inputs now, read as money and written
    with `t.money`, its quantity is a `u32`, and its second number field is the shipping weight
    ([#286 corten-ui's NumberField cannot show back a refused entry or take a price in the reader's locale](https://github.com/todd-skelton/corten/issues/286),
    [#325 ui-gallery reads the listing's quantity and price as f64 and writes money with format!](https://github.com/todd-skelton/corten/issues/325)).
28. CONTRIBUTING (Pull requests, item 2) says which measurement fits which change: instructions per
    request for work done in process (callgrind in process, `count.mjs` with `perf` over a socket),
    times as well when memory changes (`render_cost`, with its command and fixture), time on a
    quiet host for waiting and concurrency (`ab.mjs`, `cold-start.mjs`), and bytes for what the
    client receives (`render_cost`, `bytes.mjs`), from alternated release runs reported with their
    median and range; Tooling lists valgrind and `perf`. AGENTS.md, the README's values and the
    pull request template point to it rather than naming `nanoseconds per request`.
29. The checks in `CONTRIBUTING.md` build into volumes named after the checkout's folder
    (`corten-target-<folder>`, `corten-msrv-target-<folder>`) instead of one `corten-target` for
    every checkout, where cargo could take one checkout's build for another's and fail to compile
    code that was fine.
30. The views guide (Text: literals or the catalog) and the design rules say when text is a
    literal and when it goes through the catalog: an app with one locale writes fixed text as
    literals and keeps in its catalog only counts, text with arguments and text a script fills;
    an app with more than one locale writes all of it through the catalog. seller-desk's
    statement, ui-gallery's activity card and listings-feed's move status, which were the only
    fixed text those one-locale apps put in their catalogs, are literals now
    ([#218 Examples write user-facing text two ways: watchlist has a catalog but writes most text as literals](https://github.com/todd-skelton/corten/issues/218)).
31. Every prop is written where a component is used, an empty one too (`head=""`). The views guide
    says why: stable Rust cannot leave a field to a default, and filling a left-out `impl Render`
    prop would fix its type. It also shows how several answers share one component's props, with
    a function for the common value and `..` for the rest. A `compile_fail` doctest on
    `#[component]` covers a left-out prop. ui-gallery's five answers build their `Gallery` that
    way, where each wrote all six props
    ([#210 A component prop cannot be left out, so pages pass head="" and other empty values](https://github.com/todd-skelton/corten/issues/210)).
32. The views guide says how a layout gets who is signed in: as a prop from the `Caller` its
    handler took, not by reading the request. Rendering is synchronous and `Identify` is async, so
    the alternative needs a second async step before full-page renders, with its own rules for a
    failed lookup, caching and the page key
    ([#209 Every page handler extracts the viewer only to pass it to the layout](https://github.com/todd-skelton/corten/issues/209)).
33. `design.md` (Limits 14), `docs/architecture.md`, `docs/guide/app.md` and
    `docs/guide/live-data.md` say that HTTP/2 or HTTP/3 to browsers comes from the proxy or CDN in
    front of `corten::serve`, and that an app with live streams needs it: over HTTP/1.1 a browser's
    six connections to a host run out
    ([#400 architecture.md relies on HTTP/2 to share connections, but Corten serves only HTTP/1.1 and no guide says HTTP/2 must come from a proxy](https://github.com/todd-skelton/corten/issues/400)).
34. `docs/architecture.md` lists number, money and date formatting as the framework's (`t` on
    CLDR data, and `t.js()` in scripts), not as removed
    ([#401 architecture.md says number, money and date formatting was removed from the framework, but t.money, t.number and t.date_time are built in](https://github.com/todd-skelton/corten/issues/401)).
35. `design.md` Choices 11 to 14 say why a request's mode is a `corten` header and not `Accept` or
    `Sec-Fetch-*`, why the runtime prefetches links itself and not with Speculation Rules, why
    component CSS is scoped by prefix and not `@scope`, and why a feed patch is keyed `upsert` and
    `remove` and not JSON Patch or Merge Patch, each with what would let Corten move
    ([#396 The request mode is a corten header of our own, and no doc says why not Accept or Sec-Fetch-*](https://github.com/todd-skelton/corten/issues/396),
    [#397 The runtime prefetches links its own way, and no doc says why not the Speculation Rules API](https://github.com/todd-skelton/corten/issues/397),
    [#398 Component CSS is scoped by prefixing classes, and design.md does not say why not native @scope](https://github.com/todd-skelton/corten/issues/398),
    [#399 A feed patch is an upsert/remove format of our own, and no doc says why not JSON Patch (RFC 6902) or Merge Patch (RFC 7396)](https://github.com/todd-skelton/corten/issues/399)).
36. The requests guide documents `corten:error` (when a `data-patch` submit has no answer to draw,
    with `detail.status`, 0 for none or a cut-off body), and the scripts guide the one way a script
    learns what the server decided on a submit that did draw: the patch's element runs
    `on:connect` with the handler's values and dispatches a typed event, as
    `examples/deck-builder`'s `SaveNote` does. No second, status-only event is added
    ([#432 A script that submits a data-patch form cannot learn the answer's outcome, so the deck builder carries it in an on:connect element](https://github.com/todd-skelton/corten/issues/432)).
37. The requests guide says why the submit key is a form field and not the `Idempotency-Key`
    header: a browser without JavaScript cannot set one, and the header is still an IETF draft
    ([#395 An action takes its idempotency key only as a SubmitKey form field, not the Idempotency-Key header data clients send](https://github.com/todd-skelton/corten/issues/395)).
38. The live-data guide follows a job another process runs (a worker writing to a shared database):
    one `App::task` reading into a `watch` channel for `live_watch` when many pages follow it, or a
    `corten::live` loop that sends only when the version moved, ends after the settled state and
    gives up after a few failed reads. No `live_poll` helper is added: the interval, what to read
    less often and when the job is done are the app's, and a helper would take a closure for each
    ([#388 A live part that follows a job in another process writes its own poll loop, version cursor and end](https://github.com/todd-skelton/corten/issues/388)).
39. The scripts guide says a value a handler computes that the server also draws is a second copy,
    and how to keep the two in agreement: draw it on the server only when a round trip is fast
    enough, or keep the arithmetic in a `script!` module that imports nothing and run one table of
    cases through both in a test, which `corten check` runs. Corten does not compile Rust to the
    script or add a runner of its own: a test already does it
    ([#444 A value a script computes as the user types is also drawn by the server, so its arithmetic is written twice (Rust and JS) with nothing checking they agree](https://github.com/todd-skelton/corten/issues/444)).
40. The app guide's task loop gives its step the signal, and says why: a run in progress is outside
    the loop's `select!`, so one that can take longer than `App::shutdown_grace` (a remote read
    with a 10 s timeout) stops its slow awaits at the signal, and Corten does not race the whole
    step, which would leave half a write. A new section, "A queue a request fills", has a worker's
    loop: the claim and lease are the app's SQL; the job's work, made once and pinned, races its
    renewal (bounded by a timeout, the next a period after it ended) and the signal, which is safe
    because each of its writes is whole and a job can be done again; the work is dropped before the
    claim is released; `finished` and the work's writes are fenced by the claim, so a lost lease
    writes nothing; a retry's delay is the queue's (`run_after`), so a job that fails at once is not
    claimed again at once; a claim error is logged and waited out; the idle wait is a `sleep` and
    a `Notify`, not an interval. `App::task`'s example gives its refresh the signal and tests its
    shutdown with `Signal::task`. Corten has no job queue or worker type
    and no `every` helper: the queue is the app's storage, and a helper that raced the step would
    drop work mid-write
    ([#488 A worker for jobs that requests queue builds its own leased claim, heartbeat, requeue, progress and retry on App::task, once per worker](https://github.com/todd-skelton/corten/issues/488)).

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
