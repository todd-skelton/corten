//! Pages and patches: what a handler returns and how it reaches each kind of client.

use std::{
    any::Any,
    cell::Cell,
    convert::Infallible,
    future::Future,
    hash::{Hash, Hasher},
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

#[cfg(test)]
pub(crate) fn is_current_for_tests(path: &str, href: &str, section: bool) -> bool {
    is_current(path, href, section)
}

/// The context for this request, taken from the request extensions when the request arrives.
pub(crate) struct Captured {
    /// The shell key, printed as 16 hex digits.
    key: u64,
    context: Option<Box<dyn Any + Send + Sync>>,
}

impl Captured {
    fn key(&self) -> HeaderValue {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = [0u8; 16];
        for (i, digit) in text.iter_mut().enumerate() {
            *digit = HEX[(self.key >> (60 - 4 * i) & 15) as usize];
        }
        HeaderValue::from_bytes(&text).expect("hex is a valid header")
    }
}

/// Copies the app's context (`App::context`) out of the request with its hash, which goes into
/// the key that tells the runtime whether the page around `<main>` is still current.
pub(crate) type CaptureContext = dyn Fn(&Extensions) -> Option<(u64, Box<dyn Any + Send + Sync>)> + Send + Sync;

/// A failed request, given to the app's error page ([`crate::App::error_page`]).
#[derive(Clone, Debug)]
pub struct Failure {
    /// What went wrong: 404 for an unknown URL, 405 for a wrong method, 422 for input that did not
    /// parse, 503 past the deadline or offline (the PWA's offline page), 500 for a handler's error
    /// or panic, or the status a handler's [`Error::status`] gave.
    pub status: StatusCode,
    /// The error itself, under `corten dev` only, for the page to show the developer. Never set
    /// in production, where it is logged instead.
    pub detail: Option<Arc<str>>,
}

/// The error page of an app that gives none ([`crate::App::error_page`]): a plain document. An app
/// made by `corten new` has its own, in its layout. Every error page is sent with
/// `X-Robots-Tag: noindex`.
pub(crate) fn default_error_page(req: &Req, failure: Failure) -> impl crate::view::Document + '_ {
    let (title, text) = match failure.status {
        StatusCode::NOT_FOUND => ("Not found", "There is nothing at this address."),
        StatusCode::FORBIDDEN => ("Not allowed", "You do not have access to this page."),
        StatusCode::UNAUTHORIZED => ("Sign in needed", "Sign in to see this page."),
        StatusCode::SERVICE_UNAVAILABLE => ("Not available", "Check your connection and try again."),
        _ => ("Something went wrong", "Please try again."),
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
                    if let Some(detail) = &failure.detail {
                        <pre>{detail}</pre>
                    }
                </main>
            </body>
        </html>
    }
}

/// An error page with its document type erased: the response for a failed request.
pub(crate) type ErrorPage = dyn Fn(&Req, Failure) -> Response + Send + Sync;

/// A function from a failed request to its page, as [`crate::App::error_page`] takes it:
/// `fn error_page(req: &Req, failure: Failure) -> impl Document + '_`.
pub trait ErrorPageFn<'a>: Fn(&'a Req, Failure) -> <Self as ErrorPageFn<'a>>::Page {
    /// The document it returns.
    type Page: crate::view::Document + 'a;
}

impl<'a, F, D> ErrorPageFn<'a> for F
where
    F: Fn(&'a Req, Failure) -> D,
    D: crate::view::Document + 'a,
{
    type Page = D;
}

pub(crate) fn error_page_fn<F>(page: F) -> Box<ErrorPage>
where
    F: for<'a> ErrorPageFn<'a> + Send + Sync + 'static,
{
    Box::new(move |req: &Req, failure: Failure| {
        let status = failure.status;
        let mut res = match req.page_with_status(status, page(req, failure)) {
            Ok(res) => res,
            Err(error) => {
                // The error page itself failed (under `corten dev`, a component the app does not
                // list): plain text, so a broken error page cannot fail again.
                tracing::error!("corten: the error page failed: {error:?}");
                (status, "The page could not be shown.").into_response()
            }
        };
        // Search engines skip it, whatever the layout says: the offline page is a 200.
        res.headers_mut().insert(HeaderName::from_static("x-robots-tag"), HeaderValue::from_static("noindex"));
        res
    })
}

/// Everything a request needs from the app: the error page, where scripts live, the context.
pub(crate) struct Shared {
    pub error_page: Box<ErrorPage>,
    pub capture: Box<CaptureContext>,
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
    /// Dev mode: error pages show the error.
    pub dev: bool,
    /// The names of the components the app lists (`App::components` and `Area::components`).
    pub components: std::collections::HashSet<&'static str>,
    /// Each area with a stylesheet (`Area::components`): its prefix and the stylesheet's `<link>`.
    pub sections: Vec<(String, String)>,
    /// The section (an index in `sections`) whose stylesheet has a component's rules.
    pub section_of: std::collections::HashMap<&'static str, usize>,
}

impl Shared {
    pub(crate) fn new(
        error_page: Box<ErrorPage>,
        capture: Box<CaptureContext>,
        assets: Option<Arc<Assets>>,
        head_extra: String,
    ) -> Self {
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
            capture,
            assets,
            scripts_dir,
            import_map,
            head_extra,
            key_base: hasher.finish(),
            offline: None,
            locales: None,
            dev: false,
            components: Default::default(),
            sections: Vec::new(),
            section_of: Default::default(),
        }
    }
}

