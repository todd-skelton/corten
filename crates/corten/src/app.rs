//! [`App`]: an app's routes with the production pieces around them, configured by defaults.

use std::{
    any::{type_name, TypeId},
    convert::Infallible,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use axum::{
    extract::Request,
    http::{header, Extensions, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Response,
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
    security::{Applied, NonceSlot, Policy},
    signal::{self, Watch},
    view::Render,
    Meta, Mode, Signal,
};

/// Puts the context's value in the request, and says whether there was one.
type InsertContext = Box<dyn FnOnce(&mut Extensions) -> bool + Send>;
type ContextFuture = Pin<Box<dyn Future<Output = InsertContext> + Send>>;
type ContextFn = Arc<dyn Fn(&HeaderMap) -> ContextFuture + Send + Sync>;
type ReadyFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

/// An app: its routes, plus everything around them that every production app needs, on by
/// default. `App::new(routes).serve().await` gives:
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
///     .shell(shell)                                  // the document around every page
///     .context(|headers| viewer_from(headers))       // who is asking, for the shell and handlers
///     .pwa(Pwa::new(Manifest::new("Shop", "Shop").icon("icons/192.png", "192x192")))
///     .serve()
///     .await
/// ```
pub struct App {
    routes: Router,
    shell: ShellFn,
    error_page: Option<Box<page::ErrorPage>>,
    context: Option<(ContextFn, TypeId, &'static str)>,
    policy: Policy,
    static_dir: Option<PathBuf>,
    pwa: Option<Pwa>,
    ready: Option<ReadyFn>,
    locales: Option<&'static [&'static str]>,
    dev: Option<bool>,
    components: Vec<crate::Component>,
    sections: Vec<(String, Vec<crate::Component>)>,
    deadline: Option<Duration>,
    apis: Vec<(Mount, Router)>,
}

impl App {
    /// The app with these routes and every default.
    pub fn new(routes: Router) -> Self {
        App {
            routes,
            shell: default_shell.into_shell(),
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
            deadline: Some(Duration::from_secs(30)),
            apis: Vec::new(),
        }
    }

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
        let valid = prefix.len() > 1
            && prefix.starts_with('/')
            && prefix[1..]
                .split('/')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        assert!(
            valid,
            "corten: a section prefix is a path like \"/admin\": letters, digits, - and _ between slashes, without a trailing slash or parameters: {prefix:?}"
        );
        assert!(!self.sections.iter().any(|(p, _)| p == prefix), "corten: the section {prefix:?} is given twice");
        self.sections.push((prefix.to_string(), components.to_vec()));
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
    /// `Option<C>`, it also gets the request's context (see [`App::context`]).
    ///
    /// The shell renders on full page loads only; client navigation swaps `<main>`. Every page
    /// carries a key hashed from the context and the deploy, and when a navigation or a form
    /// answer arrives with a different key (sign-in, sign-out, a new release), the runtime
    /// reloads the document, so the shell is never stale.
    pub fn shell<M>(mut self, shell: impl IntoShell<M>) -> Self {
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

    /// Who is asking: `context(headers)` runs before every page request (outside the error pages,
    /// so they see it too), and a `Some` value is put in the request extensions, where the shell's
    /// second parameter, `req.context::<C>()` and axum's `Extension<C>` find it. Typically the
    /// signed-in user, read from a session cookie. Corten's own routes (static files, the
    /// runtime, health checks) skip it, and so do [`App::api`] routes unless [`Api::context`].
    ///
    /// The function reads what it needs from the headers and returns a future that owns it:
    /// `|h| { let sid = cookie::get(h, "sid").map(str::to_owned); async move { load(sid).await } }`.
    pub fn context<F, Fut, C>(mut self, context: F) -> Self
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
        self.context = Some((context, TypeId::of::<C>(), type_name::<C>()));
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
        if let (Some((_, given, given_name)), Some((expected, expected_name))) = (&self.context, self.shell.context_type) {
            assert!(
                *given == expected,
                "corten: the shell takes Option<{expected_name}> but App::context gives Option<{given_name}>"
            );
        }
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
        let layer = Arc::new(Layer {
            shared: Arc::new(shared),
            context: self.context.map(|(f, _, _)| f),
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
        let (layer, next) = (self.layer.clone(), Next(self.route.clone()));
        Box::pin(async move { Ok(layer.handle(req, next).await) })
    }
}

/// The rest of the stack under [`Layer`]: the route, then the handler.
pub(crate) struct Next(Route);

impl Next {
    pub(crate) async fn run(mut self, req: Request) -> Response {
        match tower_service::Service::call(&mut self.0, req).await {
            Ok(res) => res,
            Err(never) => match never {},
        }
    }
}

/// Everything `App` puts around a request, as one middleware: one boxed future per request,
/// where separate security, compression and error layers cost one each.
struct Layer {
    shared: Arc<Shared>,
    context: Option<ContextFn>,
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
        let context = mount.map_or(!own, |mount| mount.context);
        let credentials = cache::credentials(req.headers());
        let path = self.shared.dev.then(|| req.uri().path().to_string());
        let (mut res, from_context, mode) = self.run(req, next, context, mount.is_some()).await;
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
    async fn run(&self, mut req: Request, next: Next, context: bool, api: bool) -> (Response, bool, Mode) {
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
        let res = page::render_errors(&self.shared, req, next, self.deadline, &watch.0, api, mode).await;
        (signal::follow(res, watch), found, mode)
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
