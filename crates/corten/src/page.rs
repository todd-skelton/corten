//! Pages and patches: what a handler returns and how it reaches each kind of client.

use std::{
    cell::Cell,
    convert::Infallible,
    future::Future,
    hash::{Hash, Hasher},
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, LazyLock,
    },
};

use axum::{
    body::Body,
    extract::{FromRequestParts, Request},
    http::{header, request::Parts, Extensions, HeaderMap, HeaderName, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};

use crate::{
    answer::{HtmlResponse, HtmlResult},
    app::Next,
    assets::Assets,
    security::NonceSlot,
    view::{html, Document, Part, Render},
    Error, Result,
};

/// Request header the runtime sets: `nav` for navigations, `action` for form submits.
pub const MODE_HEADER: HeaderName = HeaderName::from_static("corten");
const PAGE_HEADER: HeaderName = HeaderName::from_static("corten-page");
const PATCH_HEADER: HeaderName = HeaderName::from_static("corten-patch");
const LOCATION_HEADER: HeaderName = HeaderName::from_static("corten-location");
const SHELL_HEADER: HeaderName = HeaderName::from_static("corten-shell");
/// A navigation's page with a notice: `m=<text>`, form-encoded, and `&d` when the page drew it.
const NOTICE_HEADER: HeaderName = HeaderName::from_static("corten-notice");
/// The cookie a redirect's notice rides in: `__Host-`, so no other site on the domain can set it.
pub(crate) const NOTICE_COOKIE: &str = "__Host-corten-notice";
/// The notice's cookie for a write from a plain `http://` origin that is not loopback (a LAN
/// tool), which refuses a `Secure` cookie and so a `__Host-` one: read only from a request without
/// `Sec-Fetch-Site`, which a current browser sends to every secure origin, so on HTTPS another site
/// on the domain cannot plant one there; never with [`crate::security::Policy::hsts`], which says
/// the app is HTTPS only.
pub(crate) const PLAIN_NOTICE_COOKIE: &str = "corten-notice";
/// The longest notice kept, in bytes: a sentence, well inside a cookie's 4 KB once encoded.
const NOTICE_MAX: usize = 500;

/// Responses depend on the mode header and, for [`Mode::Data`], on `Accept`.
const VARY: &str = "corten, accept";
/// The same, for an app with [`crate::App::messages`]: the locale follows `Accept-Language`.
const VARY_LANGUAGE: &str = "corten, accept, accept-language";

/// How the client asked for this response.
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

pub(crate) fn mode_of(headers: &HeaderMap) -> Mode {
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

fn is_current(path: &str, href: &str, section: bool) -> bool {
    let href = href.split(['?', '#']).next().unwrap_or(href);
    path == href || (section && path.strip_prefix(href.trim_end_matches('/')).is_some_and(|rest| rest.starts_with('/')))
}

/// A nav link's attributes, from [`Req::nav`] or [`Req::nav_section`]: `href`, `data-nav` and,
/// on its own page, `aria-current="page"`. The route's URL is made once, written and compared.
pub struct Nav<'a, R> {
    pub(crate) path: &'a str,
    pub(crate) to: &'a R,
    pub(crate) section: bool,
}

impl<R> crate::view::__private::Writes for Nav<'_, R> {
    const WRITES: &'static [&'static str] = &["href", "data-nav", "aria-current", "data-no-prefetch"];
}

impl<R: crate::Route> crate::view::Attrs for Nav<'_, R> {
    fn render_attrs(&self, out: &mut String) {
        let start = out.len() + r#" href=""#.len();
        out.push_str(r#" href=""#);
        crate::Route::write_href(self.to, out, true);
        // A derived route's path holds no character escaping changes, and the comparison stops
        // before the query, so the written text is compared. A hand-written route whose path was
        // escaped is compared as its unescaped URL.
        let written = &out[start..];
        let current = if written.split(['?', '#']).next().unwrap_or(written).contains('&') {
            is_current(self.path, &crate::route::link(self.to), self.section)
        } else {
            is_current(self.path, written, self.section)
        };
        out.push('"');
        out.push_str(if self.section { r#" data-nav="section""# } else { " data-nav" });
        if current {
            out.push_str(r#" aria-current="page""#);
        }
        crate::route::no_prefetch(" href", R::PREFETCH, out);
    }
}

/// What a nav entry can link to: a typed route, or a [`crate::Local`] path read from data. Not a
/// URL written as text, nor another site's (`Elsewhere`, a `Uri`): that is never this page. A route
/// is trusted as [`Req::redirect`] trusts it: one implemented by hand writes what it writes.
pub trait NavTarget: nav_target::Sealed {
    /// Writes the URL as the browser asks for it, not escaped for HTML.
    #[doc(hidden)]
    fn write_url(&self, out: &mut String);

    /// Whether the runtime may fetch the page ahead of a click: a route's [`crate::Route::PREFETCH`].
    #[doc(hidden)]
    fn prefetch(&self) -> bool {
        true
    }
}

mod nav_target {
    pub trait Sealed {}
    impl<R: crate::Route + ?Sized> Sealed for R {}
    impl Sealed for crate::Local {}
}

impl<R: crate::Route + ?Sized> NavTarget for R {
    fn write_url(&self, out: &mut String) {
        self.write_href(out, false);
    }

    fn prefetch(&self) -> bool {
        R::PREFETCH
    }
}

impl NavTarget for crate::Local {
    /// The path with the characters a browser percent-encodes in one (`` "<>`{}^| ``) encoded, so the
    /// link, the request it makes and the runtime's `pathname` are the same text in every engine
    /// (Firefox and WebKit keep `|`, Chromium encodes it).
    fn write_url(&self, out: &mut String) {
        let href = self.as_str();
        {
            let end = href.find(['?', '#']).unwrap_or(href.len());
            for c in href[..end].chars() {
                match c {
                    '"' | '<' | '>' | '`' | '{' | '}' | '^' | '|' => {
                        const HEX: &[u8; 16] = b"0123456789ABCDEF";
                        out.push('%');
                        out.push(HEX[c as usize >> 4] as char);
                        out.push(HEX[c as usize & 15] as char);
                    }
                    c => out.push(c),
                }
            }
            out.push_str(&href[end..]);
        }
    }
}

/// One entry of a nav, for [`Req::nav_of`].
pub struct NavTo<'a> {
    to: &'a dyn NavTarget,
    section: bool,
}

impl<'a> NavTo<'a> {
    /// An entry current on its own URL alone (the query and fragment aside), as [`Req::nav`].
    pub fn page(to: &'a (impl NavTarget + 'a)) -> Self {
        NavTo { to, section: false }
    }

    /// An entry current on its own URL and every one under it, as [`Req::nav_section`]: a
    /// deeper entry of the same nav that also holds the page wins over it.
    pub fn section(to: &'a (impl NavTarget + 'a)) -> Self {
        NavTo { to, section: true }
    }
}

/// A nav's links, from [`Req::nav_of`], each with its item, with the deepest entries that hold
/// the page marked current. Each URL is written once, at the call.
pub struct NavLinks<T> {
    /// The URLs one after another, not escaped.
    urls: String,
    /// Where each entry's URL ends, whether it is a section, how deep it is when it holds the
    /// page, its item, and whether the runtime may prefetch it.
    entries: Vec<(usize, bool, Option<usize>, T, bool)>,
    /// The deepest of them: the entries that deep are current.
    deepest: Option<usize>,
    /// The group the runtime ranks these links in (`data-nav-group`): 32 bits of a hash of the
    /// entries, so a patch's nav does not join another of the page's, and two equal navs agree
    /// anyway. Navs made to collide only mark the wrong link after a navigation.
    group: [u8; 8],
}

impl<T> NavLinks<T> {
    /// Each link's attributes with its item, in the entries' order:
    /// `for (link, item) in nav.links() { <a {..link}>{&item.label}</a> }`. Render every one: the
    /// runtime ranks the links in the page, the server ranked them all.
    pub fn links(&self) -> impl Iterator<Item = (NavLink<'_>, &T)> {
        let mut start = 0;
        let group = std::str::from_utf8(&self.group).expect("hex digits");
        self.entries.iter().map(move |(end, section, depth, item, prefetch)| {
            let url = &self.urls[start..*end];
            start = *end;
            (
                NavLink {
                    url,
                    section: *section,
                    current: depth.is_some() && *depth == self.deepest,
                    group,
                    prefetch: *prefetch,
                },
                item,
            )
        })
    }
}

/// One link of a [`NavLinks`]: `href`, `data-nav`, `data-nav-group`, when current
/// `aria-current="page"`, and `data-no-prefetch` for a route that says `prefetch = false`.
pub struct NavLink<'a> {
    url: &'a str,
    section: bool,
    current: bool,
    group: &'a str,
    prefetch: bool,
}

impl crate::view::__private::Writes for NavLink<'_> {
    const WRITES: &'static [&'static str] = &["href", "data-nav", "data-nav-group", "aria-current", "data-no-prefetch"];
}