/// Used by handlers that run without an [`crate::App`] (unit tests of a bare router).
static DEFAULT: LazyLock<Arc<Shared>> =
    LazyLock::new(|| Arc::new(Shared::new(error_page_fn(default_error_page), Box::new(|_| None), None, String::new())));

/// The request as a handler answers it: which [`Mode`] the client asked in, and the methods
/// that build each kind of response. Every handler that returns HTML takes one.
pub struct Req {
    pub mode: Mode,
    nonce: Option<NonceSlot>,
    uri: Uri,
    accept_language: Option<HeaderValue>,
    shared: Arc<Shared>,
    captured: Captured,
    signal: crate::Signal,
}

impl<St: Send + Sync> FromRequestParts<St> for Req {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &St) -> std::result::Result<Self, Self::Rejection> {
        let shared = parts.extensions.get::<Arc<Shared>>().unwrap_or(&DEFAULT).clone();
        Ok(Req::new(&parts.headers, &parts.extensions, &parts.uri, shared))
    }
}

impl Req {
    /// Nothing here allocates unless the app has a context (which is copied).
    pub(crate) fn new(headers: &HeaderMap, extensions: &Extensions, uri: &Uri, shared: Arc<Shared>) -> Self {
        let (hash, context) = match (shared.capture)(extensions) {
            Some((hash, context)) => (hash, Some(context)),
            None => (0, None),
        };
        let key = shared.key_base ^ hash.rotate_left(1);
        Req {
            mode: mode_of(headers),
            nonce: extensions.get::<NonceSlot>().cloned(),
            uri: uri.clone(),
            accept_language: headers.get(header::ACCEPT_LANGUAGE).cloned(),
            shared,
            captured: Captured { key, context },
            signal: extensions.get::<crate::Signal>().cloned().unwrap_or_default(),
        }
    }

    /// When the work for this request should stop, and why: the server's `request.signal`. See
    /// [`crate::Signal`]; work the handler awaits stops without it.
    pub fn signal(&self) -> &crate::Signal {
        &self.signal
    }

    /// The app's context for this request ([`crate::App::context`]), if it found one:
    /// `req.context::<Viewer>()`.
    pub fn context<C: 'static>(&self) -> Option<&C> {
        self.captured.context.as_ref()?.downcast_ref()
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
    pub fn page(&self, doc: impl Document) -> Result {
        self.page_with_status(StatusCode::OK, doc)
    }

    /// [`Req::page`] with another status.
    pub fn page_with_status(&self, status: StatusCode, doc: impl Document) -> Result {
        // The last size of each part, so the buffer is allocated once.
        static FULL: AtomicUsize = AtomicUsize::new(16 * 1024);
        static SWAP: AtomicUsize = AtomicUsize::new(16 * 1024);
        let swap = matches!(self.mode, Mode::Navigation | Mode::Action);
        let (part, size) = if swap { (Part::Swap, &SWAP) } else { (Part::Full, &FULL) };
        let mut out = String::with_capacity(size.load(Ordering::Relaxed));
        HEAD_AT.set(None);
        let modules = self.render_into(Parted(doc, part), &mut out)?;
        let Some(at) = HEAD_AT.take() else {
            return Err(Error::new("corten: the document's <head> has no <Corten />"));
        };
        let mut tags = String::new();
        if swap {
            self.section_links(&mut tags);
        } else {
            tags.reserve(1024);
            self.head_tags(&modules, &mut tags);
        }
        if !tags.is_empty() {
            out.insert_str(at, &tags);
        }
        size.store(out.len() + out.len() / 8, Ordering::Relaxed);
        // Only a swap says it is one: a document a cache gave to a navigation fetch by mistake
        // makes the runtime load the URL in full.
        Ok(self.finish(html_response(status, out, swap.then_some(PAGE_HEADER))))
    }

