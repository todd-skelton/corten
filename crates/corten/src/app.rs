//! [`App`]: an app's routes with the production pieces around them, configured by defaults.

use std::{
    convert::Infallible,
    future::Future,
    marker::PhantomData,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    extract::Request,
    http::{header, Extensions, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, Route},
    Router,
};
use futures_util::{stream, StreamExt};

use crate::{
    api::{Api, Mount},
    assets::Assets,
    cache::{self, Seen},
    page::{self, default_error_page, default_shell, error_page_fn, IntoShell, Shared, ShellFn},
    pwa::Pwa,
    route::{Href, Route as TypedRoute},
    security::{Applied, NonceSlot, Policy},
    signal::{self, Watch},
    view::Render,
    Error, Meta, Mode, Signal,
};

/// Puts the context's value in the request, and says whether there was one.
type InsertContext = Box<dyn FnOnce(&mut Extensions) -> bool + Send>;
type ContextFuture = Pin<Box<dyn Future<Output = InsertContext> + Send>>;
type ContextFn = Arc<dyn Fn(&HeaderMap) -> ContextFuture + Send + Sync>;
type ReadyFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;
/// A guard's check, reading the context from the request's extensions.
type GuardFn = Box<dyn Fn(&Extensions, &Guarded) -> Access + Send + Sync>;

/// The request a guard ([`App::guard`]) decides on: which route, and whether it reads or writes.
pub struct Guarded<'a> {
    uri: &'a Uri,
    method: &'a Method,
}

impl Guarded<'_> {
    /// The request's URI, for a sign-in link that brings the caller back.
    pub fn uri(&self) -> &Uri {
        self.uri
    }

    /// True for a request that may change something: every method but `GET`, `HEAD` and
    /// `OPTIONS`, the same split as the same-origin check, so a method nobody listed counts as a
    /// write. For screens anyone with one permission reads and another writes.
    pub fn writes(&self) -> bool {
        !matches!(*self.method, Method::GET | Method::HEAD | Method::OPTIONS)
    }

    /// True when the request is for the route `R`: `req.is::<Billing>()`, matched as the router
    /// matches `R::PATH`, for a rule that one route under the prefix needs.
    pub fn is<R: TypedRoute>(&self) -> bool {
        crate::route::matches(R::PATH, self.uri.path())
    }
}

/// What a guard ([`App::guard`]) decides for a request.
pub struct Access(Decision);

enum Decision {
    Allow,
    SignIn(HeaderValue),
    Refuse(Error),
}

impl Access {
    /// The handler runs.
    pub const ALLOW: Access = Access(Decision::Allow);

    /// No one is signed in, or their session ended: the caller is sent to `to`, a typed route or
    /// an `axum::http::Uri` on another site, the way [`crate::Req::redirect`] sends it (a 303,
    /// or `corten-location` for a form patch). A data client and an [`App::api`] route get a JSON
    /// 401 instead.
    pub fn sign_in(to: &(impl Href + ?Sized)) -> Access {
        match to.with_href(|href| HeaderValue::try_from(href)) {
            Ok(to) => Access(Decision::SignIn(to)),
            Err(error) => Access(Decision::Refuse(Error::new(error))),
        }
    }

    /// Someone is signed in but may not use these routes, or who they are cannot be known now:
    /// the request fails with `error` as if the handler had returned it, so the caller gets the
    /// error page in their mode or the JSON error. Usually
    /// `Access::refuse(Error::status(StatusCode::FORBIDDEN))`.
    pub fn refuse(error: Error) -> Access {
        Access(Decision::Refuse(error))
    }
}

struct Guard {
    prefix: String,
    check: GuardFn,
}

/// The context type of an [`App`] that has no [`App::context`]: nothing can read one, so a guard
/// and a shell that takes a context do not compile until `.context(..)` gives the app its type.
///
/// ```compile_fail
/// # use corten::prelude::*;
/// #[derive(Clone)]
/// struct Viewer;
/// let app = App::new(Router::new()).guard("/admin", |_: Option<&Viewer>, _| Access::ALLOW);
/// ```
pub enum NoContext {}

