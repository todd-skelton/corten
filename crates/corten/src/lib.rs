//! Corten: a full-stack web framework for Rust. The server renders every screen with `html!`,
//! the browser gets HTML plus a small runtime that runs the handlers the markup names, and live
//! data flows as numbered patches over server-sent events.
//!
//! ```ignore
//! use corten::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> std::io::Result<()> {
//!     corten::serve(App::new(url::router())).await
//! }
//!
//! #[corten::routes]
//! mod url {
//!     #[route("/", get = home)]
//!     pub struct Home;
//! }
//!
//! // What the route answers: a page (failures are the app's error page).
//! async fn home(req: Req) -> HtmlResult {
//!     req.page(html! { <Layout req={&req} title="Home"><h1>"Hello"</h1></Layout> })
//! }
//! ```
//!
//! A page request arrives in one of four [`Mode`]s and one handler answers all of them:
//!
//! | Mode | Sent by | Response |
//! |---|---|---|
//! | `Document` | the browser, first load or no JavaScript | the whole document: the app's layout with the page in it |
//! | `Navigation` | the runtime, same-origin link click | `<title>`, the page's head tags and `<main>`'s children, swapped in |
//! | `Action` | the runtime, `<form data-patch>` submit | fragments that replace elements with the same `id` |
//! | `Data` | a native screen, an agent or a test, with `Accept: application/json` | the screen's data as JSON ([`Req::screen`]) |
//!
//! A handler's return type is what its route answers ([`answer`]): [`HtmlResult`] for a page,
//! [`JsonResult<T>`] for JSON (every failure of the route is JSON too), [`Public`] around either
//! for an answer a shared cache may keep. Who is asking is a [`Caller`], looked up only when a
//! handler takes one. Routes that share a prefix and answer only JSON mount under [`App::api`];
//! a part of the app with its own stylesheet or caller (an admin area, an account) is an
//! [`Area`]. The app's own steps in every request are [`middleware`].
//!
//! [`App`] puts the production pieces around the app's routes with defaults: security headers
//! and a CSP, a cross-site write check, error pages, versioned static files, cache headers
//! ([`cache`]), health checks, compression (feature `compress`), and live reload under
//! `corten dev`.
//!
//! Modules: [`answer`] (what a route answers), [`caller`] (who is asking), [`middleware`],
//! [`api`] (JSON-only routes), [`cache`] (HTTP caching), [`view`] (markup), [`script`]
//! (handlers: behavior in the browser), [`style`] (component stylesheets and design tokens), [`route`] (typed links),
//! [`i18n`] (typed messages), [`security`], [`assets`], [`pwa`], [`form`], [`seo`].
//! Features: `json` (handler values, data screens), `feed` (live rows), `types` (TypeScript
//! declarations from Rust types), `agent` (MCP tools), `compress`, `log` (a log subscriber in
//! [`serve`]).

// So the code `html!` generates (`::corten::view::...`) also compiles inside this crate.
extern crate self as corten;

#[cfg(feature = "feed")]
pub use corten_feed as feed;
#[cfg(feature = "agent")]
pub mod agent;
pub mod answer;
pub mod api;
mod app;
mod ascii;
pub mod assets;
pub mod cache;
pub mod caller;
mod client;
#[cfg(feature = "compress")]
pub mod compress;
pub mod form;
mod html;
pub mod i18n;
#[cfg(feature = "json")]
pub mod json;
pub mod middleware;
mod page;
pub mod pwa;
pub mod route;
pub mod script;
pub mod security;
pub mod seo;
mod signal;
pub mod style;
mod task;
#[cfg(feature = "types")]
pub mod types;
pub mod upgrade;
pub mod view;
mod watch;

use std::{
    convert::Infallible,
    sync::{Arc, LazyLock},
};

use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::get,
    Router,
};
use futures_util::{Stream, StreamExt};

