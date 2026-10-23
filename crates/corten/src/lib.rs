//! Corten: a full-stack web framework for Rust. The server renders every screen with `html!`,
//! the browser gets HTML plus a small runtime that runs the handlers the markup names, and live
//! data flows as numbered patches over server-sent events.
//!
//! ```ignore
//! use corten::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> std::io::Result<()> {
//!     App::new(Router::new().route("/", get(home))).serve().await
//! }
//!
//! async fn home(req: Req) -> corten::Result {
//!     req.page("Home", html! { <h1>"Hello"</h1> })
//! }
//! ```
//!
//! A page request arrives in one of four [`Mode`]s and one handler answers all of them:
//!
//! | Mode | Sent by | Response |
//! |---|---|---|
//! | `Document` | the browser, first load or no JavaScript | the app's shell around the page |
//! | `Navigation` | the runtime, same-origin link click | `<title>` plus the page, swapped into `<main>` |
//! | `Action` | the runtime, `<form data-patch>` submit | fragments that replace elements with the same `id` |
//! | `Data` | a native screen, an agent or a test, with `Accept: application/json` | the screen's data as JSON ([`Req::screen`]) |
//!
//! Routes with no screen (webhooks, OAuth callbacks, public APIs) answer only JSON: mount them
//! under a prefix with [`App::api`] and every failure there is JSON ([`Api`]).
//!
//! [`App`] puts the production pieces around the app's routes with defaults: security headers
//! and a CSP, a cross-site write check, error pages, versioned static files, cache headers
//! ([`cache`]), health checks, compression (feature `compress`), and live reload under
//! `corten dev`.
//!
//! Modules: [`api`] (JSON-only routes), [`cache`] (HTTP caching), [`view`] (markup), [`script`]
//! (handlers: behavior in the browser), [`style`] (component stylesheets and design tokens), [`route`] (typed links),
//! [`i18n`] (typed messages), [`security`], [`assets`], [`pwa`], [`meta`], [`form`], [`seo`].
//! Features: `json` (handler values, data screens), `feed` (live rows), `solid` (Solid for
//! screens with client state), `types` (TypeScript declarations from Rust types), `agent` (MCP tools),
//! `compress`, `log` (a log subscriber in [`App::serve`]).

// So the code `html!` generates (`::corten::view::...`) also compiles inside this crate.
extern crate self as corten;

#[cfg(feature = "feed")]
pub use corten_feed as feed;
#[cfg(feature = "agent")]
pub mod agent;
pub mod api;
mod app;
pub mod assets;
pub mod cache;
#[cfg(feature = "compress")]
pub mod compress;
pub mod form;
mod html;
pub mod i18n;
pub mod meta;
mod page;
pub mod pwa;
pub mod route;
pub mod script;
pub mod security;
pub mod seo;
mod signal;
pub mod style;
#[cfg(feature = "types")]
pub mod types;
pub mod view;

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

pub use api::Api;
pub use app::App;
/// The axum version Corten is built on, so an app never depends on a second one.
pub use axum;
pub use cache::Cache;
/// Form field names as constants: `#[derive(Deserialize, Fields)] struct AlertInput { email: String }`
/// gives `AlertInput::FIELDS.email`, the [`FieldName`] `"email"` (serde `rename` and `rename_all`
/// applied). Use them for `name=` in the form and in [`Errors::add`], and a renamed or misspelled
/// field fails to compile instead of posting a field the handler never reads.
pub use corten_macros::Fields;
pub use form::{Errors, FieldName};
pub use html::escape;
pub use meta::Meta;
pub use page::{default_error_page, default_shell, Document, Head, IntoShell, Main, Mode, Req, ShellFn, MODE_HEADER};
pub use route::{Href, Route};
pub use script::{event, Handler, Ref, Script};
pub use signal::{Abort, Signal};
pub use style::Style;
pub use view::{component, html, raw, Render};

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
        axum::{
            extract::{Form, Path, Query, State},
            routing::{delete, get, post, put},
            Router,
        },
        component, html,
        i18n::Messages as _,
        raw,
        script::{el, refs, Handler, Ref},
        view::Id,
        Api, App, Cache, Document, Errors, Fields, Meta, Mode, Render, Req, Result, Route,
    };
}

/// The runtime, embedded so apps never copy it: `js/corten.js` minified by `scripts/build-js.mjs`
/// (`npm run check` fails if it is stale), so the documented source costs the browser nothing.
pub const RUNTIME_JS: &str = include_str!("../js/dist/corten.js");

/// Prebuilt by `scripts/build-js.mjs` from `js/solid.js`; apps need no JavaScript tooling.
#[cfg(feature = "solid")]
pub const SOLID_JS: &str = include_str!("../js/dist/solid.js");

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

