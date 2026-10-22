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
| Security | CSP with a nonce for each document, nosniff, referrer, frame and opener headers, a same-origin check on unsafe methods | `.policy(Policy::new()...)` |
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
request. The CSP nonce is made only when a document asks for it (its `<head>`, or a handler
that takes `Nonce`); a navigation swap, a patch, JSON or a static file gets the same policy
without a nonce source. Static files, the runtime and the health checks skip `App::context`, so
a session lookup runs only for the requests that can use it.

Measured with callgrind (instructions per request, release build, in-process `oneshot`, one
run each; `bare axum` is the same handler with no layers):

| Request | Before | Now | bare axum |
|---|---|---|---|
| small JSON handler under `App` (`Accept: application/json`) | 36,324 | 27,584 | 15,039 |
| the same handler under `.api("/api", ..)` | | 27,242 | |
| a small page, full document | 41,280 | 31,742 | |
| unknown URL, JSON 404 | 26,930 | 19,892 | |
| unknown URL, HTML 404 page | 37,130 | 29,325 | |

## JSON-only routes

A page answers every request mode, so how it fails depends on what the client asked for: the
error page for a browser, JSON for `Accept: application/json`. A route with no screen (a
webhook, an OAuth callback, a call between services, a public API) is called by programs, and
most HTTP clients send `Accept: */*` or nothing. Mount those routes under a prefix:

```rust
use corten::{prelude::*, axum::Json};

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
2. There is no shell, no nonce and no context. The CSP is `default-src 'none'; frame-ancestors
   'none'`. `Api::new(routes).context()` runs `App::context` for an API that serves the app's
   own signed-in users.
3. The deadline, the request signal, compression and the same-origin check on writes are the
   same as for pages.
4. `Api::allow_origin` lets one site's pages call these routes from the browser: Corten answers
   its CORS preflight, adds `Access-Control-Allow-Origin` and lets its writes through. The
   pages keep refusing that site's writes. The site authenticates with a token: a write from it
   that carries the user's cookies is refused, and no `Access-Control-Allow-Credentials` is sent.

The routes keep their full paths, so a `#[derive(Route)]` struct names the same URL in the
router and in links; under `corten dev` a route outside its prefix answers 500, saying so.
`.api("/", routes)` makes the whole app an API (the health checks and the runtime keep their
behavior).

A screen that a native app or an agent reads is not an API route: it stays a page that answers
with `req.screen`, so every surface shares one handler (see [Surfaces](../architecture.md)).