pub use answer::{Answer, ForAll, ForOne, HtmlResponse, HtmlResult, LiveResponse, Media, NoStore, Public, Relay};
#[cfg(feature = "json")]
pub use answer::{JsonError, JsonResponse, JsonResult};
pub use api::Api;
pub use app::{listeners, serve, serve_on, serve_wrapped, App, Area};
/// The axum version Corten is built on, so an app never depends on a second one.
pub use axum;
pub use caller::{Caller, Identify, Refused};
pub use client::{ClientIp, ClientZone};
/// Form field names as constants: `#[derive(Deserialize, Fields)] struct AlertInput { email: String }`
/// gives `AlertInput::FIELDS.email`, the [`FieldName`] `"email"` (serde `rename` and `rename_all`
/// applied). Use them for `name=` in the form and in [`Errors::add`], and a renamed or misspelled
/// field fails to compile instead of posting a field the handler never reads.
pub use corten_macros::Fields;
/// The app's shared state, split for handlers: each field gets axum's `FromRef`, so a handler
/// takes the part it reads (`State<Market>`) while the routes share one struct
/// (`#[corten::routes(state = Shared)]`, `App::new(url::router(shared))`). The impl clones the
/// field, as one written by hand would, and holds where the field's type is `Clone` (a generic
/// state's part only for the parameters that allow it). A field no handler takes alone is
/// `#[from_ref(skip)]`. With axum's `macros` feature on, axum exports a derive of the same name:
/// write `#[derive(corten::FromRef)]` where both are in scope.
///
/// ```
/// use corten::{axum::extract::State, prelude::*};
///
/// #[derive(Clone)]
/// struct Market(u32);
/// #[derive(Clone)]
/// struct Lists(&'static str);
///
/// #[derive(Clone, FromRef)]
/// struct Shared {
///     market: Market,
///     pub lists: Lists,
///     #[from_ref(skip)]
///     secret: String,
/// }
///
/// fn part<T: corten::axum::extract::FromRef<Shared>>(shared: &Shared) -> T {
///     T::from_ref(shared)
/// }
/// let shared = Shared { market: Market(7), lists: Lists("a"), secret: String::new() };
/// assert_eq!(part::<Market>(&shared).0, 7);
/// assert_eq!(part::<Lists>(&shared).0, "a");
/// # let _ = &shared.secret;
/// # async fn handler(State(market): State<Market>) -> String { market.0.to_string() }
/// ```
///
/// Two fields of one type would make `State<T>` ambiguous, so they do not compile until one is
/// skipped (the error names both fields):
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// #[derive(Clone, FromRef)]
/// struct Shared {
///     primary: String,
///     replica: String,
/// }
/// ```
///
/// A skipped field gets no impl, so a second field of its type is allowed, and its type is not a
/// part a handler can take:
///
/// ```
/// use corten::{axum::extract::FromRef, prelude::*};
///
/// #[derive(Clone, FromRef)]
/// struct Shared {
///     primary: String,
///     #[from_ref(skip)]
///     replica: String,
/// }
/// let shared = Shared { primary: "p".into(), replica: "r".into() };
/// assert_eq!(String::from_ref(&shared), "p");
/// # let _ = shared.replica;
/// ```
///
/// ```compile_fail
/// use corten::{axum::extract::FromRef, prelude::*};
///
/// #[derive(Clone)]
/// struct Keys(u8);
///
/// #[derive(Clone, FromRef)]
/// struct Shared {
///     name: String,
///     #[from_ref(skip)]
///     keys: Keys,
/// }
/// let shared = Shared { name: String::new(), keys: Keys(1) };
/// let keys = Keys::from_ref(&shared);
/// ```
///
/// A generic state, a bound with an associated type, and `Self` in a field all work:
///
/// ```
/// use std::sync::Arc;
/// use corten::{axum::extract::FromRef, prelude::*};
///
/// #[derive(Clone)]
/// struct Part<T>(T);
///
/// #[derive(Clone, FromRef)]
/// struct Shared<T: Iterator<Item = u8>, U> {
///     iter: Arc<T>,
///     part: Part<U>,
///     next: Option<Arc<Self>>,
/// }
///
/// let shared = Shared { iter: Arc::new([1u8].into_iter()), part: Part("x"), next: None };
/// assert_eq!(Part::<&str>::from_ref(&shared).0, "x");
/// assert!(Option::<Arc<Shared<_, &str>>>::from_ref(&shared).is_none());
/// let _ = <Arc<std::array::IntoIter<u8, 1>>>::from_ref(&shared);
///
/// // `Self` in a parameter's bound, and a `where` with nothing in it.
/// trait Wraps<S> {}
/// #[derive(Clone)]
/// struct Inner;
/// impl<S> Wraps<S> for Inner {}
/// #[derive(Clone, FromRef)]
/// struct Bound<T: Wraps<Self>> where { part: Arc<T> }
/// let bound = Bound { part: Arc::new(Inner) };
/// let _ = <Arc<Inner>>::from_ref(&bound);
///
/// // A parameter's default after an arrow in its bound.
/// #[derive(Clone, FromRef)]
/// struct Calls<F: Fn() -> u8 = fn() -> u8> { call: Arc<F> }
/// let calls: Calls = Calls { call: Arc::new((|| 7) as fn() -> u8) };
/// assert_eq!(<Arc<fn() -> u8>>::from_ref(&calls)(), 7);
/// ```
pub use corten_macros::FromRef;
pub use form::{Errors, FieldName, Files, Form, SubmitKey, Upload, UploadError};
pub use html::escape;
#[cfg(feature = "json")]
pub use json::{FromJson, Json, ToJson};
pub use middleware::{Middleware, Next};
#[cfg(feature = "json")]
pub use page::JsonLd;
pub use page::{Corten, ErrorPageFn, Failure, Mode, Nav, ReadAgain, ReadAgainFrom, Req, MODE_HEADER};
pub use route::{routes, Elsewhere, Href, Local, Methods, Params, ParamsRejection, Prefix, Route, Routes, UrlValue};
pub use script::{event, Handler, Ref, Script};
pub use signal::{Abort, Signal};
pub use style::Style;
pub use view::{component, html, raw, Document, Patch, Render};