#[cfg(not(feature = "solid"))]
const CLIENT_HASH: u64 = fnv::hash(RUNTIME_JS.as_bytes());
#[cfg(feature = "solid")]
const CLIENT_HASH: u64 = fnv::extend(fnv::hash(RUNTIME_JS.as_bytes()), SOLID_JS.as_bytes());
static CLIENT_VERSION: LazyLock<String> = LazyLock::new(|| format!("{:012x}", CLIENT_HASH >> 16));
static RUNTIME_URL: LazyLock<String> = LazyLock::new(|| format!("/corten/{}/corten.js", runtime_version()));
#[cfg(feature = "solid")]
static SOLID_URL: LazyLock<String> = LazyLock::new(|| format!("/corten/{}/solid.js", runtime_version()));

/// Content hash of the client code Corten serves; it changes whenever the runtime does.
pub fn runtime_version() -> &'static str {
    &CLIENT_VERSION
}

/// URL the runtime is served from by [`routes`]: `/corten/{version}/corten.js`, cached forever.
pub fn runtime_path() -> &'static str {
    &RUNTIME_URL
}

/// URL of the Solid module (feature `solid`), imported by handlers as `corten/solid`.
#[cfg(feature = "solid")]
pub fn solid_path() -> &'static str {
    &SOLID_URL
}

/// The import map's JSON: `corten`, `corten/solid` when that feature is on, `corten/events`
/// (the app's typed events) when `events` is the URL of their module, and `corten/views` (the
/// app's view classes) when `views` is.
pub(crate) fn import_map(events: Option<&str>, views: Option<&str>) -> String {
    #[cfg(feature = "solid")]
    let solid = format!(r#","corten/solid":"{}""#, solid_path());
    #[cfg(not(feature = "solid"))]
    let solid = "";
    let events = events.map(|url| format!(r#","corten/events":"{url}""#)).unwrap_or_default();
    let views = views.map(|url| format!(r#","corten/views":"{url}""#)).unwrap_or_default();
    format!(r#"{{"imports":{{"corten":"{}"{solid}{events}{views}}}}}"#, runtime_path())
}

/// Streams fragments to `<corten-live src="...">` as server-sent events. Each item is applied like a
/// form patch: top-level elements replace the elements with the same `id`.
///
/// When `fragments` ends, the element closes its connection instead of reconnecting, so a stream
/// of one item is a part of the page sent when it is ready. When the server shuts down the stream
/// ends without that, and the browser reconnects (to another instance, or this one restarted).
pub fn live<S>(fragments: S) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>
where
    S: Stream<Item = String> + Send + 'static,
{
    let patches = fragments.map(|html| Event::default().event("patch").data(html));
    // An event with no data is never dispatched, so `end` carries some.
    let end = futures_util::stream::once(async { Event::default().event("end").data("end") });
    Sse::new(patches.chain(end).map(Ok).take_until(draining())).keep_alive(KeepAlive::default())
}

/// Set when the process starts shutting down; open live streams end on it.
static DRAINING: LazyLock<tokio::sync::watch::Sender<bool>> = LazyLock::new(|| tokio::sync::watch::channel(false).0);

/// Resolves once [`shutdown_signal`] has fired.
async fn draining() {
    let _ = DRAINING.subscribe().wait_for(|draining| *draining).await;
}

/// Routes the runtime at [`runtime_path`] (and Solid at [`solid_path`] with the `solid`
/// feature), versioned and cached forever. Merge into the app router.
pub fn routes<St: Clone + Send + Sync + 'static>() -> Router<St> {
    let router = Router::new().route(runtime_path(), get(|headers: HeaderMap| async move { RUNTIME.respond(&headers) }));
    #[cfg(feature = "solid")]
    let router = router.route(solid_path(), get(|headers: HeaderMap| async move { SOLID.respond(&headers) }));
    router
}

const JS: &str = "text/javascript; charset=utf-8";

/// A script Corten serves from memory, at a versioned URL.
struct Served {
    source: &'static str,
    #[cfg(feature = "compress")]
    packed: std::sync::OnceLock<compress::Precompressed>,
}

static RUNTIME: Served = Served::new(RUNTIME_JS);
#[cfg(feature = "solid")]
static SOLID: Served = Served::new(SOLID_JS);

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

/// Handler result. Any `std::error::Error` converts with `?` and becomes a 500.
pub type Result<T = Response> = std::result::Result<T, Error>;

/// Resolves when the process is asked to stop (Ctrl-C, or `SIGTERM` from a container runtime),
/// after ending every open [`live`] stream and live feed stream (feature `feed`) so a graceful
/// shutdown is not held open by them. Pass it to `axum::serve(...).with_graceful_shutdown(corten::shutdown_signal())`.
pub async fn shutdown_signal() {
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
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    futures_util::future::select(std::pin::pin!(interrupt), std::pin::pin!(terminate)).await;
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

    /// An error with a specific status, for example 404 for a missing record or 403 for a
    /// permission check: `return Err(Error::status(StatusCode::NOT_FOUND))`.
    pub fn status(status: StatusCode) -> Self {
        Error { status, source: status.canonical_reason().unwrap_or("error").into() }
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
