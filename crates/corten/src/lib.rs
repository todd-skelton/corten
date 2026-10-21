//! Corten: server-rendered pages, fragment patches and small custom-element islands on axum.
//!
//! The server is the only place HTML is produced. A request arrives in one of three
//! [`Mode`]s and a handler answers with [`Req::page`] or [`Req::patch`]:
//!
//! | Mode | Sent by | Response |
//! |---|---|---|
//! | `Document` | the browser, first load or no JavaScript | the app [`Shell`] around the page |
//! | `Navigation` | the runtime, same-origin link click | `<title>` plus the page, swapped into `<main>` |
//! | `Action` | the runtime, `<form data-patch>` submit | fragments that replace elements with the same `id` |
//! | `Data` | a native screen, an agent or a test, with `Accept: application/json` | the page's data as JSON ([`Req::screen`], feature `json`) |
//!
//! The client runtime (`js/corten.js`, served at [`runtime_path`]) adds navigation, form
//! patches, live patches over server-sent events and islands. See `README.md` for the rules.
//!
//! Opt-in features for islands that keep client state:
//!
//! - `json`: [`props`], typed data from a template to its island (`this.props()`).
//! - `feed`: [`feed::Feed`] (the `corten-feed` crate), live rows for an island (snapshot +
//!   ordered patches over SSE), read in the browser with `subscribe()` from `corten`.
//! - `solid`: serves Solid at `solid_path()` as `corten/solid`, so an island can render with it.
//!
//! Always available, for a production app:
//!
//! - [`security`]: CSP with a per-request nonce, hardening headers, CSRF check, cookie helpers.
//! - [`assets`]: static files under a content-versioned, immutable URL prefix.
//! - [`pwa`]: manifest, generated service worker and offline page.
//! - [`meta`]: per-page head tags (description, canonical, Open Graph, JSON-LD).
//! - [`i18n`]: message catalogs, `Accept-Language` negotiation, money/number/date formatting.
//! - [`seo`]: `robots.txt` and sitemaps. [`error_pages`]: failed requests render in the shell.
//!
//! Other surfaces (native screens, agents) reuse the same server: see `docs/architecture.md`.

#[cfg(feature = "feed")]
pub use corten_feed as feed;
#[cfg(feature = "agent")]
pub mod agent;
pub mod assets;
mod html;
pub mod i18n;
pub mod meta;
pub mod pwa;
pub mod security;
pub mod seo;

use std::{convert::Infallible, fmt, marker::PhantomData, sync::LazyLock};

pub use askama::Template;
use axum::{
    extract::{FromRequestParts, Request},
    http::{header, request::Parts, HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::get,
    Router,
};
use futures_util::{Stream, StreamExt};

pub use html::{escape, find_islands};
pub use meta::Meta;
use security::Nonce;

/// Prints `value` as `<script type="application/json" data-props>` inside the island it belongs
/// to; the island reads it with `this.props()`. In a template: `{{ corten::Props(self.grid())|safe }}`.
/// `<` is written as `\u003c`, so no value can close the script element.
#[cfg(feature = "json")]
pub struct Props<'a, T: serde::Serialize>(pub &'a T);

#[cfg(feature = "json")]
impl<T: serde::Serialize> fmt::Display for Props<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// Streams serde_json's output into the formatter, escaping `<`.
        struct Out<'a, 'b>(&'a mut fmt::Formatter<'b>);
        impl std::io::Write for Out<'_, '_> {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                // serde_json writes whole UTF-8 sequences per call.
                let text = std::str::from_utf8(buf).map_err(std::io::Error::other)?;
                for (i, part) in text.split('<').enumerate() {
                    if i > 0 {
                        self.0.write_str("\\u003c").map_err(std::io::Error::other)?;
                    }
                    self.0.write_str(part).map_err(std::io::Error::other)?;
                }
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        f.write_str(r#"<script type="application/json" data-props>"#)?;
        serde_json::to_writer(Out(f), self.0).map_err(|_| fmt::Error)?;
        f.write_str("</script>")
    }
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

/// The import map: `corten`, plus `corten/solid` when that feature is on.
fn write_import_map(f: &mut fmt::Formatter<'_>, nonce: &NonceAttr<'_>) -> fmt::Result {
    write!(f, r#"<script type="importmap"{nonce}>{{"imports":{{"corten":"{}""#, runtime_path())?;
    #[cfg(feature = "solid")]
    write!(f, r#","corten/solid":"{}""#, solid_path())?;
    f.write_str("}}</script>")
}

/// ` nonce="..."` when the request has a CSP nonce, nothing otherwise.
struct NonceAttr<'a>(Option<&'a str>);

impl fmt::Display for NonceAttr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(nonce) => write!(f, r#" nonce="{nonce}""#),
            None => Ok(()),
        }
    }
}