/// What a component sends to the browser: its handlers' script (`#[component(js)]` or
/// [`script::script!`]), its stylesheet (`#[component(css)]`, or a token file's), its view class
/// (`#[component(view)]`), or the class of a typed event ([`script::event`]).
///
/// Each such component has one, named like the component (`OfferList`, a constant next to the
/// struct `OfferList`). A library lists its own, `pub const COMPONENTS: &[Component] = &[Button,
/// Dialog];`, and the app passes every list to [`App::components`]: the scripts are served and
/// preloaded where they are used, the stylesheets joined, in order, into one file, and the events
/// served as `corten/events`.
#[derive(Clone, Copy, Debug)]
pub struct Component {
    /// The component's Rust name, `"OfferList"`.
    pub name: &'static str,
    pub script: Option<Script>,
    pub style: Option<Style>,
    pub event: Option<script::EventDef>,
    pub view: Option<script::ViewDef>,
}

/// What most files need: `use corten::prelude::*;`.
pub mod prelude {
    pub use crate::{
        axum::extract::State,
        component, html,
        i18n::Messages as _,
        raw,
        route::{delete, get, post, put},
        script::{el, refs, Handler, Ref},
        view::Id,
        Api, App, Area, Caller, Corten, Document, Errors, Failure, FieldName, Fields, Files, Form, FromRef, HtmlResult, Identify,
        Methods, Middleware, Mode, Next, NoStore, Params, Patch, Public, ReadAgain, Refused, Render, Req, Route, Routes,
        SubmitKey, Upload, UploadError, UrlValue,
    };
    #[cfg(feature = "json")]
    pub use crate::{FromJson, Json, JsonLd, JsonResult, ToJson};
}