/// An app: its routes, plus everything around them that every production app needs, on by
/// default. `C` is the type [`App::context`] gives ([`NoContext`] until it is called), and a
/// guard or a shell reading the context takes that type or does not compile.
/// `App::new(routes).serve().await` gives:
///
/// - security headers, a CSP (Corten's inline tags allowed by hash, a nonce only when the shell
///   asks for one), and a check that refuses cross-site writes ([`Policy::new`]; change it with
///   [`App::policy`]);
/// - `Cache-Control: private, no-cache` on what the app renders, unless a handler returns a
///   [`crate::Cache`] (see [`crate::cache`]);
/// - the shell around every page ([`crate::default_shell`] until [`App::shell`]);
/// - error pages for failed requests and unknown URLs, in the request's mode, with the error
///   shown under `corten dev`;
/// - the files in `static/` (or `STATIC_DIR`), the scripts of the app's components and its
///   components' stylesheets joined into one file ([`App::components`]), at content-versioned,
///   immutable URLs, each script preloaded on the pages whose handlers use it;
/// - `/health/live` and `/health/ready`, the runtime at a versioned URL;
/// - compression of pages and static files (feature `compress`);
/// - under `corten dev` (`CORTEN_DEV=1`): open pages reload when the server restarts.
///
/// ```ignore
/// App::new(routes)
///     .context(|headers| viewer_from(headers))       // who is asking, for the shell and handlers
///     .shell(shell)                                  // the document around every page
///     .pwa(Pwa::new(Manifest::new("Shop", "Shop").icon("icons/192.png", "192x192")))
///     .serve()
///     .await
/// ```
pub struct App<C = NoContext> {
    routes: Router,
    shell: ShellFn,
    error_page: Option<Box<page::ErrorPage>>,
    context: Option<ContextFn>,
    policy: Policy,
    static_dir: Option<PathBuf>,
    pwa: Option<Pwa>,
    ready: Option<ReadyFn>,
    locales: Option<&'static [&'static str]>,
    dev: Option<bool>,
    components: Vec<crate::Component>,
    sections: Vec<(String, Vec<crate::Component>)>,
    guards: Vec<Guard>,
    unguarded: Vec<&'static str>,
    deadline: Option<Duration>,
    apis: Vec<(Mount, Router)>,
    context_type: PhantomData<fn() -> C>,
}

impl App {
    /// The app with these routes and every default.
    pub fn new(routes: Router) -> Self {
        App {
            routes,
            shell: IntoShell::<NoContext, _>::into_shell(default_shell),
            error_page: None,
            context: None,
            policy: Policy::new(),
            static_dir: None,
            pwa: None,
            ready: None,
            locales: None,
            dev: None,
            components: Vec::new(),
            sections: Vec::new(),
            guards: Vec::new(),
            unguarded: Vec::new(),
            deadline: Some(Duration::from_secs(30)),
            apis: Vec::new(),
            context_type: PhantomData,
        }
    }

    /// Who is asking: `context(headers)` runs before every page request (outside the error pages,
    /// so they see it too), and a `Some` value is put in the request extensions, where the shell's
    /// second parameter, `req.context::<C>()` and axum's `Extension<C>` find it. Typically the
    /// signed-in user, read from a session cookie. Corten's own routes (static files, the
    /// runtime, health checks) skip it, and so do [`App::api`] routes unless [`Api::context`].
    ///
    /// The function reads what it needs from the headers and returns a future that owns it:
    /// `|h| { let sid = cookie::get(h, "sid").map(str::to_owned); async move { load(sid).await } }`.
    ///
    /// It gives the app its context type, `App<C>`: call it before [`App::shell`] and
    /// [`App::guard`], which read that type. It is called once:
    ///
    /// ```compile_fail
    /// # use corten::prelude::*;
    /// let app = App::new(Router::new()).context(|_| async { Some(1) }).context(|_| async { Some(2) });
    /// ```
    pub fn context<F, Fut, C>(self, context: F) -> App<C>
    where
        F: Fn(&HeaderMap) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Option<C>> + Send + 'static,
        C: Clone + Send + Sync + 'static,
    {
        let context: ContextFn = Arc::new(move |headers| {
            let value = context(headers);
            Box::pin(async move {
                let value = value.await;
                Box::new(move |extensions: &mut Extensions| match value {
                    Some(value) => {
                        extensions.insert(value);
                        true
                    }
                    None => false,
                }) as InsertContext
            })
        });
        App {
            routes: self.routes,
            shell: self.shell,
            error_page: self.error_page,
            context: Some(context),
            policy: self.policy,
            static_dir: self.static_dir,
            pwa: self.pwa,
            ready: self.ready,
            locales: self.locales,
            dev: self.dev,
            components: self.components,
            sections: self.sections,
            guards: self.guards,
            unguarded: self.unguarded,
            deadline: self.deadline,
            apis: self.apis,
            context_type: PhantomData,
        }
    }
}

