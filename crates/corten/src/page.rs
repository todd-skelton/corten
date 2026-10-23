//! Pages, patches and the shell: what a handler returns and how it reaches each kind of client.

use std::{
    any::Any,
    convert::Infallible,
    future::Future,
    hash::{Hash, Hasher},
    sync::{Arc, LazyLock},
};

use axum::{
    extract::{FromRequestParts, Request},
    http::{header, request::Parts, Extensions, HeaderMap, HeaderName, HeaderValue, StatusCode, Uri},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{
    assets::Assets,
    security::NonceSlot,
    view::{html, Render},
    Error, Meta, Result,
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

/// The document around a page, given to the app's shell function (see [`crate::App::shell`]).
///
/// Print `{doc.head}` inside `<head>` before any other script and `{doc.main}` inside `<main>`.
pub struct Document {
    /// Title, the page's [`Meta`] tags, the runtime, script preloads, the manifest. Print it
    /// before any other module script.
    pub head: Head,
    /// The page's HTML, already rendered.
    pub main: Main,
    lang: &'static str,
    path: String,
    nonce: Option<NonceSlot>,
    shared: Arc<Shared>,
}

impl Document {
    /// The page title, as in `<title>`.
    pub fn title(&self) -> &str {
        &self.head.title
    }

    /// The request path, for links that mark the current section.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The negotiated locale (`"en"` without [`crate::App::messages`]), for `<html lang>`.
    pub fn lang(&self) -> &str {
        self.lang
    }

    /// True when `href` (a typed route) is the current page, or with `section` any page
    /// under it: the rule the runtime applies to `<a data-nav>` (`data-nav="section"`) after each
    /// navigation.
    pub fn is_current(&self, href: &(impl crate::route::Href + ?Sized), section: bool) -> bool {
        href.with_href(|href| is_current(&self.path, href, section))
    }

    /// `aria-current="page"` for a nav link when [`Document::is_current`]:
    /// `<a href={About} data-nav aria-current={doc.current(&About)}>`.
    pub fn current(&self, href: &(impl crate::route::Href + ?Sized)) -> Option<&'static str> {
        self.is_current(href, false).then_some("page")
    }

    /// The app's messages in this document's locale, for text in the shell.
    pub fn t<M: crate::i18n::Messages>(&self) -> M {
        M::for_locale(self.lang)
    }

    /// This request's CSP nonce, for the shell's own inline scripts. Empty without a policy.
    ///
    /// Asking for it makes one and puts it in the CSP, so the page is different bytes on every
    /// request and is never cached for other users (see [`crate::App`], caching). Prefer a
    /// script file from the static directory.
    pub fn nonce(&self) -> &str {
        self.nonce.as_ref().map_or("", |slot| &slot.get().0)
    }

    /// The versioned URL of a static file: `doc.asset("style.css")`. A missing file is a bug:
    /// it panics in debug builds, and in release the error is logged and the page still renders
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
}

/// The tags Corten owns in `<head>`, rendered once per document.
pub struct Head {
    title: String,
    html: String,
}

impl Render for Head {
    fn render(&self, out: &mut String) {
        out.push_str(&self.html)
    }
}

/// The page's rendered HTML, for `<main>`.
pub struct Main(String);

impl Render for Main {
    fn render(&self, out: &mut String) {
        out.push_str(&self.0)
    }
}

#[cfg(test)]
pub(crate) fn is_current_for_tests(path: &str, href: &str, section: bool) -> bool {
    is_current(path, href, section)
}

fn is_current(path: &str, href: &str, section: bool) -> bool {
    let href = href.split(['?', '#']).next().unwrap_or(href);
    path == href || (section && path.strip_prefix(href.trim_end_matches('/')).is_some_and(|rest| rest.starts_with('/')))
}

/// The shell's per-request context, taken from the request extensions when the request arrives.
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

type RenderShell = dyn Fn(Document, Option<&(dyn Any + Send + Sync)>) -> String + Send + Sync;
type CaptureContext = dyn Fn(&Extensions) -> Option<(u64, Box<dyn Any + Send + Sync>)> + Send + Sync;

/// A shell function with its context type erased.
pub struct ShellFn {
    render: Box<RenderShell>,
    capture: Box<CaptureContext>,
    /// The context type the shell takes, checked against [`crate::App::context`].
    pub(crate) context_type: Option<(std::any::TypeId, &'static str)>,
}

/// A function that renders the document around every page. Implemented for
/// `fn(Document) -> impl Render` and, for a shell that shows per-request data such as the
/// signed-in user, `fn(Document, Option<C>) -> impl Render`, where `C` is whatever the app's
/// middleware put in the request extensions (see [`crate::App::shell`]).
pub trait IntoShell<Marker>: Send + Sync + 'static {
    fn into_shell(self) -> ShellFn;
}

impl<F, V> IntoShell<fn(Document) -> V> for F
where
    F: Fn(Document) -> V + Send + Sync + 'static,
    V: Render,
{
    fn into_shell(self) -> ShellFn {
        ShellFn { render: Box::new(move |doc, _| self(doc).to_html()), capture: Box::new(|_| None), context_type: None }
    }
}

impl<F, V, C> IntoShell<fn(Document, Option<C>) -> V> for F
where
    F: Fn(Document, Option<C>) -> V + Send + Sync + 'static,
    V: Render,
    C: Clone + Hash + Send + Sync + 'static,
{
    fn into_shell(self) -> ShellFn {
        ShellFn {
            render: Box::new(move |doc, context| {
                let context = context.and_then(|c| c.downcast_ref::<C>()).cloned();
                self(doc, context).to_html()
            }),
            capture: Box::new(|extensions| {
                let context = extensions.get::<C>()?;
                let mut hasher = std::hash::DefaultHasher::new();
                context.hash(&mut hasher);
                Some((hasher.finish(), Box::new(context.clone())))
            }),
            context_type: Some((std::any::TypeId::of::<C>(), std::any::type_name::<C>())),
        }
    }
}

/// The shell used when the app sets none: a plain document that links `style.css` when the
/// static directory has one.
pub fn default_shell(doc: Document) -> impl Render {
    let style = doc.has_asset("style.css").then(|| doc.asset("style.css"));
    html! {
        <!DOCTYPE html>
        <html lang={doc.lang()}>
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                if let Some(href) = &style { <link rel="stylesheet" href={href}> }
                {doc.head}
            </head>
            <body><main>{doc.main}</main></body>
        </html>
    }
}

/// The error page used when the app sets none.
pub fn default_error_page(status: StatusCode) -> (Meta, impl Render) {
    let (title, text) = match status {
        StatusCode::NOT_FOUND => ("Not found", "There is nothing at this address."),
        StatusCode::FORBIDDEN => ("Not allowed", "You do not have access to this page."),
        StatusCode::UNAUTHORIZED => ("Sign in needed", "Sign in to see this page."),
        _ => ("Something went wrong", "Please try again."),
    };
    (
        Meta::from(title),
        html! {
            <h1>{title}</h1>
            <p>{text}</p>
        },
    )
}

pub(crate) type ErrorPage = dyn Fn(StatusCode) -> (Meta, String) + Send + Sync;

/// Everything a request needs from the app: the shell, the error page, where scripts live.
pub(crate) struct Shared {
    pub shell: ShellFn,
    pub error_page: Box<ErrorPage>,
    pub assets: Option<Arc<Assets>>,
    pub scripts_dir: String,
    /// The import map's JSON: the runtime, Solid, the app's typed events.
    pub import_map: String,
    /// Extra `<head>` tags from the app's configuration (manifest link, theme color, service
    /// worker), rendered once.
    pub head_extra: String,
    /// Hash of what the shell shows that a navigation cannot update: runtime and asset versions.
    pub key_base: u64,
    /// The PWA's offline page path; Corten answers it when the app has no route for it.
    pub offline: Option<String>,
    /// The app's message catalogs, negotiated per request by [`Req::t`].
    pub locales: Option<&'static [&'static str]>,
    /// Dev mode: error pages show the error.
    pub dev: bool,
    /// The names of the components the app lists (`App::components`).
    pub components: std::collections::HashSet<&'static str>,
}