/// The runtime, embedded so apps never copy it: `js/corten.js` minified by `scripts/build-js.mjs`
/// (`npm run check` fails if it is stale), so the documented source costs the browser nothing.
pub const RUNTIME_JS: &str = include_str!("../js/dist/corten.js");

/// FNV-1a, 64 bit: a stable content hash for versioned URLs (not for security).
pub(crate) mod fnv {
    pub const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    pub const fn extend(mut hash: u64, bytes: &[u8]) -> u64 {
        let mut i = 0;
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
            i += 1;
        }
        hash
    }
    pub const fn hash(bytes: &[u8]) -> u64 {
        extend(OFFSET, bytes)
    }
}

const CLIENT_HASH: u64 = fnv::hash(RUNTIME_JS.as_bytes());
static CLIENT_VERSION: LazyLock<String> = LazyLock::new(|| format!("{:012x}", CLIENT_HASH >> 16));
static RUNTIME_URL: LazyLock<String> = LazyLock::new(|| format!("/corten/{}/corten.js", runtime_version()));

/// Content hash of the client code Corten serves; it changes whenever the runtime does.
pub fn runtime_version() -> &'static str {
    &CLIENT_VERSION
}

/// URL the runtime is served from by [`routes`]: `/corten/{version}/corten.js`, cached forever.
pub fn runtime_path() -> &'static str {
    &RUNTIME_URL
}

