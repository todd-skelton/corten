# The app

[Back to the guides](../../README.md#documentation)

`App::new(routes).serve().await` puts everything a production app needs around your axum
routes. Each default can be replaced in one line.

| Concern | Default | Override |
|---|---|---|
| Address | `HOST` (`127.0.0.1`), `PORT` (`3000`) | environment, or `app.router()` to serve it yourself |
| Shell | a minimal HTML document | `.shell(fn(Document) -> impl Render)`, or `fn(Document, Option<C>)` with a context |
| Who is asking | nothing | `.context(\|headers\| async { Some(viewer) })`: shells, handlers (`req.context::<C>()`) and error pages see it |
| Error pages | 404, 405 and 500 inside the shell, in the request's mode (page, swap or JSON); the error is logged, never shown | `.error_page(\|status\| (title, view))` |
| Security | CSP with a per-request nonce, nosniff, referrer, frame and opener headers, a same-origin check on unsafe methods | `.policy(Policy::new()...)` |
| Static files | `static/` (or `STATIC_DIR`), island scripts and the components' stylesheets (one file) at content-hashed, immutable URLs; each island preloaded on the pages that render it | `.assets(dir)`, `.components(&[..])` |
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
    .components(&[TOKENS, OfferList])              // islands and styled components
    .messages::<Msg>()                             // the locales the shell's lang follows
    .pwa(Pwa::new(Manifest::new("Shop", "Shop").icon("icons/192.png", "192x192")))
    .serve()
    .await
```

The shell renders on full page loads only; client navigation swaps `<main>`. Every page
carries a key hashed from the context and the deploy, and when a navigation or a form answer
arrives with a different key (sign-in, sign-out, a new release), the runtime loads the whole
document, so the shell is never stale.