impl Shared {
    pub(crate) fn new(shell: ShellFn, error_page: Box<ErrorPage>, assets: Option<Arc<Assets>>, head_extra: String) -> Self {
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
            shell,
            error_page,
            assets,
            scripts_dir,
            import_map,
            head_extra,
            key_base: hasher.finish(),
            offline: None,
            locales: None,
            dev: false,
            components: Default::default(),
        }
    }
}

/// Used by handlers that run without an [`crate::App`] (unit tests of a bare router).
static DEFAULT: LazyLock<Arc<Shared>> =
    LazyLock::new(|| Arc::new(Shared::new(default_shell.into_shell(), error_page_fn(default_error_page), None, String::new())));

pub(crate) fn error_page_fn<F, V>(page: F) -> Box<ErrorPage>
where
    F: Fn(StatusCode) -> (Meta, V) + Send + Sync + 'static,
    V: Render,
{
    Box::new(move |status| {
        let (meta, view) = page(status);
        (meta.noindex(), view.to_html())
    })
}

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
    /// Nothing here allocates unless the shell takes a context (which is copied).
    pub(crate) fn new(headers: &HeaderMap, extensions: &Extensions, uri: &Uri, shared: Arc<Shared>) -> Self {
        let (hash, context) = match (shared.shell.capture)(extensions) {
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

    /// The shell's context for this request (what the app's middleware inserted), if any:
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

    fn lang(&self) -> &'static str {
        match self.shared.locales {
            Some(locales) => locales[crate::i18n::negotiate(locales, self.accept_language())],
            None => "en",
        }
    }

    /// A page: the whole document for a browser, only the page for a client navigation. `meta`
    /// is the title (`"About"`) or a [`Meta`] with the head tags.
    pub fn page(&self, meta: impl Into<Meta>, view: impl Render) -> Result {
        self.page_with_status(StatusCode::OK, meta, view)
    }

    pub fn page_with_status(&self, status: StatusCode, meta: impl Into<Meta>, view: impl Render) -> Result {
        let meta = meta.into();
        if matches!(self.mode, Mode::Navigation | Mode::Action) {
            // The runtime swaps the page into <main>, so render it straight after the title.
            let mut body = String::with_capacity(16 * 1024);
            swap_prefix(&meta, &mut body);
            self.render_into(view, &mut body)?;
            return Ok(self.finish(html_response(status, body, Some(PAGE_HEADER))));
        }
        let mut main = String::new();
        let modules = self.render_into(view, &mut main)?;
        Ok(self.respond_with(status, meta, main, &modules))
    }

    /// Renders `view` and returns the script modules its handlers use, for the page to preload.
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
        Ok(modules)
    }

    /// The response for already-rendered `<main>` HTML.
    fn respond(&self, status: StatusCode, meta: Meta, main: String) -> Response {
        self.respond_with(status, meta, main, &[])
    }

    /// The same, preloading the script modules `modules`.
    fn respond_with(&self, status: StatusCode, meta: Meta, main: String, modules: &[&'static str]) -> Response {
        let body = match self.mode {
            // The runtime swaps the page into <main> and the head tags into <head>; an action
            // that answers with a page (a form with errors, say) is swapped the same way.
            Mode::Navigation | Mode::Action => {
                let mut body = String::with_capacity(main.len() + 256);
                swap_prefix(&meta, &mut body);
                body.push_str(&main);
                body
            }
            Mode::Document | Mode::Data => {
                let head = self.head(&meta, modules);
                let doc = Document {
                    head: Head { title: meta.title().to_string(), html: head },
                    main: Main(main),
                    lang: self.lang(),
                    path: self.uri.path().to_string(),
                    nonce: self.nonce.clone(),
                    shared: self.shared.clone(),
                };
                let context = self.captured.context.as_deref();
                (self.shared.shell.render)(doc, context.map(|c| c as &(dyn Any + Send + Sync)))
            }
        };
        // Only a swap says it is one: a document a cache gave to a navigation fetch by mistake
        // makes the runtime load the URL in full.
        let kind = matches!(self.mode, Mode::Navigation | Mode::Action).then_some(PAGE_HEADER);
        self.finish(html_response(status, body, kind))
    }

    /// Adds the shell key, which tells the runtime whether the shell is still current.
    fn finish(&self, mut res: Response) -> Response {
        res.headers_mut().insert(SHELL_HEADER, self.captured.key());
        res.headers_mut().insert(header::VARY, self.vary());
        res
    }

    fn vary(&self) -> HeaderValue {
        HeaderValue::from_static(if self.shared.locales.is_some() { VARY_LANGUAGE } else { VARY })
    }

    fn head(&self, meta: &Meta, modules: &[&'static str]) -> String {
        use std::fmt::Write;
        let mut out = String::with_capacity(1024);
        out.push_str("<title>");
        crate::view::escape_into(meta.title(), &mut out);
        out.push_str("</title>");
        out.push_str(meta.tags());
        let dir = &self.shared.scripts_dir;
        let _ = write!(
            out,
            r#"<meta name="corten-scripts" content="{dir}"><meta name="corten-shell" content="{:016x}">"#,
            self.captured.key
        );
        out.push_str(&self.shared.head_extra);
        let _ = write!(out, r#"<script type="importmap">{}</script>"#, self.shared.import_map);
        let _ = write!(out, r#"<script type="module" src="{}"></script>"#, crate::runtime_path());
        // The modules the page's handlers call, fetched with the runtime rather than on first use.
        let mut preloaded = false;
        for name in modules.iter().filter(|m| self.shared.assets.as_ref().is_some_and(|a| a.has_script(m))) {
            let _ = write!(out, r#"<link rel="modulepreload" href="{dir}{name}.js">"#);
            preloaded = true;
        }
        // A handler module may import the views (corten/views): fetched with it, not after it.
        if let Some(views) = self.shared.assets.as_ref().filter(|_| preloaded).and_then(|a| a.views()) {
            let _ = write!(out, r#"<link rel="modulepreload" href="{views}">"#);
        }
        out
    }

    /// [`Req::page`] for a page other clients read too: a browser gets HTML as usual, and a
    /// [`Mode::Data`] request gets `{"title": ..., "data": data}`. `data` is the screen's view
    /// model, so a native screen or an agent sees exactly what the view shows; `view` is only
    /// rendered for browsers.
    #[cfg(feature = "json")]
    pub fn screen<T: serde::Serialize>(&self, meta: impl Into<Meta>, data: &T, view: impl Render) -> Result {
        self.screen_with_status(StatusCode::OK, meta, data, view)
    }

    #[cfg(feature = "json")]
    pub fn screen_with_status<T: serde::Serialize>(
        &self,
        status: StatusCode,
        meta: impl Into<Meta>,
        data: &T,
        view: impl Render,
    ) -> Result {
        let meta = meta.into();
        if self.mode != Mode::Data {
            return self.page_with_status(status, meta, view);
        }
        #[derive(serde::Serialize)]
        struct Screen<'a, T> {
            title: &'a str,
            data: &'a T,
        }
        let body = serde_json::to_string(&Screen { title: meta.title(), data })?;
        let headers = [(header::CONTENT_TYPE, HeaderValue::from_static("application/json")), (header::VARY, self.vary())];
        Ok((status, headers, body).into_response())
    }

    /// Answers a `<form data-patch>` submit with fragments. Every top-level element in `view`
    /// must carry an `id`; the runtime replaces the element with that `id` on the page.
    pub fn patch(&self, view: impl Render) -> Result {
        self.patch_with_status(StatusCode::OK, view)
    }

    /// Same as [`Req::patch`] with another status.
    pub fn patch_with_status(&self, status: StatusCode, view: impl Render) -> Result {
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
    pub async fn invalid<F, Fut>(&self, errors: &crate::Errors, form: impl Render, page: F) -> Result
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
        Ok(match self.mode {
            Mode::Action => (StatusCode::NO_CONTENT, [(LOCATION_HEADER, value)]).into_response(),
            _ => (StatusCode::SEE_OTHER, [(header::LOCATION, value)]).into_response(),
        })
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
        let view = html! {
            <h1>"You are offline"</h1>
            <p>"Check your connection and try again."</p>
        };
        return req.respond(StatusCode::OK, Meta::new("Offline").noindex(), view.to_html());
    }
    let (meta, mut main) = (shared.error_page)(status);
    if let Some(crate::Failed(message)) = failed.filter(|_| shared.dev) {
        // Dev mode only: the error itself, under the page the user would see.
        let detail = html! { <pre class="corten-error">{message}</pre> };
        detail.render(&mut main);
    }
    let mut res = req.respond(status, meta, main);
    if let Some(allow) = allow {
        res.headers_mut().insert(header::ALLOW, allow);
    }
    res
}

/// The handler's response, or an empty 503 once it has waited past `deadline`.
async fn run(req: Request, next: Next, deadline: Option<std::time::Duration>, signal: &crate::Signal) -> Response {
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

/// `<title>` and the head tags, which the runtime moves into `<head>` when it swaps a page.
fn swap_prefix(meta: &Meta, out: &mut String) {
    out.push_str("<title>");
    crate::view::escape_into(meta.title(), out);
    out.push_str("</title>");
    out.push_str(meta.tags());
}

pub(crate) fn html_response(status: StatusCode, body: String, kind: Option<HeaderName>) -> Response {
    let mut res = (
        status,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/html; charset=utf-8")),
            (header::VARY, HeaderValue::from_static(VARY)),
        ],
        body,
    )
        .into_response();
    if let Some(kind) = kind {
        res.headers_mut().insert(kind, HeaderValue::from_static("1"));
    }
    res
}