impl crate::view::Attrs for NavLink<'_> {
    fn render_attrs(&self, out: &mut String) {
        out.push_str(r#" href=""#);
        crate::view::escape_into(self.url, out);
        out.push('"');
        out.push_str(if self.section { r#" data-nav="section""# } else { " data-nav" });
        out.push_str(r#" data-nav-group=""#);
        out.push_str(self.group);
        out.push('"');
        if self.current {
            out.push_str(r#" aria-current="page""#);
        }
        crate::route::no_prefetch(" href", self.prefetch, out);
    }
}

/// How deep an entry is, by its path without the query, fragment or trailing `/`, when it holds
/// `path`; the runtime's rule for the links of one `data-nav-group` (`markCurrent` in `corten.js`).
fn holds(path: &str, url: &str, section: bool) -> Option<usize> {
    is_current(path, url, section).then(|| url.split(['?', '#']).next().unwrap_or(url).trim_end_matches('/').len())
}

/// [`Req::nav_of`] at `path`.
pub(crate) fn nav_links<'a, T>(path: &str, entries: impl IntoIterator<Item = (NavTo<'a>, T)>) -> NavLinks<T> {
    let mut urls = String::new();
    let entries: Vec<_> = entries
        .into_iter()
        .map(|(entry, item)| {
            let start = urls.len();
            entry.to.write_url(&mut urls);
            (urls.len(), entry.section, holds(path, &urls[start..], entry.section), item, entry.to.prefetch())
        })
        .collect();
    let deepest = entries.iter().filter_map(|entry| entry.2).max();
    // The URLs and which are sections name the group; its 32 low bits, as hex.
    let hash = entries.iter().fold(crate::fnv::extend(crate::fnv::OFFSET, urls.as_bytes()), |hash, entry| {
        crate::fnv::extend(hash, &[entry.1 as u8, (entry.0 >> 8) as u8, entry.0 as u8])
    });
    let mut group = [0; 8];
    for (i, digit) in group.iter_mut().enumerate() {
        *digit = b"0123456789abcdef"[(hash >> (28 - 4 * i) & 15) as usize];
    }
    NavLinks { urls, entries, deepest, group }
}

#[cfg(test)]
pub(crate) fn is_current_for_tests(path: &str, href: &str, section: bool) -> bool {
    is_current(path, href, section)
}

/// The page's key, printed as 16 hex digits.
fn key_header(key: u64) -> HeaderValue {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut text = [0u8; 16];
    for (i, digit) in text.iter_mut().enumerate() {
        *digit = HEX[(key >> (60 - 4 * i) & 15) as usize];
    }
    HeaderValue::from_bytes(&text).expect("hex is a valid header")
}

/// Every way a request can fail, given to the app's error page ([`crate::App::error_page`]) for a
/// browser. A JSON route's failures, and a data client's, are JSON instead. A page that matches
/// each variant without `_` does not compile when Corten adds one, so it cannot miss it.
#[derive(Clone, Debug)]
pub enum Failure {
    /// Nothing is at this URL (404): no route has it, or its handler returned
    /// `Error::not_found()` (the error is logged).
    NotFound,
    /// A route has this URL but not this method (405; the `Allow` header is kept).
    WrongMethod,
    /// A write another site sent (403), refused before anything else ran.
    CrossSite,
    /// The handler needs a [`crate::Caller`] the request does not have, or one who may not do
    /// this, and there is nowhere to sign in: 401 or 403, or 429 for a client with too many failed
    /// attempts ([`crate::Refused`]); or a middleware turned the request away with a `Refused`
    /// (a rate limit's 429).
    Refused { status: StatusCode },
    /// Who is asking could not be found out now ([`crate::Refused::unavailable`]: the session
    /// store is down), so the request was neither let through nor sent to sign in (503). Not a
    /// handler's own 503, which is [`Failure::Failed`]. The detail is the lookup's error.
    Unidentified { detail: Option<Arc<str>> },
    /// The path, the query or the body did not parse (400, 415 or 422). The detail is the
    /// extractor's text, written for a developer.
    Rejected { status: StatusCode, detail: Option<Arc<str>> },
    /// The handler returned an [`Error`]: 500, or the status [`Error::status`] gave (a 404 is
    /// [`Failure::NotFound`]).
    Failed { status: StatusCode, detail: Option<Arc<str>> },
    /// The handler panicked (500).
    Panicked { detail: Option<Arc<str>> },
    /// The handler ran past the deadline ([`crate::App::deadline`], 503).
    TimedOut,
    /// The PWA's offline page, which the service worker shows when the network is down (sent
    /// with 200 so the worker keeps it).
    Offline,
}

impl Failure {
    /// The status the error page is sent with (the offline page's is 200).
    pub fn status(&self) -> StatusCode {
        match self {
            Failure::NotFound => StatusCode::NOT_FOUND,
            Failure::WrongMethod => StatusCode::METHOD_NOT_ALLOWED,
            Failure::CrossSite => StatusCode::FORBIDDEN,
            Failure::Refused { status } | Failure::Rejected { status, .. } | Failure::Failed { status, .. } => *status,
            Failure::Panicked { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Failure::TimedOut | Failure::Unidentified { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Failure::Offline => StatusCode::OK,
        }
    }

    /// What went wrong, for the developer: set under `corten dev` only, and logged instead in
    /// production.
    pub fn detail(&self) -> Option<&str> {
        match self {
            Failure::Rejected { detail, .. }
            | Failure::Failed { detail, .. }
            | Failure::Panicked { detail }
            | Failure::Unidentified { detail } => detail.as_deref(),
            _ => None,
        }
    }

    fn without_detail(self) -> Self {
        match self {
            Failure::Rejected { status, .. } => Failure::Rejected { status, detail: None },
            Failure::Failed { status, .. } => Failure::Failed { status, detail: None },
            Failure::Panicked { .. } => Failure::Panicked { detail: None },
            Failure::Unidentified { .. } => Failure::Unidentified { detail: None },
            other => other,
        }
    }
}

/// Why a failed response failed, when its status and body do not say: set by the stage that
/// answered it, read when the error page is chosen.
#[derive(Clone, Copy)]
pub(crate) enum Cause {
    Panicked,
    TimedOut,
    Refused,
    Unidentified,
}

/// The error page of an app that gives none ([`crate::App::error_page`]): a plain document. An app
/// made by `corten new` has its own, in its layout. Every error page is sent with
/// `X-Robots-Tag: noindex`.
pub(crate) async fn default_error_page(failure: Failure, req: Req) -> HtmlResult {
    req.page(default_error_document(&req, failure))
}

fn default_error_document(req: &Req, failure: Failure) -> impl crate::view::Document + '_ {
    let (title, text) = match failure {
        Failure::NotFound => ("Not found", "There is nothing at this address."),
        Failure::CrossSite => ("Not allowed", "This request came from another site."),
        Failure::Refused { status } if status == StatusCode::UNAUTHORIZED => ("Sign in needed", "Sign in to see this page."),
        Failure::Refused { status } if status == StatusCode::TOO_MANY_REQUESTS => {
            ("Too many requests", "Wait a moment and try again.")
        }
        Failure::Refused { .. } => ("Not allowed", "You do not have access to this page."),
        Failure::Unidentified { .. } => ("Not available", "We could not check who you are. Try again in a moment."),
        Failure::TimedOut | Failure::Offline => ("Not available", "Check your connection and try again."),
        Failure::WrongMethod | Failure::Rejected { .. } | Failure::Failed { .. } | Failure::Panicked { .. } => {
            ("Something went wrong", "Please try again.")
        }
    };
    html! {
        <!DOCTYPE html>
        <html lang={req.lang()}>
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>{title}</title>
                <Corten />
            </head>
            <body>
                <main>
                    <h1>{title}</h1>
                    <p>{text}</p>
                    if let Some(detail) = failure.detail() {
                        <pre>{detail}</pre>
                    }
                </main>
            </body>
        </html>
    }
}

/// An error page with its arguments and state erased: the page's answer for a failed request,
/// from the failure and the request as [`Asked`] kept it ([`error_page`] finishes it).
pub(crate) type ErrorPage = dyn Fn(Failure, Parts) -> Pin<Box<dyn Future<Output = Response> + Send>> + Send + Sync;

/// The app's error page, as [`crate::App::error_page`] takes it: an async function of the
/// [`Failure`] and then extractors, as a handler's arguments are (`Req`, the caller who is
/// asking with `Option<Caller<Viewer>>`, `State<..>` of the state it is given), that answers
/// `req.page(..)`:
///
/// ```ignore
/// async fn error_page(failure: Failure, req: Req, viewer: Option<Caller<Viewer>>) -> HtmlResult {
///     let viewer = viewer.map(|Caller(v)| v);
///     req.page(html! { <Layout req={&req} viewer={viewer.as_ref()} title="..">..</Layout> })
/// }
/// ```
///
/// It is sent with the failure's status (or as it answers, when it redirects). The extractors see
/// the request's URL, `Cookie` and `Authorization` (so a caller is found as for the handler),
/// `Accept-Language` and its mode; not its other headers or its body:
///
/// ```
/// use corten::prelude::*;
///
/// async fn error_page(failure: Failure, req: Req) -> HtmlResult {
///     req.page(html! { <html><head><title>"Error"</title><Corten /></head><body><main>{failure.status().as_str()}</main></body></html> })
/// }
/// let _ = App::new(Routes::new()).error_page(error_page, ());
/// ```
///
/// So one that reads the body does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// async fn error_page(failure: Failure, req: Req, Form(_): Form<std::collections::HashMap<String, String>>) -> HtmlResult {
///     req.page(html! { <html><head><title>"Error"</title><Corten /></head><body><main>{failure.status().as_str()}</main></body></html> })
/// }
/// let _ = App::new(Routes::new()).error_page(error_page, ());
/// ```
pub trait ErrorPageFn<T, S>: Clone + Send + Sync + 'static {
    #[doc(hidden)]
    fn answer(self, failure: Failure, parts: Parts, state: S) -> impl Future<Output = Response> + Send + 'static;
}

macro_rules! error_page_fn {
    ($($t:ident),*) => {
        #[allow(non_snake_case, unused_mut, unused_variables)]
        impl<F, Fut, S, $($t,)*> ErrorPageFn<($($t,)*), S> for F
        where
            F: FnOnce(Failure, $($t,)*) -> Fut + Clone + Send + Sync + 'static,
            Fut: Future<Output = HtmlResult> + Send + 'static,
            S: Send + Sync + 'static,
            $($t: FromRequestParts<S> + Send + 'static,)*
        {
            fn answer(self, failure: Failure, mut parts: Parts, state: S) -> impl Future<Output = Response> + Send + 'static {
                async move {
                $(
                    let $t = match $t::from_request_parts(&mut parts, &state).await {
                        Ok(value) => value,
                        Err(rejection) => return rejection.into_response(),
                    };
                )*
                match self(failure, $($t,)*).await {
                    Ok(res) => res.0,
                    Err(error) => error.into_response(),
                }
                }
            }
        }
    };
}
error_page_fn!();
error_page_fn!(T1);
error_page_fn!(T1, T2);
error_page_fn!(T1, T2, T3);
error_page_fn!(T1, T2, T3, T4);
error_page_fn!(T1, T2, T3, T4, T5);
error_page_fn!(T1, T2, T3, T4, T5, T6);

pub(crate) fn error_page_fn<H, T, S>(page: H, state: S) -> Box<ErrorPage>
where
    H: ErrorPageFn<T, S>,
    S: Clone + Send + Sync + 'static,
{
    Box::new(move |failure: Failure, parts: Parts| Box::pin(page.clone().answer(failure, parts, state.clone())))
}

/// The app's error page for `failure`, sent with its status.
pub(crate) async fn error_page(shared: &Shared, asked: Asked, failure: Failure) -> Response {
    let status = failure.status();
    let mut res = (shared.error_page)(failure, asked.parts()).await;
    if res.status().is_success() {
        *res.status_mut() = status;
    } else if !res.status().is_redirection() {
        // The error page itself failed (under `corten dev`, a component the app does not list):
        // plain text, so a broken error page cannot fail again.
        tracing::error!(status = res.status().as_u16(), "corten: the error page failed");
        res = (status, "The page could not be shown.").into_response();
    }
    // Search engines skip it, whatever the layout says: the offline page is a 200.
    res.headers_mut().insert(HeaderName::from_static("x-robots-tag"), HeaderValue::from_static("noindex"));
    res
}

/// The request's credentials, `Cookie` and `Authorization`: read once, for the cache rules and for
/// the error page's caller. Values already in the request, shared, not copied.
#[derive(Clone, Default)]
pub(crate) struct Credentials {
    cookie: Option<HeaderValue>,
    authorization: Option<HeaderValue>,
}

impl Credentials {
    pub(crate) fn of(headers: &HeaderMap) -> Self {
        Credentials { cookie: headers.get(header::COOKIE).cloned(), authorization: headers.get(header::AUTHORIZATION).cloned() }
    }
}

/// What the error page's extractors see of a request, kept before the handler took it: what
/// [`Req`] reads and the credentials a caller is found by.
pub(crate) struct Asked {
    req: Req,
    credentials: Credentials,
    /// A `HEAD` request, which leaves a redirect's notice for the `GET`.
    head: bool,
}

impl Asked {
    pub(crate) fn new(req: &Request, shared: Arc<Shared>, credentials: Credentials) -> Self {
        // The error page's own `Req` reads the notice; this one only answers what was asked.
        let head = req.method() == axum::http::Method::HEAD;
        Asked { req: Req::new(req.headers(), req.extensions(), req.uri(), shared, req.method(), false), credentials, head }
    }

    fn path(&self) -> &str {
        self.req.path()
    }

    /// The request again, for the error page's extractors.
    pub(crate) fn parts(self) -> Parts {
        let Asked { req, credentials: Credentials { cookie, authorization }, head } = self;
        let (mut parts, ()) = Request::new(()).into_parts();
        parts.uri = req.uri.clone();
        if head {
            parts.method = axum::http::Method::HEAD;
        }
        let mode = match req.mode {
            Mode::Navigation => Some(HeaderValue::from_static("nav")),
            Mode::Action => Some(HeaderValue::from_static("action")),
            Mode::Document | Mode::Data => None,
        };
        let kept = [
            (MODE_HEADER, mode),
            (header::ACCEPT_LANGUAGE, req.accept_language.clone()),
            (header::COOKIE, cookie),
            (header::AUTHORIZATION, authorization),
        ];
        parts.headers.reserve(kept.iter().filter(|(_, value)| value.is_some()).count());
        for (name, value) in kept {
            if let Some(value) = value {
                parts.headers.insert(name, value);
            }
        }
        if let Some(nonce) = req.nonce.clone() {
            parts.extensions.insert(nonce);
        }
        // An optional caller that cannot be looked up is none there, so the page is drawn signed
        // out rather than not at all.
        req.signal.mark_error_page();
        parts.extensions.insert(req.signal);
        parts
    }
}

/// Everything a request needs from the app: the error page, where scripts live.
pub(crate) struct Shared {
    pub error_page: Box<ErrorPage>,
    pub assets: Option<Arc<Assets>>,
    pub scripts_dir: String,
    /// The import map's JSON: the runtime, the app's typed events and views.
    pub import_map: String,
    /// Extra `<head>` tags from the app's configuration (manifest link, theme color, service
    /// worker), rendered once.
    pub head_extra: String,
    /// Hash of what the page around `<main>` shows that a navigation cannot update: runtime and
    /// asset versions.
    pub key_base: u64,
    /// The PWA's offline page path; Corten answers it when the app has no route for it.
    pub offline: Option<String>,
    /// The app's message catalogs, negotiated per request by [`Req::t`].
    pub locales: Option<&'static [&'static str]>,
    /// [`crate::App::client_zone`]: the runtime keeps the browser's time zone in a cookie.
    pub client_zone: bool,
    /// [`crate::App::zoned_dates`]: dates are written in the visitor's zone while Corten renders.
    #[cfg(feature = "zones")]
    pub zoned_dates: bool,
    /// A redirect's notice may ride in [`PLAIN_NOTICE_COOKIE`]: the app is not HTTPS only
    /// ([`crate::security::Policy::hsts`]).
    pub plain_notice: bool,
    /// Dev mode: error pages show the error.
    pub dev: bool,
    /// The names of the components the app lists (`App::components` and `Area::components`).
    pub components: std::collections::HashSet<&'static str>,
    /// Each area stylesheet (`Area::components`): the prefixes whose pages link it and its `<link>`.
    pub sections: Vec<(Vec<String>, String)>,
    /// The section (an index in `sections`) whose stylesheet has a component's rules.
    pub section_of: std::collections::HashMap<&'static str, usize>,
    /// The proxies whose `X-Forwarded-For` [`crate::ClientIp`] follows ([`crate::security::Policy::proxy`]).
    pub proxies: Vec<crate::client::Network>,
    /// The other sites [`crate::security::Policy::allow_origin`] lets write, for a switch to a
    /// WebSocket, which [`crate::upgrade::Upgrade`] checks as a write.
    pub allowed_origins: Vec<String>,
    /// Each `Api::allow_origin` mount's prefix and origins, which may also switch under it.
    pub api_origins: Vec<(String, Vec<String>)>,
    /// The app's route patterns, its own and its areas', each with whether it has a `GET`
    /// handler ([`Req::serves`]).
    pub pages: Box<[(&'static str, bool)]>,
    /// Paths no page is at: Corten's own and the APIs'.
    pub not_pages: crate::app::NotPages,
    /// The app's work that handlers finish ([`crate::Signal::finish`]).
    pub finishing: Arc<crate::task::Finishing>,
}

impl Shared {
    pub(crate) fn new(error_page: Box<ErrorPage>, assets: Option<Arc<Assets>>, head_extra: String) -> Self {
        let scripts_dir = assets.as_ref().map_or_else(|| "/assets/scripts/".to_string(), |a| a.scripts_dir().to_string());
        let import_map = crate::import_map(
            assets.as_ref().and_then(|a| a.events()).as_deref(),
            assets.as_ref().and_then(|a| a.views()).as_deref(),
        );
        let mut hasher = std::hash::DefaultHasher::new();
        crate::runtime_version().hash(&mut hasher);
        scripts_dir.hash(&mut hasher);
        import_map.hash(&mut hasher);
        head_extra.hash(&mut hasher);
        Shared {
            error_page,
            finishing: Arc::default(),
            assets,
            scripts_dir,
            import_map,
            head_extra,
            key_base: hasher.finish(),
            offline: None,
            locales: None,
            client_zone: false,
            #[cfg(feature = "zones")]
            zoned_dates: false,
            plain_notice: true,
            dev: false,
            components: Default::default(),
            sections: Vec::new(),
            section_of: Default::default(),
            proxies: Vec::new(),
            allowed_origins: Vec::new(),
            api_origins: Vec::new(),
            pages: Box::default(),
            not_pages: Default::default(),
        }
    }
}

/// Used by handlers that run without an [`crate::App`] (unit tests of a bare router).
static DEFAULT: LazyLock<Arc<Shared>> =
    LazyLock::new(|| Arc::new(Shared::new(error_page_fn(default_error_page, ()), None, String::new())));

/// What draws the form's patch in [`Req::invalid_with`]: a patch and nothing else.
pub struct FormPatch<'r>(&'r Req);

impl FormPatch<'_> {
    /// The form, re-rendered with the errors, as the refusal's patch.
    pub fn draw(self, view: impl crate::view::Patch) -> Result<DrawnPatch> {
        let mut body = String::new();
        self.0.render_into(view, &mut body)?;
        Ok(DrawnPatch(body))
    }
}

/// A patch [`FormPatch::draw`] drew, which [`Req::invalid_with`] sends with the refusal's status.
pub struct DrawnPatch(String);

/// The request as a handler answers it: which [`Mode`] the client asked in, and the methods
/// that build each kind of response. Every handler that returns HTML takes one.
pub struct Req {
    pub mode: Mode,
    nonce: Option<NonceSlot>,
    uri: Uri,
    accept_language: Option<HeaderValue>,
    notice: Option<Notice>,
    /// A write from a plain `http://` origin that is not loopback ([`plain_origin`]): its notice
    /// rides in [`PLAIN_NOTICE_COOKIE`].
    plain_write: bool,
    shared: Arc<Shared>,
    signal: crate::Signal,
}

/// What a write said on the page it redirected to ([`Req::redirect_with_notice`]): "Batch 3517
/// deleted." Text, escaped where it renders, for the page's toaster or status line. Rendered in
/// `html!` it counts as drawn: a navigation's page that rendered it does not also hand it to the
/// runtime, and one whose layout (not run for a navigation) holds it does.
#[derive(Clone)]
pub struct Notice {
    text: Box<str>,
    signal: crate::Signal,
    /// Which cookies it was read from, which the answer removes.
    gone: Gone,
}

/// The notice cookies a request carried that its page's answer removes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gone {
    Host,
    Plain,
    Both,
}