/// Request header the runtime sets: `nav` for navigations, `action` for form submits.
pub const MODE_HEADER: HeaderName = HeaderName::from_static("corten");
const PAGE_HEADER: HeaderName = HeaderName::from_static("corten-page");
const PATCH_HEADER: HeaderName = HeaderName::from_static("corten-patch");
const LOCATION_HEADER: HeaderName = HeaderName::from_static("corten-location");

/// How the browser asked for this response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A normal browser request: first load, reload, or any request without JavaScript.
    Document,
    /// The runtime is navigating to a page and will swap it into `<main>`.
    Navigation,
    /// The runtime submitted a `<form data-patch>` and expects fragments back.
    Action,
    /// A client that renders by itself (a native screen, an agent) asked for
    /// `Accept: application/json`. [`Req::screen`] answers with the page's data; [`Req::page`]
    /// still answers with the HTML document.
    Data,
}

/// A template that renders the contents of `<main>`.
pub trait Page: Template {
    /// Text for `<title>`. Escaped by Corten.
    fn title(&self) -> String;

    /// Description, canonical URL, Open Graph and other head tags for this page.
    fn head(&self) -> Meta {
        Meta::new()
    }
}

/// The app-wide document around every page: `<html>`, `<head>`, site header, `<main>`.
///
/// Implement it with an askama template that prints `{{ head|safe }}` early in `<head>` (before
/// any module script) and `{{ main|safe }}` inside `<main>`. An inline script of its own needs
/// `nonce="{{ head.nonce() }}"` once [`security::Policy`] is on.
pub trait Shell: 'static {
    /// Where island modules live. `<foo-bar>` loads `{islands_dir}foo-bar.js`. With
    /// [`assets::Assets`], return its `islands_dir()`.
    fn islands_dir() -> &'static str {
        "/static/islands/"
    }

    /// The service worker to register ([`pwa::WORKER_PATH`]), if the app is installable.
    const SERVICE_WORKER: Option<&'static str> = None;

    fn render(head: &Head<'_>, main: &str) -> askama::Result<String>;

    /// Title and `<main>` HTML for a request that failed with `status` (see [`error_pages`]).
    fn error_page(status: StatusCode) -> (String, String) {
        match status {
            StatusCode::NOT_FOUND => ("Not found".into(), "<h1>Not found</h1><p>There is nothing at this address.</p>".into()),
            StatusCode::FORBIDDEN => {
                ("Not allowed".into(), "<h1>Not allowed</h1><p>You do not have access to this page.</p>".into())
            }
            _ => ("Something went wrong".into(), "<h1>Something went wrong</h1><p>Please try again.</p>".into()),
        }
    }
}

/// Tags Corten owns in `<head>`: title, the page's [`Meta`], import map, the runtime script,
/// module preloads for the islands the page uses, and the rule that makes island wrappers
/// layout-neutral. Print it with `|safe`, before any other module script.
pub struct Head<'a> {
    pub title: &'a str,
    meta: &'a Meta,
    nonce: Option<&'a str>,
    islands_dir: &'a str,
    islands: Vec<&'a str>,
    service_worker: Option<&'a str>,
}

impl Head<'_> {
    /// This request's CSP nonce, for the shell's own inline scripts. Empty without a policy.
    pub fn nonce(&self) -> &str {
        self.nonce.unwrap_or("")
    }
}

