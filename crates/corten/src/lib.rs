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
//! The client runtime (`js/corten.js`, served at [`RUNTIME_PATH`]) adds navigation, form
//! patches, live patches over server-sent events and islands. See `README.md` for the rules.
//!
//! Opt-in features for islands that keep client state:
//!
//! - `json`: [`props`], typed data from a template to its island (`this.props()`).
//! - `feed`: [`feed::Feed`] (the `corten-feed` crate), live rows for an island (snapshot +
//!   ordered patches over SSE), read in the browser with `subscribe()` from `corten`.
//! - `solid`: serves Solid at [`SOLID_PATH`] as `corten/solid`, so an island can render with it.
//!
//! Other surfaces (native screens, agents) reuse the same server: see `docs/architecture.md`.

#[cfg(feature = "feed")]
pub use corten_feed as feed;
mod html;

use std::{convert::Infallible, fmt, marker::PhantomData};

pub use askama::Template;
use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts, HeaderName, HeaderValue, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::get,
    Router,
};
use futures_util::{Stream, StreamExt};

pub use html::{escape, find_islands};

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

/// URL the runtime is served from by [`routes`].
pub const RUNTIME_PATH: &str = "/corten/corten.js";
/// The runtime source, embedded so apps never copy it.
pub const RUNTIME_JS: &str = include_str!("../js/corten.js");

/// URL of the Solid module (feature `solid`), imported by islands as `corten/solid`.
#[cfg(feature = "solid")]
pub const SOLID_PATH: &str = "/corten/solid.js";
/// Prebuilt by `scripts/build-solid.mjs` from `js/solid.js`; apps need no JavaScript tooling.
#[cfg(feature = "solid")]
pub const SOLID_JS: &str = include_str!("../js/dist/solid.js");

/// The import map: `corten`, plus `corten/solid` when that feature is on.
fn write_import_map(f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, r#"<script type="importmap">{{"imports":{{"corten":"{RUNTIME_PATH}""#)?;
    #[cfg(feature = "solid")]
    write!(f, r#","corten/solid":"{SOLID_PATH}""#)?;
    f.write_str("}}</script>")
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
}

/// The app-wide document around every page: `<html>`, `<head>`, site header, `<main>`.
///
/// Implement it with an askama template that prints `{{ head|safe }}` early in `<head>` (before
/// any module script) and `{{ main|safe }}` inside `<main>`.
pub trait Shell {
    /// Where island modules live. `<foo-bar>` loads `{ISLANDS_DIR}foo-bar.js`.
    const ISLANDS_DIR: &'static str = "/static/islands/";
    fn render(head: &Head<'_>, main: &str) -> askama::Result<String>;
}

/// Tags Corten owns in `<head>`: title, import map, the runtime script, module preloads for the
/// islands the page uses, and the rule that makes island wrappers layout-neutral. Print it with
/// `|safe`, before any other module script.
pub struct Head<'a> {
    pub title: &'a str,
    islands_dir: &'a str,
    islands: Vec<&'a str>,
}

impl fmt::Display for Head<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<title>{}</title>", escape(self.title))?;
        write!(f, r#"<meta name="corten-islands" content="{}">"#, self.islands_dir)?;
        write_import_map(f)?;
        write!(f, r#"<script type="module" src="{RUNTIME_PATH}"></script>"#)?;
        for tag in &self.islands {
            write!(f, r#"<link rel="modulepreload" href="{}{tag}.js">"#, self.islands_dir)?;
        }
        if !self.islands.is_empty() {
            write!(f, "<style>{}{{display:contents}}</style>", self.islands.join(","))?;
        }
        Ok(())
    }
}

/// Extractor that tells a handler which [`Mode`] it is answering, and builds the response.
///
/// Apps alias it once: `pub type Req = corten::Req<Shell>;`
pub struct Req<S: Shell> {
    pub mode: Mode,
    shell: PhantomData<fn() -> S>,
}

impl<S: Shell, St: Send + Sync> FromRequestParts<St> for Req<S> {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &St) -> std::result::Result<Self, Self::Rejection> {
        let mode = match parts.headers.get(MODE_HEADER).map(HeaderValue::as_bytes) {
            Some(b"nav") => Mode::Navigation,
            Some(b"action") => Mode::Action,
            _ if accepts_json(parts) => Mode::Data,
            _ => Mode::Document,
        };
        Ok(Req { mode, shell: PhantomData })
    }
}

/// True when the first media type in `Accept` is `application/json`. Browsers never send that
/// for a navigation, so a document request cannot turn into data by accident.
fn accepts_json(parts: &Parts) -> bool {
    let accept = parts.headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("");
    accept.split(',').next().is_some_and(|first| first.split(';').next().unwrap_or("").trim() == "application/json")
}

impl<S: Shell> Req<S> {
    /// Renders `page` with status 200: the whole document, or only the page for a navigation.
    pub fn page(&self, page: &impl Page) -> Result {
        self.page_with_status(StatusCode::OK, page)
    }

    pub fn page_with_status(&self, status: StatusCode, page: &impl Page) -> Result {
        let main = page.render()?;
        let title = page.title();
        let body = match self.mode {
            // The runtime swaps these into <main>; an action that answers with a page (for
            // example a form with errors) is swapped the same way.
            Mode::Navigation | Mode::Action => format!("<title>{}</title>{main}", escape(&title)),
            Mode::Document | Mode::Data => {
                let head = Head { title: &title, islands_dir: S::ISLANDS_DIR, islands: find_islands(&main) };
                S::render(&head, &main)?
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

/// Routes the runtime at [`RUNTIME_PATH`] (and Solid at [`SOLID_PATH`] with the `solid`
/// feature). Merge into the app router.
pub fn routes<St: Clone + Send + Sync + 'static>() -> Router<St> {
    let router = Router::new().route(RUNTIME_PATH, get(|| async { js(RUNTIME_JS) }));
    #[cfg(feature = "solid")]
    let router = router.route(SOLID_PATH, get(|| async { js(SOLID_JS) }));
    router
}

fn js(source: &'static str) -> Response {
    ([(header::CONTENT_TYPE, "text/javascript; charset=utf-8")], source).into_response()
}

/// Handler result. Any `std::error::Error` converts with `?` and becomes a 500.
pub type Result<T = Response> = std::result::Result<T, Error>;

/// An error that ends the request with 500. The message goes to stderr, not to the browser.
#[derive(Debug)]
pub struct Error(Box<dyn std::error::Error + Send + Sync>);

impl Error {
    pub fn new(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        Error(error.into())
    }
}

impl<E: std::error::Error + Send + Sync + 'static> From<E> for Error {
    fn from(error: E) -> Self {
        Error(Box::new(error))
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        eprintln!("corten: request failed: {}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong.").into_response()
    }
}

#[cfg(test)]
mod tests;