impl<C: Clone + Send + Sync + 'static> App<C> {
    /// Routes that only some callers may use: every route at `prefix` and below it, including
    /// ones added later, unknown URLs and wrong methods there; `"/"` is the whole app. `check`
    /// gets the request's context (see [`App::context`]; `None` when it found no one) and the
    /// request ([`Guarded`]: its URI, whether it writes, which route it is), and decides with
    /// [`Access`]:
    ///
    /// ```
    /// use corten::{axum::http::StatusCode, prelude::*, Error};
    ///
    /// #[derive(Clone)]
    /// struct Viewer { staff: bool, editor: bool }
    ///
    /// #[derive(Route)]
    /// #[route("/sign-in")]
    /// struct SignIn;
    ///
    /// fn app(routes: Router) -> App<Viewer> {
    ///     App::new(routes)
    ///         .context(|_| async { None::<Viewer> })
    ///         .guard("/admin", |viewer, req| match viewer {
    ///             None => Access::sign_in(&SignIn),
    ///             Some(v) if v.staff && (v.editor || !req.writes()) => Access::ALLOW,
    ///             Some(_) => Access::refuse(Error::status(StatusCode::FORBIDDEN)),
    ///         })
    /// }
    /// # let _ = app(Router::new()).router();
    /// ```
    ///
    /// It runs after the context and before the handler, so a refused request never reaches the
    /// handler or its extractors (the body is not read). Who passes is the app's model; the guard
    /// gives the answer in the request's mode: a page or a form post without JavaScript is
    /// redirected to sign in, a form patch gets `corten-location`, a data client and an
    /// [`App::api`] route get a JSON 401, and a refused caller gets the error page or the JSON
    /// error. Under a guard the context also runs for an API route without [`Api::context`].
    ///
    /// Guards may nest (`/admin` and `/admin/billing`): every guard a path is under must allow
    /// it, the outer one first. A prefix is `"/"` or a path like a section's. Corten's own routes
    /// (static files, the runtime, health, the PWA's files and offline page) and the routes
    /// marked [`App::unguarded`] are under none. The check reads the app's context type, so a
    /// guard on an app without [`App::context`], or one that takes another type, does not compile:
    ///
    /// ```compile_fail
    /// # use corten::prelude::*;
    /// #[derive(Clone)]
    /// struct Viewer;
    /// let app = App::new(Router::new())
    ///     .context(|_| async { Some(Viewer) })
    ///     .guard("/admin", |_: Option<&String>, _| Access::ALLOW);
    /// ```
    ///
    /// A sign-in answer that points at a path a guard would also send to sign in (a sign-in page
    /// under `App::guard("/", ..)` that is not [`App::unguarded`]) would loop; the request fails
    /// with a 500 that says so instead.
    pub fn guard<F>(mut self, prefix: &str, check: F) -> Self
    where
        F: Fn(Option<&C>, &Guarded) -> Access + Send + Sync + 'static,
    {
        assert!(
            prefix == "/" || plain_prefix(prefix),
            "corten: a guard prefix is \"/\" or a path like \"/admin\": letters, digits, - and _ between slashes, without a trailing slash or parameters: {prefix:?}"
        );
        assert!(!self.guards.iter().any(|g| g.prefix == prefix), "corten: the guard {prefix:?} is given twice");
        self.guards.push(Guard {
            prefix: prefix.to_string(),
            check: Box::new(move |extensions, req| check(extensions.get::<C>(), req)),
        });
        self
    }
}

impl<C> App<C> {
    /// Routes that answer only JSON (a webhook, an OAuth callback, a public API), under `prefix`:
    /// `.api("/api", api_routes)`, or `.api("/api", Api::new(api_routes).allow_origin(..))` for
    /// settings. The routes keep their full paths (`/api/items/{id}`), so a `#[derive(Route)]`
    /// struct names the same URL in the router and in links, and every path must start with
    /// `prefix` (under `corten dev` one that does not answers 500, saying so).
    ///
    /// Under the prefix every failure is JSON, `{"error":{"status","message"}}`, whatever the
    /// client accepts, and an unknown URL or a wrong method there is a JSON 404 or 405. There is no
    /// shell, no nonce and no context (unless [`Api::context`]); the deadline, the request signal,
    /// compression and the same-origin check on writes stay. See [`crate::api`]. `"/"` makes the
    /// whole app an API (Corten's own routes, like the health checks, keep their behavior).
    pub fn api(mut self, prefix: &str, api: impl Into<Api>) -> Self {
        let prefixes: Vec<String> = self.apis.iter().map(|(mount, _)| mount.prefix.clone()).collect();
        crate::api::check_prefix(prefix, &prefixes);
        let api = api.into();
        self.apis.push((Mount::new(prefix.to_string(), &api), api.routes));
        self
    }