impl fmt::Display for Head<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nonce = NonceAttr(self.nonce);
        write!(f, "<title>{}</title>{}", escape(self.title), self.meta)?;
        write!(f, r#"<meta name="corten-islands" content="{}">"#, self.islands_dir)?;
        if let Some(worker) = self.service_worker {
            write!(f, r#"<meta name="corten-sw" content="{worker}">"#)?;
        }
        write_import_map(f, &nonce)?;
        write!(f, r#"<script type="module" src="{}"></script>"#, runtime_path())?;
        for tag in &self.islands {
            write!(f, r#"<link rel="modulepreload" href="{}{tag}.js">"#, self.islands_dir)?;
        }
        if !self.islands.is_empty() {
            write!(f, "<style{nonce}>{}{{display:contents}}</style>", self.islands.join(","))?;
        }
        Ok(())
    }
}

/// Extractor that tells a handler which [`Mode`] it is answering, and builds the response.
///
/// Apps alias it once: `pub type Req = corten::Req<Shell>;`
pub struct Req<S: Shell> {
    pub mode: Mode,
    nonce: Option<Nonce>,
    shell: PhantomData<fn() -> S>,
}

impl<S: Shell, St: Send + Sync> FromRequestParts<St> for Req<S> {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &St) -> std::result::Result<Self, Self::Rejection> {
        Ok(Req { mode: mode_of(&parts.headers), nonce: parts.extensions.get::<Nonce>().cloned(), shell: PhantomData })
    }
}

fn mode_of(headers: &HeaderMap) -> Mode {
    match headers.get(MODE_HEADER).map(HeaderValue::as_bytes) {
        Some(b"nav") => Mode::Navigation,
        Some(b"action") => Mode::Action,
        _ if accepts_json(headers) => Mode::Data,
        _ => Mode::Document,
    }
}

/// True when the first media type in `Accept` is `application/json`. Browsers never send that
/// for a navigation, so a document request cannot turn into data by accident.
fn accepts_json(headers: &HeaderMap) -> bool {
    let accept = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("");
    accept.split(',').next().is_some_and(|first| first.split(';').next().unwrap_or("").trim() == "application/json")
}

impl<S: Shell> Req<S> {
    /// Renders `page` with status 200: the whole document, or only the page for a navigation.
    pub fn page(&self, page: &impl Page) -> Result {
        self.page_with_status(StatusCode::OK, page)
    }

    pub fn page_with_status(&self, status: StatusCode, page: &impl Page) -> Result {
        let main = page.render()?;
        self.main(status, &page.title(), &page.head(), &main)
    }

    /// The response for already-rendered `<main>` HTML.
    fn main(&self, status: StatusCode, title: &str, meta: &Meta, main: &str) -> Result {
        let body = match self.mode {
            // The runtime swaps these into <main> and the head tags into <head>; an action that
            // answers with a page (for example a form with errors) is swapped the same way.
            Mode::Navigation | Mode::Action => format!("<title>{}</title>{meta}{main}", escape(title)),
            Mode::Document | Mode::Data => {
                let head = Head {
                    title,
                    meta,
                    nonce: self.nonce.as_ref().map(|n| &*n.0),
                    islands_dir: S::islands_dir(),
                    islands: find_islands(main),
                    service_worker: S::SERVICE_WORKER,
                };
                S::render(&head, main)?
            }
        };
        Ok(html_response(status, body, PAGE_HEADER))
    }

    /// [`Req::page`] for a page that other surfaces read too: a browser gets HTML as usual, and a
    /// [`Mode::Data`] request gets `{"title": ..., "data": <the page struct>}`. The struct is the
    /// screen's view model, so a native screen or an agent sees exactly what the template sees,
    /// and the template is never parsed for data.
    #[cfg(feature = "json")]
    pub fn screen<P: Page + serde::Serialize>(&self, page: &P) -> Result {
        self.screen_with_status(StatusCode::OK, page)
    }

    #[cfg(feature = "json")]
    pub fn screen_with_status<P: Page + serde::Serialize>(&self, status: StatusCode, page: &P) -> Result {
        if self.mode != Mode::Data {
            return self.page_with_status(status, page);
        }
        #[derive(serde::Serialize)]
        struct Screen<'a, P> {
            title: String,
            data: &'a P,
        }
        let body = serde_json::to_string(&Screen { title: page.title(), data: page })?;
        let headers = [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/json")),
            (header::VARY, HeaderValue::from_static(VARY)),
        ];
        Ok((status, headers, body).into_response())
    }

    /// Answers a `<form data-patch>` submit. Every top-level element in `fragment` must carry an
    /// `id`; the runtime replaces the element with that `id` on the page.
    pub fn patch(&self, fragment: &impl Template) -> Result {
        self.patch_with_status(StatusCode::OK, fragment)
    }

    /// Same as [`Req::patch`] with another status, for example 422 for validation errors.
    pub fn patch_with_status(&self, status: StatusCode, fragment: &impl Template) -> Result {
        Ok(html_response(status, fragment.render()?, PATCH_HEADER))
    }

    /// After a successful write, send the browser to `location` (post/redirect/get). Works with
    /// and without JavaScript.
    pub fn redirect(&self, location: &str) -> Result {
        let value = HeaderValue::try_from(location).map_err(Error::new)?;
        Ok(match self.mode {
            Mode::Action => (StatusCode::NO_CONTENT, [(LOCATION_HEADER, value)]).into_response(),
            _ => (StatusCode::SEE_OTHER, [(header::LOCATION, value)]).into_response(),
        })
    }
}

