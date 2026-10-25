# The app

[Back to the guides](../../README.md#documentation)

`corten::serve(App::new(routes)).await` puts everything a production app needs around your
axum routes. Each default can be replaced in one line.

| Concern | Default | Override |
|---|---|---|
| Address | `HOST` (both loopback addresses, `127.0.0.1` and `[::1]`, so `localhost` answers at once however it resolves), `PORT` (`3000`) | environment, or `app.router()` to serve it yourself |
| Shell | a minimal HTML document | `.shell(fn(Document) -> impl Render)`, or `fn(Document, Option<C>)` with a context |
| Who is asking | nothing | `.context(\|headers\| async { Some(viewer) })`, first: it makes the app an `App<Viewer>`, and shells, guards, page handlers (`req.context::<C>()`) and error pages see it; Corten's own routes and API routes skip it |
| A part of the app | none | `.area::<Admin>(\|admin\| admin.route::<AdminUser>(get(user)))`: its routes under the prefix `Admin::PATH` (a route outside it does not compile), with `.components(&[..])` for a stylesheet only its pages load and `.guard(\|viewer, req\| ..)` for who may use it ([below](#areas-and-guarded-routes)) |
| Who may use the app | anyone | `.guard(\|viewer, req\| ..)`: every route needs the guard's `Access::ALLOW`; `.unguarded::<SignIn>()` opens one route ([below](#areas-and-guarded-routes)) |
| Error pages | 404, 405 and 500 inside the shell, in the request's mode (page, swap or JSON); the error is logged, never shown | `.error_page(\|status\| (title, view))` |
| JSON-only routes | none | `.api::<ApiRoot>(\|api\| api.route::<ItemJson>(get(item_json)))`: every failure under the prefix is JSON, with no shell and no context ([below](#json-only-routes)) |
| Security | a CSP that allows Corten's inline tag (the import map) by hash (a nonce only when the shell or a handler asks for one), nosniff, referrer, frame and opener headers, a same-origin check on unsafe methods | `.policy(Policy::new()...)` |
| Caching | static files and the runtime cached forever; everything rendered `private, no-cache` ([below](#caching)) | return `Cache::public(max_age)` next to a response that is the same for everyone |
| Static files | `static/` (or `STATIC_DIR`), component scripts and the components' stylesheets (one file, plus one per area that lists components) at content-hashed, immutable URLs; each script loaded by the pages whose handlers use it | `.assets(dir)`, `.components(&[..])`, `Area::components(&[..])` ([styles](styles.md#areas)) |
| Middleware of your own | none | tower layers on the routes run inside Corten's (after the context and guards); layers around `app.router()` run outside it, given to `corten::serve` ([below](#your-own-middleware)) |
| Health | `/health/live`, `/health/ready` (always ready) | `.ready(\|\| async { db_ok().await })` |
| Shutdown | Ctrl-C and `SIGTERM` end live streams, then the server | |
| Logs | errors, startup and shutdown to stderr (feature `log`) | set your own `tracing` subscriber first |
| Compression | pages per request (zstd, brotli, gzip), static files once at the highest levels, live streams per event with the context kept, so a redraw costs what changed (zstd or brotli; feature `compress`) | |
| Installable app | off | `.pwa(Pwa::new(manifest))`: manifest, generated service worker, offline page |
| Languages | none | `.messages::<Msg>()`: `req.t()`, `doc.t()` and `doc.lang()` follow `Accept-Language` |
| Dev mode | when `CORTEN_DEV` is set (`corten dev` sets it): errors shown on the page, open pages reload after a rebuild | `.dev(bool)` |

`App::deadline` (30 seconds unless set) bounds how long a handler may take to start its answer;
see [Cancellation](cancellation.md).

```rust,ignore
let app = App::new(routes)
    .context(|headers| viewer_from(headers))       // who is asking: the app is an App<Viewer>
    .shell(shell)                                  // the document around every page
    .components(&[TOKENS, OfferList])              // components with scripts or styles
    .area::<Admin>(|admin| admin::routes(admin).components(&[SalesChart]).guard(staff_only))
    .messages::<Msg>()                             // the locales the shell's lang follows
    .pwa(Pwa::new(Manifest::new("Shop", "Shop").icon("icons/192.png", "192x192")));
corten::serve(app).await
```

The shell renders on full page loads only; client navigation swaps `<main>`. Every page
carries a key hashed from the context and the deploy, and when a navigation or a form answer
arrives with a different key (sign-in, sign-out, a new release), the runtime loads the whole
document, so the shell is never stale.

## The request path

`App` is one middleware around the routes: the same-origin check, the request signal, the
context, the deadline, error pages, security headers and compression run in one future per
request. It is a tower layer of its own rather than axum's `from_fn`, which boxes the rest of the
stack again for every request (about 700 instructions). Corten's own inline tag (the import map) is the same on every page of a deploy, so the
CSP allows it by its SHA-256 hash, computed once at startup. A nonce is made only when something asks for one
(`doc.nonce()` in the shell, or a handler that takes `Nonce`). Static files, the runtime and the
health checks skip `App::context`, so a session lookup runs only for the requests that can use
it.

Measured with callgrind (instructions per request, release build, in-process `oneshot`; `bare
axum` is the same handler with no layers). "Three layers" and "one middleware" are one run each;
the last two columns are the median of 5 runs before and 4 after (range in parentheses), since
the same build moves by up to 2,000 instructions between runs:

| Request | Three layers | One middleware | Before caching | With caching | bare axum |
|---|---|---|---|---|---|
| small JSON handler under `App` (`Accept: application/json`) | 36,324 | 27,584 | 27,551 (27,381-28,007) | 28,255 (27,418-28,531) | 15,039 |
| the same handler under `.api("/api", ..)` | | 27,242 | 27,322 (27,136-27,371) | 27,578 (26,731-28,093) | |
| a small page, full document | 41,280 | 31,742 | 31,757 (31,728-31,814) | 30,912 (29,767-31,773) | |
| unknown URL, JSON 404 | 26,930 | 19,892 | 19,892 (19,876-19,943) | 20,218 (19,899-20,714) | |
| unknown URL, HTML 404 page | 37,130 | 29,325 | 29,325 (29,033-29,900) | 28,376 (27,182-29,333) | |

Caching adds one header lookup and insert to every response (`Cache-Control`) and two to read
the request's credentials: a few hundred instructions on JSON. Pages got cheaper, because a
page no longer makes a nonce (16 random bytes, base64, and the CSP string around it).

## Your own middleware

Corten is one tower layer around a plain axum `Router`, so the app's own layers go in one of
two places:

| Where | Runs | For |
|---|---|---|
| On the routes: `Router::new().route(..).layer(l)` for the app's router, `.route::<R>(get(h).layer(l))` in an area or an API | inside Corten's middleware, after the same-origin check, the context and the guards, so it sees the context in the request's extensions | a limit per user, a header on some routes, `tower-http`'s CORS beyond one allowed site |
| Around the whole app: `corten::serve(app.router().layer(l))` | outside, before anything of Corten's and after its compression, for every request including Corten's own routes | a request id, a trace span, a limit per address, a cap on body size |

There are no hooks of Corten's own: a tower layer is the one way in. axum's `from_fn` boxes the
rest of the stack again for every request (about 700 instructions, measured above), so a layer
on a hot path is worth writing as a `tower::Layer`.

## Caching

Static files, component scripts, the components' stylesheet and the runtime have content-hashed
URLs and are sent `public, max-age=31536000, immutable`; the PWA's worker keeps them. Everything
else `App` serves that does not set `Cache-Control` is sent `private, no-cache`: the browser
may keep it but asks again before using it, and no shared cache (a CDN, a proxy) stores it. A
page carries the shell built from `App::context`, so a shared copy could show one user's shell
to another.

A page that is the same for every visitor says so:

```rust,ignore
async fn home(req: Req) -> corten::Result<(Cache, Response)> {
    let cache = Cache::public(Duration::from_secs(60)).stale_while_revalidate(Duration::from_secs(600));
    Ok((cache, req.page("Home", view)?))
}
```

`App` checks what it can see before it lets a shared cache keep the page:

| The request or response | Sent |
|---|---|
| a full document, no `Cookie` or `Authorization`, the context found no one | as asked, plus `Vary: cookie` |
| a request with a `Cookie` or `Authorization`, or one the context found someone for | `private, no-cache` |
| a client navigation, a form answer or a data client's JSON from a page route | `private`, same lifetime: only the document is shared |
| a response that sets a cookie, or HTML with the request's nonce | `private, no-cache`, logged under `corten dev` |

The last row applies to a `Cache-Control` the handler wrote itself too; otherwise a handler's
own header is its choice. Corten cannot see a cookie a handler reads through its own extractor:
do not mark such a page public. An `App::api` route has one form only, so its public responses
stay public for clients without credentials. A cache in front of the app must key on `Vary`
(`corten`, `accept`, `accept-encoding`, and `accept-language` with `App::messages`).
`Cache::no_store()` keeps a response out of every cache, the browser's back/forward cache
included.

A full document never carries `corten-page`, so if a cache hands one to a client navigation by
mistake, the runtime loads the URL as a whole page instead of swapping it in.

There is no page cache on the server. Rendering is the cheap part of a page: the measured
item page renders in about 15 µs and costs several times that to compress (see the
`render_cost` test), and a server cache's key would have to cover the context, the locale, the
mode and the deploy, where a mistake shows one user's page to another. Cache the data a page
reads in the app, where it knows when the data changes, and let a CDN keep public pages. There
are no ETags on pages either, since they would save bytes but not the render.

## JSON-only routes

A page answers every request mode, so how it fails depends on what the client asked for: the
error page for a browser, JSON for `Accept: application/json`. A route with no screen (a
webhook, an OAuth callback, a call between services, a public API) is called by programs, and
most HTTP clients send `Accept: */*` or nothing. Mount those routes under a prefix:

```rust
use corten::prelude::*;

async fn price(Path(id): Path<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": id, "lowest": "$0.50" }))
}

#[derive(Route)]
#[route("/api")]
struct ApiRoot;

#[derive(Route)]
#[route("/api/items/{id}/price")]
struct ItemPrice<'a> { id: &'a str }

fn app(pages: Router) -> App {
    App::new(pages).api::<ApiRoot>(|api| api.route::<ItemPrice>(get(price)).allow_origin("https://partner.example"))
}
# let _ = app(Router::new()).router();
```

Under the prefix:

1. Every failure is one JSON shape, whatever the client accepts:
   `{"error":{"status":404,"message":"Not Found"}}`. That covers a handler's `corten::Error`, an
   extractor's rejection (a body that does not parse), an unknown URL, a wrong method (with
   `Allow`) and the deadline's 503. A 4xx message is sent, so write it for the client
   (`Error::new("no item 7").with_status(StatusCode::NOT_FOUND)`); a 5xx message stays in the
   log except under `corten dev`. A failure answered with a JSON body of the handler's own
   passes through, and validation errors are `Errors` as the error, which adds
   `"fields": {"email": "..."}`.
2. There is no shell, no nonce and no context, and responses are `private, no-cache` unless
   the handler returns a `Cache`. The CSP is `default-src 'none'; frame-ancestors
   'none'`. `.context()` in the API's chain runs `App::context` for an API that serves the app's
   own signed-in users.
3. The deadline, the request signal, compression and the same-origin check on writes are the
   same as for pages.
4. `Api::allow_origin` lets one site's pages call these routes from the browser: Corten answers
   its CORS preflight, adds `Access-Control-Allow-Origin` and lets its writes through. The
   pages keep refusing that site's writes. The site authenticates with a token: a write from it
   that carries the user's cookies is refused, and no `Access-Control-Allow-Credentials` is sent.
5. `Json` in the prelude is `corten::Json` (feature `json`). It reads a request body into a
   `FromJson` (item 6) with the value and the rejections axum's `Json` gives: Corten's reader takes
   an `application/json` body, and axum (`serde_json`) any other JSON content type and any body
   the reader gives up on. What it writes is a `ToJson`: derive it
   (`#[derive(ToJson)]`, also in the prelude) on the response type, next to `Serialize` if other
   surfaces read it too. The derive lays the JSON out when the crate compiles, so a response
   copies one byte string between each pair of values instead of writing every key, quote and
   comma, into a `Vec` sized from the type's last response that the body keeps. It reads
   `#[serde(...)]` and writes the bytes `serde_json` would. A `#[serde(flatten)]` field whose type
   is a struct that derives `ToJson` is written in place, its members laid out the same way; an
   attribute it cannot follow (`flatten` of a map or an enum, `serialize_with`) does not compile. A field whose type has only `Serialize` (an id or a
   date from another crate) is written by `serde_json`. `serde_json::Value` is a `ToJson`, as above.
   axum's `Json` works too, and writes any `Serialize` through `serde_json` and a `BytesMut`.
6. To read JSON another service answers, derive `FromJson` (in the prelude) next to `Deserialize`
   and read it with `corten::json::from_slice` or `from_str`. Corten's reader takes the fields the
   type declares and skips the rest; on anything it does not follow (an escaped key, a missing or
   duplicate field) `serde_json` reads the text instead, so the value and every error are
   `serde_json`'s. It reads `#[serde(...)]` as `Deserialize` does and refuses one it cannot follow
   (`flatten`, `deserialize_with`); a field of a type with only `Deserialize` is read by `serde_json`.

Each route is a `#[derive(Route)]` struct, and the API's prefix is one too (`ApiRoot` above), so
the router and the links name the same URL, and a route outside the prefix (`/apis/items`
under `/api`) does not compile rather than be served as a page. A handler that takes `State`
gets it on its method router: `.route::<ItemJson>(get(item_json).with_state(source))`. An API
whose prefix is `/` makes the whole app an API (the health checks and the runtime keep their
behavior). `Api::guard` decides who may call the routes, as
an area's guard does below; a caller sent to sign in gets a JSON 401.

A screen that a native app or an agent reads is not an API route: it stays a page that answers
with `req.screen`, so every surface shares one handler (see [Surfaces](../architecture.md)).

## Areas and guarded routes

A part of the app with something of its own (an admin area, an account, seller tools) is an
area: its routes under one prefix, and optionally a stylesheet only its pages load
([styles](styles.md#areas)) and a guard that decides who may use it. The guard reads the
context and decides; Corten answers for it, in the request's mode:

```rust
use corten::{axum::http::StatusCode, prelude::*, Error};

#[derive(Clone)]
struct Viewer { staff: bool, editor: bool }

#[derive(Route)]
#[route("/sign-in")]
struct SignIn;

#[derive(Route)]
#[route("/admin")]
struct Admin;

#[derive(Route)]
#[route("/admin/users/{id}")]
struct AdminUser<'a> { id: &'a str }

async fn dashboard(req: Req) -> Result {
    req.page("Admin", html! { <h1>"Admin"</h1> })
}

async fn user(req: Req, Path(id): Path<String>) -> Result {
    req.page("User", html! { <h1>{id}</h1> })
}

fn app(pages: Router) -> App<Viewer> {
    App::new(pages).context(|_headers| async { None::<Viewer> }).area::<Admin>(|admin| {
        admin.route::<Admin>(get(dashboard)).route::<AdminUser>(get(user)).guard(|viewer, req| match viewer {
            None => Access::sign_in(&SignIn),
            Some(v) if v.staff && (v.editor || !req.writes()) => Access::ALLOW,
            Some(_) => Access::refuse(Error::status(StatusCode::FORBIDDEN)),
        })
    })
}
# let _ = app(Router::new()).router();
```

| The guard says | Page, or a form post without JavaScript | Form patch | Data client, `App::api` route |
|---|---|---|---|
| `Access::ALLOW` | the handler runs | the handler runs | the handler runs |
| `Access::sign_in(&route)` | 303 to the route | `corten-location`, and the runtime goes there | JSON 401 |
| `Access::refuse(error)` | the error page for the error's status | the error page, swapped in | the JSON error |

1. The guard covers every route at the prefix and below it, whole segments only (`/admin`
   holds `/admin/users`, not `/administrators`), including routes added later, unknown URLs
   and wrong methods. A handler there cannot forget the check, and a refused request never
   reaches it or its extractors: the body is not read and nothing is written.
2. The area's prefix is a route type (`Admin`, whose path is `/admin`), and so is each of its
   routes, so the compiler holds them to it: a route typed wrong (`/admn/users` in the
   `/admin` area) does not build, rather than be served without the guard, and neither does a
   prefix that is not a plain path (`/admin/{id}`). The check runs when the app is built
   (`cargo build`, `cargo test`, `corten check`); `cargo check` alone does not reach it. A
   route the area cannot take (a `nest_service` of files under `/admin/files`) goes in the
   app's router: the guard and the stylesheet follow the path, not the router.
3. Who passes is the app's model. The guard gets the context `App::context` found (`None`
   when it found no one) and the request: `req.uri()`, to send the caller back after signing
   in (`Access::sign_in(&SignIn { next: Some(req.uri().path()) })`, or an
   `axum::http::Uri` on another site); `req.writes()`; and `req.is::<Route>()`. Refuse with a
   503 or a 502 when who they are cannot be known now (the session store is down), so no one
   is sent to sign in again for nothing.
4. The context's type is part of the app's: `App::context` turns `App` into `App<Viewer>`, so
   a guard reading another type, or a guard on an app without a context, does not compile.
   `App::area` hands its function an area of that type, so a guard reads the context's
   fields (`v.staff`) without naming it. An area a library ships is a function the app passes
   on, `.area::<Admin>(|admin| shop_admin::routes(admin).guard(staff_only))`.
5. Areas nest (`/admin` and `/admin/billing`, each with its own routes): every guard over the
   path must allow it, the outer one first. Under a guard the context runs for `App::api`
   routes too, without `Api::context`.
6. Under `corten dev` the error page and the JSON error say which guard refused, and every
   refusal is logged at `debug`.
7. Test it the way a user meets it: a router test that sends each route signed out and signed
   in without the permission.

How the common cases look:

| The app | Guard |
|---|---|
| A signed-in area (account, dashboard) | `.area::<Account>(\|account\| account.route::<..>(..).guard(..))`: `None` signs in, anyone else passes |
| A role's area, stricter inside | an area on `/admin` and one on `/admin/billing`, each with a guard |
| Anyone reads, signed-in users write (reviews, comments, a price alert posted back to its page) | the area's guard allows `!req.writes()` for everyone |
| Viewers and editors on the same screens | `v.can(VIEW) && (!req.writes() \|\| v.can(EDIT))` |
| Private everywhere (an internal tool) | `App::guard(..)` plus `.unguarded::<SignIn>()`, and any webhook the same way |
| An API only some callers may use | `.api::<ApiRoot>(\|api\| api.route::<..>(..).guard(..))` |
| One route under the prefix needs more | `req.is::<Export>()` inside the guard, or a nested area when the route has a prefix of its own |
| Only the author edits, members see their organization's records | the handler, which loads the record |

A page and the form that posts back to it share one URL in Corten (`get(show).post(save)`),
so a path cannot tell a read from a write; `req.writes()` can. It counts every method but
`GET`, `HEAD` and `OPTIONS` as a write, the same split as the same-origin check, so a method no
rule names (`PATCH`, `DELETE`) is held to the stricter rule.

`App::guard` covers the whole app and denies by default: every route, every unknown URL and
every API route is under it, so a route added later is private until it is marked, and it
answers before any area's guard. Corten's own routes (static
files, the runtime, health, the PWA's manifest, worker and offline page) are never under a
guard. `.unguarded::<SignIn>()` takes a route type, matched as the router matches its path, for
every method, so the sign-in form posts too. A sign-in answer that points at a URL a guard also
sends to sign in would loop in the browser; Corten answers it with a 500 instead, and under
`corten dev` the page says to mark the sign-in route.

A check that depends on the record (only the author may edit a draft) stays in the handler,
which loads the record; `Error::status(StatusCode::FORBIDDEN)` gives the same answers there.

A guard costs a request under it about 300 instructions (the prefix match, the check, the
context lookup). Measured with callgrind on the item page as a full document (release build, in
process, no compression, instructions in Corten's middleware and axum's routing together, per
request, two runs each): 3,968-3,975 with no guard, 4,059-4,066 with a context, 4,358 with a
context and the item route in an `/items` area with a guard, and 4,384-4,391 with a context,
`App::guard` reading `req.writes()` and one unguarded route. Before areas and the typed context
the same setups (a prefix guard on `/items` for the third) measured 3,969, 4,061, 4,355 and
4,399: the same, since the context type is checked by the compiler and an area is resolved at
startup into what the middleware already read. Checking that a route given in an area or an API
answers under its prefix adds no measurable work. A whole request is about 111,000
instructions, which moves by up to 2,000 between runs.