    /// The app's components with a script, a stylesheet or a typed event
    /// (`#[component(js)]`, `#[component(css)]`, `script!`, `#[event]`, `style::tokens!`), each listed by its constant: `.components(&[TOKENS, OfferList,
    /// ListingSort])`, or a library's list, `.components(ui::COMPONENTS)`. Call it once per list;
    /// the order is the stylesheets' order, so list tokens and libraries first.
    ///
    /// Scripts are served with the static files and preloaded on the pages whose handlers use them;
    /// the stylesheets are one file, `components.css`, linked from every document's `<head>`.
    /// A component listed twice counts once.
    pub fn components(mut self, components: &[crate::Component]) -> Self {
        self.components.extend_from_slice(components);
        self
    }

    /// Components only the pages under `prefix` render: `.section("/admin", &[AdminTable,
    /// SalesChart])`. Their stylesheets are joined into a file of the section's own,
    /// `sections/admin.css`, linked after `components.css` from the pages at `/admin` and under it,
    /// so other pages do not download them. Their scripts are served as with [`App::components`].
    ///
    /// A client navigation into the section loads its stylesheet before it swaps the page in, and
    /// the stylesheet stays (its rules only match its components' classes). Sections may nest
    /// (`/admin` and `/admin/reports`): a page links every section it is under.
    ///
    /// A component with a stylesheet is listed once: one that pages in several sections render
    /// belongs in [`App::components`], and so does a token file (`tokens!`), which styles every
    /// page. Under `corten dev` a page outside a section that renders one of its components is an
    /// error that names both; a patch is checked against its own URL. Without sections there is
    /// one stylesheet, which is the right choice until a part of the app most visitors never open
    /// adds enough CSS to measure (see the styles guide).
    pub fn section(mut self, prefix: &str, components: &[crate::Component]) -> Self {
        assert!(
            plain_prefix(prefix),
            "corten: a section prefix is a path like \"/admin\": letters, digits, - and _ between slashes, without a trailing slash or parameters: {prefix:?}"
        );
        assert!(!self.sections.iter().any(|(p, _)| p == prefix), "corten: the section {prefix:?} is given twice");
        self.sections.push((prefix.to_string(), components.to_vec()));
        self
    }

    /// A route no guard covers, for every method: the sign-in page under `App::guard("/", ..)`,
    /// a password reset, a public landing page. `.unguarded::<SignIn>()`, once per route; the
    /// route is matched as the router matches `R::PATH`.
    ///
    /// It takes a route type, never a path as text:
    ///
    /// ```compile_fail
    /// let app = corten::App::new(corten::axum::Router::new()).unguarded::<&str>();
    /// ```
    pub fn unguarded<R: TypedRoute>(mut self) -> Self {
        if !self.unguarded.contains(&R::PATH) {
            self.unguarded.push(R::PATH);
        }
        self
    }

    /// How long a handler may take to answer (30 seconds unless set). Past it the client gets
    /// the 503 error page, the handler's future is dropped, and the request's [`crate::Signal`]
    /// aborts with [`crate::Abort::Deadline`] for work the handler spawned. It bounds the time to
    /// the response's start, so a stream (a feed, server-sent events) runs as long as it needs;
    /// reading the request body counts, so an app that takes large uploads raises it. `None`
    /// removes it, for an app whose handlers bound themselves.
    pub fn deadline(mut self, deadline: impl Into<Option<Duration>>) -> Self {
        self.deadline = deadline.into();
        self
    }

    /// The app's messages (a type from [`crate::i18n::messages!`]): the shell's `doc.lang()` is
    /// the locale negotiated from `Accept-Language`, as `req.t()` and `doc.t()` pick it.
    pub fn messages<M: crate::i18n::Messages>(mut self) -> Self {
        self.locales = Some(M::LOCALES);
        self
    }