impl Notice {
    /// The text, for something other than the page (a log line). Drawn from this, it does not
    /// count as drawn: render the `Notice` itself.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The request's notice: the cookie's, when it was written for this path.
    fn of(headers: &HeaderMap, path: &str, signal: &crate::Signal, plain: bool) -> Option<Notice> {
        let (value, gone) = notice_cookie(headers, plain)?;
        let (mut to, mut text) = (None, None);
        for (key, value) in form_urlencoded::parse(value.as_bytes()) {
            match &*key {
                "p" => to = Some(value),
                "m" => text = Some(value),
                _ => {}
            }
        }
        (*to? == *path_key(path)).then(|| Notice { text: text.unwrap_or_default().into(), signal: signal.clone(), gone })
    }
}

/// The request's notice cookie and the cookies its answer removes, in one pass over the cookies:
/// the `__Host-` one's text first; the plain one when `plain` (the app is not HTTPS only) and
/// the browser does not count the page's origin as secure (no `Sec-Fetch-Site`).
fn notice_cookie(headers: &HeaderMap, plain: bool) -> Option<(&str, Gone)> {
    let (mut host, mut plain_value) = (None, None);
    for pair in headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')) {
        let Some((name, value)) = pair.trim().split_once('=') else { continue };
        if name == NOTICE_COOKIE && host.is_none() {
            host = Some(value);
        } else if plain && name == PLAIN_NOTICE_COOKIE && plain_value.is_none() {
            plain_value = Some(value);
        }
    }
    let plain_value = plain_value.filter(|_| !headers.contains_key("sec-fetch-site"));
    match (host, plain_value) {
        (Some(value), None) => Some((value, Gone::Host)),
        (Some(value), Some(_)) => Some((value, Gone::Both)),
        (None, Some(value)) => Some((value, Gone::Plain)),
        (None, None) => None,
    }
}

/// Whether a write comes from a page on a plain `http://` origin that is not loopback
/// (`localhost`, a name under it, `127.0.0.0/8`, `[::1]`), which a browser does not count as
/// secure, so it refuses a `Secure` cookie from it: no `Sec-Fetch-Site` (which the page reading
/// the notice is held to as well), and an `Origin` of this host, as the same-origin check on a
/// write without that header requires ([`crate::security::Policy::same_origin`]).
pub(crate) fn plain_origin(headers: &HeaderMap) -> bool {
    if headers.contains_key("sec-fetch-site") {
        return false;
    }
    let Some(origin) = headers.get(header::ORIGIN).map(HeaderValue::as_bytes) else { return false };
    let Some(rest) = origin.strip_prefix(b"http://") else { return false };
    if !headers.get(header::HOST).is_some_and(|host| host.as_bytes().eq_ignore_ascii_case(rest)) {
        return false;
    }
    let host = match rest.first() {
        Some(b'[') => return !rest.starts_with(b"[::1]"),
        _ => rest.split(|&b| b == b':').next().unwrap_or_default(),
    };
    let host = host.strip_suffix(b".").unwrap_or(host);
    let loopback = host.eq_ignore_ascii_case(b"localhost")
        || (host.len() > 10 && host[host.len() - 10..].eq_ignore_ascii_case(b".localhost"))
        || std::str::from_utf8(host).ok().and_then(|h| h.parse::<std::net::Ipv4Addr>().ok()).is_some_and(|ip| ip.is_loopback());
    !loopback
}

/// The page a notice is for, as its cookie names it: a hash of the path with its escapes decoded,
/// so the link the app wrote (`/items/a b` as `a%20b`, a [`crate::Local`] with a `{`) and the path
/// the browser asks for (which escapes the `{`) name one page, in 16 characters however long it is.
/// An escaped `/` is kept as written, since it is part of one segment.
pub(crate) fn path_key(path: &str) -> String {
    let (bytes, mut hash, mut i) = (path.as_bytes(), crate::fnv::OFFSET, 0);
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        let byte = match (bytes[i], bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
            // An escaped `/` stays escaped: `/a%2Fb` is one segment, not the page `/a/b`.
            (b'%', Some(hi), Some(lo)) if hi * 16 + lo != u32::from(b'/') => {
                i += 2;
                (hi * 16 + lo) as u8
            }
            (byte, ..) => byte,
        };
        hash = crate::fnv::extend(hash, &[byte]);
        i += 1;
    }
    format!("{hash:016x}")
}

impl Render for Notice {
    fn render(&self, out: &mut String) {
        self.signal.mark_notice_drawn();
        crate::view::escape_into(&self.text, out)
    }
}

impl std::fmt::Debug for Notice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Notice").field(&self.text).finish()
    }
}

impl PartialEq for Notice {
    fn eq(&self, other: &Self) -> bool {
        self.text == other.text
    }
}

impl<St: Send + Sync> FromRequestParts<St> for Req {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &St) -> std::result::Result<Self, Self::Rejection> {
        let shared = parts.extensions.get::<crate::Signal>().and_then(crate::Signal::shared).unwrap_or(&DEFAULT).clone();
        // A HEAD request (a link checker, a preview) gets no body, so it leaves the notice for the
        // page's GET.
        Ok(Req::new(
            &parts.headers,
            &parts.extensions,
            &parts.uri,
            shared,
            &parts.method,
            parts.method != axum::http::Method::HEAD,
        ))
    }
}

impl Req {
    /// `notice`: read the request's notice (its answer then removes it). Only a write's `Origin`
    /// is looked at, so a page's request pays nothing for it.
    pub(crate) fn new(
        headers: &HeaderMap,
        extensions: &Extensions,
        uri: &Uri,
        shared: Arc<Shared>,
        method: &axum::http::Method,
        notice: bool,
    ) -> Self {
        let signal: crate::Signal = extensions.get::<crate::Signal>().cloned().unwrap_or_default();
        Req {
            mode: mode_of(headers),
            nonce: extensions.get::<NonceSlot>().cloned(),
            uri: uri.clone(),
            accept_language: headers.get(header::ACCEPT_LANGUAGE).cloned(),
            notice: if notice { Notice::of(headers, uri.path(), &signal, shared.plain_notice) } else { None },
            plain_write: shared.plain_notice
                && !matches!(*method, axum::http::Method::GET | axum::http::Method::HEAD)
                && plain_origin(headers),
            shared,
            signal,
        }
    }