    /// The versioned URL of a static file: `req.asset("style.css")`. A missing file is a bug: it
    /// panics in debug builds, and in release the error is logged and the page still renders
    /// (with a link that 404s) rather than failing every request.
    pub fn asset(&self, path: &str) -> String {
        match &self.shared.assets {
            Some(assets) => assets.url(path),
            None => {
                let message = format!("corten: no static directory is loaded, so there is no {path:?}; see App::assets");
                if cfg!(debug_assertions) {
                    panic!("{message}");
                }
                tracing::error!("{message}");
                format!("/{path}")
            }
        }
    }

    /// True when the static directory has `path`.
    pub fn has_asset(&self, path: &str) -> bool {
        self.shared.assets.as_ref().is_some_and(|a| a.contains(path))
    }

    /// True when `href` (a typed route) is the current page, or with `section` any page under
    /// it: the rule the runtime applies to `<a data-nav>` (`data-nav="section"`) after each
    /// navigation.
    pub fn is_current(&self, href: &(impl crate::route::Href + ?Sized), section: bool) -> bool {
        href.with_href(|href| is_current(self.uri.path(), href, section))
    }

    /// `aria-current="page"` for a nav link when [`Req::is_current`]:
    /// `<a href={About} data-nav aria-current={req.current(&About)}>`.
    pub fn current(&self, href: &(impl crate::route::Href + ?Sized)) -> Option<&'static str> {
        self.is_current(href, false).then_some("page")
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
        if !self.shared.dev {
            let ((), modules) = crate::script::collect(|| {
                // An empty buffer: to_html sizes it from earlier renders of the same view.
                if out.capacity() == 0 {
                    *out = view.to_html();
                } else {
                    view.render(out);
                }
            });
            return Ok(modules);
        }
        let (((), used), modules) = crate::script::collect(|| crate::style::collect(|| view.render(out)));
        self.check_rendered(&used)?;
        Ok(modules)
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
            let prefix = &self.shared.sections[index].0;
            if !crate::api::under(prefix, self.path()) {
                return Err(Error::new(format!(
                    "{path} renders {name}, whose stylesheet is in the area {prefix}, which is not linked here: list {name} in App::components, or route the page in that area",
                    path = self.path(),
                    name = component.name
                )));
            }
        }
        Ok(())
    }

    /// The `<link>` tags of the section stylesheets for this request's path.
    #[inline]
    fn section_links(&self, out: &mut String) {
        let path = self.uri.path();
        for (prefix, link) in &self.shared.sections {
            if crate::api::under(prefix, path) {
                out.push_str(link);
            }
        }
    }

    /// Adds the shell key, which tells the runtime whether the page around `<main>` is still current.
    fn finish(&self, mut res: Response) -> Response {
        res.headers_mut().insert(SHELL_HEADER, self.captured.key());
        res.headers_mut().insert(header::VARY, self.vary());
        res
    }

    fn vary(&self) -> HeaderValue {
        HeaderValue::from_static(if self.shared.locales.is_some() { VARY_LANGUAGE } else { VARY })
    }

    /// The tags Corten needs in `<head>` after the page's own: script settings, the shell key,
    /// the app's configured tags, section stylesheets, the import map, the runtime and modules.
    fn head_tags(&self, modules: &[&'static str], out: &mut String) {
        use std::fmt::Write;
        let dir = &self.shared.scripts_dir;
        let _ = write!(
            out,
            r#"<meta name="corten-scripts" content="{dir}"><meta name="corten-shell" content="{:016x}">"#,
            self.captured.key
        );
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
            for tag in modules.iter().filter_map(|name| assets.module_tag(name)) {
                out.push_str(tag);
                preloaded = true;
            }
            // A handler module may import the views (corten/views): fetched with it, not after it.
            if let Some(views) = assets.views_tag().filter(|_| preloaded) {
                out.push_str(views);
            }
        }
    }

    /// [`Req::page`] for a page other clients read too: a browser gets HTML as usual, and a
    /// [`Mode::Data`] request gets `{"title": ..., "data": data}`, the title being the text of the
    /// document's `<title>`. `data` is the screen's view model, so a native screen or an agent sees
    /// exactly what the view shows; for it, only the document's `<title>` renders.
    #[cfg(feature = "json")]
    pub fn screen<T: crate::json::ToJson + ?Sized>(&self, data: &T, doc: impl Document) -> Result {
        self.screen_with_status(StatusCode::OK, data, doc)
    }

    #[cfg(feature = "json")]
    pub fn screen_with_status<T: crate::json::ToJson + ?Sized>(
        &self,
        status: StatusCode,
        data: &T,
        doc: impl Document,
    ) -> Result {
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
        Ok((status, headers, body).into_response())
    }

    /// Answers a `<form data-patch>` submit with fragments: the runtime replaces each element on
    /// the page that has the id of one of `view`'s top-level elements. One without an id would
    /// be dropped in the browser, so `view` is a [`Patch`](crate::view::Patch), which the compiler
    /// checks: usually a component declared `-> impl Patch` whose root has a `#[derive(Id)]` id.
    pub fn patch(&self, view: impl crate::view::Patch) -> Result {
        self.patch_with_status(StatusCode::OK, view)
    }

    /// Same as [`Req::patch`] with another status.
    pub fn patch_with_status(&self, status: StatusCode, view: impl crate::view::Patch) -> Result {
        let mut body = String::new();
        self.render_into(view, &mut body)?;
        Ok(html_response(status, body, Some(PATCH_HEADER)))
    }

    /// Answers a submit whose input failed validation, with 422 Unprocessable Content:
    ///
    /// - [`Mode::Action`]: `form`, the form re-rendered with the errors, as a patch. The runtime
    ///   then moves focus to the first field marked `aria-invalid="true"`.
    /// - [`Mode::Data`] (feature `json`): the JSON error with the fields,
    ///   `{"error": {"status": 422, "message": "email: ...", "fields": {"email": "..."}}}`.
    /// - Otherwise (no JavaScript): `page()`, usually `req.page(..)` for the whole page with the
    ///   form showing the errors; a success status becomes 422. It is only called here, so an
    ///   action never loads data for a page it does not send.
    pub async fn invalid<F, Fut>(&self, errors: &crate::Errors, form: impl crate::view::Patch, page: F) -> Result
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result>,
    {
        let status = StatusCode::UNPROCESSABLE_ENTITY;
        match self.mode {
            Mode::Action => self.patch_with_status(status, form),
            #[cfg(feature = "json")]
            Mode::Data => Ok(errors.response()),
            _ => {
                let _ = errors;
                let mut res = page().await?;
                if res.status().is_success() {
                    *res.status_mut() = status;
                }
                Ok(res)
            }
        }
    }

    /// After a successful write, send the browser to `to` (post/redirect/get): a typed route, or
    /// an `axum::http::Uri` to leave the app. Works with and without JavaScript.
    pub fn redirect(&self, to: &(impl crate::route::Href + ?Sized)) -> Result {
        let value = to.with_href(|href| HeaderValue::try_from(href)).map_err(Error::new)?;
        Ok(redirect(self.mode, value))
    }

    /// The page is at `to` for good (an item asked for by its id or an old slug, a renamed
    /// category): `301 Moved Permanently`, so search engines move the old URL's place to `to` and
    /// the browser keeps the redirect for a day (privately; a shared cache does not), skipping the
    /// round trip next time. A client navigation follows it and puts `to` in the address; a
    /// `data-patch` form is sent there as by [`Req::redirect`]. For a `GET`: after a write,
    /// [`Req::redirect`]. Parameters the old URL had are the route's to carry
    /// (`#[route(prefix = ..)]` for ones named by data), so only the ones it knows move with it.
    pub fn moved(&self, to: &(impl crate::route::Href + ?Sized)) -> Result {
        let value = to.with_href(|href| HeaderValue::try_from(href)).map_err(Error::new)?;
        if self.mode == Mode::Action {
            return Ok(redirect(self.mode, value));
        }
        let headers = [(header::LOCATION, value), (header::CACHE_CONTROL, HeaderValue::from_static(MOVED_CACHE))];
        Ok((StatusCode::MOVED_PERMANENTLY, headers).into_response())
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
/// otherwise.
pub(crate) fn redirect(mode: Mode, to: HeaderValue) -> Response {
    match mode {
        Mode::Action => (StatusCode::NO_CONTENT, [(LOCATION_HEADER, to)]).into_response(),
        _ => (StatusCode::SEE_OTHER, [(header::LOCATION, to)]).into_response(),
    }
}

/// Middleware the app installs: a failed request (an [`Error`] from a handler, or an empty error
/// response such as axum's 404 for an unknown URL or 405 for a wrong method) renders the error
/// page in the request's mode, and the PWA's offline path gets Corten's offline page when the app
/// has no route for it. A [`Mode::Data`] request, and every request on an `api` route, gets the
/// failure as JSON instead ([`crate::api`]).
pub(crate) async fn render_errors(
    shared: &Arc<Shared>,
    req: Request,
    next: Next,
    deadline: Option<std::time::Duration>,
    signal: &crate::Signal,
    api: bool,
    mode: Mode,
) -> Response {
    use axum::body::HttpBody;
    if api || mode == Mode::Data {
        return crate::api::json_failure(run(req, next, deadline, signal).await, shared.dev).await;
    }
    let offline = req.method() == axum::http::Method::GET && shared.offline.as_deref() == Some(req.uri().path());
    // A browser: the runtime, or a document request that asks for HTML (a health probe or a
    // webhook sender does not).
    let browser = matches!(mode, Mode::Navigation | Mode::Action) || (mode == Mode::Document && accepts_html(req.headers()));
    // Taken now, while the request (its context, its nonce) is here; it allocates nothing unless
    // the shell takes a context.
    let failed_req = Req::new(req.headers(), req.extensions(), req.uri(), shared.clone());
    let res = run(req, next, deadline, signal).await;
    #[cfg(debug_assertions)]
    let res = refresh_header_is_an_error(res);
    let status = res.status();
    let mut failed = res.extensions().get::<crate::Failed>().cloned();
    let error = status.is_client_error() || status.is_server_error();
    let empty_error = error && res.body().size_hint().exact() == Some(0);
    // An extractor's rejection (a form field missing, a path segment that does not parse) is
    // plain text written for a developer. A browser gets the error page instead, with the text
    // under it in dev mode; any other client (probes, webhooks) still gets the text.
    let rejection = error && failed.is_none() && browser && crate::api::is_plain_text(res.headers());
    if failed.is_none() && !empty_error && !rejection {
        return res;
    }
    let allow = res.headers().get(header::ALLOW).cloned();
    if rejection {
        let text = axum::body::to_bytes(res.into_body(), 16 * 1024).await.unwrap_or_default();
        let text = String::from_utf8_lossy(&text);
        tracing::debug!(status = status.as_u16(), "corten: request rejected: {text}");
        failed = Some(crate::Failed(Arc::from(text.as_ref())));
    }
    let req = failed_req;
    if offline && status == StatusCode::NOT_FOUND {
        // The PWA's offline page, which the service worker keeps: the error page for 503.
        let mut res = (shared.error_page)(&req, Failure { status: StatusCode::SERVICE_UNAVAILABLE, detail: None });
        *res.status_mut() = StatusCode::OK;
        return res;
    }
    // Dev mode only: the error itself, for the page to show.
    let detail = failed.filter(|_| shared.dev).map(|crate::Failed(message)| message);
    let mut res = (shared.error_page)(&req, Failure { status, detail });
    if let Some(allow) = allow {
        res.headers_mut().insert(header::ALLOW, allow);
    }
    res
}

/// The handler's response, through [`within_deadline`]; a handler that panics is answered as a
/// handler error (500), whose message only `corten dev` shows.
async fn run(req: Request, next: Next, deadline: Option<std::time::Duration>, signal: &crate::Signal) -> Response {
    use std::{future::Future, panic::AssertUnwindSafe, pin::pin, task::Poll};
    let (method, path) = (req.method().clone(), req.uri().path().to_string());
    // A handler that panics fails its request like one that returned an error, instead of
    // dropping the connection with no answer. Nothing runs unless it panics.
    let mut fut = pin!(within_deadline(req, next, deadline, signal));
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
            tracing::error!("corten: {method} {path} panicked: {message}");
            Error::new(format!("the handler panicked: {message}")).into_response()
        }
    }
}

/// The handler's response, or an empty 503 once it has waited past `deadline`.
async fn within_deadline(req: Request, next: Next, deadline: Option<std::time::Duration>, signal: &crate::Signal) -> Response {
    let Some(limit) = deadline else {
        return next.run(req).await;
    };
    let (method, uri) = (req.method().clone(), req.uri().clone());
    match within(limit, next.run(req)).await {
        Some(res) => res,
        None => {
            signal.abort(crate::Abort::Deadline);
            tracing::warn!("corten: {method} {} passed its {limit:?} deadline", uri.path());
            StatusCode::SERVICE_UNAVAILABLE.into_response()
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
#[derive(Clone, Copy, Debug)]
pub struct ReadAgain {
    /// How long until the page is read again.
    pub after: std::time::Duration,
}

impl Render for ReadAgain {
    fn render(&self, out: &mut String) {
        use std::fmt::Write;
        let seconds = (self.after.as_secs() + u64::from(self.after.subsec_nanos() > 0)).max(1);
        let _ = write!(
            out,
            r#"<noscript data-corten-read-again="{seconds}"><meta http-equiv="refresh" content="{seconds}"></noscript>"#
        );
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