/// The import map's JSON: `corten`, `corten/events` (the app's typed events) when `events` is the
/// URL of their module, and `corten/views` (the app's view classes) when `views` is.
pub(crate) fn import_map(events: Option<&str>, views: Option<&str>) -> String {
    let events = events.map(|url| format!(r#","corten/events":"{url}""#)).unwrap_or_default();
    let views = views.map(|url| format!(r#","corten/views":"{url}""#)).unwrap_or_default();
    format!(r#"{{"imports":{{"corten":"{}"{events}{views}}}}}"#, runtime_path())
}

/// Streams fragments to `<corten-live src="...">` as server-sent events. Each item is applied like a
/// form patch: top-level elements replace the elements with the same `id`. In debug builds a
/// fragment with a top-level element the browser would drop (no id, or text outside an element)
/// panics, naming it.
///
/// An item is rendered HTML (a `String`), or a [`Fragment`] that also carries the app's cursor
/// after it, so a reconnect resumes there ([`LiveSince`]).
///
/// When `fragments` ends, the element closes its connection instead of reconnecting, so a stream
/// of one item is a part of the page sent when it is ready. When the server shuts down the stream
/// ends without that, and the browser reconnects (to another instance, or this one restarted).
pub fn live<S>(fragments: S) -> LiveResponse
where
    S: Stream + Send + 'static,
    S::Item: Into<Fragment>,
{
    let patches = fragments.map(|fragment| {
        let Fragment { html, cursor } = fragment.into();
        // A top-level element without an id would be dropped in the browser: a bug in the view.
        #[cfg(debug_assertions)]
        if let Some(problem) = crate::html::patch_problem(&html) {
            panic!("corten: a live fragment's top-level {problem}, so the browser would drop it: give each top-level element a #[derive(Id)] id the page has");
        }
        let event = Event::default().event("patch");
        match cursor {
            Some(cursor) => event.id(cursor),
            None => event,
        }
        .data(html)
    });
    // An event with no data is never dispatched, so `end` carries some.
    let end = futures_util::stream::once(async { Event::default().event("end").data("end") });
    LiveResponse(
        Sse::new(patches.chain(end).map(Ok::<_, Infallible>).take_until(draining()))
            .keep_alive(KeepAlive::default())
            .into_response(),
    )
}

/// A [`live`] stream of one value that changes (the last trade, a count, a status), kept in a
/// `tokio::sync::watch` channel: `view` renders it and `cursor` names the version it is, so a
/// page already showing the current one (`since`, [`LiveSince`]) gets nothing until the next
/// change, and one that is behind gets the current value first. Values the browser missed in
/// between are skipped: it only ever needs the latest. The stream ends when the channel's sender
/// is dropped. Each change is cloned out for each page before it renders, so the sender never
/// waits on a render; a large value goes in an `Arc`.
///
/// ```
/// use corten::{prelude::*, LiveResponse, LiveSince};
/// use tokio::sync::watch;
///
/// #[derive(Id)]
/// struct LastTradeId;
///
/// #[component]
/// fn LastTrade(text: &str) -> impl Patch {
///     html! { <p id={LastTradeId}>{text}</p> }
/// }
///
/// // The trade count and the last trade's text.
/// async fn trades(State(trades): State<watch::Receiver<(u64, String)>>, since: LiveSince) -> LiveResponse {
///     corten::live_watch(trades, since, |(n, _)| *n, |(_, text)| LastTrade { text }.to_html())
/// }
/// ```
pub fn live_watch<T, K, C, V>(mut values: tokio::sync::watch::Receiver<T>, since: LiveSince, cursor: C, view: V) -> LiveResponse
where
    T: Clone + Send + Sync + 'static,
    K: ToString,
    C: Fn(&T) -> K + Send + 'static,
    V: Fn(&T) -> String + Send + 'static,
{
    // Marked seen here, so a receiver cloned from one that never read (kept in the app's state)
    // or a value sent since it subscribed is not sent again to a page that shows it.
    let current = cursor(&values.borrow_and_update()).to_string();
    if since.0.as_deref() != Some(current.as_str()) {
        values.mark_changed();
    }
    live(futures_util::stream::unfold((values, cursor, view), |(mut values, cursor, view)| async move {
        values.changed().await.ok()?;
        // Copied out, so the sender never waits on a render.
        let value = values.borrow_and_update().clone();
        Some((Fragment::new(view(&value)).cursor(cursor(&value).to_string()), (values, cursor, view)))
    }))
}

/// One item of a [`live`] stream: rendered HTML, and the app's cursor after it.
///
/// The cursor is the point in the app's own data this fragment brings the page to (a version,
/// a sequence number, a realtime stream's position); Corten only carries it. It is sent as the
/// event's `id`, so a browser that reconnects sends it back as `Last-Event-ID`, which
/// [`LiveSince`] reads.
///
/// ```
/// let fragment = corten::Fragment::new("<p id=\"n\">3</p>".into()).cursor(42.to_string());
/// ```
pub struct Fragment {
    html: String,
    cursor: Option<String>,
}

impl Fragment {
    pub fn new(html: String) -> Self {
        Fragment { html, cursor: None }
    }

    /// The app's cursor after this fragment. A cursor cannot hold a line break or a NUL, which
    /// would end the event: that panics in debug builds, and in release the fragment is sent
    /// without its cursor (logged), so a reconnect draws the part again.
    pub fn cursor(mut self, cursor: impl Into<String>) -> Self {
        let cursor = cursor.into();
        if cursor.contains(['\n', '\r', '\0']) {
            let message = format!("corten: a live fragment's cursor holds a line break or NUL: {cursor:?}");
            if cfg!(debug_assertions) {
                panic!("{message}");
            }
            tracing::error!("{message}");
            return self;
        }
        self.cursor = Some(cursor);
        self
    }
}

impl From<String> for Fragment {
    fn from(html: String) -> Self {
        Fragment::new(html)
    }
}

/// Where a [`live`] stream resumes: the cursor of the last [`Fragment`] the browser received,
/// from `Last-Event-ID` when it reconnects, or else the `since` the page gave its element,
/// `<corten-live src={..} since={cursor}>`, on the first connection. `None` when there is
/// neither, and the handler draws the part as it is now.
///
/// A handler sends what changed after the cursor, or nothing until the next change when the
/// cursor is current, so a page view or a reconnect costs no extra read or render.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveSince(pub Option<String>);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for LiveSince {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, _: &S) -> std::result::Result<Self, Self::Rejection> {
        let header =
            parts.headers.get("last-event-id").and_then(|v| v.to_str().ok()).filter(|v| !v.is_empty()).map(str::to_string);
        let query = || {
            form_urlencoded::parse(parts.uri.query()?.as_bytes())
                .find(|(key, _)| key == "since")
                .map(|(_, value)| value.into_owned())
                .filter(|value| !value.is_empty())
        };
        Ok(LiveSince(header.or_else(query)))
    }
}

/// Set when the process starts shutting down; open live streams end on it.
static DRAINING: LazyLock<tokio::sync::watch::Sender<bool>> = LazyLock::new(|| tokio::sync::watch::channel(false).0);

/// Resolves once [`shutdown_signal`] has fired.
async fn draining() {
    let _ = DRAINING.subscribe().wait_for(|draining| *draining).await;
}

/// Routes the runtime at [`runtime_path`], versioned and cached forever. Merge into the app
/// router.
pub fn routes<St: Clone + Send + Sync + 'static>() -> Router<St> {
    Router::new().route(runtime_path(), get(|headers: HeaderMap| async move { RUNTIME.respond(&headers) }))
}

const JS: &str = "text/javascript; charset=utf-8";

/// A script Corten serves from memory, at a versioned URL.
struct Served {
    source: &'static str,
    #[cfg(feature = "compress")]
    packed: std::sync::OnceLock<compress::Precompressed>,
}

static RUNTIME: Served = Served::new(RUNTIME_JS);

impl Served {
    const fn new(source: &'static str) -> Self {
        Served {
            source,
            #[cfg(feature = "compress")]
            packed: std::sync::OnceLock::new(),
        }
    }

    fn respond(&self, headers: &HeaderMap) -> Response {
        let head = [(header::CONTENT_TYPE, HeaderValue::from_static(JS)), (header::CACHE_CONTROL, assets::IMMUTABLE)];
        #[cfg(feature = "compress")]
        {
            let packed = self.packed.get_or_init(|| compress::Precompressed::new(self.source.as_bytes().into(), JS));
            let (body, coding) = packed.pick(headers);
            let mut res = (head, body).into_response();
            compress::encoded_headers(res.headers_mut(), JS, coding);
            res
        }
        #[cfg(not(feature = "compress"))]
        {
            let _ = headers;
            (head, self.source).into_response()
        }
    }
}

/// `GET /health/live` (the process answers) and `GET /health/ready` (`ready` says it can serve:
/// database reachable, feeds loaded). Both answer `no-store` plain text.
pub fn health<St, F, Fut>(ready: F) -> Router<St>
where
    St: Clone + Send + Sync + 'static,
    F: Fn() -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = bool> + Send,
{
    let answer = |ok: bool| {
        let status = if ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
        (status, [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], if ok { "ok" } else { "unavailable" })
            .into_response()
    };
    Router::new().route("/health/live", get(move || async move { answer(true) })).route(
        "/health/ready",
        get(move || {
            let ready = ready.clone();
            async move { answer(ready().await) }
        }),
    )
}

/// A result whose error is Corten's [`Error`], for the code a handler calls: any
/// `std::error::Error` converts with `?` and becomes a 500. A handler itself returns what it
/// answers: [`HtmlResult`] is this with the answer in it, and [`JsonResult`]'s error, a
/// [`JsonError`], takes this one with `?` (and the input's field errors too). It is not in
/// the prelude, so `Result` in an app is the standard one:
///
/// ```
/// use corten::prelude::*;
///
/// fn count(text: &str) -> corten::Result<u32> {
///     Ok(text.parse::<u32>()?)
/// }
///
/// fn parse(text: &str, field: FieldName, errors: &mut Errors) -> Result<u32, ()> {
///     text.parse().map_err(|_| errors.add(field, "Enter a whole number."))
/// }
/// ```
///
/// A bare `corten::Result` names no answer, so it is not a handler's type:
///
/// ```compile_fail
/// async fn page() -> corten::Result {
///     Ok(())
/// }
/// ```
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Whether the app has asked to stop ([`shutdown`]).
static ASKED: LazyLock<tokio::sync::watch::Sender<bool>> = LazyLock::new(|| tokio::sync::watch::channel(false).0);

/// Begins the graceful shutdown a signal begins ([`shutdown_signal`]): the app's
/// [tasks](App::task) are told to stop, live streams end, the listeners close, requests in
/// progress finish, and [`serve`] returns once the tasks have too (or [`App::shutdown_grace`] has
/// passed). For a process with no signal to send it: a route a supervisor calls (whose answer is
/// sent, as a request in progress, before its connection closes), a task that decides the app is
/// done, or a service on Windows started without a console. A second call, or a signal after it,
/// changes nothing.
///
/// It is process-wide and cannot be undone: it stops every [`serve`] in the process, including
/// one started after the call. A test binary that runs a route calling it stops every other test's
/// server too, so such a route is tested with the call behind something the test replaces.
///
/// ```
/// use corten::{axum::http::StatusCode, prelude::*};
///
/// async fn stop(/* Caller<Supervisor> */) -> StatusCode {
///     corten::shutdown();
///     StatusCode::ACCEPTED
/// }
/// # let _: Methods = post(stop);
/// ```
pub fn shutdown() {
    ASKED.send_replace(true);
}

/// Resolves when the process is asked to stop (Ctrl-C; `SIGTERM` from a container runtime on
/// Unix; Ctrl-Break, closing the console or the system shutting down on Windows) or the app asks
/// ([`shutdown`]), after ending every open [`live`] stream and live feed stream (feature `feed`)
/// so a graceful shutdown is not held open by them. [`serve`] uses it; an app that serves its
/// router another way passes it to `axum::serve(...).with_graceful_shutdown(corten::shutdown_signal())`,
/// and then runs no [`App::task`].
pub async fn shutdown_signal() {
    use futures_util::future::select;
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    // What Windows sends a console process: Ctrl-Break (how a supervisor stops a process group),
    // the console closing, and the system shutting down. Windows ends the process a few seconds
    // after the console closes, whatever is still running.
    #[cfg(windows)]
    let terminate = async {
        use tokio::signal::windows;
        let close = async {
            match windows::ctrl_close() {
                Ok(mut close) => close.recv().await,
                Err(_) => std::future::pending().await,
            }
        };
        let shutdown = async {
            match windows::ctrl_shutdown() {
                Ok(mut shutdown) => shutdown.recv().await,
                Err(_) => std::future::pending().await,
            }
        };
        let interrupt = async {
            match windows::ctrl_break() {
                Ok(mut interrupt) => interrupt.recv().await,
                Err(_) => std::future::pending().await,
            }
        };
        select(std::pin::pin!(close), select(std::pin::pin!(shutdown), std::pin::pin!(interrupt))).await;
    };
    #[cfg(not(any(unix, windows)))]
    let terminate = std::future::pending::<()>();
    let asked = async {
        let _ = ASKED.subscribe().wait_for(|asked| *asked).await;
    };
    select(std::pin::pin!(asked), select(std::pin::pin!(interrupt), std::pin::pin!(terminate))).await;
    tracing::info!("corten: shutting down");
    DRAINING.send_replace(true);
    #[cfg(feature = "feed")]
    corten_feed::drain();
}

/// An error that ends the request with a status (500 unless set). The message is logged with
/// `tracing` and never sent to the browser (except under `corten dev`); the app's error page is
/// what the user sees.
#[derive(Debug)]
pub struct Error {
    status: StatusCode,
    source: Box<dyn std::error::Error + Send + Sync>,
}

impl Error {
    pub fn new(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Error { status: StatusCode::INTERNAL_SERVER_ERROR, source: error.into() }
    }

    /// An error with a specific status, for example 403 for a permission check (404 is
    /// [`Error::not_found`]): `return Err(Error::status(StatusCode::FORBIDDEN))` (in a JSON handler,
    /// `Err(Error::status(..))?` or `.into()`, since its error is a [`JsonError`]).
    pub fn status(status: StatusCode) -> Self {
        Error { status, source: status.canonical_reason().unwrap_or("error").into() }
    }

    /// 404, for a record that is not there: `load(id).await?.ok_or_else(Error::not_found)?`. The
    /// most common status a handler sets, so it has a name; others are [`Error::status`]. Its
    /// message is `Not Found`; for a JSON client that should read a specific one,
    /// `Error::new("no item 7").with_status(StatusCode::NOT_FOUND)`.
    ///
    /// ```
    /// use corten::{axum::http::StatusCode, Error};
    ///
    /// fn find(id: u32) -> corten::Result<&'static str> {
    ///     let name = [(7, "Ada")].into_iter().find(|(n, _)| *n == id).map(|(_, name)| name);
    ///     name.ok_or_else(Error::not_found)
    /// }
    /// # use corten::axum::response::IntoResponse;
    /// assert_eq!(find(8).unwrap_err().into_response().status(), StatusCode::NOT_FOUND);
    /// ```
    pub fn not_found() -> Self {
        Error::status(StatusCode::NOT_FOUND)
    }

    /// The same error with another status: `Error::new("no item 7").with_status(StatusCode::NOT_FOUND)`.
    /// A JSON client (an [`App::api`] route, or a request for `application/json`) gets a 4xx
    /// error's message, so give it one written for the client; the error page never shows it.
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
}

impl<E: std::error::Error + Send + Sync + 'static> From<E> for Error {
    fn from(error: E) -> Self {
        Error::new(error)
    }
}

/// Marks a response as a failure, so the app renders its error page. Carries the message, which
/// only an app in dev mode shows.
#[derive(Clone)]
pub(crate) struct Failed(pub Arc<str>);

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(status = self.status.as_u16(), error = %self.source, "corten: request failed");
        } else {
            tracing::debug!(status = self.status.as_u16(), error = %self.source, "corten: request refused");
        }
        let mut res = (self.status, self.status.canonical_reason().unwrap_or("Error")).into_response();
        res.extensions_mut().insert(Failed(Arc::from(self.source.to_string())));
        res
    }
}

#[cfg(test)]
mod tests;

/// The README and the guides as doctests: `cargo test` compiles every `rust` block in them that
/// is not marked `ignore`, so the documented examples cannot drift from the API.
#[cfg(all(doctest, feature = "json"))]
mod doc_examples {
    #[doc = include_str!("../../../README.md")]
    struct Readme;
    #[doc = include_str!("../../../docs/guide/app.md")]
    struct App;
    #[doc = include_str!("../../../docs/guide/cancellation.md")]
    struct Cancellation;
    #[doc = include_str!("../../../docs/guide/getting-started.md")]
    struct GettingStarted;
    #[doc = include_str!("../../../docs/guide/scripts.md")]
    struct Scripts;
    #[doc = include_str!("../../../docs/guide/live-data.md")]
    struct LiveData;
    #[doc = include_str!("../../../docs/guide/requests-and-forms.md")]
    struct RequestsAndForms;
    #[doc = include_str!("../../../docs/guide/styles.md")]
    struct Styles;
    #[doc = include_str!("../../../docs/guide/type-safety.md")]
    struct TypeSafety;
    #[doc = include_str!("../../../docs/guide/ui.md")]
    struct Ui;
    #[doc = include_str!("../../../docs/guide/views.md")]
    struct Views;
}