    /// The key of the page around `<main>`: the app's versions, who it is for (the callers the
    /// request read, read when the page renders, after the handler's extractors ran) and which
    /// layout rendered it (`shell`, from the document's `html!`).
    fn key(&self, shell: u64) -> u64 {
        // What the page's area policy adds (`Area::policy`): a document keeps the CSP it loaded
        // with, so a navigation to a page with another loads it in full.
        let area = self.nonce.as_ref().map_or(0, |slot| slot.1);
        self.shared.key_base ^ self.signal.callers().rotate_left(1) ^ shell ^ area
    }

    /// When the work for this request should stop, and why: the server's `request.signal`. See
    /// [`crate::Signal`]; work the handler awaits stops without it.
    pub fn signal(&self) -> &crate::Signal {
        &self.signal
    }

    /// The request path.
    pub fn path(&self) -> &str {
        self.uri.path()
    }

    /// The app's messages (from [`crate::i18n::messages!`]) in the best locale for this request's
    /// `Accept-Language`: `let t: Msg = req.t();`.
    pub fn t<M: crate::i18n::Messages>(&self) -> M {
        M::negotiate(self.accept_language())
    }

    /// The visitor's time zone (`zones` feature, [`crate::App::client_zone`]), with its rules: a
    /// date the user picked (`<input type="date">`) becomes the instant it means where they are.
    /// UTC until the browser has reported it (its first page) and for a name the tz database
    /// lacks. Looked up in the tz database at each call, so a handler keeps it. With
    /// [`crate::App::zoned_dates`] a page needs it for nothing else: while Corten renders the
    /// answer, `t.date`, `t.time` and `t.date_time` write in it unless `.zone(..)` chooses
    /// another. A known zone makes the answer private. For a [`crate::Public`] answer, which is the same for everyone,
    /// it is UTC and dates are written in UTC (corten-ui's `LocalTime` writes one again in the
    /// browser's zone).
    #[cfg(feature = "zones")]
    pub fn zone(&self) -> crate::i18n::Visitor {
        // The request's signal holds the zone the browser reported, read once for the request.
        let zone = self.signal.zone();
        let Some(name) = zone.as_ref().map(crate::client::ZoneName::as_str) else {
            #[cfg(debug_assertions)]
            if !self.shared.client_zone {
                tracing::warn!(
                    "corten: req.zone() is UTC: the app does not learn its visitors' time zones; add App::client_zone()"
                );
            } else if self.signal.is_for_everyone() && !self.signal.is_error_page() {
                tracing::warn!("corten: req.zone() is UTC in a Public answer, which is the same for everyone");
            }
            return crate::i18n::Visitor::default();
        };
        let zone = crate::i18n::Visitor::named(name);
        if zone.is_known() {
            self.signal.mark_personal();
        }
        zone
    }

    /// While Corten renders this request's answer: dates in the visitor's zone.
    #[cfg(feature = "zones")]
    fn zone_scope(&self) -> Option<crate::i18n::ZoneScope> {
        self.shared.zoned_dates.then(|| crate::i18n::ZoneScope::enter(crate::i18n::Scope::of(self.signal.zone(), &self.signal)))
    }

    fn accept_language(&self) -> Option<&str> {
        self.accept_language.as_ref().and_then(|v| v.to_str().ok())
    }