    /// The document around every page: a function from [`crate::Document`] to a view, printing
    /// `{doc.head}` in `<head>` and `{doc.main}` in `<main>`. With a second parameter,
    /// `Option<C>`, it also gets the request's context (see [`App::context`]), whose type it must
    /// be:
    ///
    /// ```compile_fail
    /// # use corten::prelude::*;
    /// fn shell(doc: Document, viewer: Option<String>) -> impl Render {
    ///     html! { <html><body>{doc.main}</body></html> }
    /// }
    /// let app = App::new(Router::new()).context(|_| async { Some(1_u32) }).shell(shell);
    /// ```
    ///
    /// The shell renders on full page loads only; client navigation swaps `<main>`. Every page
    /// carries a key hashed from the context and the deploy, and when a navigation or a form
    /// answer arrives with a different key (sign-in, sign-out, a new release), the runtime
    /// reloads the document, so the shell is never stale.
    pub fn shell<M>(mut self, shell: impl IntoShell<C, M>) -> Self {
        self.shell = shell.into_shell();
        self
    }

    /// The page for failed requests and unknown URLs: a function from the status to a title (or
    /// [`Meta`]) and a view, shown inside the shell. Pages are marked `noindex`.
    pub fn error_page<F, T, V>(mut self, page: F) -> Self
    where
        F: Fn(StatusCode) -> (T, V) + Send + Sync + 'static,
        T: Into<Meta>,
        V: Render,
    {
        self.error_page = Some(error_page_fn(move |status| {
            let (meta, view) = page(status);
            (meta.into(), view)
        }));
        self
    }

    /// Dev mode (default: `CORTEN_DEV` is set, as `corten dev` does): error pages show the error,
    /// and open pages reload when the server restarts. Never on in production.
    pub fn dev(mut self, dev: bool) -> Self {
        self.dev = Some(dev);
        self
    }

    /// The security policy (default [`Policy::new`]: same-origin everything).
    pub fn policy(mut self, policy: Policy) -> Self {
        self.policy = policy;
        self
    }

    /// The static directory (default `STATIC_DIR`, else `static` in the working directory).
    pub fn assets(mut self, dir: impl Into<PathBuf>) -> Self {
        self.static_dir = Some(dir.into());
        self
    }

    /// Makes the app installable: the manifest, a generated service worker and an offline page
    /// (Corten's own unless the app routes `/offline`). Relative icon and script paths are files
    /// in the static directory.
    pub fn pwa(mut self, pwa: Pwa) -> Self {
        self.pwa = Some(pwa);
        self
    }

