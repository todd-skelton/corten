# The app

[Back to the guides](../../README.md#documentation)

`App::new(routes).serve().await` puts everything a production app needs around your axum
routes. Each default can be replaced in one line.

| Concern | Default | Override |
|---|---|---|
| Address | `HOST` (`127.0.0.1`), `PORT` (`3000`) | environment, or `app.router()` to serve it yourself |
| Shell | a minimal HTML document | `.shell(fn(Document) -> impl Render)`, or `fn(Document, Option<C>)` with a context |
| Who is asking | nothing | `.context(\|headers\| async { Some(viewer) })`: shells, page handlers (`req.context::<C>()`) and error pages see it; Corten's own routes and API routes skip it |
| Error pages | 404, 405 and 500 inside the shell, in the request's mode (page, swap or JSON); the error is logged, never shown | `.error_page(\|status\| (title, view))` |
| JSON-only routes | none | `.api("/api", routes)`: every failure under the prefix is JSON, with no shell and no context ([below](#json-only-routes)) |
| Security | a CSP that allows Corten's inline tag (the import map) by hash (a nonce only when the shell or a handler asks for one), nosniff, referrer, frame and opener headers, a same-origin check on unsafe methods | `.policy(Policy::new()...)` |
| Caching | static files and the runtime cached forever; everything rendered `private, no-cache` ([below](#caching)) | return `Cache::public(max_age)` next to a response that is the same for everyone |
| Static files | `static/` (or `STATIC_DIR`), component scripts and the components' stylesheets (one file) at content-hashed, immutable URLs; each script preloaded on the pages whose handlers use it | `.assets(dir)`, `.components(&[..])` |
| Health | `/health/live`, `/health/ready` (always ready) | `.ready(\|\| async { db_ok().await })` |
| Shutdown | Ctrl-C and `SIGTERM` end live streams, then the server | |
| Logs | errors, startup and shutdown to stderr (feature `log`) | set your own `tracing` subscriber first |
| Compression | pages per request (zstd, brotli, gzip), static files once at the highest levels (feature `compress`) | |
| Installable app | off | `.pwa(Pwa::new(manifest))`: manifest, generated service worker, offline page |
| Languages | none | `.messages::<Msg>()`: `req.t()`, `doc.t()` and `doc.lang()` follow `Accept-Language` |
| Dev mode | when `CORTEN_DEV` is set (`corten dev` sets it): errors shown on the page, open pages reload after a rebuild | `.dev(bool)` |

`App::deadline` (30 seconds unless set) bounds how long a handler may take to start its answer;
see [Cancellation](cancellation.md).

```rust,ignore
App::new(routes)
    .shell(shell)                                  // the document around every page
    .context(|headers| viewer_from(headers))       // who is asking, for the shell and handlers
    .components(&[TOKENS, OfferList])              // components with scripts or styles
    .messages::<Msg>()                             // the locales the shell's lang follows
    .pwa(Pwa::new(Manifest::new("Shop", "Shop").icon("icons/192.png", "192x192")))
    .serve()
    .await
```

The shell renders on full page loads only; client navigation swaps `<main>`. Every page
carries a key hashed from the context and the deploy, and when a navigation or a form answer
arrives with a different key (sign-in, sign-out, a new release), the runtime loads the whole
document, so the shell is never stale.

## The request path

`App` is one middleware around the routes: the same-origin check, the request signal, the
context, the deadline, error pages, security headers and compression run in one future per
request. Corten's own inline tag (the import map) is the same on every page of a deploy, so the
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

fn app(pages: Router) -> App {
    let api = Router::new().route("/api/items/{id}/price", get(price));
    App::new(pages).api("/api", Api::new(api).allow_origin("https://partner.example"))
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
   'none'`. `Api::new(routes).context()` runs `App::context` for an API that serves the app's
   own signed-in users.
3. The deadline, the request signal, compression and the same-origin check on writes are the
   same as for pages.
4. `Api::allow_origin` lets one site's pages call these routes from the browser: Corten answers
   its CORS preflight, adds `Access-Control-Allow-Origin` and lets its writes through. The
   pages keep refusing that site's writes. The site authenticates with a token: a write from it
   that carries the user's cookies is refused, and no `Access-Control-Allow-Credentials` is sent.
5. `Json` in the prelude is `corten::Json` (feature `json`). It reads a request body as
   axum's `Json` does, with the same rejections, and writes the response with
   `serde_json::to_vec` into a `Vec` the body keeps. axum's `Json` works too, but writes
   through a `BytesMut` writer that costs more for each of the many small writes JSON makes.

The routes keep their full paths, so a `#[derive(Route)]` struct names the same URL in the
router and in links; under `corten dev` a route outside its prefix answers 500, saying so.
`.api("/", routes)` makes the whole app an API (the health checks and the runtime keep their
behavior).

A screen that a native app or an agent reads is not an API route: it stays a page that answers
with `req.screen`, so every surface shares one handler (see [Surfaces](../architecture.md)).