    /// The negotiated locale (`"en"` without [`crate::App::messages`]), for `<html lang>`.
    pub fn lang(&self) -> &'static str {
        match self.shared.locales {
            Some(locales) => locales[crate::i18n::negotiate(locales, self.accept_language())],
            None => "en",
        }
    }

    /// A page: `doc` is the whole document, the app's layout with the page in it
    /// (`html! { <Layout req={req} title="About">..</Layout> }`). A browser gets all of it; a
    /// client navigation gets the title, the page's head tags and `<main>`'s children, and the rest
    /// of the layout does not run.
    pub fn page<For>(&self, doc: impl Document) -> crate::Result<HtmlResponse<For>> {
        self.page_with_status(StatusCode::OK, doc)
    }

    /// [`Req::page`] with another status.
    pub fn page_with_status<For>(&self, status: StatusCode, doc: impl Document) -> crate::Result<HtmlResponse<For>> {
        // The last size of each part, so the buffer is allocated once.
        static FULL: AtomicUsize = AtomicUsize::new(16 * 1024);
        static SWAP: AtomicUsize = AtomicUsize::new(16 * 1024);
        let swap = matches!(self.mode, Mode::Navigation | Mode::Action);
        let (part, size) = if swap { (Part::Swap, &SWAP) } else { (Part::Full, &FULL) };
        let mut out = String::with_capacity(size.load(Ordering::Relaxed));
        HEAD_AT.set(None);
        crate::view::SHELL.set(0);
        // A full page's custom properties (`style={..}`) go in one style block its CSP allows by
        // hash; a swap's are set by the runtime, under the CSP the document loaded with.
        let (modules, vars) = if swap {
            (self.render_into(Parted(doc, part), &mut out)?, String::new())
        } else {
            let (modules, vars) = crate::style::collect_vars(|| self.render_into(Parted(doc, part), &mut out));
            (modules?, vars)
        };
        let Some(at) = HEAD_AT.take() else {
            return Err(Error::new("corten: the document's <head> has no <Corten />"));
        };
        let key = self.key(crate::view::SHELL.get());
        let mut tags = String::new();
        if swap {
            self.section_links(&mut tags);
        } else {
            tags.reserve(1024 + vars.len());
            self.head_tags(&modules, key, &mut tags);
            // After every stylesheet the page links, so its values win a tie as an inline style would.
            if !vars.is_empty() {
                tags.push_str("<style>");
                tags.push_str(&vars);
                tags.push_str("</style>");
            }
        }
        if !tags.is_empty() {
            out.insert_str(at, &tags);
        }
        size.store(out.len() + out.len() / 8, Ordering::Relaxed);
        // Only a swap says it is one: a document a cache gave to a navigation fetch by mistake
        // makes the runtime load the URL in full.
        let mut res = self.finish(html_response(status, out, swap.then_some(PAGE_HEADER)), key);
        if !vars.is_empty() {
            res.extensions_mut().insert(crate::security::StyleBlock(crate::security::hash_source(&vars)));
        }
        Ok(HtmlResponse::new(res))
    }

    /// The versioned URL of a static file, named by the constant [`crate::assets::embed!`] made
    /// for it: `req.asset(Static::STYLE_CSS)`. It is written where it is used, with nothing
    /// allocated. A file of a directory the app was not given (`App::assets`) panics in debug
    /// builds; in release the page still renders, with a link that 404s.
    pub fn asset(&self, file: crate::assets::Asset) -> crate::assets::AssetUrl<'_> {
        match &self.shared.assets {
            Some(assets) => assets.asset(file),
            None => no_assets(file.path()),
        }
    }

    /// The versioned URL of the static file at `path`, a name that comes from data (an item's
    /// picture, `cards/item_1.svg`), or `None` when the directory has no such file.
    pub fn find_asset<'a>(&'a self, path: &'a str) -> Option<crate::assets::AssetUrl<'a>> {
        self.shared.assets.as_ref()?.find(path)
    }

    /// True when `href` (a typed route) is the current page, or with `section` any page under
    /// it: the rule the runtime applies to `<a data-nav>` (`data-nav="section"`) after each
    /// navigation.
    pub fn is_current(&self, href: &(impl crate::route::Href + ?Sized), section: bool) -> bool {
        match href.as_text() {
            Some(text) => is_current(self.uri.path(), text, section),
            None => is_current(self.uri.path(), &crate::route::link(href), section),
        }
    }

    /// A nav link to `to`, from one mention of the route: `<a {..req.nav(&url::About)}>"About"</a>`
    /// writes `href`, `data-nav` (so the runtime keeps `aria-current` right after a client
    /// navigation) and `aria-current="page"` when `to` is this page.
    ///
    /// ```
    /// use corten::prelude::*;
    ///
    /// #[derive(Route)]
    /// #[route("/news")]
    /// struct News;
    /// #[derive(Route)]
    /// #[route("/about")]
    /// struct About;
    ///
    /// // On /news: <a href="/news" data-nav aria-current="page">News</a><a href="/about" data-nav>About</a>
    /// fn nav(req: &Req) -> String {
    ///     html! { <nav><a {..req.nav(&News)}>"News"</a><a {..req.nav(&About)}>"About"</a></nav> }.to_html()
    /// }
    /// let _: fn(&Req) -> String = nav;
    /// ```
    ///
    /// The tag cannot also write one of those attributes, which the browser would ignore:
    ///
    /// ```compile_fail,E0080
    /// use corten::prelude::*;
    ///
    /// #[derive(Route)]
    /// #[route("/news")]
    /// struct News;
    ///
    /// fn nav(req: &Req) -> String {
    ///     html! { <a href={News} {..req.nav(&News)}>"News"</a> }.to_html()
    /// }
    /// let _: fn(&Req) -> String = nav;
    /// ```
    ///
    /// Nor can a link be marked by hand, which would name its route twice:
    ///
    /// ```compile_fail
    /// use corten::prelude::*;
    ///
    /// #[derive(Route)]
    /// #[route("/news")]
    /// struct News;
    ///
    /// let link = html! { <a href={News} data-nav>"News"</a> };
    /// ```
    pub fn nav<'a, R: crate::Route>(&'a self, to: &'a R) -> Nav<'a, R> {
        Nav { path: self.uri.path(), to, section: false }
    }

    /// [`Req::nav`] for a link to a section: current on any page under `to` too
    /// (`data-nav="section"`), as a nav's "News" stays current on a story's page.
    pub fn nav_section<'a, R: crate::Route>(&'a self, to: &'a R) -> Nav<'a, R> {
        Nav { path: self.uri.path(), to, section: true }
    }

    /// A whole nav's links, for a nav whose entries nest (a section and a page under it) or come
    /// from data: of the entries that hold this page, only the deepest is current. Seller Desk
    /// (`/account/desk`, a section) is current on `/account/desk/repricing`, and only Money
    /// (`/account/desk/money`) on its own page. An entry is a typed route or a [`crate::Local`]
    /// read from data, each with its item (a label, or the row it came from), which
    /// [`NavLinks::links`] hands back with its link. The links name their group (`data-nav-group`),
    /// so the runtime ranks the same links after a client navigation wherever the markup puts
    /// them; a link of [`Req::nav`] or [`Req::nav_section`] is still judged alone. Leave out an
    /// entry the user may not see before the call, not in the markup: the runtime ranks only the
    /// links in the page.
    ///
    /// ```
    /// use corten::prelude::*;
    /// use corten::{NavTo, Local};
    ///
    /// #[derive(Route)]
    /// #[route("/account/desk")]
    /// struct Desk;
    /// #[derive(Route)]
    /// #[route("/account/desk/money")]
    /// struct Money;
    ///
    /// fn nav(req: &Req, help: &Local) -> String {
    ///     let nav = req.nav_of([
    ///         (NavTo::section(&Desk), "Seller Desk"),
    ///         (NavTo::page(&Money), "Money"),
    ///         (NavTo::page(help), "Help"),
    ///     ]);
    ///     html! { <nav>for (link, label) in nav.links() { <a {..link}>{*label}</a> }</nav> }.to_html()
    /// }
    /// let _: fn(&Req, &Local) -> String = nav;
    /// ```
    ///
    /// A URL written as text is not an entry:
    ///
    /// ```compile_fail,E0277
    /// let _ = corten::NavTo::page(&"/account/desk");
    /// ```
    ///
    /// Nor can a link join a group by hand, which the server did not rank:
    ///
    /// ```compile_fail
    /// use corten::prelude::*;
    ///
    /// let link = html! { <a data-nav-group="a">"News"</a> };
    /// ```
    pub fn nav_of<'a, T>(&self, entries: impl IntoIterator<Item = (NavTo<'a>, T)>) -> NavLinks<T> {
        nav_links(self.uri.path(), entries)
    }

    /// This request's CSP nonce, for the layout's own inline scripts. Empty without a policy.
    ///
    /// Asking for it makes one and puts it in the CSP, so the page is different bytes on every
    /// request and is never cached for other users (see [`crate::App`], caching). Prefer a
    /// script file from the static directory.
    pub fn nonce(&self) -> &str {
        self.nonce.as_ref().map_or("", |slot| &slot.get().0)
    }

    /// Renders `view` and returns the script modules its handlers use, for the page to load.
    /// Under `corten dev`, a component with a script or stylesheet that the app does not list is
    /// an error, since its script or stylesheet would not be served (debug builds of the app
    /// record what renders; release builds do not).
    fn render_into(&self, view: impl Render, out: &mut String) -> Result<Vec<&'static str>> {
        #[cfg(feature = "zones")]
        let _zone = self.zone_scope();
        if !self.shared.dev {
            let ((), modules) = crate::script::collect(|| {
                // An empty buffer: to_html sizes it from earlier renders of the same view.
                if out.capacity() == 0 {
                    *out = view.to_html();
                } else {
                    view.render(out);
                }
            });
            #[cfg(debug_assertions)]
            self.check_modules(&modules)?;
            return Ok(modules);
        }
        let (((), used), modules) = crate::script::collect(|| crate::style::collect(|| view.render(out)));
        self.check_rendered(&used)?;
        #[cfg(debug_assertions)]
        self.check_modules(&modules)?;
        Ok(modules)
    }

    /// In debug builds (a router test, `cargo run`): an error when the page's handlers call a
    /// script module the app does not serve, which a browser would only find as a 404 and
    /// handlers that do nothing. Release builds do not check a swap; a full page logs it.
    #[cfg(debug_assertions)]
    #[cold]
    #[inline(never)]
    fn check_modules(&self, modules: &[&'static str]) -> Result<()> {
        match modules.iter().find(|name| !self.shared.assets.as_ref().is_some_and(|a| a.module_tag(name).is_some())) {
            Some(name) => Err(Error::new(unserved_module(name))),
            None => Ok(()),
        }
    }

    /// Under `corten dev`: an error when the page rendered a component whose script or stylesheet
    /// it does not get, because the app does not list it or lists it in a section the page is
    /// not under. Kept out of [`Req::render_into`], which is compiled once per view.
    #[cold]
    #[inline(never)]
    fn check_rendered(&self, used: &[&'static crate::Component]) -> Result<()> {
        if let Some(missing) = used.iter().find(|c| !self.shared.components.contains(c.name)) {
            let what = match (missing.script.is_some(), missing.style.is_some()) {
                (true, true) => "script and stylesheet are",
                (true, false) => "script is",
                _ => "stylesheet is",
            };
            return Err(Error::new(format!(
                "the page renders {name}, which the app does not list, so its {what} not served: add {name} to App::components",
                name = missing.name
            )));
        }
        for component in used {
            let Some(&index) = self.shared.section_of.get(component.name) else { continue };
            let prefixes = &self.shared.sections[index].0;
            if !prefixes.iter().any(|prefix| crate::api::under(prefix, self.path())) {
                return Err(Error::new(format!(
                    "{path} renders {name}, whose stylesheet is linked from the area {areas} only: list {name} in this page's area too, or route the page in that area",
                    path = self.path(),
                    name = component.name,
                    areas = prefixes.join(" and the area ")
                )));
            }
        }
        Ok(())
    }

    /// The `<link>` tags of the section stylesheets for this request's path.
    #[inline]
    fn section_links(&self, out: &mut String) {
        let path = self.uri.path();
        for (prefixes, link) in &self.shared.sections {
            if prefixes.iter().any(|prefix| crate::api::under(prefix, path)) {
                out.push_str(link);
            }
        }
    }

    /// Adds the shell key, which tells the runtime whether the page around `<main>` is still current.
    fn finish(&self, mut res: Response, key: u64) -> Response {
        res.headers_mut().insert(SHELL_HEADER, key_header(key));
        res.headers_mut().insert(header::VARY, self.vary());
        if let Some(notice) = &self.notice {
            // Shown once: this page is the one it was for, drawn here or by the runtime.
            if notice.gone != Gone::Plain {
                res.headers_mut().append(header::SET_COOKIE, crate::security::cookie::SetCookie::remove(NOTICE_COOKIE).header());
            }
            if notice.gone != Gone::Host {
                let plain = crate::security::cookie::SetCookie::remove(PLAIN_NOTICE_COOKIE).insecure();
                res.headers_mut().append(header::SET_COOKIE, plain.header());
            }
            self.signal.mark_personal();
            let swap = matches!(self.mode, Mode::Navigation | Mode::Action);
            #[cfg(debug_assertions)]
            if matches!(self.mode, Mode::Document) && !self.signal.notice_drawn() {
                tracing::warn!(
                    "corten: {} was sent a notice (req.redirect_with_notice) it did not draw (req.notice()), so a full load drops it",
                    self.uri.path()
                );
            }
            if swap {
                // Drawn or not (`&d`): the runtime keeps it across a reload in the page's place,
                // and announces only one the page did not draw.
                let mut text = form_urlencoded::Serializer::new(String::new()).append_pair("m", notice.as_str()).finish();
                if self.signal.notice_drawn() {
                    text.push_str("&d");
                }
                if let Ok(value) = HeaderValue::from_str(&text) {
                    res.headers_mut().insert(NOTICE_HEADER, value);
                }
            }
        }
        res
    }

    fn vary(&self) -> HeaderValue {
        HeaderValue::from_static(if self.shared.locales.is_some() { VARY_LANGUAGE } else { VARY })
    }

    /// The tags Corten needs in `<head>` after the page's own: script settings, the shell key,
    /// the app's configured tags, section stylesheets, the import map, the runtime and modules.
    fn head_tags(&self, modules: &[&'static str], key: u64, out: &mut String) {
        use std::fmt::Write;
        let dir = &self.shared.scripts_dir;
        let _ = write!(out, r#"<meta name="corten-scripts" content="{dir}"><meta name="corten-shell" content="{key:016x}">"#);
        out.push_str(&self.shared.head_extra);
        self.section_links(out);
        out.push_str(r#"<script type="importmap">"#);
        out.push_str(&self.shared.import_map);
        out.push_str(r#"</script><script type="module" src=""#);
        out.push_str(crate::runtime_path());
        out.push_str(r#""></script>"#);
        // The modules the page's handlers call, fetched with the runtime rather than on first use.
        // A component's module hands its exports to the runtime as it runs, so it is a module script
        // of the page: its handlers are in before DOMContentLoaded, with no import() to wait on.
        // Each module's tag was made when the assets loaded.
        if let Some(assets) = &self.shared.assets {
            let mut preloaded = false;
            let mut events = false;
            for name in modules {
                let Some((tag, imports_events)) = assets.module(name) else {
                    tracing::error!("corten: {}", unserved_module(name));
                    continue;
                };
                out.push_str(tag);
                preloaded = true;
                events |= imports_events;
            }
            // A handler module may import the views (corten/views): fetched with it, not after it.
            if let Some(views) = assets.views_tag().filter(|_| preloaded) {
                out.push_str(views);
            }
            // And one that imports the typed events (corten/events) has them fetched with it.
            if let Some(tag) = assets.events_tag().filter(|_| events) {
                out.push_str(tag);
            }
        }
    }

    /// [`Req::page`] for a page other clients read too: a browser gets HTML as usual, and a
    /// [`Mode::Data`] request gets `{"title": ..., "data": data}`, the title being the text of the
    /// document's `<title>`. `data` is the screen's view model, so a native screen or an agent sees
    /// exactly what the view shows; for it, only the document's `<title>` renders.
    #[cfg(feature = "json")]
    pub fn screen<For, T: crate::json::ToJson + ?Sized>(&self, data: &T, doc: impl Document) -> crate::Result<HtmlResponse<For>> {
        self.screen_with_status(StatusCode::OK, data, doc)
    }

    #[cfg(feature = "json")]
    pub fn screen_with_status<For, T: crate::json::ToJson + ?Sized>(
        &self,
        status: StatusCode,
        data: &T,
        doc: impl Document,
    ) -> crate::Result<HtmlResponse<For>> {
        if self.mode != Mode::Data {
            return self.page_with_status(status, doc);
        }
        use crate::json::ToJson;
        let mut title = String::new();
        doc.render_part(&mut title, Part::Title);
        // `{"title":...,"data":...}`, as serde_json writes the pair.
        let mut body = Vec::with_capacity(T::__json_size().map_or(128, |s| s.load(std::sync::atomic::Ordering::Relaxed) + 64));
        body.extend_from_slice(br#"{"title":"#);
        crate::html::unescape(&title).as_ref().write_json(&mut body);
        body.extend_from_slice(br#","data":"#);
        data.write_json(&mut body);
        body.push(b'}');
        let headers = [(header::CONTENT_TYPE, HeaderValue::from_static("application/json")), (header::VARY, self.vary())];
        Ok(HtmlResponse::new((status, headers, body).into_response()))
    }

    /// Answers a `<form data-patch>` submit with fragments: the runtime replaces each element on
    /// the page that has the id of one of `view`'s top-level elements. One without an id would
    /// be dropped in the browser, so `view` is a [`Patch`](crate::view::Patch), which the compiler
    /// checks: usually a component declared `-> impl Patch` whose root has a `#[derive(Id)]` id.
    pub fn patch<For>(&self, view: impl crate::view::Patch) -> crate::Result<HtmlResponse<For>> {
        self.patch_with_status(StatusCode::OK, view)
    }

    /// Same as [`Req::patch`] with another status.
    pub fn patch_with_status<For>(&self, status: StatusCode, view: impl crate::view::Patch) -> crate::Result<HtmlResponse<For>> {
        let mut body = String::new();
        self.render_into(view, &mut body)?;
        Ok(HtmlResponse::new(html_response(status, body, Some(PATCH_HEADER))))
    }

    /// Answers a `data-patch` form (a write, Load more) both ways: with JavaScript
    /// ([`Mode::Action`]) `view` as a patch; otherwise `page()`, the whole page or a redirect, so
    /// the form works without JavaScript too. `page` only runs then, so an action never loads
    /// data for a page it does not send. A data client ([`Mode::Data`]) gets `page()` as well. A
    /// submit refused for its input is [`Req::invalid`] (or [`Req::invalid_with`] when the form needs
    /// a read); a patch that needs data, a status or headers of its own checks
    /// `req.mode == Mode::Action` itself.
    ///
    /// ```
    /// use corten::prelude::*;
    ///
    /// #[corten::routes]
    /// mod url {
    ///     #[route("/", get = home, post = greet)]
    ///     pub struct Home;
    /// }
    ///
    /// #[derive(Id)]
    /// struct GreetingId;
    ///
    /// #[component]
    /// fn Greeting<'a>(name: &'a str) -> impl Patch {
    ///     html! { <p id={GreetingId}>"Hello, "{name}</p> }
    /// }
    ///
    /// # async fn home(req: Req) -> HtmlResult { req.redirect(&url::Home) }
    /// async fn greet(req: Req) -> HtmlResult {
    ///     req.patch_or(Greeting { name: "Ada" }, || async { req.redirect(&url::Home) }).await
    /// }
    /// # let _ = App::new(url::router()).router();
    /// ```
    pub fn patch_or<For, F, Fut>(
        &self,
        view: impl crate::view::Patch,
        page: F,
    ) -> impl Future<Output = crate::Result<HtmlResponse<For>>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = crate::Result<HtmlResponse<For>>>,
    {
        // The patch is rendered before the future is made, so the future holds the answer and not
        // `view`: a view of non-`Send` references would make the handler's future not `Send`.
        let patch = (self.mode == Mode::Action).then(|| self.patch(view));
        async move {
            match patch {
                Some(patch) => patch,
                None => page().await,
            }
        }
    }

    /// Answers a refused submit: its input failed validation (422 Unprocessable Content), it was
    /// made against a state that has moved on ([`Errors::refuse`](crate::Errors::refuse), 409 for
    /// a stale revision, 410 for a deleted record), or a service the handler calls failed (502),
    /// with the status of [`Errors::refusal`](crate::Errors::refusal):
    ///
    /// - [`Mode::Action`]: `form`, the form re-rendered with the errors, as a patch. The runtime
    ///   then moves focus to the first field marked `aria-invalid="true"`.
    /// - [`Mode::Data`] (feature `json`): the Problem Details document with the fields,
    ///   `{"title": "Unprocessable Entity", "status": 422, "detail": "email: ...", "fields": {"email": "..."}}`.
    /// - Otherwise (no JavaScript): `page()`, usually `req.page(..)` for the whole page with the
    ///   form showing the errors; a success status becomes the refusal's. It is only called here,
    ///   so an action never loads data for a page it does not send. Draw the page, never redirect:
    ///   a redirect drops the message and what the user typed.
    ///
    /// The runtime moves focus to the first invalid field only for a 422; a refusal's message is a
    /// `role="alert"` that is announced where it lands.
    pub async fn invalid<For, F, Fut>(
        &self,
        errors: &crate::Errors,
        form: impl crate::view::Patch,
        page: F,
    ) -> crate::Result<HtmlResponse<For>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = crate::Result<HtmlResponse<For>>>,
    {
        let status = errors.refusal().status();
        match self.mode {
            Mode::Action => self.patch_with_status(status, form),
            #[cfg(feature = "json")]
            Mode::Data => Ok(HtmlResponse::new(errors.response())),
            _ => {
                let mut res = page().await?;
                if res.status().is_success() {
                    *res.status_mut() = status;
                }
                Ok(res)
            }
        }
    }

    /// [`Req::invalid`] for a refusal whose form needs a read of its own (the record as it is
    /// now): both sides are drawn only for the mode that sends them, so a browser without
    /// JavaScript reads nothing for a patch it never gets, and the patch reads nothing for the
    /// page. `form` gets a [`FormPatch`], which draws the patch and nothing else, so a page or a
    /// redirect on that side does not compile; a data client gets the Problem Details document,
    /// as from `invalid` (feature `json`; without it, the page), and neither side runs. A form drawn from what the handler already has is
    /// `invalid`.
    ///
    /// ```
    /// use corten::{prelude::*, Refusal};
    ///
    /// #[corten::routes]
    /// mod url {
    ///     #[route("/batch", get = show, post = price)]
    ///     pub struct Batch;
    /// }
    ///
    /// #[derive(Id)]
    /// struct PanelId;
    ///
    /// #[component]
    /// fn Panel<'a>(status: &'a str) -> impl Patch {
    ///     html! { <p id={PanelId}>{status}</p> }
    /// }
    ///
    /// async fn status_now() -> String {
    ///     "Publishing".to_string()
    /// }
    ///
    /// # async fn show(req: Req) -> HtmlResult { req.redirect(&url::Batch) }
    /// async fn price(req: Req) -> HtmlResult {
    ///     let mut errors = Errors::new();
    ///     errors.refuse(Refusal::Conflict, "The batch moved on; here it is now.");
    ///     req.invalid_with(
    ///         &errors,
    ///         |form| async move {
    ///             let status = status_now().await;
    ///             form.draw(Panel { status: &status })
    ///         },
    ///         || async { req.page(html! { <html><head><title>"Batch"</title><Corten /></head><body><main>"Batch"</main></body></html> }) },
    ///     )
    ///     .await
    /// }
    /// # let _ = App::new(url::router()).router();
    /// ```
    ///
    /// The form side draws a patch, so it cannot answer a page:
    ///
    /// ```compile_fail
    /// use corten::prelude::*;
    ///
    /// async fn price(req: Req, errors: Errors) -> HtmlResult {
    ///     req.invalid_with(&errors, |_form| async { req.redirect(&corten::Local::parse("/").unwrap()) }, || async { req.redirect(&corten::Local::parse("/").unwrap()) }).await
    /// }
    /// ```
    pub async fn invalid_with<'r, For, F, FormFut, P, PageFut>(
        &'r self,
        errors: &crate::Errors,
        form: F,
        page: P,
    ) -> crate::Result<HtmlResponse<For>>
    where
        F: FnOnce(FormPatch<'r>) -> FormFut,
        FormFut: Future<Output = crate::Result<DrawnPatch>>,
        P: FnOnce() -> PageFut,
        PageFut: Future<Output = crate::Result<HtmlResponse<For>>>,
    {
        let status = errors.refusal().status();
        match self.mode {
            Mode::Action => {
                let DrawnPatch(body) = form(FormPatch(self)).await?;
                Ok(HtmlResponse::new(html_response(status, body, Some(PATCH_HEADER))))
            }
            #[cfg(feature = "json")]
            Mode::Data => Ok(HtmlResponse::new(errors.response())),
            _ => {
                let mut res = page().await?;
                if res.status().is_success() {
                    *res.status_mut() = status;
                }
                Ok(res)
            }
        }
    }

    /// After a successful write, send the browser to `to` (post/redirect/get): a typed route, a
    /// place on its page (`route.at(RowId(&id))`, so the browser comes back to the element the
    /// user acted on), a return address checked to be on this site ([`crate::Local`]), or an
    /// `axum::http::Uri` to leave the app. Works with and without
    /// JavaScript.
    pub fn redirect<For>(&self, to: &(impl crate::route::Href + ?Sized)) -> crate::Result<HtmlResponse<For>> {
        let value = crate::route::header_value(to).map_err(Error::new)?;
        Ok(HtmlResponse::new(redirect(self.mode, value)))
    }

    /// [`Req::redirect`] with what the write did, said once on the page it lands on ("Batch 3517
    /// deleted."), where [`Req::notice`] reads it: no field on the route carries it, so a reload,
    /// a copied link or Back to a page the browser asks for again does not say it again. It rides in a short-lived cookie for that
    /// page's path (`__Host-corten-notice`, a minute; `corten-notice` for a write from a plain
    /// `http://` origin that is not loopback, which refuses a `Secure` cookie), which the page's
    /// answer removes. A
    /// navigation's page that did not draw it hands it to the runtime, which dispatches
    /// `corten:notice` on the document for the app's toaster, since the layout around `<main>`
    /// does not run there. Text past 500 bytes is cut at a character's end.
    ///
    /// Only for a page of the app ([`crate::OnSite`]: a route, a place on one, a checked
    /// [`crate::Local`], a `#[derive(Href)]` enum of those): a URL of another site does not compile.
    ///
    /// ```compile_fail,E0277
    /// use corten::prelude::*;
    ///
    /// async fn pay(req: Req) -> HtmlResult {
    ///     let to: corten::axum::http::Uri = "https://pay.example/checkout".parse().unwrap();
    ///     req.redirect_with_notice(&to, "Sent to payment.")
    /// }
    /// ```
    ///
    /// A `#[derive(Href)]` enum is a page of the app when each of its variants is:
    ///
    /// ```
    /// use corten::{prelude::*, Href};
    ///
    /// #[derive(Route)] #[route("/inventory")] struct Inventory;
    /// #[derive(Route)] #[route("/pull-sheet/{id}")] struct PullSheet<'a> { id: &'a str }
    ///
    /// #[derive(Href)]
    /// enum Back<'a> { Inventory(Inventory), PullSheet(PullSheet<'a>) }
    ///
    /// async fn delete(req: Req) -> HtmlResult {
    ///     req.redirect_with_notice(&Back::PullSheet(PullSheet { id: "7" }), "Batch 3517 deleted.")
    /// }
    /// ```
    ///
    /// and not when one of them may go to another site:
    ///
    /// ```compile_fail,E0277
    /// use corten::{prelude::*, Elsewhere, Href};
    ///
    /// #[derive(Route)] #[route("/inventory")] struct Inventory;
    ///
    /// #[derive(Href)]
    /// enum Back<'a> { Inventory(Inventory), Payment(Elsewhere<'a>) }
    ///
    /// async fn delete(req: Req) -> HtmlResult {
    ///     req.redirect_with_notice(&Back::Inventory(Inventory), "Batch 3517 deleted.")
    /// }
    /// ```
    pub fn redirect_with_notice(&self, to: &(impl crate::route::OnSite + ?Sized), notice: impl std::fmt::Display) -> HtmlResult {
        let value = crate::route::header_value(to).map_err(Error::new)?;
        let link = value.to_str().unwrap_or_default();
        let path = link.split(['?', '#']).next().unwrap_or_default();
        let mut text = notice.to_string();
        if text.len() > NOTICE_MAX {
            let mut end = NOTICE_MAX;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
        }
        let cookie =
            form_urlencoded::Serializer::new(String::new()).append_pair("p", &path_key(path)).append_pair("m", &text).finish();
        // Form encoding writes only letters, digits, `*-._+%&=`: all a cookie can carry.
        let cookie = if self.plain_write {
            crate::security::cookie::SetCookie::new(PLAIN_NOTICE_COOKIE, &cookie).insecure()
        } else {
            crate::security::cookie::SetCookie::new(NOTICE_COOKIE, &cookie)
        };
        let cookie = cookie.max_age(60);
        Ok(HtmlResponse::new(redirect(self.mode, value)).with_cookie(cookie))
    }

    /// The notice the write that redirected here left for this page ([`Req::redirect_with_notice`]),
    /// once: the answer removes it. Draw it where the app says what an action did, the layout's
    /// toaster (`Toast { message: req.notice() }`) or a status line. A page that has one is
    /// private, as one for a signed-in caller is.
    pub fn notice(&self) -> Option<&Notice> {
        let notice = self.notice.as_ref()?;
        self.signal.mark_personal();
        Some(notice)
    }

    /// Whether a redirect to `to` lands on a route of the app's: the route given to
    /// [`crate::App::new`] or to an area that the router picks for its path (the most specific,
    /// as axum picks it) has a `GET` handler. A return address the app does not serve (a page of
    /// another app of the product, or none) goes elsewhere; one it serves is `req.redirect(&to)`.
    /// A route that only takes writes, a path under an API and Corten's own paths are not served.
    /// It does not look at who is asking: a page an area's caller check refuses still counts, and
    /// a record that is not there is still a 404.
    ///
    /// ```
    /// use corten::{prelude::*, Local};
    ///
    /// #[corten::routes]
    /// mod url {
    ///     #[route("/", get = home)]
    ///     pub struct Home;
    ///
    ///     #[route("/sign-in", post = sign_in)]
    ///     pub struct SignIn;
    /// }
    ///
    /// # async fn home(req: Req) -> HtmlResult { req.page(html! { <html><head><title>"Home"</title><Corten /></head><body><main>"Home"</main></body></html> }) }
    /// #[derive(serde::Deserialize)]
    /// struct Credentials { next: Option<String> }
    ///
    /// async fn sign_in(req: Req, Form(form): Form<Credentials>) -> HtmlResult {
    ///     // .. start the session ..
    ///     match form.next.as_deref().and_then(Local::parse) {
    ///         Some(next) if req.serves(&next) => req.redirect(&next),
    ///         _ => req.redirect(&url::Home),
    ///     }
    /// }
    /// # let _ = App::new(url::router()).router();
    /// ```
    pub fn serves(&self, to: &crate::Local) -> bool {
        let path = to.path();
        !self.shared.not_pages.contains(path) && crate::route::chosen(&self.shared.pages, path).is_some_and(|(_, get)| get)
    }

    /// The page is at `to` for good (an item asked for by its id or an old slug, a renamed
    /// category): `301 Moved Permanently`, so search engines move the old URL's place to `to` and
    /// the browser keeps the redirect for a day (privately; a shared cache does not), skipping the
    /// round trip next time. A client navigation follows it and puts `to` in the address; a
    /// `data-patch` form is sent there as by [`Req::redirect`]. For a `GET`: after a write,
    /// [`Req::redirect`]. Parameters the old URL had are the route's to carry
    /// (`#[route(prefix = ..)]` for ones named by data), so only the ones it knows move with it.
    pub fn moved<For>(&self, to: &(impl crate::route::Href + ?Sized)) -> crate::Result<HtmlResponse<For>> {
        let value = crate::route::header_value(to).map_err(Error::new)?;
        if self.mode == Mode::Action || (self.mode == Mode::Navigation && (has_fragment(&value) || elsewhere(&value))) {
            return Ok(HtmlResponse::new(redirect(self.mode, value)));
        }
        let headers = [(header::LOCATION, value), (header::CACHE_CONTROL, HeaderValue::from_static(MOVED_CACHE))];
        Ok(HtmlResponse::new((StatusCode::MOVED_PERMANENTLY, headers).into_response()))
    }
}

/// A page with a `Refresh` header: browsers keep a full load's after the runtime navigates
/// away, then load that URL again (Chromium, Firefox, WebKit). [`ReadAgain`] is the way.
#[cfg(debug_assertions)]
fn refresh_header_is_an_error(res: Response) -> Response {
    if !(res.headers().contains_key(SHELL_HEADER) && res.headers().contains_key("refresh")) {
        return res;
    }
    Error::new(
        "corten: this page sends a Refresh header, which the browser keeps after the runtime navigates away and then loads this URL again: put <ReadAgain after={..} /> in the page's head tags instead",
    )
    .into_response()
}

/// How long the browser keeps a [`Req::moved`] redirect: a day, its own copy only.
const MOVED_CACHE: &str = "private, max-age=86400";

/// A redirect in `mode`: `corten-location` for a form patch (the runtime navigates), a 303
/// otherwise. A client navigation sent to a place on a page (`#id`) or to another site gets
/// `corten-location` too: `fetch` follows a 303 but drops its fragment, so the runtime would not
/// know where to scroll, and may not read another origin's answer.
pub(crate) fn redirect(mode: Mode, to: HeaderValue) -> Response {
    match mode {
        Mode::Action => (StatusCode::NO_CONTENT, [(LOCATION_HEADER, to)]).into_response(),
        // `fetch` follows a redirect but drops its `#id`, and one to another origin is refused
        // (the CSP's `connect-src`, else CORS): the runtime navigates there itself.
        Mode::Navigation if has_fragment(&to) || elsewhere(&to) => {
            (StatusCode::NO_CONTENT, [(LOCATION_HEADER, to)]).into_response()
        }
        _ => (StatusCode::SEE_OTHER, [(header::LOCATION, to)]).into_response(),
    }
}

fn has_fragment(to: &HeaderValue) -> bool {
    to.as_bytes().contains(&b'#')
}

/// An address outside the app: anything but a path on this origin (`https://..`, `//host/..`).
/// A route's address always starts with one `/`, and an `axum::http::Uri` is a path or absolute
/// (it cannot be relative).
fn elsewhere(to: &HeaderValue) -> bool {
    let to = to.as_bytes();
    to.first() != Some(&b'/') || to.get(1) == Some(&b'/')
}

/// What a page whose handlers call an unserved script module is told.
#[cold]
fn unserved_module(name: &str) -> String {
    format!(
        "the page's handlers call the script module {name}.js, which the app does not serve, so they would do nothing: list the component it belongs to in App::components (for a script! module, its COMPONENT)"
    )
}

/// Middleware the app installs: a failed request (an [`Error`] from a handler, or an empty error
/// response such as axum's 404 for an unknown URL or 405 for a wrong method) renders the error
/// page in the request's mode, and the PWA's offline path gets Corten's offline page when the app
/// has no route for it. A [`Mode::Data`] request, every request on an `api` route and every
/// failure of a JSON route ([`crate::JsonResult`]) gets the failure as JSON instead
/// ([`crate::api`]). A request a [`crate::Caller`] sent to sign in is redirected there.
pub(crate) async fn render_errors<S: crate::app::Steps>(
    shared: &Arc<Shared>,
    req: Request,
    next: Next<'_, S>,
    deadline: Option<std::time::Duration>,
    signal: &crate::Signal,
    api: Option<&crate::api::Mount>,
    mode: Mode,
) -> Response {
    use axum::body::HttpBody;
    if api.is_some() || mode == Mode::Data {
        let shape = api.and_then(|mount| mount.errors.as_ref());
        return crate::api::json_failure(run(req, next, deadline, signal).await, shared.dev, shape).await;
    }
    let offline = req.method() == axum::http::Method::GET && shared.offline.as_deref() == Some(req.uri().path());
    // A browser: the runtime, or a document request that asks for HTML (a health probe or a
    // webhook sender does not).
    let browser = matches!(mode, Mode::Navigation | Mode::Action) || (mode == Mode::Document && accepts_html(req.headers()));
    // Taken now, while the request (its nonce, its credentials) is here; it allocates nothing.
    let asked = Asked::new(&req, shared.clone(), Credentials::of(req.headers()));
    let res = run(req, next, deadline, signal).await;
    #[cfg(debug_assertions)]
    let mut res = refresh_header_is_an_error(res);
    #[cfg(not(debug_assertions))]
    let mut res = res;
    let status = res.status();
    let error = status.is_client_error() || status.is_server_error();
    if !error || res.extensions().get::<crate::answer::Relayed>().is_some() {
        return res;
    }
    // A JSON route answers every failure as JSON, a path that did not parse included.
    if res.extensions().get::<crate::answer::Answered>().is_some() {
        // Not kept across the await, so the request's future stays as small as it was.
        drop(asked);
        return crate::api::json_failure(res, shared.dev, None).await;
    }
    if let Some(crate::caller::SignIn(to)) = res.extensions().get::<crate::caller::SignIn>().cloned() {
        let kept = std::mem::take(res.headers_mut());
        let Some(message) = sign_in_loops(asked.path(), &to) else {
            return carried(kept, redirect(mode, to));
        };
        tracing::error!("corten: {message}");
        let failure = Failure::Failed { status: StatusCode::INTERNAL_SERVER_ERROR, detail: Some(Arc::from(message)) };
        let failure = if shared.dev { failure } else { failure.without_detail() };
        return carried(kept, error_page(shared, asked, failure).await);
    }
    let mut failed = res.extensions().get::<crate::Failed>().cloned();
    let cause = res.extensions().get::<Cause>().copied();
    let empty_error = res.body().size_hint().exact() == Some(0);
    // An extractor's rejection (a form field missing, a path segment that does not parse) is
    // plain text written for a developer. A browser gets the error page instead, with the text
    // under it in dev mode; any other client (probes, webhooks) still gets the text.
    let rejection = failed.is_none() && cause.is_none() && browser && crate::api::is_plain_text(res.headers());
    if failed.is_none() && cause.is_none() && !empty_error && !rejection {
        return res;
    }
    // What the handler and the app's middleware said besides the body (a request id,
    // `Retry-After`, `WWW-Authenticate`, `Allow`) goes out with the error page.
    let kept = std::mem::take(res.headers_mut());
    if rejection {
        let text = axum::body::to_bytes(res.into_body(), 16 * 1024).await.unwrap_or_default();
        let text = String::from_utf8_lossy(&text);
        tracing::debug!(status = status.as_u16(), "corten: request rejected: {text}");
        failed = Some(crate::Failed(Arc::from(text.trim())));
    }
    if offline && status == StatusCode::NOT_FOUND {
        // The PWA's offline page, which the service worker keeps: a 200.
        return carried(kept, error_page(shared, asked, Failure::Offline).await);
    }
    let detail = failed.map(|crate::Failed(message)| message);
    let failure = match cause {
        Some(Cause::Panicked) => Failure::Panicked { detail },
        Some(Cause::TimedOut) => Failure::TimedOut,
        Some(Cause::Refused) => Failure::Refused { status },
        Some(Cause::Unidentified) => Failure::Unidentified { detail },
        None if rejection => Failure::Rejected { status, detail },
        // Nothing is here, whether no route has the URL or the handler found nothing.
        None if status == StatusCode::NOT_FOUND => Failure::NotFound,
        None if detail.is_some() => Failure::Failed { status, detail },
        None => match status {
            StatusCode::METHOD_NOT_ALLOWED => Failure::WrongMethod,
            status => Failure::Failed { status, detail: None },
        },
    };
    let failure = if shared.dev { failure } else { failure.without_detail() };
    carried(kept, error_page(shared, asked, failure).await)
}

/// [`Req::asset`] with no static directory: a bug, which panics in debug builds.
#[cold]
fn no_assets(path: &'static str) -> crate::assets::AssetUrl<'static> {
    let message = format!("corten: no static directory is loaded, so there is no {path:?}; see App::assets");
    if cfg!(debug_assertions) {
        panic!("{message}");
    }
    tracing::error!("{message}");
    crate::assets::AssetUrl::unserved(path)
}

/// `res` with the headers of the failed response it replaces, `kept`, except the ones that
/// described its body or that Corten sets for the answer it sends (`Cache-Control`, its own
/// `corten-` headers); a header `res` has already wins. A `Cache-Control` that says `no-store`
/// (`NoStore`, or one written by hand, on any of its lines) makes `res` `no-store` too: whoever
/// set it said nothing may keep the answer, a failure included. Its other directives described the
/// failed answer, so they are not carried.
fn carried(kept: HeaderMap, mut res: Response) -> Response {
    if kept.is_empty() {
        return res;
    }
    use axum::http::header::Entry;
    let headers = res.headers_mut();
    if kept.get_all(header::CACHE_CONTROL).iter().any(|line| crate::cache::says(line.as_bytes()).no_store) {
        headers.insert(header::CACHE_CONTROL, crate::cache::NO_STORE);
    }
    let mut kept_name = None;
    // A name comes with its first value only; the values after it have `None`.
    for (key, value) in kept {
        if let Some(key) = key {
            kept_name = None;
            let replaced = matches!(
                key,
                header::CONTENT_TYPE
                    | header::CONTENT_LENGTH
                    | header::CONTENT_ENCODING
                    | header::CONTENT_LANGUAGE
                    | header::CONTENT_DISPOSITION
                    | header::CONTENT_RANGE
                    | header::TRANSFER_ENCODING
                    | header::ETAG
                    | header::LAST_MODIFIED
                    | header::CACHE_CONTROL
            ) || key.as_str().starts_with("corten-");
            if !replaced {
                if let Entry::Vacant(entry) = headers.entry(key) {
                    kept_name = Some(entry.insert_entry(value));
                }
            }
        } else if let Some(entry) = &mut kept_name {
            entry.append(value);
        }
    }
    res
}

/// Why a request a [`crate::Caller`] sent to sign in at `to` would loop: the sign-in page itself
/// needs a caller, so the browser would be sent round until it gives up. `None` when it does not.
fn sign_in_loops(path: &str, to: &HeaderValue) -> Option<String> {
    let target = to.to_str().unwrap_or("");
    (target.split(['?', '#']).next() == Some(path)).then(|| {
        format!("{path} needs a caller and sends to sign in at {target}, itself: the sign-in page's handler cannot take Caller")
    })
}

/// The handler's response, through [`within_deadline`]; a handler that panics is answered as a
/// handler error (500), whose message only `corten dev` shows.
async fn run<S: crate::app::Steps>(
    req: Request,
    next: Next<'_, S>,
    deadline: Option<std::time::Duration>,
    signal: &crate::Signal,
) -> Response {
    use std::{future::Future, panic::AssertUnwindSafe, pin::pin, task::Poll};
    // For the log only: a method and a URI clone without allocating.
    let (method, uri) = (req.method().clone(), req.uri().clone());
    // A handler that panics fails its request like one that returned an error, instead of
    // dropping the connection with no answer. Nothing runs unless it panics.
    let mut fut = pin!(within_deadline(req, next, deadline, signal, (&method, &uri)));
    let outcome = std::future::poll_fn(|cx| match std::panic::catch_unwind(AssertUnwindSafe(|| fut.as_mut().poll(cx))) {
        Ok(Poll::Ready(res)) => Poll::Ready(Ok(res)),
        Ok(Poll::Pending) => Poll::Pending,
        Err(panic) => Poll::Ready(Err(panic)),
    })
    .await;
    match outcome {
        Ok(res) => res,
        Err(panic) => {
            let message = panic
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "a panic with no message".to_string());
            tracing::error!("corten: {method} {} panicked: {message}", uri.path());
            let mut res = Error::new(format!("the handler panicked: {message}")).into_response();
            res.extensions_mut().insert(Cause::Panicked);
            res
        }
    }
}

/// The handler's response, or an empty 503 once it has waited past `deadline`.
async fn within_deadline<S: crate::app::Steps>(
    req: Request,
    next: Next<'_, S>,
    deadline: Option<std::time::Duration>,
    signal: &crate::Signal,
    (method, uri): (&axum::http::Method, &Uri),
) -> Response {
    let Some(limit) = deadline else {
        return next.run(req).await;
    };
    match within(limit, next.run(req)).await {
        Some(res) => res,
        None => {
            signal.abort(crate::Abort::Deadline);
            tracing::warn!("corten: {method} {} passed its {limit:?} deadline", uri.path());
            let mut res = StatusCode::SERVICE_UNAVAILABLE.into_response();
            res.extensions_mut().insert(Cause::TimedOut);
            res
        }
    }
}

fn accepts_html(headers: &HeaderMap) -> bool {
    headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("text/html"))
}

/// `fut`'s output, or `None` once it has waited `limit`. The timer starts the first time `fut` is
/// not ready, so a handler that answers at once (a page rendered from memory) never registers
/// one: about 600 ns a request measured with `tokio::time::timeout`.
async fn within<F: std::future::Future>(limit: std::time::Duration, fut: F) -> Option<F::Output> {
    use std::{future::Future, pin::pin, task::Poll};
    let mut fut = pin!(fut);
    let mut timer = pin!(None::<tokio::time::Sleep>);
    std::future::poll_fn(|cx| {
        if let Poll::Ready(out) = fut.as_mut().poll(cx) {
            return Poll::Ready(Some(out));
        }
        if timer.is_none() {
            timer.set(Some(tokio::time::sleep(limit)));
        }
        match timer.as_mut().as_pin_mut().map(|timer| timer.poll(cx)) {
            Some(Poll::Ready(())) => Poll::Ready(None),
            _ => Poll::Pending,
        }
    })
    .await
}

thread_local! {
    /// Where `<Corten />` was written in the document being rendered.
    static HEAD_AT: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Where Corten writes its tags in a document's `<head>`: the import map, the runtime, the
/// page's script modules, the app's configured tags (manifest, theme color) and the key that
/// tells the runtime whether the page around `<main>` is current. A document holds it once, after
/// its `<title>`: `<head>..<title>{title}</title>{head}<Corten /></head>`.
pub struct Corten;

impl Render for Corten {
    #[inline]
    fn render(&self, out: &mut String) {
        HEAD_AT.set(Some(out.len()));
    }
}

/// A page that shows a read still catching up with a write ("saved, updating") asks to be read
/// again after `after` (whole seconds, at least one), however it was reached (a full load, a
/// navigation, a form's redirect). It goes among the page's head tags, which the layout puts in
/// `<head>`: `head={pending.then_some(ReadAgain { after: Duration::from_secs(2) })}`.
///
/// The runtime fetches the same URL and swaps it in, replacing the history entry and keeping the
/// scroll position and the focused element (by its id); a navigation away stops it, and a hidden
/// tab waits until it is shown. Without JavaScript the browser reloads the page
/// (`<meta http-equiv="refresh">` in `<noscript>`). The page asks again each time it is read, so
/// it stops when the server stops asking (the write has landed, or is too old to wait for).
///
/// Not the `Refresh` header: browsers keep a full load's header after the runtime has navigated
/// away, and then load that first URL again. Debug builds answer 500 to a page that sends one.
///
/// A `data-patch` form's answer asks the same with [`ReadAgain::from`], and so does a page that
/// should have only some of its parts read again; with JavaScript, a page that does is not read
/// whole, so its head's `ReadAgain` is then for a browser without it. A page that keeps itself
/// fresh another way (a `<corten-live>` relaying the part) asks a browser without JavaScript only:
/// [`ReadAgain::without_javascript`].
#[derive(Clone, Copy, Debug)]
pub struct ReadAgain {
    /// How long until the page is read again.
    pub after: std::time::Duration,
}

impl ReadAgain {
    /// Whole seconds, at least one.
    fn seconds(self) -> u64 {
        (self.after.as_secs() + u64::from(self.after.subsec_nanos() > 0)).max(1)
    }

    /// In a patch (a `data-patch` form's answer, [`Req::patch`]) whose part shows a read still
    /// catching up with the write: after `after`, the runtime fetches `route` as a GET
    /// `data-patch` form would (`corten: action`) and applies the patch it answers, which asks
    /// again while it is behind. The route's handler answers with [`Req::patch_or`] (the part,
    /// or the page without JavaScript), and its URL carries what the read waits for (the write's receipt),
    /// since the page's own address does not.
    ///
    /// The read waits while the tab is hidden or a form's answer is on its way, keeps what the
    /// user typed, opened and focused as any patch does, and stops once an element the patch put
    /// on the page is replaced (a later patch, a navigation away) or the read fails. Without
    /// JavaScript a form gets a page, never a patch, so the page's own [`ReadAgain`] applies.
    ///
    /// In a page's markup (the first load or a navigation) it starts the same read once the page
    /// is on the screen, beside the parts it reads: it stops when the page is left or an element
    /// with an id beside it is replaced. With JavaScript, the page's own [`ReadAgain`] then does
    /// not read the page whole; it stays for a browser without JavaScript.
    ///
    /// ```
    /// use std::time::Duration;
    /// use corten::{prelude::*, view::Patch};
    ///
    /// #[corten::routes]
    /// mod url {
    ///     #[route("/alerts", get = alerts)]
    ///     pub struct Alerts<'a> {
    ///         pub after: Option<&'a str>,
    ///     }
    /// }
    /// # async fn alerts(req: Req) -> HtmlResult { req.patch(html! {}) }
    ///
    /// #[derive(Id)]
    /// struct AlertsId;
    ///
    /// fn saving<'a>(from: &'a url::Alerts<'a>) -> impl Patch + 'a {
    ///     let again = ReadAgain { after: Duration::from_secs(2) };
    ///     html! { <section id={AlertsId}>"Saving..."</section> {again.from(from)} }
    /// }
    /// let from = url::Alerts { after: Some("r7") };
    /// assert_eq!(
    ///     saving(&from).to_html(),
    ///     r#"<section id="alerts">Saving...</section><template data-corten-read-again="2" data-from="/alerts?after=r7"></template>"#
    /// );
    /// ```
    ///
    /// It takes the route by value too, so a component that is part of the page and the patch
    /// builds the read in its body and returns it:
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use corten::prelude::*;
    /// # #[derive(Route)]
    /// # #[route("/alerts")]
    /// # struct Alerts<'a> { after: Option<&'a str> }
    /// #[derive(Id)]
    /// struct ListId;
    ///
    /// fn list(after: &str, saving: bool) -> impl Patch + '_ {
    ///     let again = saving.then(|| ReadAgain { after: Duration::from_secs(2) }.from(Alerts { after: Some(after) }));
    ///     html! { <ul id={ListId}></ul> {again} }
    /// }
    /// assert!(list("r7", true).to_html().ends_with(r#"data-from="/alerts?after=r7"></template>"#));
    /// ```
    ///
    /// A URL written as text does not compile:
    ///
    /// ```compile_fail,E0277
    /// use std::time::Duration;
    /// use corten::prelude::*;
    ///
    /// let _ = ReadAgain { after: Duration::from_secs(2) }.from("/alerts?after=r7");
    /// ```
    pub fn from<R: crate::route::Route>(self, route: R) -> ReadAgainFrom<R> {
        ReadAgainFrom { again: self, route }
    }

    /// Asks only a browser without JavaScript to read the page again: for a page that keeps
    /// itself fresh with a script, such as one whose changing part a `<corten-live>` relays
    /// while the job it follows runs. With JavaScript the page is not read whole (which would
    /// replace the `<corten-live>` and open its stream again); without it, the browser reloads it.
    /// A page whose stream is unrelated to what it waits for (a ticker beside a saving form) uses
    /// plain [`ReadAgain`].
    ///
    /// ```
    /// use std::time::Duration;
    /// use corten::prelude::*;
    ///
    /// let again = ReadAgain { after: Duration::from_secs(2) }.without_javascript();
    /// assert_eq!(again.to_html(), r#"<noscript><meta http-equiv="refresh" content="2"></noscript>"#);
    /// ```
    pub fn without_javascript(self) -> ReadAgainWithoutJavaScript {
        ReadAgainWithoutJavaScript(self)
    }
}

/// [`ReadAgain::without_javascript`]: `<noscript>` and its refresh, without the mark the
/// runtime reads.
#[derive(Clone, Copy, Debug)]
pub struct ReadAgainWithoutJavaScript(ReadAgain);

impl Render for ReadAgainWithoutJavaScript {
    fn render(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = write!(out, r#"<noscript><meta http-equiv="refresh" content="{}"></noscript>"#, self.0.seconds());
    }
}

impl Render for ReadAgain {
    fn render(&self, out: &mut String) {
        use std::fmt::Write;
        let seconds = self.seconds();
        let _ = write!(
            out,
            r#"<noscript data-corten-read-again="{seconds}"><meta http-equiv="refresh" content="{seconds}"></noscript>"#
        );
    }
}

/// A patch's or a page's request to be read again from a route, made by [`ReadAgain::from`]:
/// `<template data-corten-read-again="2" data-from="/alerts?after=r7">`.
pub struct ReadAgainFrom<R> {
    again: ReadAgain,
    route: R,
}

impl<R: crate::route::Route> Render for ReadAgainFrom<R> {
    fn render(&self, out: &mut String) {
        use std::fmt::Write;
        let _ = write!(out, r#"<template data-corten-read-again="{}" data-from=""#, self.again.seconds());
        self.route.write_href(out, true);
        out.push_str(r#""></template>"#);
    }
}

impl<R: crate::route::Route> crate::view::__private::PatchSealed for ReadAgainFrom<R> {}
impl<R: crate::route::Route> crate::view::Patch for ReadAgainFrom<R> {}

/// Structured data for search engines (a product, its breadcrumbs), as
/// `<script type="application/ld+json">`, among a page's head tags:
/// `head={JsonLd { value: &product }}`, or `<JsonLd value={&product} />` in `html!`. The value
/// is any [`crate::json::ToJson`]; its `<` are written `\u003c`, which JSON reads the same, so no
/// text in it (a seller's description that says `</script>`) can end the element. It is data,
/// not a script, so the CSP does not apply.
///
/// ```
/// use corten::prelude::*;
///
/// #[derive(serde::Serialize, corten::ToJson)]
/// struct Product<'a> {
///     #[serde(rename = "@type")]
///     kind: &'a str,
///     name: &'a str,
/// }
///
/// let ld = JsonLd { value: &Product { kind: "Product", name: "</script><b>" } };
/// assert_eq!(
///     html! { {ld} }.to_html(),
///     r#"<script type="application/ld+json">{"@type":"Product","name":"\u003c/script>\u003cb>"}</script>"#
/// );
/// ```
///
/// An expression inside `<script>` or `<style>` would be escaped as HTML text, which neither
/// reads back, so it does not compile:
///
/// ```compile_fail
/// use corten::prelude::*;
///
/// let ld = r#"{"@type":"Product"}"#;
/// let head = html! { <script type="application/ld+json">{ld}</script> };
/// ```
#[cfg(feature = "json")]
pub struct JsonLd<'a, T: ?Sized> {
    /// The structured data.
    pub value: &'a T,
}

#[cfg(feature = "json")]
impl<T: crate::json::ToJson + ?Sized> Render for JsonLd<'_, T> {
    fn render(&self, out: &mut String) {
        let json = crate::json::to_vec(self.value);
        let json = std::str::from_utf8(&json).expect("ToJson writes UTF-8");
        out.push_str(r#"<script type="application/ld+json">"#);
        // `<` is only ever inside a JSON string, where `\u003c` reads the same.
        let mut parts = json.split('<');
        out.push_str(parts.next().unwrap_or(""));
        for part in parts {
            out.push_str("\\u003c");
            out.push_str(part);
        }
        out.push_str("</script>");
    }
}
/// A part of a document, for [`Req::render_into`].
struct Parted<D>(D, Part);

impl<D: Document> Render for Parted<D> {
    #[inline]
    fn render(&self, out: &mut String) {
        self.0.render_part(out, self.1)
    }
}

pub(crate) fn html_response(status: StatusCode, body: String, kind: Option<HeaderName>) -> Response {
    let mut res = Response::new(Body::from(body));
    *res.status_mut() = status;
    let headers = res.headers_mut();
    // Room for the headers `App` adds too, so the map is allocated once.
    headers.reserve(12);
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8"));
    headers.insert(header::VARY, HeaderValue::from_static(VARY));
    if let Some(kind) = kind {
        headers.insert(kind, HeaderValue::from_static("1"));
    }
    res
}
