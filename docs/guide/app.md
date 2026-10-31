# The app

[Back to the guides](../../README.md#documentation)

`corten::serve(App::new(url::router())).await` puts everything a production app needs around
your routes. Each default can be replaced in one line.

| Concern | Default | Override |
|---|---|---|
| Address | `HOST` (both loopback addresses, `127.0.0.1` and `[::1]`, so `localhost` answers at once however it resolves), `PORT` (`3000`) | environment; `corten::serve_on(listeners, app)` for listeners of your own (a Unix socket) with the same shutdown, `corten::serve_wrapped(listeners, move \|tcp\| acceptor.accept(tcp), app)` for HTTPS ([below](#your-own-listeners)) |
| Names it answers to | `localhost`, IP addresses; any other `Host` gets `421` ([below](#host-names)) | `HOSTS=shop.example,.shop.example` in the environment, or `Policy::host("shop.example")` |
| The document | none: each page brings its own, the app's layout component with the page in it (`req.page(html! { <Layout ..>..</Layout> })`); `corten new` writes the layout | the layout is the app's code ([views](views.md#the-document)) |
| Routes | a `#[corten::routes]` module: each route names its handlers, and `router()` is what `App::new` takes; every handler is checked when the crate compiles, there and in `Area::route` and `Api::route` (Corten's `get`, `post`, ...), so no route reaches the app unchecked | |
| What handlers share | nothing | `#[corten::routes(state = Shared)]` and `router(shared)`; `#[derive(Clone, FromRef)]` on `Shared` lets a handler take one part, `State<Market>` (each field's type once; `#[from_ref(skip)]` leaves a field out) |
| What a route answers | its handler's return type: `HtmlResult` (a page, a patch, a redirect; failures are the error page) or `JsonResult<T>` (failures are JSON too; `Result<JsonResponse, JsonError>` for a status or cookie of the handler's own) | `Public<..>` around either for an answer that is the same for everyone, `NoStore<..>` for one nothing may keep ([below](#caching)) |
| The client's address | the connection's peer; no `X-Forwarded-For` is read | a handler takes `ClientIp`; behind a proxy, `PROXIES=10.0.0.0/8` in the environment or `Policy::proxy("10.0.0.0/8")` ([below](#the-clients-address)) |
| Who is asking | nothing: no lookup runs | a handler takes `Caller<Viewer>` (`Viewer: Identify`), looked up only then, once per request ([below](#who-is-asking)) |
| A part of the app | none | `.area(admin::area)` for a `#[corten::routes(area = Admin)]` module: its routes under the prefix (a route outside it does not compile), with `.components(&[..])` for a stylesheet only its pages load and `caller = Staff` for who may use it ([below](#areas)) |
| Error pages | every [`Failure`](#error-pages) in a plain document, in the request's mode (page, swap or JSON), sent with `X-Robots-Tag: noindex`; the error is logged, and shown only under `corten dev` | `.error_page(page, state)`: `async fn(Failure, Req, ..extractors) -> HtmlResult`, in the app's layout and for who is asking; `corten new` writes one that names each case |
| JSON-only routes | none | `.api(api::api)` for a `#[corten::routes(api = ApiRoot)]` module: every failure under the prefix is JSON, unknown URLs included, with no document ([below](#json-only-routes)) |
| WebSockets and relays | none | a handler takes `Upgrade` and returns its `websocket(..)` or `switch(..)`; `Relay` passes another server's answer on as it came ([below](#websockets-and-relays)) |
| Security | a CSP that allows Corten's inline tag (the import map) by hash (a nonce only when the layout or a handler asks for one), nosniff, referrer, frame and opener headers, a same-origin check on unsafe methods; forms go to this origin only | `.policy(Policy::new()...)`: another origin for scripts, styles, images, `fetch`, frames, fonts, or `.form_action("https://pay.example")` for a form (or a form's redirect) that hands the user to another site; one part of the app's pages only: `Area::policy` ([below](#a-policy-for-part-of-the-app)) |
| Caching | static files and the runtime cached forever; everything rendered `private, no-cache` ([below](#caching)) | return `Public<HtmlResult>` (or `Public<JsonResult<T>>`) for an answer that is the same for everyone |
| Static files | component scripts and the components' stylesheets (one file, plus one per area that lists components and one per set of areas that list the same components) at content-hashed, immutable URLs; each script loaded by the pages whose handlers use it | `.assets(Static)`: `static/`, compiled into the program by `corten::assets::embed!` (a constant per file, `req.asset(Static::STYLE_CSS)`) and served the same way ([below](#static-files)); `.components(&[..])`, `Area::components(&[..])` ([styles](styles.md#areas)) |
| Middleware of your own | none | `.middleware((RequestId, RateLimit))`: steps of the app's own, at one place in Corten's order; `Area::middleware`, `Api::middleware` for the requests at a prefix ([below](#your-own-middleware)) |
| Health | `/health/live`, `/health/ready` (always ready) | `.ready(\|\| async { db_ok().await })` |
| Work that lives as long as the app | none | `.task("refresh", refresh)`: started by `serve`, told when shutdown begins ([below](#tasks)); on a schedule, with tokio's intervals ([below](#on-a-schedule)) |
| Connections | a request's head (its line and headers) must arrive within 30 seconds of the connection opening or its last answer, or the connection is closed; no limit on how many are open | `.header_timeout(d)` (`None` behind a proxy that buffers heads), `.max_connections(n)`: past it, connections wait to be accepted |
| Shutdown | Ctrl-C and `SIGTERM` (on Windows Ctrl-C, Ctrl-Break, the console closing and system shutdown), or the app's `corten::shutdown()`, tell the tasks, end live streams, close the listeners, let requests in progress finish, then wait for the tasks and for work handlers finish (`Signal::finish`) (5 seconds) | `.shutdown_grace(d)`, under the platform's own bound (`docker stop`: 10 seconds) |
| Logs | errors, startup and shutdown to stderr (feature `log`) | set your own `tracing` subscriber first |
| Compression | pages per request (zstd, brotli, gzip), static files once at the highest levels, JSON held as bytes (`json::Raw`) once when it is sent again, live streams per event with the compression context kept, so a redraw costs what changed (zstd or brotli; feature `compress`); a `Relay`'s answer goes on as the upstream sent it | |
| Installable app | off | `.pwa(Pwa::new(manifest))`: manifest, generated service worker, offline page |
| Languages | none | `.messages::<Msg>()`: `req.t()` and `req.lang()` follow `Accept-Language` |
| Visitors' time zones | not known: dates are UTC unless a zone is given | `.client_zone()`: the runtime keeps the browser's zone in a `corten-zone` cookie, read by `Option<ClientZone>` ([Views](views.md)) |
| Dev mode | when `CORTEN_DEV` is set (`corten dev` sets it): errors shown on the page, open pages reload after a rebuild | `.dev(bool)` |

`App::header_timeout` (30 seconds unless set) bounds what comes before a request: a client that
opens connections and sends its head slowly, or never finishes it, cannot hold them (an idle
keep-alive connection closes after it too). A clock of the server's own checks it a few times a
second, so a connection closes within a tenth of the timeout of it (at most a second either way). It applies to `serve`, `serve_on` and `serve_wrapped` alike (where a TLS handshake's time counts toward it), on
every listener. `App::max_connections(n)` caps how many are open across them.
`App::deadline` (30 seconds unless set) bounds how long a handler may take to start its answer;
an area or an API with slower routes sets its own (`Area::deadline`, `Api::deadline`), so the
rest keep the app's. See [Cancellation](cancellation.md).

```rust,ignore
let app = App::new(url::router())
    .error_page(views::error_page, db.clone())     // in the app's layout, for who is asking
    .middleware((RequestId, RateLimit))            // the app's own steps, in this order
    .components(&[TOKENS, OfferList])              // components with scripts or styles
    .area(|area| admin::area(area).components(&[SalesChart]))
    .messages::<Msg>()                             // the locales req.lang() follows
    .pwa(Pwa::new(Manifest::new("Shop", "Shop").icon(Static::ICONS_192_PNG, "192x192")));
corten::serve(app).await
```

The layout around `<main>` renders on full page loads only; a client navigation gets the
`<title>`, the page's head tags and `<main>`'s children. Every page carries a key hashed from the
caller its handler read (if any), the deploy and the layout that rendered it (where its `html!`
document is written), and when a navigation or a form answer arrives with a different key
(sign-in, sign-out, a new release, a page in another layout), the runtime loads the whole
document, so the layout is never stale.

## Host names

An app answers a request only when its `Host` is `localhost` (or a name under it, such as
`app.localhost`), an IP address, missing (HTTP/1.0, a test's `oneshot`), or a name the app lists.
Any other name gets `421 Misdirected Request` before routing: no handler, middleware, caller
lookup or error page runs. Otherwise a page on a name someone else controls could point that
name at this machine (DNS rebinding) and then use every route as its own origin, writes
included, since the browser treats the rebound page as same-origin. On loopback, where only this
machine can reach the app, that is the access model it would break, so the check is the default
rather than a setting to remember.

A deployed app lists the names it is served under, where the deploy sets `HOST` and `PORT`:

```sh
HOSTS=shop.example,.shop.example   # the domain, and (leading dot) every subdomain
```

or in code, `App::policy(Policy::new().host("shop.example"))`. Behind a proxy that rewrites
`Host`, it is the name the proxy sends. A health check that calls the app by an IP address, or
by `localhost`, is answered as before. The first refused name is logged, so a deploy that forgot
its name says why it answers 421. An app served under names it cannot list in advance (each
customer's own domain) says `Policy::any_host()` (`HOSTS=*`) and checks them itself. The check
compares one header with a short list.

## The client's address

A handler that needs the visitor's IP address (to pass it to an API whose limits are per client,
or to limit by it itself) takes `ClientIp`:

```rust,ignore
async fn sign_up(req: Req, ClientIp(ip): ClientIp, form: Form<SignUp>) -> HtmlResult {
    accounts.create(&form, ip).await?; // the API counts each visitor on its own
    req.redirect(&url::Welcome)
}
```

Without a proxy it is the connection's peer. Behind one, every request comes from the proxy, which
names the client in `X-Forwarded-For`; the app trusts that header only from the proxies it lists,
where the deploy sets `HOST` and `PORT`:

```sh
PROXIES=10.0.0.0/8,fc00::/7   # an address, or a network, per proxy
```

or in code, `App::policy(Policy::new().proxy("10.0.0.0/8"))`. A request from a listed address is
followed back through `X-Forwarded-For`, right to left, to the first address that is not a proxy;
anything further left is whatever the client wrote, so it is never read. A request on a Unix socket
counts as from a proxy when the app lists any. Only `X-Forwarded-For` is read, which every common
proxy sets: a proxy sets one of it and `Forwarded`, and reading the other would take what a client
wrote there. List only the addresses requests reach the app from through the proxy: a client that
can reach the app directly from a listed address can name any address it likes.

`ClientIp` tells who is asking, so a `Public` handler cannot take it, and an answer that read it is
private like one that read a `Caller`. Where the address cannot be known (a Unix socket and no
proxy, a listener of the app's own whose address is not a socket address, a test's `oneshot`),
`ClientIp` is a 500 that says why and `Option<ClientIp>` is `None`. A test gives the request an
address with axum's `MockConnectInfo` layer around `app.router()`. The peer is at hand while
the request's own future runs, so a layer of the app's own that hands the request to another task
(`tower`'s `Buffer`) leaves it unknown past that layer.

Giving each request its connection's peer costs 44 instructions per request whether a handler
reads it or not (callgrind, release build, keep-alive requests to `serve_on` over TCP, the
difference between 1,200 and 200 requests over 1,000: 54,507 before, 54,551 after). It is not a
request extension: one more of those cost 497.

## Tasks

Work that is not a request's (a refresh loop on a timer, a queue consumer, a supervised child
process, a periodic flush) is a task of the app's, an async function of a `Signal`:

```rust,ignore
async fn refresh(quotas: Quotas, signal: Signal) {
    let mut every = tokio::time::interval(Duration::from_secs(120));
    every.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            _ = signal.aborted() => return,
            _ = every.tick() => quotas.refresh().await, // finishes its journal write
        }
    }
}

let app = App::new(url::router(quotas.clone())).task("refresh", move |signal| refresh(quotas, signal));
corten::serve(app).await
```

`serve` starts the tasks once it listens; `app.router()` starts none, so a test of the app does
not run them. On Ctrl-C or `SIGTERM` the signal aborts with `Abort::Shutdown` first, then live
streams end and the listeners close, and `serve` returns once requests in progress and the
tasks have finished, waiting for the tasks up to `App::shutdown_grace` (5 seconds unless set),
and then, within the same grace, for work handlers finish ([`Signal::finish`](cancellation.md#work-that-must-finish)).
A task still running then is ended and logged by name, so a stuck one cannot hold the process,
and so is one that panics; whether to restart it is the app's choice.

Two lines in the loop matter. `biased` polls the signal first, so no run starts once shutdown has
begun (`select!` otherwise picks among ready branches at random); one in progress has until
`App::shutdown_grace` to finish. With `MissedTickBehavior::Skip` a run that took longer than the
interval is followed by one late run, and then the interval's marks again: tokio's default,
`Burst`, runs once for each mark missed, back to back.

### On a schedule

A job at a mark on the clock (hourly at :15) is an interval that starts there; jobs that must not
overlap (they share a lock) are branches of one task's loop, which runs one at a time, and the
loop keeps what each did for the app's status page:

```rust,ignore
const HOUR: Duration = Duration::from_secs(3600);

/// The next time the clock (UTC) is `past` a multiple of `period` (`past` under `period`).
fn next_mark(past: Duration, period: Duration) -> Instant {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let into = (now + period - past).as_nanos() % period.as_nanos();
    Instant::now() + period - Duration::from_nanos(into as u64)
}

async fn models(models: Models, status: Status, signal: Signal) {
    let mut hourly = interval_at(next_mark(Duration::from_secs(15 * 60), HOUR), HOUR);
    let mut recheck = interval(Duration::from_secs(5 * 60)); // the first tick is at once
    hourly.set_missed_tick_behavior(MissedTickBehavior::Skip);
    recheck.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        let job = tokio::select! {
            biased;
            _ = signal.aborted() => return,
            _ = hourly.tick() => Job::Refresh,
            _ = recheck.tick() => Job::Recheck,
        };
        let (started, clock) = (SystemTime::now(), Instant::now());
        let outcome = match job {
            Job::Refresh => models.refresh().await,
            Job::Recheck => models.recheck().await,
        };
        let ran = Ran { started, took: clock.elapsed(), outcome };
        status.lock().expect("never poisoned").insert(job, ran);
    }
}
```

`App::task`'s documentation has it whole, run on a paused tokio clock (`start_paused`): three
hours in which each refresh is an hour after the last, at :15, and a refresh that overruns the
recheck's marks is followed by one recheck, not one for each; a test of an app's own schedule does
the same. Daily at 03:30 UTC is `next_mark(HOUR * 7 / 2, 24 * HOUR)`. Jobs in separate tasks that
must not overlap share a `tokio::sync::Mutex`.

Limits, which are the app's to handle:

1. The interval keeps time on the monotonic clock, which a change of the wall clock does not move
   and some systems stop while asleep (a laptop): after such a jump its marks are off the clock's
   until the interval is made again from `next_mark` (when a tick comes a period late, say).
2. The next run's time, for a status page, is worked out the same way; tokio's interval does not
   say.
3. UTC marks are the same local marks only in zones a whole number of hours from UTC. A wall time in
   a zone with daylight saving time (daily at 03:30 where the server is) takes a time-zone library
   (jiff, chrono-tz).

Corten has no scheduler of its own: a task and tokio's intervals are the one way.

## Your own listeners

`corten::serve(app)` is `corten::serve_on(corten::listeners(3000).await?, app)`: `listeners(3000)`
binds `HOST`:`PORT` (both loopback addresses without `HOST`, port 3000 without `PORT`), and `serve_on` serves the app on anything
that implements axum's `axum::serve::Listener`, with `serve`'s shutdown across all of them
(tasks told, live streams ended, every listener closed and drained, tasks waited for). A Unix
socket, or a socket a supervisor hands over, is a listener of the app's own.

HTTPS is a step on each connection before HTTP, which `corten::serve_wrapped` takes: the TLS
handshake. TLS stays the app's choice of library, and so does its configuration. With
`tokio-rustls`, around the same loopback pair:

```rust,ignore
let mut config = server_config; // the app's rustls ServerConfig, from its certificate and key
config.alpn_protocols = vec![b"http/1.1".to_vec()]; // what Corten speaks
let acceptor = tokio_rustls::TlsAcceptor::from(std::sync::Arc::new(config));
corten::serve_wrapped(corten::listeners(3000).await?, move |tcp| acceptor.accept(tcp), app).await
```

The step is held to what a connection is. It runs in the connection's own task, so a client
that stalls in its handshake holds up no other; it counts against `App::max_connections` from
before the TCP accept; and `App::header_timeout` runs from the accept, so a client that never
sends its ClientHello, or never sends a request after the handshake, is closed at the same time.
A step that fails closes the connection, and shutdown ends the steps in progress. A listener of
the app's own that ran the handshake before `serve_on` saw the connection would have it outside
both limits.

An app behind a proxy that ends TLS needs none of this: `serve` is plain HTTP for the proxy.
The proxy is also where HTTP/2 or HTTP/3 to browsers comes from (Corten speaks HTTP/1.1), which an
app with live streams needs: over HTTP/1.1 a browser opens about six connections to a host, each
open stream holds one, and a page with a few live elements open in a few tabs uses them up.
An app whose own default port is not 3000 (another service has it on that machine) passes its
port, and `PORT` still overrides it: `corten::serve_on(corten::listeners(8320).await?, app)`,
rather than setting `PORT` from the program, which is `unsafe` once threads may be running.

## The request path

`App` is one middleware around the routes: the same-origin check, the request signal, the
deadline, the app's middleware, error pages, security headers and compression run in one future
per request, in a fixed order (`corten::middleware`). It is a tower layer of its own rather than axum's `from_fn`, which boxes the rest of the
stack again for every request (about 700 instructions). Corten's own inline tag (the import map) is the same on every page of a deploy, so the
CSP allows it by its SHA-256 hash, computed once at startup. A nonce is made only when something asks for one
(`req.nonce()` in the layout, or a handler that takes `Nonce`). A session lookup runs only for a
handler that takes a `Caller`, so static files, the runtime, the health checks and public pages
never touch the session store.

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

When the route's answer type, `Caller` and `Middleware` replaced the app's context and guards,
the same requests were measured before and after (callgrind, release build, in-process
`oneshot`, the difference between 1,200 and 200 requests over 1,000, median of three runs;
range in parentheses):

| Request | Before | After |
|---|---|---|
| a small page, full document, with a cookie | 24,021 (24,020-24,327) | 23,525 (23,514-23,555) |
| the same page without a cookie | 23,116 (23,075-23,116) | 22,614 (22,608-22,615) |
| JSON under `App::api` | 19,579 (19,579-19,758) | 19,307 (19,306-19,307) |
| unknown URL, HTML 404 page | 21,849 (21,832-21,987) | 21,125 (21,122-21,157) |
| a path that does not parse under `App::api` (JSON 400) | 27,657 (27,590-27,760) | 27,421 (27,322-27,857) |
| a handler that panics (error page) | 91,926 (91,891-92,015) | 87,631 (87,619-87,658) |
| a page whose handler reads who is asking (before: `App::context`) | 28,104 (27,938-28,237) | 26,315 (26,179-26,331) |
| a page that does not, on an app that can tell | 28,190 (27,843-28,312) | 23,538 (23,516-23,865) |
| a cross-site form post from a browser | 14,159 (14,159-14,165) | 21,339 (21,293-21,456) |

Two changes made with it account for most of the gain on every request: the security headers
reserve room only for what they add, so an error page's header map is no longer reallocated,
and the route is called without an `async` layer around it, which moved the request once more.
A lookup now runs only for a handler that takes a `Caller`. A cross-site post costs more
because a browser now gets the app's error page (`Failure::CrossSite`) instead of a bare 403.

When every route came to go through the same checks (`App::new` taking `Routes`, `Area::route`
and `Api::route` taking `Methods`), the same requests were measured again the same way:

| Request | Before | After |
|---|---|---|
| a small page, full document, with a cookie | 23,530 (23,454-23,882) | 22,954 (22,821-23,186) |
| the same page without a cookie | 22,738 (22,609-22,763) | 21,949 (21,946-21,995) |
| JSON under `App::api` | 19,306 (19,306-19,306) | 19,205 (19,202-19,205) |
| JSON from a routes module (`JsonResult`) | 20,027 (19,964-20,057) | 19,298 (19,287-19,350) |
| unknown URL, HTML 404 page | 21,099 (21,083-21,294) | 21,014 (21,000-21,016) |
| a path that does not parse under `App::api` (JSON 400) | 27,776 (27,742-27,784) | 28,268 (27,965-28,320) |
| a handler that panics (error page) | 87,566 (87,487-87,594) | 86,619 (86,615-86,753) |
| a page whose handler reads who is asking | 26,177 (26,163-26,200) | 26,288 (25,890-26,323) |
| a page that does not, on an app that can tell | 23,525 (23,466-23,885) | 22,902 (22,817-22,970) |
| a cross-site form post from a browser | 21,430 (21,313-21,450) | 20,883 (20,803-20,890) |

The checks happen when the app compiles, so nothing was added to the request path for them. The
JSON 400 costs about 500 more because the API's `get` is now Corten's (before, it was axum's),
which marks a JSON route's failed response with an extension, as a routes module's handler always
did, so the app answers it as JSON wherever the route is mounted. Profiled in an app with only
that route (26,781 before and 27,509 after, medians of three runs), almost all of the difference
is that insert: the extension map's allocation, growth and free, about 550 a request. Only a
failed response from a JSON route pays it. The other differences, a few hundred either way,
were not profiled; the cross-site post among them never reaches a route.

## Your own middleware

Every request goes through the same stages in the same order, outside in, each needing the one
before it:

1. Compression is negotiated (`Accept-Encoding`); it is applied last.
2. A write another site sent is refused (`Failure::CrossSite`).
3. The request gets its `Signal`, its nonce and the app.
4. The deadline and the panic catch start (`Failure::TimedOut`, `Failure::Panicked`).
5. The app's middleware, in the order given. The route has been matched, so axum's
   `MatchedPath` names it. Then, for a request under an area's or an API's prefix, its own
   middleware: an outer area's before an inner one's, each in the order given.
6. The handler's extractors (a `Caller` is looked up here, only when read), then the handler.
7. A failure becomes its route's answer: JSON for a JSON route or a data client, the app's error
   page for a browser.
8. Security headers, `Cache-Control` (private when anything read a caller), compression.

A step of the app's own is a `Middleware`, given once to `App::middleware`; several are a tuple,
run in order:

```rust
use std::time::Duration;
use corten::{axum::{extract::Request, http::HeaderValue, response::{IntoResponse, Response}}, prelude::*};

struct RequestId;

impl Middleware for RequestId {
    async fn call(&self, req: Request, next: impl Next) -> Response {
        let mut res = next.run(req).await;
        res.headers_mut().insert("x-request-id", HeaderValue::from_static("r-1"));
        res
    }
}

struct RateLimit;

impl Middleware for RateLimit {
    async fn call(&self, req: Request, next: impl Next) -> Response {
        if req.headers().contains_key("x-flood") {
            return Refused::too_many(Duration::from_secs(30)).into_response();
        }
        next.run(req).await
    }
}

let app = App::new(Routes::new()).middleware((RequestId, RateLimit));
# let _ = app.router();
```

Whatever a middleware answers (the 429 above) leaves through stages 7 and 8 like a handler's
answer, so it gets the error page (with the headers it set, `Retry-After` included), the security
headers and `Cache-Control`. One that turns a request away answers with a `Refused`
(`too_many`, `forbidden`, `unauthorized`, `unavailable`), which is answered in the request's
mode as a `Caller`'s refusal is: the error page for a browser, JSON for an API or a data client.
A middleware can
read a `Caller` the same way a handler does (`Caller::<Viewer>::from_request_parts`), and the
handler then reuses that lookup; `ClientIp::of(&mut req)` gives the client's address, as a
handler's `Option<ClientIp>` does. The tuple is resolved when the crate compiles, so nothing is
boxed: a middleware that only passes the request on costs about 230 instructions (its `async fn`
moves the request into its future), where axum's `from_fn` boxes the rest of the stack again
for every request (about 700).

Middleware that belongs to part of the app is given to that part, an area or an API, and runs for
every request at its prefix, never by a path the middleware compares. A request anywhere else
never meets it:

```rust
# use corten::{axum::{extract::Request, response::Response}, prelude::*};
# struct WriteLimit;
# impl Middleware for WriteLimit {
#     async fn call(&self, req: Request, next: impl Next) -> Response { next.run(req).await }
# }
#[corten::routes(area = Items)]
mod items {
    #[route("/items")]
    pub struct Items;
}

#[corten::routes(api = ApiRoot)]
mod api {
    #[route("/api")]
    pub struct ApiRoot;
}

let app = App::new(Routes::new())
    .area(|area| items::area(area).middleware(WriteLimit))   // the item pages' forms
    .api(|routes| api::api(routes).middleware(WriteLimit)); // the partner API
# let _ = app.router();
```

Like an area's deadline and policy, its middleware covers every request at its prefix or under
it: its routes, its nested areas' and APIs' (the outer area's middleware runs first), an app
route declared at a path under it, and an unknown URL there. Given again, a middleware runs after what was given before. Each one a request meets costs
one boxed future on top of what it does (about 900 instructions for one that only passes the
request on). In an app where no area or API has any, the request runs the same code as before;
where some do, a request outside them pays a scan of their prefixes. card-market limits writes
this way.

A tower layer still goes around the whole app (`.layer(l)`), outside every stage, for what must
see the raw request first: a cap on body size.
There is no layer for one route: what a route needs is in its handler's arguments (an
extractor) and its return type.

## Static files

The app's `static/` directory is compiled into the program, so the binary is the whole deploy:
nothing beside it to copy, find at run time or keep in step with the build. `embed!` reads the
directory when the crate compiles and gives each file a constant (`icons/192.png` is
`ICONS_192_PNG`), and a page names a file by it:

```rust,ignore
corten::assets::embed!(pub struct Static = "static"); // from the crate's root

App::new(url::router()).assets(Static)
// in the layout:
<link rel="stylesheet" href={req.asset(Static::STYLE_CSS)}>
```

A misspelled or removed file does not compile. The PWA names its files the same way:
`Manifest::icon(Static::ICONS_192_PNG, "192x192")` and `Pwa::import_script(Static::SW_PUSH_JS)`;
an icon another site serves is a `Uri`, as every URL outside the app is. A name that comes from
data (a stored picture's path) is `req.find_asset(name)`, `None` when the directory has no such
file. Files are hashed
when the app starts, and each is served from memory at a URL with that version.

An edited file rebuilds the crate. A file added to the directory with no change to the code is in
the program from the next build that compiles the crate (a macro cannot ask cargo to watch a
directory); the Dockerfiles `corten new` writes touch every source before building, so a release
build has every file. `corten dev` reads the directory where `embed!` read it, so it serves a
new file at once, and an edit or a new file there alone restarts the app without compiling it
(a removed file, or any other change, builds). File
names are URLs, so they are letters, digits and `- _ . ~ @ + , = !`; another character is a
compile error. A program with two apps gives
each its own `embed!`. Dotfiles are left out.

## Caching

Static files, component scripts, the components' stylesheet and the runtime have content-hashed
URLs and are sent `public, max-age=31536000, immutable`; the PWA's worker keeps them. Everything
else `App` serves that does not set `Cache-Control` is sent `private, no-cache`: the browser
may keep it but asks again before using it, and no shared cache (a CDN, a proxy) stores it. A
page's layout may show who is asking, so a shared copy could show one user's page to another.

An answer that is the same for every visitor says so in its type, `Public<..>`:

```rust,ignore
async fn home(req: Req) -> Public<HtmlResult> {
    let page = req.page(html! { <Layout req={&req} title="Home">{view}</Layout> });
    Public::new(Duration::from_secs(60), page).stale_while_revalidate(Duration::from_secs(600))
}
```

Its handler may only take inputs that cannot tell who is asking (`Params`, `Form`,
`State`, `Req`, `Signal`, `Json`, ..., marked `Anonymous`), so a `Caller`, a `HeaderMap` or a
cookie in a public handler does not compile. Only a successful answer or a redirect is shared;
a failure keeps `private, no-cache`.

`max_age` is the browser's lifetime and every shared cache's. A page whose data changes slowly
(a catalog, a help article) can let a CDN keep it longer than a browser with `.shared(..)`, so the
origin renders it once a day instead of once a minute for each edge:

```rust,ignore
Public::new(Duration::from_secs(60), page)
    .shared(Duration::from_secs(86_400))
    .stale_while_revalidate(Duration::from_secs(3600))
// public, max-age=60, s-maxage=86400, stale-while-revalidate=3600
```

The lifetimes are values, so one known only when the request is answered (an API's own
`s-maxage`, an operator's setting) is passed the same way. A response sent privately (the rows
below) drops `s-maxage` with `public`.

`App` still checks what it can see before it lets a shared cache keep the answer:

| The request or response | Sent |
|---|---|
| a full document, no `Cookie` or `Authorization`, nothing read a caller | as asked, plus `Vary: cookie` |
| a request with a `Cookie` or `Authorization`, or one whose handler or middleware read a caller | `private, no-cache` |
| a client navigation, a form answer or a data client's JSON from a page route | `private`, same lifetime: only the document is shared |
| a response that sets a cookie, or HTML with the request's nonce | `private, no-cache`, logged under `corten dev` |

A `Public` answer cannot set a cookie the way a private one does: the page inside
`Public<HtmlResult>` is made for everyone (`HtmlResponse<ForAll>`), and `with_cookie` on it does
not compile. `req.page`, `req.redirect` and the other `Req` methods take who the page is for from
where it goes, the handler's return type or `Public::new`, so a handler, a closure given to
Corten's `get` or `post`, and `Public::new(age, req.page(..))` are written as before. A helper is
where it shows: one that makes a page for a `Public` handler returns
`corten::Result<HtmlResponse<For>>` for any `For` (not `HtmlResult`, a page for one visitor,
which `Public::new` refuses: "is not an answer made for everyone, so it cannot be `Public`"), and one that
wraps a page in `Public` takes `corten::Result<HtmlResponse<ForAll>>`. A `Set-Cookie` header
written by hand is still caught when the answer is sent (the row above).

A `Cache-Control` written by hand (by a middleware, or on an `HtmlResponse`) is kept as written,
except that the last row applies to it, and a caller read makes it private. An extractor of the app's own is not `Anonymous` until the app says
so, so a public answer cannot read a credential through one by accident. An `App::api` route has one form only, so its public responses
stay public for clients without credentials. A cache in front of the app must key on `Vary`
(`corten`, `accept`, `accept-encoding`, and `accept-language` with `App::messages`).
`NoStore<..>` keeps an answer out of every cache, the browser's back/forward cache included:
`async fn statement(req: Req) -> NoStore<HtmlResult>`. A handler cannot mark an answer public
any other way: a response it builds by hand is not an answer a route can name, and if anything
read a caller the answer is private whatever its header says.

An answer at a URL that names its content (a hash of its bytes, as an uploaded picture's can be)
never changes there. `Public::immutable(answer)` sends it as Corten's versioned files are sent,
`public, max-age=31536000, immutable`, and since the URL alone says what it is, a request's
cookies do not make it private and no `Vary: cookie` is added. A cookie it sets or a caller
something read still does. Its handler is held to the same rule as any `Public` handler. It takes
`Media` or `corten::Result<Media>` only: a page, a redirect or JSON changes with the deploy, so
`Public::immutable(req.page(..))` does not compile, and neither does a `PageOr<Media>`, which
may be a redirect. `Public::new` takes one, with the page inside made for everyone. A URL that does not name its content
(`/me/picture`) takes `Public::new` with a lifetime.

A header of the app's goes on a public answer with `Public::header`, named by a type: a file
that must stay out of search (`llms.txt`, a manifest), a canonical link. A download's file
name is the answer's own, `Media::attachment(name)`
(see [Requests and forms](requests-and-forms.md)).

```rust,ignore
use corten::answer::XRobotsTag;

async fn manifest() -> Public<Media> {
    let noindex = HeaderValue::from_static("noindex, nofollow");
    Public::new(Duration::from_secs(3600), Media::new("application/json", MANIFEST)).header::<XRobotsTag>(noindex)
}
```

The answer is compressed and shared as it would be without the header, which a `Relay` (another
server's answer, sent as it came) is not. Corten names `XRobotsTag`, `Link` and
`ContentLanguage`; an app names another with `impl
corten::answer::Header for ContentLocation { const NAME: &'static str = "content-location"; }`.
Which names a public answer may set is checked when the app builds (`cargo build` and `cargo
test`; `cargo check` does not evaluate it): `set-cookie` (the answer would be one visitor's), the
headers Corten sets itself (`cache-control`, a CDN's `cdn-cache-control` and
`surrogate-control`, which it would obey over Corten's, `vary`, the encoding and length, the
security headers, CORS's `access-control-*`, `corten-*`), those the answer has (`content-type`,
`location`, `content-disposition`) and the connection's (`transfer-encoding`) do not compile. Headers are added to a
success or a redirect; an error is the error page, without them.

A full document never carries `corten-page`, so if a cache hands one to a client navigation by
mistake, the runtime loads the URL as a whole page instead of swapping it in.

There is no page cache on the server. Rendering is the cheap part of a page: the measured
item page renders in about 15 µs and costs several times that to compress (see the
`render_cost` test), and a server cache's key would have to cover the caller, the locale, the
mode and the deploy, where a mistake shows one user's page to another. Cache the data a page
reads in the app, where it knows when the data changes, and let a CDN keep public pages. There
are no ETags on pages either, since they would save bytes but not the render.

## JSON-only routes

A page answers every request mode, so how it fails depends on what the client asked for: the
error page for a browser, JSON for `Accept: application/json`. A route with no screen (a
webhook, an OAuth callback, a call between services, a public API) is called by programs, and
most HTTP clients send `Accept: */*` or nothing. A handler that returns `JsonResult<T>` answers
every failure of its route as JSON, a path that does not parse included, wherever the route is.
Routes that share a prefix and its settings (cross-origin callers) go in a module of their own,
`#[corten::routes(api = ApiRoot)]`, whose `api` function `App::api` takes. Each route names its
handler, as any routes module's does, so an API route nobody serves does not compile; the
prefix's own route is the one that may name none:

```rust
use corten::prelude::*;

#[corten::routes(api = ApiRoot)]
mod api {
    #[route("/api")]
    pub struct ApiRoot;

    #[route("/api/items/{id}/price", get = price)]
    pub struct ItemPrice<'a> { pub id: &'a str }
}

async fn price(Params(item): Params<api::ItemPrice<'_>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "id": item.id, "lowest": "$0.50" }))
}

fn app(pages: Routes) -> App {
    App::new(pages).api(|routes| api::api(routes).allow_origin("https://partner.example"))
}
# let _ = app(Routes::new()).router();
```

A JSON answer is `200` with the value's JSON. One with a status or a cookie of its own is a
`JsonResponse`, made from the value, and its handler returns `Result<JsonResponse, JsonError>`:
`Json(started).with_status(StatusCode::ACCEPTED)` for a job begun,
`Json(problem).with_status(StatusCode::SERVICE_UNAVAILABLE)` for a document that does not exist
yet, `Json(session).with_cookie(SetCookie::try_new("session", &token)?)` for a sign-in (never a
`Set-Cookie` header by hand). That status and body are the app's own answer and reach the client
as written; a failure (`Err`) is still answered in Corten's shape (or the API's own,
`Api::errors`). A `JsonResponse` is for the client who asked, so `Public` of one does not
compile; a public JSON answer is a `JsonResult<T>`.

JSON the app already holds as bytes (a document built once when its data changes, a file sent as
it was written) is a `corten::json::Raw`: `Json(raw.clone())` sends the bytes as they are, with
no serialization and no copy. `Raw::new` reads them through once and returns an `InvalidJson`
error when they are not one JSON value in UTF-8 (a file saved in another encoding, or with a byte
order mark, included), so a corrupt file is an error the app handles, not a broken answer. Keep
the `Raw` and clone it for each request: with feature `compress`, one sent again is compressed
once, at brotli 9 and gzip 9, on one background thread that takes them in turn, and later reads
get those forms instead of compressing it per request (a client that ranks zstd higher still
gets zstd). Measured on an 80 KB document (release build): compressing it on every read costs
220 µs with zstd 1 and 2.1 ms with brotli 2; compressing it once costs 15 ms, and each read then
sends 5,000 bytes for brotli against 6,992 for zstd 1. A `Raw` made for one request is
compressed as any answer is.

An endpoint built from a value rather than a function, the MCP endpoint (`agent::Tools`), is
added to its API with `Api::route`: `.api(|routes| mcp::api(routes).route::<mcp::Mcp>(tools.route()))`.
Corten's `get` and the others check its handlers as a routes module does.

A module costs nothing per request: its `api` function adds the same routes a hand-written
`Api::route` would. Measured with callgrind (instructions per request, release build, in-process
`oneshot`, the difference between 1,200 and 200 requests over 1,000, median of three runs;
range in parentheses), the item page's `/api/items/{id}` under an app with nothing else is
21,913 (21,816-22,228) added by hand and 21,836 (21,808-21,862) from its module.

Under the prefix:

1. Every failure is one JSON shape, whatever the client accepts: a Problem Details document
   (RFC 9457, `application/problem+json`), `{"title":"Not Found","status":404,"detail":"No item 7."}`,
   which clients, gateways and API tools read without knowing Corten. `title` is the status's
   reason, `detail` the message (left out when it would only repeat the title), and `type` is
   left out, which means `about:blank`: the status says what went wrong. That covers a handler's `corten::Error`, an
   extractor's rejection (a body that does not parse), an unknown URL, a wrong method (with
   `Allow`) and the deadline's 503. A 4xx message is sent, so write it for the client
   (`Error::new("no item 7").with_status(StatusCode::NOT_FOUND)`); a 5xx message stays in the
   log except under `corten dev`. A failure answered with a JSON body of the handler's own
   passes through, and validation errors are `Errors` as the error, which adds
   `"fields": {"email": "..."}`.
2. There is no document and no nonce, and responses are `private, no-cache` unless the handler
   returns `Public<..>`. The CSP is `default-src 'none'; frame-ancestors 'none'`. A handler
   that needs to know who is calling takes a `Caller` ([below](#who-is-asking)); a refused one
   is a JSON 401 or 403.
3. The deadline, the request signal, compression and the same-origin check on writes are the
   same as for pages; `Api::deadline` gives these routes a deadline of their own.
4. `Api::errors(|status, message| ..)` writes those failures in another shape, for an API that
   speaks another protocol's errors (an Anthropic- or OpenAI-compatible gateway, or an MCP
   endpoint's JSON-RPC errors, as `examples/card-market`'s `/mcp` answers them): it returns a
   `ToJson` value, so a key check in `Identify` and the API's unknown URLs, wrong methods,
   deadline and refusals all answer as its clients parse them. A `421` for a host the app does
   not answer is not the API's (no route ran), and a URL outside the prefix is the pages' 404.
5. `Api::allow_origin` lets one site's pages call these routes from the browser: Corten answers
   its CORS preflight, adds `Access-Control-Allow-Origin` and lets its writes through. The
   pages keep refusing that site's writes. The site authenticates with a token: a write from it
   that carries the user's cookies is refused, and no `Access-Control-Allow-Credentials` is sent.
6. `Json` in the prelude is `corten::Json` (feature `json`). It reads a request body into a
   `FromJson` (item 7) with the value and the rejections axum's `Json` gives: Corten's reader takes
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
7. To read JSON another service answers, derive `FromJson` (in the prelude) next to `Deserialize`
   and read it with `corten::json::from_slice` or `from_str`. Corten's reader takes the fields the
   type declares and skips the rest; on anything it does not follow (an escaped key, a missing or
   duplicate field) `serde_json` reads the text instead, so the value and every error are
   `serde_json`'s. It reads `#[serde(...)]` as `Deserialize` does and refuses one it cannot follow
   (`flatten`, `deserialize_with`); a field of a type with only `Deserialize` is read by `serde_json`.

Each route is a `#[derive(Route)]` struct, and the API's prefix is one too (`ApiRoot` above), so
the router and the links name the same URL, and a route outside the prefix (`/apis/items`
under `/api`) does not compile rather than be served as a page. A handler that takes `State`
gets it on its `Methods`: `.route::<ItemJson>(get(item_json).with_state(source))`. `get`
(and `post`, ...) is Corten's, which checks the handler as a routes module does. An API
whose prefix is `/` makes the whole app an API (the health checks and the runtime keep their
behavior).

A screen that a native app or an agent reads is not an API route: it stays a page that answers
with `req.screen`, so every surface shares one handler (see [Surfaces](../architecture.md)).

## WebSockets and relays

A route can switch its connection to another protocol, or pass on another server's answer, and
stay a route like any other: it is in a routes module, an area or an API, takes `Caller<T>`, and
goes through the host check and the app's middleware.

A WebSocket's handler takes `corten::upgrade::Upgrade` and returns what `upgrade.websocket(run)`
gives: the 101 with its `Sec-WebSocket-Accept`, and `run(stream, signal)` on the connection once
it has switched. Corten reads the handshake and hands over the bytes; the frames are a WebSocket
library's (`examples/watchlist`'s `/quotes` uses tokio-tungstenite):

```rust,ignore
async fn quotes(viewer: Caller<Viewer>, upgrade: Upgrade) -> corten::Result<Switch> {
    upgrade.websocket(move |stream, signal| async move {
        let mut socket = WebSocketStream::from_raw_socket(stream, Role::Server, Some(config)).await;
        loop {
            let message = tokio::select! {
                message = socket.next() => message,
                _ = signal.aborted() => {
                    let _ = socket.close(None).await; // shutting down: say goodbye
                    break;
                }
            };
            match message {
                // The library queued the answer to a client's close; a flush sends it.
                Some(Ok(Message::Close(_))) => { let _ = socket.flush().await; break; }
                Some(Err(_)) | None => break,
                // .. answer it; check the viewer again here if a revoked key must end the socket
                Some(Ok(_message)) => {}
            }
        }
    })
}
```

1. The handshake is a `GET` that carries the user's cookies with no CORS check, so it is held to
   what a write is: the app's own pages, a site `Policy::allow_origin` names (under an API, its
   `Api::allow_origin` sites too, without cookies), or a client that is not a browser. Another
   site's page gets a 403. A route whose callers must not be browsers at all refuses them in its
   `Identify` (a request with `Origin` or `Sec-Fetch-Site`).
2. A request that does not ask to switch gets a 426, and a route that does not take `Upgrade`
   never switches: only an `Upgrade` makes the 101, so a socket opens only where a route
   declared one. A URL that is a socket and also something else (a page, or a gateway's relayed
   answer) takes `Option<Upgrade>`, `None` when the request does not ask, and returns
   `SwitchOr<A>`: `upgrade.websocket(run)?.into()` or `SwitchOr::Answer(page)`. Its failures
   are answered as `A`'s are, and a switch it asks for is checked as above.
3. The request's own `Signal` ends with the 101, so the work gets its own, which aborts with
   `Abort::Shutdown` when the app begins to shut down; the work is not waited for.
4. Message size, compression, what may wait to be sent and per-message checks are the
   WebSocket library's settings and the route's code: Corten does not read frames.
5. `upgrade.switch(headers, run)` answers with an upstream's 101 and its headers instead, for a
   tunnel that relays another server's switch and then passes bytes both ways
   (`tokio::io::copy_bidirectional`).
6. Only HTTP/1.1 connections served by `corten::serve` switch; an HTTP/2 request (no extended
   `CONNECT`) and a test's `oneshot` get a 426.

`corten::Relay::new(status, headers, body)` is another server's answer passed on as it came,
for a gateway or a backend for a frontend: its status, its headers less the hop-by-hop ones, and
its body as it arrives (`Body::from_stream(res.bytes_stream())` from reqwest). A failure status
keeps the upstream's body, never the error page or the JSON error. The app's security and cache
rules still apply, except that a `Content-Security-Policy` the upstream sent replaces the app's:
a relayed page's scripts were written for its own policy (their nonces and hosts), which the app's
would block. Without one, the app's applies. A streamed body is passed on as each piece arrives.
Corten compresses no relayed body, buffered or streamed, an event stream included, so a long
relayed stream holds no compression context: the encoding is the upstream's. A relay that should
be compressed forwards the client's `Accept-Encoding`, and the upstream's encoded body goes on as
it is, as long as the HTTP client does not decode it (for reqwest, leave its `gzip`, `brotli` and
`zstd` features off). When the client leaves, the body is dropped, and
the upstream request with it. What else to drop (`Set-Cookie`) is the route's to decide.

An app moved onto Corten a slice at a time serves its own paths and relays the rest to the
old server behind a catch-all route. `any = relay` serves every method there, those the app never
names included (`OPTIONS`, `PROPFIND`), so the upstream answers them, not Corten's 405. Corten
still answers what it answers for every route first: a write from another site is a 403, and
under an API that allows other origins a CORS preflight from one of them is Corten's (one from
any other origin reaches the old server, and the write after it is still refused). It is
the route's only handler, which the module checks when it compiles, so `any` is named in a
`#[corten::routes]` module (an area's or an API's as well) and has no builder for routes added by
hand. A path the app routes itself keeps only the methods it names: when the
old server still takes a `POST` where the app now serves the `GET`, name a relay for it there
too. A route with a parameter holds its handlers to taking its `Params`, so a handler cannot
ignore what the path says by mistake; for a catch-all that means the relay sees the paths its
`rest: &str` can read (one that is not UTF-8 once decoded is a 400). One relay for several
catch-alls is generic over the route, and the compiler picks the route at each name. A route
without a parameter has no `Params`, so it takes a handler without one, `any = relay_here`, that
calls the same forwarding code:

```rust,ignore
async fn relay<R: FromUrl>(_: Params<R>, req: Request) -> corten::Result<Relay> {
    legacy::forward(req).await // the old server's answer, as Relay::new(status, headers, body)
}

#[corten::routes]
pub mod url {
    #[route("/{*rest}", any = relay)]
    pub struct Elsewhere<'a> { pub rest: &'a str }

    #[route("/api/{*rest}", any = relay)]
    pub struct OldApi<'a> { pub rest: &'a str }
}
```

## Who is asking

A handler that needs to know who is asking takes a `Caller<T>`, where `T` says how one is
found (`Identify`): a person by a session cookie, a partner by an API key. The handler's
arguments say what the route needs, and Corten answers for it in the request's mode:

```rust
use corten::{axum::http::request::Parts, prelude::*, security::cookie};

#[derive(Clone, Hash)]
struct Viewer { name: String }

impl<S: Sync> Identify<S> for Viewer {
    async fn identify(parts: &Parts, _state: &S) -> Result<Self, Refused> {
        // A real app reads its session store here (`S` is the routes' state).
        match cookie::get(&parts.headers, "session") {
            Some(name) => Ok(Viewer { name: name.to_string() }),
            None => Err(Refused::sign_in(&url::SignIn)),
        }
    }
}

#[corten::routes]
mod url {
    #[route("/", get = home)]
    pub struct Home;

    #[route("/account", get = account)]
    pub struct Account;

    #[route("/sign-in", get = sign_in)]
    pub struct SignIn;
}

// "Sign in" or the name: the handler runs either way.
async fn home(req: Req, viewer: Option<Caller<Viewer>>) -> HtmlResult {
    let name = viewer.as_deref().map_or("Sign in", |v| v.name.as_str());
    req.page(html! { <Layout req={&req} title="Home"><p>{name}</p></Layout> })
}

// Someone signed in, or the browser goes to sign in: the handler never runs without one.
async fn account(req: Req, Caller(viewer): Caller<Viewer>) -> HtmlResult {
    req.page(html! { <Layout req={&req} title="Account"><h1>{&viewer.name}</h1></Layout> })
}

async fn sign_in(req: Req) -> HtmlResult {
    req.page(html! { <Layout req={&req} title="Sign in"><h1>"Sign in"</h1></Layout> })
}

#[component]
fn Layout(req: &Req, title: &str, children: impl Render) -> impl Document {
    html! { <html lang={req.lang()}><head><title>{title}</title><Corten /></head><body><main>{children}</main></body></html> }
}
# let _ = App::new(url::router()).router();
```

| `identify` says | Page, or a form post without JavaScript | Form patch | Data client, JSON route |
|---|---|---|---|
| `Ok(caller)` | the handler runs | the handler runs | the handler runs |
| `Refused::sign_in(&route)` | 303 to the route | `corten-location`, and the runtime goes there | JSON 401 |
| `Refused::unauthorized()`, `Refused::forbidden()` | the error page, `Failure::Refused` (401, 403) | the error page, swapped in | JSON 401, 403 |
| `Refused::too_many(wait)` | the error page, `Failure::Refused` (429), with `Retry-After` | the error page, swapped in | JSON 429, with `Retry-After` |
| `Refused::unavailable(error)` | the error page, `Failure::Unidentified` (503) | the error page, swapped in | JSON 503 |

1. Only a handler that takes a `Caller` pays for the lookup: a public landing page, static
   files and the health checks never touch the session store. Several readers in one request
   (a middleware and the handler) share one lookup.
2. Reading one makes the answer private (`Cache-Control: private`) and puts the caller in the
   page's key, so after signing in or out the runtime loads the next page in full and the layout
   shows who is asking now. A `Public` answer's handler cannot take one: that does not compile.
3. A refused request never reaches the handler's later extractors: the body is not read and
   nothing is written. Refuse with `Refused::unavailable` when who they are cannot be known
   now (the session store is down), so no one is sent to sign in again for nothing.
4. Callers of different kinds are different types with their own lookup: `Caller<Viewer>` from
   a session cookie on the pages, `Caller<Partner>` from an `X-Api-Key` header on a
   `JsonResult` route, where a missing key is a JSON 401.
5. A sign-in page whose handler needs a caller would send the browser round in a loop; Corten
   answers it with a 500 instead, and under `corten dev` the page says why.
6. Signing in and out set the cookie on the answer:
   `Ok(req.redirect(&url::Home)?.with_cookie(SetCookie::new("session", &token)))`, and
   `SetCookie::remove("session")` to sign out. `new` panics on a value a cookie cannot carry,
   which suits a token the app made; for one from elsewhere (another service's session token),
   `SetCookie::try_new` returns an `InvalidCookie` the handler maps to its own answer (a 502,
   say; `?` alone is a 500). `SetCookie` (`corten::security::cookie`) makes
   `HttpOnly; Secure; SameSite=Lax; Path=/` the default, and an answer that sets a cookie is never
   stored by a shared cache. A JSON sign-in sets it the same way, `Json(session).with_cookie(..)`.
   Never append a `Set-Cookie` header by hand.
   A return address the sign-in page was given (`?next=/account/cart`) is text from outside:
   `corten::Local::parse(&next)` keeps it only when it is a path on this site (not `//host`,
   `/\host`, a `..` segment or a full URL, so the redirect cannot lead elsewhere; nor a
   percent-escaped letter, digit or `-._~`, which a browser may ask for as another path), and
   `req.serves(&local)` says whether the route the router picks for its path (the app's own or
   an area's, not an API's) has a `GET` handler. Send the user to it with `req.redirect(&local)`.
   A path another app of the product serves is that app's address with the path after it
   (`Uri::try_from(format!("https://shop.example{local}"))`), and anything else goes to a page of
   the app's own.
7. `.message("Invalid API key")` on a refusal is what a client that reads the failure is told:
   the JSON error's message (in the API's own shape, `Api::errors`). The error page a browser
   gets does not show it, and a 503 from `Refused::unavailable` never sends its error. Counting
   failed attempts is the app's (by address or account, in its own store); `Refused::too_many`
   is how the lookup says the count is reached, checked before the credentials so a locked-out
   client learns nothing about them.
8. A check that depends on the record (only the author may edit a draft) stays in the handler,
   which loads the record; `Error::status(StatusCode::FORBIDDEN)` gives the same answers there.
9. Test it the way a user meets it: a router test that sends each route signed out and signed
   in without the permission.

[examples/auction-room](../../examples/auction-room) refuses each way: its `Identify` sends someone
signed out to sign in, answers `Refused::unavailable` when its SQLite store cannot answer, and
refuses with `Refused::forbidden` a signed-in bidder at the house's desk and the house where buyers
bid (each area's caller type); its bid limit, the buyers' area's middleware, reads the same
`Caller` and refuses with `Refused::too_many`, as its sign-in limit does for an address with many
refused sign-ins.

A page and the form that posts back to it share one URL (`get = show, post = save`), and each
method names its handler, so who may read and who may write are each handler's arguments:
`show` takes `Option<Caller<Viewer>>`, `save` takes `Caller<Viewer>`.

## Areas

A part of the app with something of its own (an admin area, an account, seller tools) is an
area: its routes under one prefix, a stylesheet only its pages load
([styles](styles.md#areas)) if it needs one, and the caller every route there needs. Its routes
are a module of their own, `#[corten::routes(area = Admin)]`, where `Admin` is the route at
the prefix, mounted with the module's `area` function (`area(area, state)` with
`state = Db`). `caller = Staff` holds every handler in the module to taking
`Caller<Staff>`:

```rust
use corten::{axum::http::request::Parts, prelude::*};

/// Someone on the staff: anyone else is refused (403).
#[derive(Clone, Hash)]
struct Staff;

impl<S: Sync> Identify<S> for Staff {
    async fn identify(_parts: &Parts, _state: &S) -> Result<Self, Refused> {
        // A real app reads the session and checks the role.
        Err(Refused::forbidden())
    }
}

#[corten::routes(area = Admin, caller = Staff)]
mod admin {
    #[route("/admin", get = dashboard)]
    pub struct Admin;

    #[route("/admin/users/{id}", get = user)]
    pub struct User<'a> { pub id: &'a str }
}

async fn dashboard(req: Req, _: Caller<Staff>) -> HtmlResult {
    req.page(html! { <Layout req={&req} title="Admin"><h1>"Admin"</h1></Layout> })
}

async fn user(req: Req, _: Caller<Staff>, Params(user): Params<admin::User<'_>>) -> HtmlResult {
    req.page(html! { <Layout req={&req} title="User"><h1>{user.id}</h1></Layout> })
}

#[component]
fn Layout(req: &Req, title: &str, children: impl Render) -> impl Document {
    html! { <html lang={req.lang()}><head><title>{title}</title><Corten /></head><body><main>{children}</main></body></html> }
}

fn app(pages: Routes) -> App {
    App::new(pages).area(admin::area)
}
# let _ = app(Routes::new()).router();
```

1. A route added to the module later without `Caller<Staff>` does not compile, and the error
   is at the handler's name in `#[route(..)]`. A handler cannot forget who may use it.
2. A route without a handler, two routes with one path, and a route outside the prefix
   (`/admn/users` in the `/admin` area) do not compile, so a link from an area's page cannot
   reach a route nobody serves. Neither does a prefix that is not a plain path (`/admin/{id}`).
   An area with no page at its prefix (`/docs/{page}` and nothing at `/docs`) gives the prefix
   route no handler, `#[route("/docs")] pub struct Docs;`: it is then only the prefix (a
   `corten::Prefix`), not a link, so `Docs.href()` does not compile and the app's 404 answers
   `/docs`.
3. Who passes is the app's model: `Staff::identify` reads the session and refuses with
   `Refused::sign_in` (no one signed in) or `Refused::forbidden` (not staff). Stricter parts
   are stricter types: `caller = Billing` on a `/admin/billing` area of its own.
4. A route in an area that another caller may use (an order's review form under
   `/account/purchases`, which needs another permission than the orders do) is in a module of
   its own in the same area, with that caller: `#[corten::routes(area = super::orders::Purchases,
   caller = Reviewer)]`. The app passes both modules to one area,
   `.area(|area| reviews::area(orders::area(area)))`, and each module's handlers are held to its
   caller. The area's stylesheet, deadline and prefix check apply to both. A module may hold a
   single route. The checks across modules are the app's at build rather than the module's:
   a route outside the prefix is a compile error at the `.area(..)` call, and two modules
   serving one path and method, or the same area given in two `.area(..)` calls, stop the app
   at startup.
5. An area a library ships is a function the app passes on,
   `.area(shop_admin::area)`. `Area::route::<R>(get(h))` adds a route by hand; Corten's `get`
   checks the handler as a routes module does, and `R`'s path must have the shape
   `#[derive(Route)]` writes (whole-segment parameters, plain text), or the app does not compile.
6. Areas nest (`/admin` and `/admin/billing`, each with its own routes and stylesheet).

How the common cases look:

| The app | How |
|---|---|
| A signed-in area (account, dashboard) | `#[corten::routes(area = Account, caller = Viewer)]`, whose `identify` sends anyone else to sign in |
| A role's area, stricter inside | an area with `caller = Staff` and one on `/admin/billing` with `caller = Billing` |
| A page in an area that another permission opens (`/account/purchases/{id}/review`) | a second module in the area with its own caller, `.area(\|area\| reviews::area(orders::area(area)))` |
| Anyone reads, signed-in users write (reviews, a price alert posted back to its page) | the `get` handler takes `Option<Caller<Viewer>>`, the `post` handler `Caller<Viewer>` |
| Private everywhere (an internal tool) | `caller = Staff` on the app's routes module, and the sign-in route (and any webhook) in a module without it |
| An API only some callers may use | handlers that take `Caller<Partner>` and return `JsonResult<T>` |
| Only the author edits, members see their organization's records | the handler, which loads the record |
| A payment provider's scripts and frames, or style attributes in rich text, on some pages | an area with `.policy(..)` ([below](#a-policy-for-part-of-the-app)) |

### A policy for part of the app

What only some pages need is allowed on those pages only: an area adds to the app's policy for
the pages at its prefix and under it. A payment page that embeds a provider's fields, for example,
or the pages that show rich text with `style` attributes written in it (a size drawn from data needs
no policy: it is a [custom property](styles.md#values-from-data)):

```rust,ignore
App::new(url::router())
    .area(|area| payments::url::area(area).policy(|p| {
        p.script_src("https://js.stripe.com").frame_src("https://js.stripe.com").connect_src("https://api.stripe.com")
    }))
    .area(|area| help::url::area(area).policy(|p| p.allow_style_attributes()))
```

An area only adds (the closure gets `Sources`, which has no way to remove one), so no page allows
less than the app; nested areas add up, outermost first. The referrer policy below is the one
setting that goes the other way: an area can only make it send less. Each area's header is built once at
startup, and a request looks up its area by path only when some area sets a policy. A document
keeps the policy it loaded with, so a client navigation into or out of such an area loads the page
in full (the page's shell key includes the area's policy); a patch is applied under the policy of
the page it lands on. A `data-patch` form on a page in such an area whose route, elsewhere,
answers with a whole page (not a patch) gets that page under the other policy, so the runtime
loads the current page again rather than swap it in: answer such a form with a patch or a redirect.
An area that adds only sources the app already has keeps the app's policy and shell key. A policy for a single route is an area of one page. An `App::api` keeps its
own `default-src 'none'`.

Every page is sent with `Cross-Origin-Opener-Policy: same-origin`, so a window another site
opened, or one this page opens on another site, cannot reach it. Two things loosen it, for the
whole app on `Policy` or, better, for the pages that need it on their area's `Sources`:
`allow_popups()` (`same-origin-allow-popups`: a popup this page opens keeps `window.opener`, for a
sign-in or payment flow that reports back from it) and `allow_cross_origin_opener()`
(`unsafe-none`, the browser's default, for an embed whose sign-in popup on the provider's origin
talks back to the page, such as Stripe's Connect embedded components, which both stricter values
break). `unsafe-none` gives up the page's isolation from other sites' windows (one that opened
it can navigate it and see when it closes, which cross-site leak attacks use), so keep it to the
pages that embed such a thing. An area only loosens it, and like a CSP source it gives its pages
their own shell key:

```rust,ignore
// With the script, frame, connect and image sources the provider's guide lists for its embed.
.area(|area| settings::url::area(area).policy(|p| p.script_src("https://connect-js.stripe.com").allow_cross_origin_opener()))
```

Every answer is sent with `Referrer-Policy: strict-origin-when-cross-origin`: other sites get the
origin only. A page whose address carries a secret (an OAuth consent's `?token=`, a password reset,
an invitation, a magic link) should not send it even to the app's own logs with the stylesheet and
font requests it makes, so its area makes the policy stricter with `referrer(Referrer::StrictOrigin)`
(the origin only, to everyone), `Referrer::SameOrigin` (nothing to other sites) or
`Referrer::NoReferrer` (nothing at all); `Policy::referrer` does it for the whole app. There is no
looser value, so nothing sends more than the default; nested areas, the app and the area each send
the least any of them allows, and like a CSP source a changed policy gives the area's pages their
own shell key. An `App::api` under the area sends the area's policy too. An answer that brings a
policy of its own (a relayed service's redirect, a handler's response) sends the least of its and
the area's, read as the browser reads it (every line, the last token it knows): `no-referrer` is
kept, `origin` becomes `strict-origin`, and `unsafe-url` gives the area's. A `<meta name="referrer">`
in the page applies only from where the parser reaches it, after the layout's own requests, so use
the area:

```rust,ignore
use corten::security::Referrer;
.area(|area| consent::url::area(area).policy(|p| p.referrer(Referrer::NoReferrer)))
```

## Error pages

Every way a request can fail is a `Failure`, given to the app's error page for a browser
(`App::error_page`), sent with `failure.status()` and `X-Robots-Tag: noindex`:

| `Failure` | When | Status |
|---|---|---|
| `NotFound` | no route has the URL, or the handler returned a 404 | 404 |
| `WrongMethod` | the route has the URL but not the method (`Allow` is kept) | 405 |
| `CrossSite` | a write another site sent, refused before anything else ran | 403 |
| `Refused { status }` | a `Caller` refused, with nowhere to sign in | 401, 403, 429 |
| `Unidentified { detail }` | who is asking could not be looked up (`Refused::unavailable`), unlike a handler's own 503 | 503 |
| `Rejected { status, detail }` | the path, the query or the body did not parse | 400, 415, 422 |
| `Failed { status, detail }` | the handler returned an `Error` | 500, or its status |
| `Panicked { detail }` | the handler panicked | 500 |
| `TimedOut` | the handler ran past its deadline (`App::deadline`, or its area's or API's) | 503 |
| `Offline` | the PWA's offline page | 200, so the worker keeps it |

The error page keeps the headers the failed response had (a request id, `Retry-After`,
`WWW-Authenticate`, `Server-Timing`, `Allow`), so what a handler or a middleware said about a
failure still reaches the client. The ones that described the old body (`Content-*`, `ETag`,
`Last-Modified`) and `Cache-Control` are the error page's own, and a header the error page sets
wins.

The error page is an async function of the `Failure` and then extractors, as a handler's
arguments are, with the state they read given beside it (`()` for none). A layout that shows
who is asking takes the caller as a page does, so a signed-in user who mistypes a URL or is
refused still sees themselves signed in, in their theme:

```rust,ignore
async fn error_page(failure: Failure, req: Req, viewer: Option<Caller<Viewer>>) -> HtmlResult {
    req.page(html! { <Layout req={&req} viewer={viewer.as_deref()} title="Not found">..</Layout> })
}
// App::new(url::router()).error_page(error_page, store.clone())
```

Its extractors see the request's URL, `Cookie`, `Authorization`, `Accept-Language` and mode,
not its body (a body extractor does not compile). The caller is looked up only when the error
page takes one and only for a failure: an unknown URL looks it up once, and a handler that read
it and then failed has it looked up again, since the error page reads a request of its own.
When the lookup itself fails, an optional caller is `None` there, so the page is drawn signed
out rather than not at all. Keeping the credentials for it is measured below.

`failure.detail()` is what went wrong, under `corten dev` only; in production it is logged.
The error page `corten new` writes names every case without `_`, so when Corten adds one the
app stops compiling instead of showing the wrong text. A JSON route's failures, a data
client's and every failure under `App::api` are a JSON Problem Details document instead
(`application/problem+json`, `{"title","status","detail"}`).

When the error page came to take extractors (callgrind, release build, in-process `oneshot`,
the difference between 1,200 and 200 requests over 1,000, three runs each, a minimal app with
the default error page):

| Request | Before | After |
|---|---|---|
| a small page, full document | 21,100 (20,836-21,155) | 21,201 (21,150-21,203) |
| the same page with a cookie | 21,639 (21,601-21,654) | 22,030 (21,713-22,087) |
| a page whose handler reads who is asking | 24,969 (24,955-24,992) | 25,369 (25,346-25,395) |
| unknown URL, HTML 404 page | 20,650 (20,650-20,716) | 26,122 (26,097-26,150) |

A page keeps the request's credentials for the error page (two header lookups and a reference
count each); a failure rebuilds the request for the error page's extractors and runs them.

The cost of a request through all of this is measured in [the request path](#the-request-path).