    /// What `/health/ready` checks (default: always ready).
    pub fn ready<F, Fut>(mut self, ready: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = bool> + Send + 'static,
    {
        self.ready = Some(Arc::new(move || Box::pin(ready())));
        self
    }

    /// The finished router: the app's routes with everything around them. For tests
    /// (`app.router().oneshot(request)`) and for serving it another way.
    pub fn router(self) -> Router {
        let dev = self.dev.unwrap_or_else(|| std::env::var_os("CORTEN_DEV").is_some());
        let sections: Vec<(&str, &[crate::Component])> = self.sections.iter().map(|(p, c)| (p.as_str(), c.as_slice())).collect();
        let assets = load_assets(self.static_dir, &self.components, &sections).map(Arc::new);
        let mut head = String::new();
        if let Some(href) = assets.as_ref().and_then(|a| a.stylesheet()) {
            head.push_str(&format!(r#"<link rel="stylesheet" href="{href}">"#));
        }
        let mut routes = self.routes;
        let mut mounts = Vec::with_capacity(self.apis.len());
        for (mount, api) in self.apis {
            routes = routes.merge(if dev { crate::api::checked(&mount.prefix, api) } else { api });
            mounts.push(mount);
        }
        // Corten's own routes: they skip the context, and an API prefix does not take them over.
        let mut own = Own { exact: vec!["/health/live", "/health/ready"], prefixes: vec!["/corten/".to_string()] };
        routes = routes.merge(crate::routes());
        let mut offline = None;
        if let Some(pwa) = self.pwa {
            let pwa = pwa.resolve(assets.as_deref());
            head.push_str(r#"<link rel="manifest" href="/manifest.webmanifest">"#);
            if let Some(color) = pwa.theme_color() {
                head.push_str(&format!(r#"<meta name="theme-color" content="{}">"#, crate::escape(color)));
            }
            if !dev {
                head.push_str(&format!(r#"<meta name="corten-sw" content="{}">"#, crate::pwa::WORKER_PATH));
            }
            offline = Some(pwa.offline_path().to_string());
            own.exact.extend([crate::pwa::MANIFEST_PATH, crate::pwa::WORKER_PATH]);
            routes = routes.merge(pwa.routes());
        }
        if dev {
            let boot = format!(
                "{:x}",
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()
            );
            head.push_str(&format!(r#"<meta name="corten-dev" content="{boot}">"#));
            routes = routes.route(
                "/corten/dev",
                get(move || {
                    let first = Event::default().retry(Duration::from_millis(250)).data(boot.clone());
                    let events = stream::once(async move { Ok::<_, std::convert::Infallible>(first) }).chain(stream::pending());
                    async move { Sse::new(events).keep_alive(KeepAlive::default()) }
                }),
            );
        }
        if let Some(assets) = &assets {
            own.prefixes.push(assets.mount_prefix());
            routes = routes.merge(assets.routes());
        }
        let ready = self.ready.unwrap_or_else(|| Arc::new(|| Box::pin(async { true })));
        routes = routes.merge(crate::health(move || ready()));

        let error_page = self.error_page.unwrap_or_else(|| error_page_fn(default_error_page));
        let mut shared = Shared::new(self.shell, error_page, assets, head);
        shared.offline = offline;
        shared.locales = self.locales;
        shared.dev = dev;
        shared.components = self.components.iter().chain(self.sections.iter().flat_map(|(_, c)| c)).map(|c| c.name).collect();
        if let Some(assets) = &shared.assets {
            for (prefix, href) in assets.section_stylesheets() {
                let index = shared.sections.len();
                let listed = self.sections.iter().filter(|(p, _)| p == prefix).flat_map(|(_, c)| c);
                shared.section_of.extend(listed.filter(|c| c.style.is_some()).map(|c| (c.name, index)));
                let link = format!(r#"<link rel="stylesheet" href="{}" data-corten-style>"#, crate::escape(href));
                shared.sections.push((prefix.clone(), link));
            }
        }
        let policy = Applied::new(self.policy, &shared.import_map);
        // A path's guards answer outer first: a prefix inside another is longer.
        let mut guards = self.guards;
        guards.sort_by_key(|guard| guard.prefix.len());
        let layer = Arc::new(Layer {
            shared: Arc::new(shared),
            context: self.context,
            guards,
            unguarded: self.unguarded,
            deadline: self.deadline,
            policy,
            mounts,
            own,
        });
        routes.layer(Wrap(layer))
    }

    /// Serves the app on `HOST`:`PORT` (default `127.0.0.1:3000`) until Ctrl-C or `SIGTERM`,
    /// ending live streams first so shutdown is quick. With feature `log`, prints logs to stderr
    /// (`RUST_LOG` style filtering is the app's choice: set its own subscriber first).
    pub async fn serve(self) -> std::io::Result<()> {
        #[cfg(feature = "log")]
        let _ = tracing_subscriber::fmt().with_target(false).try_init();
        let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let port: u16 = match std::env::var("PORT") {
            Ok(port) => port.parse().map_err(|_| std::io::Error::other(format!("PORT is not a port number: {port}")))?,
            Err(_) => 3000,
        };
        let router = self.router();
        let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await?;
        let shown = if host == "0.0.0.0" { "localhost" } else { &host };
        println!("corten: listening on http://{shown}:{}", listener.local_addr()?.port());
        axum::serve(listener, router).with_graceful_shutdown(crate::shutdown_signal()).await
    }
}

/// The paths of Corten's own routes.
struct Own {
    exact: Vec<&'static str>,
    prefixes: Vec<String>,
}

impl Own {
    fn contains(&self, path: &str) -> bool {
        self.exact.contains(&path) || self.prefixes.iter().any(|prefix| path.starts_with(prefix.as_str()))
    }
}

/// A section's or a guard's prefix: `/admin`, `/admin/reports`.
fn plain_prefix(prefix: &str) -> bool {
    prefix.len() > 1
        && prefix.starts_with('/')
        && prefix[1..]
            .split('/')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
}

/// Puts [`Layer`] around each route. A tower layer of its own rather than axum's `from_fn`, which
/// boxes the rest of the stack again for every request (`Next`) on top of the future.
#[derive(Clone)]
struct Wrap(Arc<Layer>);

impl tower_layer::Layer<Route> for Wrap {
    type Service = Wrapped;

    fn layer(&self, route: Route) -> Wrapped {
        Wrapped { layer: self.0.clone(), route }
    }
}

#[derive(Clone)]
struct Wrapped {
    layer: Arc<Layer>,
    route: Route,
}

impl tower_service::Service<Request> for Wrapped {
    type Response = Response;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>>;

    // A route is always ready: it clones its service for each call.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let (layer, next) = (self.layer.clone(), Next::Route(self.route.clone()));
        Box::pin(async move { Ok(layer.handle(req, next).await) })
    }
}

/// The rest of the stack under [`Layer`]: the route, then the handler; or the answer a guard gave
/// in their place, boxed so every request's future stays as small as without guards.
pub(crate) enum Next {
    Route(Route),
    Refused(Box<Response>),
}

impl Next {
    pub(crate) async fn run(self, req: Request) -> Response {
        match self {
            Next::Route(mut route) => match tower_service::Service::call(&mut route, req).await {
                Ok(res) => res,
                Err(never) => match never {},
            },
            Next::Refused(res) => *res,
        }
    }
}

/// Everything `App` puts around a request, as one middleware: one boxed future per request,
/// where separate security, compression and error layers cost one each.
struct Layer {
    shared: Arc<Shared>,
    context: Option<ContextFn>,
    guards: Vec<Guard>,
    unguarded: Vec<&'static str>,
    deadline: Option<Duration>,
    policy: Applied,
    mounts: Vec<Mount>,
    own: Own,
}

impl Layer {
    async fn handle(&self, req: Request, next: Next) -> Response {
        #[cfg(feature = "compress")]
        let coding = crate::compress::negotiate(req.headers());
        let res = self.secure(req, next).await;
        #[cfg(feature = "compress")]
        let res = crate::compress::finish(res, coding).await;
        res
    }

    /// The security policy around the request: the same-origin check first, the headers last.
    async fn secure(&self, mut req: Request, next: Next) -> Response {
        let path = req.uri().path();
        let own = self.own.contains(path);
        let mount = if own { None } else { self.mounts.iter().find(|mount| mount.contains(path)) };
        let guarded = !own && !self.guards.is_empty() && self.guarded(path);
        if let Some(mount) = mount {
            if let Some(mut res) = mount.preflight(req.method(), req.headers()) {
                self.policy.headers(&mut res, None);
                res.headers_mut().insert(header::CACHE_CONTROL, cache::PRIVATE);
                return res;
            }
        }
        let origins = mount.map_or(&[][..], |mount| mount.origins());
        if let Some(mut refused) = self.policy.refuse(req.method(), req.headers(), origins) {
            if mount.is_some() {
                refused = crate::api::json_failure(refused, false).await;
            }
            self.policy.headers(&mut refused, None);
            refused.headers_mut().insert(header::CACHE_CONTROL, cache::PRIVATE);
            return refused;
        }
        let cors = mount.and_then(|mount| mount.allowed_origin(req.headers()));
        // A page's nonce is made only if its HTML asks for one; an API route never has one.
        let nonce = mount.is_none().then(NonceSlot::default);
        if let Some(nonce) = &nonce {
            req.extensions_mut().insert(nonce.clone());
        }
        // A guard reads the context, so it runs under one even for an API route without it.
        let context = mount.map_or(!own, |mount| mount.context) || guarded;
        let credentials = cache::credentials(req.headers());
        let path = self.shared.dev.then(|| req.uri().path().to_string());
        let (mut res, from_context, mode) = self.run(req, next, context, guarded, mount.is_some()).await;
        // First: the security headers reserve room for `Cache-Control` and `Vary` too.
        self.policy.headers(&mut res, nonce.as_ref());
        let seen = Seen {
            personal: credentials || from_context,
            // A page route answers several forms at one URL; only the full document may be shared.
            partial: mount.is_none() && mode != Mode::Document,
            nonce: nonce.as_ref().is_some_and(|slot| slot.taken().is_some()),
            dev: self.shared.dev,
        };
        // Corten's own routes (assets, the runtime) are settled the same way: they say what they
        // are, and anything that does not gets the default.
        cache::settle(&mut res, seen, path.as_deref().unwrap_or(""));
        if let Some(mount) = mount {
            mount.cors(cors, &mut res);
        }
        res
    }

    /// The response, whether the context function returned a value for the request, and the
    /// request's mode.
    async fn run(&self, mut req: Request, next: Next, context: bool, guarded: bool, api: bool) -> (Response, bool, Mode) {
        // Dropped with this future if the client leaves first; then the response's.
        let watch = Watch(Signal::default());
        req.extensions_mut().insert(watch.0.clone());
        let mut found = false;
        if let Some(run) = self.context.as_ref().filter(|_| context) {
            let insert = run(req.headers()).await;
            found = insert(req.extensions_mut());
        }
        req.extensions_mut().insert(self.shared.clone());
        let mode = page::mode_of(req.headers());
        let refused = if guarded { self.refusal(&req, mode, api) } else { None };
        let next = refused.map_or(next, |answer| Next::Refused(Box::new(answer)));
        let res = page::render_errors(&self.shared, req, next, self.deadline, &watch.0, api, mode).await;
        (signal::follow(res, watch), found, mode)
    }

    /// The answer of the first guard over the request's path that does not allow it.
    fn refusal(&self, req: &Request, mode: Mode, api: bool) -> Option<Response> {
        let path = req.uri().path();
        let asked = Guarded { uri: req.uri(), method: req.method() };
        let (prefix, Access(decision)) = self.decide(req.extensions(), &asked)?;
        tracing::debug!("corten: {} {path} refused by App::guard({prefix:?})", req.method());
        let error = match decision {
            Decision::Allow => return None,
            Decision::SignIn(_) if api || mode == Mode::Data => Error::status(StatusCode::UNAUTHORIZED),
            Decision::SignIn(to) if self.loops(req.extensions(), &to) => Error::new(format!(
                "App::guard({prefix:?}) sends {path} to sign in at {}, which a guard sends to sign in too; \
                 mark the sign-in route with App::unguarded",
                to.to_str().unwrap_or("")
            )),
            Decision::SignIn(to) => return Some(page::redirect(mode, to)),
            Decision::Refuse(error) => error,
        };
        let mut res = error.into_response();
        // Under `corten dev` the error page and a JSON client say which guard answered.
        if self.shared.dev {
            if let Some(crate::Failed(message)) = res.extensions_mut().get_mut::<crate::Failed>() {
                *message = Arc::from(format!("{message} (refused by App::guard({prefix:?}))"));
            }
        }
        Some(res)
    }

    /// True when `path` is under a guard: under a guard's prefix, and not the offline page or an
    /// [`App::unguarded`] route.
    fn guarded(&self, path: &str) -> bool {
        self.guards.iter().any(|g| crate::api::under(&g.prefix, path))
            && self.shared.offline.as_deref() != Some(path)
            && !self.unguarded.iter().any(|pattern| crate::route::matches(pattern, path))
    }

    /// The first guard over the request that does not allow it, outer first, and its answer.
    fn decide(&self, extensions: &Extensions, asked: &Guarded) -> Option<(&str, Access)> {
        let path = asked.uri.path();
        self.guards
            .iter()
            .filter(|guard| crate::api::under(&guard.prefix, path))
            .map(|guard| (guard.prefix.as_str(), (guard.check)(extensions, asked)))
            .find(|(_, access)| !matches!(access.0, Decision::Allow))
    }

    /// True when following a sign-in answer to `to` (a path in this app) would be sent to sign
    /// in again: the browser would loop until it gives up.
    fn loops(&self, extensions: &Extensions, to: &HeaderValue) -> bool {
        let Some(uri) =
            to.to_str().ok().filter(|to| to.starts_with('/') && !to.starts_with("//")).and_then(|to| to.parse::<Uri>().ok())
        else {
            return false;
        };
        if !self.guarded(uri.path()) || self.own.contains(uri.path()) {
            return false;
        }
        let asked = Guarded { uri: &uri, method: &Method::GET };
        matches!(self.decide(extensions, &asked), Some((_, Access(Decision::SignIn(_)))))
    }
}

fn load_assets(
    dir: Option<PathBuf>,
    components: &[crate::Component],
    sections: &[(&str, &[crate::Component])],
) -> Option<Assets> {
    let explicit = dir.is_some() || std::env::var_os("STATIC_DIR").is_some();
    let dir = dir.or_else(|| std::env::var_os("STATIC_DIR").map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("static"));
    if !dir.is_dir() {
        assert!(!explicit, "corten: the static directory {} does not exist", dir.display());
        if components.is_empty() && sections.is_empty() {
            return None;
        }
    }
    let found = dir.is_dir().then_some(dir.as_path());
    match Assets::build_with(found, components, sections, "/assets") {
        Ok(assets) => Some(assets),
        Err(error) => panic!("corten: could not load the static files in {} and the components: {error}", dir.display()),
    }
}