/// Renders failed requests (an [`Error`] from a handler) with [`Shell::error_page`], in the same
/// mode as the request: a full document, a page swap, or JSON for a data client. Apply it inside
/// [`security::Policy::apply`] so the error page gets the nonce:
/// `policy.apply(corten::error_pages::<Shell, _>(router))`.
pub fn error_pages<S: Shell, St: Clone + Send + Sync + 'static>(router: Router<St>) -> Router<St> {
    router.layer(axum::middleware::from_fn(render_errors::<S>))
}

async fn render_errors<S: Shell>(req: Request, next: Next) -> Response {
    let mode = mode_of(req.headers());
    let nonce = req.extensions().get::<Nonce>().cloned();
    let res = next.run(req).await;
    if res.extensions().get::<Failed>().is_none() {
        return res;
    }
    let status = res.status();
    if mode == Mode::Data {
        let body = format!(r#"{{"error":{{"status":{}}}}}"#, status.as_u16());
        return (status, [(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))], body).into_response();
    }
    let (title, main) = S::error_page(status);
    let req = Req::<S> { mode, nonce, shell: PhantomData };
    req.main(status, &title, &Meta::new().noindex(), &main).unwrap_or(res)
}

fn html_response(status: StatusCode, body: String, kind: HeaderName) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8")),
            (header::VARY, HeaderValue::from_static(VARY)),
            (kind, HeaderValue::from_static("1")),
        ],
        body,
    )
        .into_response()
}

/// Responses depend on the mode header and, for [`Mode::Data`], on `Accept`.
const VARY: &str = "corten, accept";

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
    let router = Router::new().route(runtime_path(), get(|| async { js(RUNTIME_JS) }));
    #[cfg(feature = "solid")]
    let router = router.route(solid_path(), get(|| async { js(SOLID_JS) }));
    router
}

fn js(source: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/javascript; charset=utf-8")),
            (header::CACHE_CONTROL, assets::IMMUTABLE),
        ],
        source,
    )
        .into_response()
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
/// `tracing`, never sent to the browser; [`error_pages`] renders the page the user sees.
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

/// Marks a response as a failure, so [`error_pages`] knows to render it.
#[derive(Clone, Copy)]
struct Failed;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        if self.status.is_server_error() {
            tracing::error!(status = self.status.as_u16(), error = %self.source, "corten: request failed");
        } else {
            tracing::debug!(status = self.status.as_u16(), error = %self.source, "corten: request refused");
        }
        let mut res = (self.status, self.status.canonical_reason().unwrap_or("Error")).into_response();
        res.extensions_mut().insert(Failed);
        res
    }
}

#[cfg(test)]
mod tests;
