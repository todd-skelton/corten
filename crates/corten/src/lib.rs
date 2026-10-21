//! Corten: a full-stack web framework for Rust. The server renders every screen with `html!`,
//! the browser gets HTML plus a small runtime and custom-element islands, and live data flows as
//! numbered patches over server-sent events.
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
//! A request arrives in one of four [`Mode`]s and one handler answers all of them:
//!
//! | Mode | Sent by | Response |
//! |---|---|---|
//! | `Document` | the browser, first load or no JavaScript | the app's shell around the page |
//! | `Navigation` | the runtime, same-origin link click | `<title>` plus the page, swapped into `<main>` |
//! | `Action` | the runtime, `<form data-patch>` submit | fragments that replace elements with the same `id` |
//! | `Data` | a native screen, an agent or a test, with `Accept: application/json` | the screen's data as JSON ([`Req::screen`]) |
//!
//! [`App`] puts the production pieces around the app's routes with defaults: security headers
//! and a CSP nonce, a cross-site write check, error pages, versioned static files, health checks,
//! compression (feature `compress`), and live reload under `corten dev`.
//!
//! Modules: [`view`] (markup), [`island`] (components with a script), [`route`] (typed links),
//! [`i18n`] (typed messages), [`security`], [`assets`], [`pwa`], [`meta`], [`form`], [`seo`].
//! Features: `json` (island props, data screens), `feed` (live rows), `solid` (Solid for
//! stateful islands), `types` (TypeScript declarations from Rust types), `agent` (MCP tools),
//! `compress`, `log` (a log subscriber in [`App::serve`]).

// So the code `html!` generates (`::corten::view::...`) also compiles inside this crate.
extern crate self as corten;

#[cfg(feature = "feed")]
pub use corten_feed as feed;
#[cfg(feature = "agent")]
pub mod agent;
mod app;
pub mod assets;
#[cfg(feature = "compress")]
pub mod compress;
pub mod form;
mod html;
pub mod i18n;
pub mod island;
pub mod meta;
mod page;
pub mod pwa;
pub mod route;
pub mod security;
pub mod seo;
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

pub use app::App;
/// The axum version Corten is built on, so an app never depends on a second one.
pub use axum;
/// Form field names as constants: `#[derive(Deserialize, Fields)] struct AlertInput { email: String }`
/// gives `AlertInput::FIELDS.email == "email"` (serde `rename` and `rename_all` applied). Use them
/// for `name=` in the form and in [`Errors::add`], and a renamed or misspelled field fails to
/// compile instead of posting a field the handler never reads.
pub use corten_macros::Fields;
pub use form::Errors;
pub use html::{escape, find_islands};
pub use island::{island, Island};
pub use meta::Meta;
pub use page::{default_error_page, default_shell, Document, Head, IntoShell, Main, Mode, Req, ShellFn, MODE_HEADER};
pub use route::{Href, Route};
pub use view::{component, html, raw, Render};

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
        island, raw, App, Document, Errors, Fields, Meta, Mode, Render, Req, Result, Route,
    };
}

/// The runtime source, embedded so apps never copy it.
pub const RUNTIME_JS: &str = include_str!("../js/corten.js");

/// Prebuilt by `scripts/build-solid.mjs` from `js/solid.js`; apps need no JavaScript tooling.
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

/// URL of the Solid module (feature `solid`), imported by islands as `corten/solid`.
#[cfg(feature = "solid")]
pub fn solid_path() -> &'static str {
    &SOLID_URL
}

/// The import map's JSON: `corten`, plus `corten/solid` when that feature is on.
pub(crate) fn import_map() -> &'static str {
    static MAP: LazyLock<String> = LazyLock::new(|| {
        #[cfg(feature = "solid")]
        let solid = format!(r#","corten/solid":"{}""#, solid_path());
        #[cfg(not(feature = "solid"))]
        let solid = "";
        format!(r#"{{"imports":{{"corten":"{}"{solid}}}}}"#, runtime_path())
    });
    &MAP
}

/// Streams fragments to `<corten-live src="...">` as server-sent events. Each item is applied like a
/// form patch: top-level elements replace the elements with the same `id`.
pub fn live<S>(fragments: S) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>
where
    S: Stream<Item = String> + Send + 'static,
{
    Sse::new(fragments.map(|html| Ok(Event::default().event("patch").data(html)))).keep_alive(KeepAlive::default())
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
struct Script {
    source: &'static str,
    #[cfg(feature = "compress")]
    packed: std::sync::OnceLock<compress::Precompressed>,
}

static RUNTIME: Script = Script::new(RUNTIME_JS);
#[cfg(feature = "solid")]
static SOLID: Script = Script::new(SOLID_JS);

impl Script {
    const fn new(source: &'static str) -> Self {
        Script {
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
/// after ending every live feed stream (feature `feed`) so a graceful shutdown is not held open
/// by them. Pass it to `axum::serve(...).with_graceful_shutdown(corten::shutdown_signal())`.
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
