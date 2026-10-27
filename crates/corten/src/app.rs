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
    http::header,
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
    middleware::Middleware,
    page::{self, default_error_page, error_page_fn, Asked, Credentials, ErrorPageFn, Failure, Shared},
    pwa::Pwa,
    route::{Methods, Route as TypedRoute, Routes},
    security::{Applied, NonceSlot, Policy},
    signal::{self, Watch},
    Mode, Signal,
};

type ReadyFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

/// A part of an app under the prefix `P` (a route's path, like `Admin::PATH`), filled in by the
/// function given to [`App::area`]: its routes and the components only its pages render. Who may
/// use its routes is in their handlers' types: a routes module with `caller = Staff` holds every
/// handler in it to taking `Caller<Staff>` ([`crate::routes`]).
///
/// ```
/// use corten::prelude::*;
///
/// #[derive(Route)]
/// #[route("/admin")]
/// struct Admin;
///
/// #[derive(Route)]
/// #[route("/admin/users/{id}")]
/// struct AdminUser<'a> { id: &'a str }
///
/// async fn dashboard(req: Req) -> HtmlResult {
///     req.page(html! { <Layout req={&req} title="Admin"><h1>"Admin"</h1></Layout> })
/// }
/// # #[component]
/// # fn Layout(req: &Req, title: &str, children: impl Render) -> impl Document {
/// #     html! { <html lang={req.lang()}><head><title>{title}</title><Corten /></head><body><main>{children}</main></body></html> }
/// # }
///
/// fn app(pages: Routes) -> App {
///     App::new(pages).area::<Admin>(|admin| admin.route::<Admin>(get(dashboard)).route::<AdminUser>(get(dashboard)))
/// }
/// # let _ = app(Routes::new()).router();
/// ```
///
/// A library that ships an area is a function, `pub fn admin(area: Area<Admin>) -> Area<Admin>`,
/// that the app passes to [`App::area`].
pub struct Area<P> {
    routes: Router,
    components: Vec<crate::Component>,
    types: PhantomData<fn() -> P>,
}

impl<P: TypedRoute> Area<P> {
    /// A route of the area, at `R::PATH`: `.route::<AdminUser>(get(user).post(save))`, with
    /// `.with_state(state)` on the [`Methods`] when its handlers take `State`. `R`'s path is at
    /// or under the area's prefix, or the app does not build, so no route of the area is served
    /// outside its stylesheet:
    ///
    /// ```compile_fail,E0080
    /// # use corten::prelude::*;
    /// #[derive(Route)]
    /// #[route("/admin")]
    /// struct Admin;
    ///
    /// #[derive(Route)]
    /// #[route("/admn/users")]
    /// struct Users;
    ///
    /// let app = App::new(Routes::new()).area::<Admin>(|admin| admin.route::<Users>(get(|| async { "users" })));
    /// # let _ = app.router();
    /// ```
    pub fn route<R: TypedRoute>(mut self, methods: Methods) -> Self {
        const {
            assert!(
                crate::api::under_const(P::PATH, R::PATH),
                "corten: a route of an area is at or under its prefix (the note below names the area's prefix and the route)"
            )
        };
        self.routes = self.routes.route(R::PATH, methods.0);
        self
    }

    /// Components only this area's pages render: `.components(&[AdminTable, SalesChart])`. Their
    /// stylesheets are joined into a file of the area's own (`sections/admin.css` for `/admin`),
    /// linked after `components.css` from the pages at the prefix and under it, so other pages
    /// do not download them; their scripts are served as with [`App::components`].
    ///
    /// A client navigation into the area loads its stylesheet before it swaps the page in, and
    /// the stylesheet stays (its rules only match its components' classes). A page under nested
    /// areas (`/admin` and `/admin/reports`) links each one's stylesheet.
    ///
    /// A component that pages in several areas render is listed in each of them: its rules are
    /// in one file those areas share (`sections/search+categories.css`), linked from their pages
    /// only, with any other component the same areas list. One that most pages render belongs in
    /// [`App::components`] instead (a component with a stylesheet is in one or the other), and so
    /// does a token file (`tokens!`), which styles every page. Under `corten dev` a page outside an area that renders one of its components is an
    /// error that names both; a patch is checked against its own URL. Without areas there is one
    /// stylesheet, which is the right choice until a part of the app most visitors never open
    /// adds enough CSS to measure (see the styles guide).
    pub fn components(mut self, components: &[crate::Component]) -> Self {
        self.components.extend_from_slice(components);
        self
    }
}

/// An [`Area`] given to [`App::area`], without its type.
struct Placed {
    prefix: &'static str,
    routes: Router,
    components: Vec<crate::Component>,
}

/// An app: its routes, plus everything around them that every production app needs, on by
/// default. `M` is the app's own [`Middleware`] (`()` until [`App::middleware`]).
/// `corten::serve(App::new(routes)).await` gives, in a fixed order ([`crate::middleware`]):
///
/// - security headers, a CSP (Corten's inline tags allowed by hash, a nonce only when the page
///   asks for one), and a check that refuses cross-site writes ([`Policy::new`]; change it with
///   [`App::policy`]);
/// - `Cache-Control: private, no-cache` on what the app renders, unless a route answers
///   [`crate::Public`] (see [`crate::cache`]);
/// - error pages for failed requests, unknown URLs and handlers that panic, in the request's mode
///   ([`App::error_page`], the app's own in its layout), with the error shown under `corten dev`,
///   and JSON for a JSON route ([`crate::JsonResult`]);
/// - the files in `static/` (or `STATIC_DIR`), the scripts of the app's components and its
///   components' stylesheets joined into one file ([`App::components`]), at content-versioned,
///   immutable URLs, each script preloaded on the pages whose handlers use it;
/// - `/health/live` and `/health/ready`, the runtime at a versioned URL;
/// - compression of pages and static files (feature `compress`);
/// - under `corten dev` (`CORTEN_DEV=1`): open pages reload when the server restarts.
///
/// Who is asking is not the app's: a handler that needs it takes a [`crate::Caller`].
///
/// ```ignore
/// App::new(url::router())
///     .error_page(views::error_page, db.clone())     // failed requests, in the app's layout
///     .middleware((RequestId, RateLimit))            // the app's own steps, in this order
///     .area::<admin::Admin>(|area| admin::area(area).components(&[SalesChart]))
///     .pwa(Pwa::new(Manifest::new("Shop", "Shop").icon("icons/192.png", "192x192")))
/// ```
pub struct App<M = ()> {
    routes: Router,
    error_page: Option<Box<page::ErrorPage>>,
    policy: Policy,
    static_dir: Option<PathBuf>,
    pwa: Option<Pwa>,
    ready: Option<ReadyFn>,
    locales: Option<&'static [&'static str]>,
    dev: Option<bool>,
    components: Vec<crate::Component>,
    areas: Vec<Placed>,
    deadline: Option<Duration>,
    apis: Vec<(Mount, Router)>,
    middleware: M,
}

impl App {
    /// The app with these routes (a `#[corten::routes]` module's `router()`) and every default.
    pub fn new(routes: Routes) -> Self {
        App {
            routes: routes.0,
            error_page: None,
            policy: Policy::new(),
            static_dir: None,
            pwa: None,
            ready: None,
            locales: None,
            dev: None,
            components: Vec::new(),
            areas: Vec::new(),
            deadline: Some(Duration::from_secs(30)),
            apis: Vec::new(),
            middleware: (),
        }
    }

    /// The app's own steps in every request, run in order after Corten's checks and before the
    /// handler: `.middleware((RequestId, RateLimit))`. What one answers still gets the error
    /// page, the security headers and `Cache-Control`. See [`crate::middleware`]. Given once:
    ///
    /// ```compile_fail,E0599
    /// # use corten::prelude::*;
    /// let app = App::new(Routes::new()).middleware(((),)).middleware(());
    /// ```
    pub fn middleware<M: Middleware>(self, middleware: M) -> App<M> {
        App {
            routes: self.routes,
            error_page: self.error_page,
            policy: self.policy,
            static_dir: self.static_dir,
            pwa: self.pwa,
            ready: self.ready,
            locales: self.locales,
            dev: self.dev,
            components: self.components,
            areas: self.areas,
            deadline: self.deadline,
            apis: self.apis,
            middleware,
        }
    }
}

impl<M: Middleware> App<M> {
    /// A part of the app under `P::PATH`, with its own routes and, if it needs them, its own
    /// stylesheet, given by `build`: `.area::<admin::Admin>(|area| admin::area(area))` for a
    /// routes module of the area's own, or `.area::<Admin>(|admin| admin.route::<AdminUser>(get(user)).components(&[..]))`.
    /// See [`Area`]. The routes keep their full paths (`/admin/users/{id}`), so a
    /// `#[derive(Route)]` struct names the same URL in the router and in links, and a route
    /// outside the prefix does not compile.
    ///
    /// `P` is a route whose path is the prefix, like `/admin` (letters, digits, `-`, `_`, `.` and
    /// `~` between slashes, no trailing slash, no parameters; anything else does not compile).
    /// Areas may nest (`/admin` and `/admin/billing`, each with routes of its own); one under an
    /// [`App::api`] prefix, or one given twice, is a startup error.
    ///
    /// `build` is called once, here.
    ///
    /// ```compile_fail,E0080
    /// # use corten::prelude::*;
    /// #[derive(Route)]
    /// #[route("/items/{id}")]
    /// struct Item<'a> { id: &'a str }
    ///
    /// let app = App::new(Routes::new()).area::<Item>(|items| items);
    /// # let _ = app.router();
    /// ```
    pub fn area<P: TypedRoute>(mut self, build: impl FnOnce(Area<P>) -> Area<P>) -> Self {
        const {
            assert!(
                crate::api::plain_prefix(P::PATH, false),
                "corten: an area's prefix is a path like \"/admin\": letters, digits, -, _, . and ~ between slashes, without a trailing slash or parameters (the note below names the prefix's route)"
            )
        };
        let prefix = P::PATH;
        assert!(!self.areas.iter().any(|placed| placed.prefix == prefix), "corten: the area {prefix:?} is given twice");
        let area = build(Area { routes: Router::new(), components: Vec::new(), types: PhantomData });
        self.areas.push(Placed { prefix, routes: area.routes, components: area.components });
        self
    }

    /// Routes that answer only JSON (a webhook, an OAuth callback, a public API), under `P::PATH`,
    /// given by `build`: `.api::<ApiRoot>(|api| api.route::<ItemJson>(get(item_json)))`, with
    /// [`Api`]'s settings (`.allow_origin(..)`) in the same chain. The
    /// routes keep their full paths (`/api/items/{id}`), so a `#[derive(Route)]` struct names the
    /// same URL in the router and in links, and a route outside the prefix does not compile. A
    /// prefix is `"/"` or a path as [`App::area`] takes; API prefixes that overlap are a startup
    /// error.
    ///
    /// Under the prefix every failure is JSON, `{"error":{"status","message"}}`, whatever the
    /// client accepts, and an unknown URL or a wrong method there is a JSON 404 or 405. There is no
    /// shell and no nonce; the deadline, the request signal,
    /// compression and the same-origin check on writes stay. See [`crate::api`]. `"/"` makes the
    /// whole app an API (Corten's own routes, like the health checks, keep their behavior).
    pub fn api<P: TypedRoute>(mut self, build: impl FnOnce(Api<P>) -> Api<P>) -> Self {
        const {
            assert!(
                crate::api::plain_prefix(P::PATH, true),
                "corten: an API's prefix is \"/\" or a path like \"/api\": letters, digits, -, _, . and ~ between slashes, without a trailing slash or parameters (the note below names the prefix's route)"
            )
        };
        let prefix = P::PATH;
        let under = crate::api::under;
        if let Some((other, ..)) = self.apis.iter().find(|(m, ..)| under(&m.prefix, prefix) || under(prefix, &m.prefix)) {
            panic!("corten: the API prefixes {:?} and {prefix:?} overlap", other.prefix);
        }
        let api = build(Api::new());
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

    /// The app's messages (a type from [`crate::i18n::messages!`]): `req.lang()`, for the layout's
    /// `<html lang>`, is the locale negotiated from `Accept-Language`, as `req.t()` picks it.
    pub fn messages<T: crate::i18n::Messages>(mut self) -> Self {
        self.locales = Some(T::LOCALES);
        self
    }

    /// The page for a failed request from a browser: every [`Failure`] (an unknown URL, a wrong
    /// method, a cross-site write, a missing caller, input that does not parse, a handler's error
    /// or panic, the deadline, the PWA's offline page), in the app's layout, for who is asking.
    /// An async function of the failure and then extractors, as a handler's are, with the state
    /// they read (`()` for none): `.error_page(views::error_page, db)` for
    /// `async fn error_page(failure: Failure, req: Req, viewer: Option<Caller<Viewer>>) -> HtmlResult`
    /// ([`ErrorPageFn`]). Its caller is looked up only when it asks for one, so an unknown URL
    /// costs a session lookup only for an error page that shows who is asking. It is sent in the
    /// request's mode, like any page, with the failure's status; `failure.detail()` is the error,
    /// set only under `corten dev`. A JSON route's failures, and a data client's, are JSON instead.
    /// `corten new` writes one. Without it, Corten sends a plain document.
    pub fn error_page<H, T, S>(mut self, page: H, state: S) -> Self
    where
        H: ErrorPageFn<T, S>,
        S: Clone + Send + Sync + 'static,
    {
        self.error_page = Some(error_page_fn(page, state));
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
        for area in &self.areas {
            if let Some((mount, ..)) = self.apis.iter().find(|(mount, ..)| mount.contains(area.prefix)) {
                panic!("corten: the area {:?} is under App::api({:?}), whose routes answer only JSON", area.prefix, mount.prefix);
            }
        }
        // An area's stylesheet is a section of the asset pipeline.
        let sections: Vec<(&str, &[crate::Component])> = self
            .areas
            .iter()
            .filter(|area| !area.components.is_empty())
            .map(|area| (area.prefix, area.components.as_slice()))
            .collect();
        let assets = load_assets(self.static_dir, &self.components, &sections).map(Arc::new);
        let mut head = String::new();
        if let Some(href) = assets.as_ref().and_then(|a| a.stylesheet()) {
            head.push_str(&format!(r#"<link rel="stylesheet" href="{href}">"#));
        }
        let mut routes = self.routes;
        // Corten's own routes: an API prefix does not take them over.
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

        let error_page = self.error_page.unwrap_or_else(|| error_page_fn(default_error_page, ()));
        let mut shared = Shared::new(error_page, assets, head);
        shared.offline = offline;
        shared.locales = self.locales;
        shared.dev = dev;
        shared.components = self.components.iter().chain(sections.iter().flat_map(|(_, c)| c.iter())).map(|c| c.name).collect();
        if let Some(assets) = &shared.assets {
            for section in assets.section_stylesheets() {
                let index = shared.sections.len();
                shared.section_of.extend(section.components.iter().map(|name| (*name, index)));
                let link = format!(r#"<link rel="stylesheet" href="{}" data-corten-style>"#, crate::escape(&section.href));
                shared.sections.push((section.prefixes.clone(), link));
            }
        }
        let policy = Applied::new(hosts_from_env(self.policy), &shared.import_map);

        // The routes of the APIs and areas: each is under its prefix, which the compiler checked.
        let mut owned = Vec::with_capacity(self.apis.len() + self.areas.len());
        let mut mounts = Vec::with_capacity(self.apis.len());
        for (mount, api) in self.apis {
            owned.push(api);
            mounts.push(mount);
        }
        owned.extend(self.areas.into_iter().map(|area| area.routes));
        let layer = Arc::new(Layer {
            shared: Arc::new(shared),
            deadline: self.deadline,
            policy,
            mounts,
            own,
            middleware: self.middleware,
        });
        // The app's own routes are merged last: when two routers both have axum's default
        // fallback, a merge keeps the one merged in, and the app's answers unknown URLs.
        let mut router = Router::new();
        for routes in owned {
            router = router.merge(routes);
        }
        router.merge(routes).layer(Wrap(layer))
    }
}

impl<M: Middleware> From<App<M>> for Router {
    fn from(app: App<M>) -> Router {
        app.router()
    }
}

/// Serves an app on `HOST`:`PORT` until Ctrl-C or `SIGTERM`, ending live streams first so
/// shutdown is quick: `corten::serve(App::new(routes)).await`. Without `HOST` it listens on the
/// loopback addresses, `127.0.0.1` and `[::1]`, so `http://localhost:PORT` answers at once
/// however the system resolves `localhost`; `PORT` defaults to 3000. With feature `log`, prints
/// logs to stderr (`RUST_LOG` style filtering is the app's choice: set its own subscriber first).
///
/// It takes anything that becomes a router, so tower layers of the app's own go around the whole
/// app, outside Corten's (a request id, a trace span, a limit per address):
/// `corten::serve(app.router().layer(layer)).await`. A step of the app's own inside Corten's
/// order is a [`Middleware`] ([`App::middleware`]).
pub async fn serve(app: impl Into<Router>) -> std::io::Result<()> {
    #[cfg(feature = "log")]
    let _ = tracing_subscriber::fmt().with_target(false).try_init();
    let host = std::env::var("HOST").ok();
    let port: u16 = match std::env::var("PORT") {
        Ok(port) => port.parse().map_err(|_| std::io::Error::other(format!("PORT is not a port number: {port}")))?,
        Err(_) => 3000,
    };
    let router = app.into();
    let listeners = bind(host.as_deref(), port).await?;
    let shown = match host.as_deref() {
        None | Some("0.0.0.0" | "::") => "localhost",
        Some(host) => host,
    };
    println!("corten: listening on http://{shown}:{}", listeners[0].local_addr()?.port());
    serve_on(listeners, router, crate::shutdown_signal()).await
}

/// The listeners for `host`:`port`; without a host, the IPv4 and IPv6 loopback addresses on one
/// port. Browsers resolve `localhost` to `::1` first on some systems (Windows does), and a server
/// only on `127.0.0.1` then answers each new connection a fallback later, about 200 ms (measured
/// on Windows with Chromium and curl). A system without IPv6 gets `127.0.0.1` alone, but a `[::1]`
/// port another process holds is an error: `localhost` would reach that process instead (port 0,
/// any free port, tries another pair).
pub(crate) async fn bind(host: Option<&str>, port: u16) -> std::io::Result<Vec<tokio::net::TcpListener>> {
    use std::net::{Ipv4Addr, Ipv6Addr};
    if let Some(host) = host {
        return Ok(vec![tokio::net::TcpListener::bind((host, port)).await?]);
    }
    let mut tries = 0;
    loop {
        let v4 = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
        match tokio::net::TcpListener::bind((Ipv6Addr::LOCALHOST, v4.local_addr()?.port())).await {
            Ok(v6) => return Ok(vec![v4, v6]),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                tries += 1;
                if port != 0 || tries == 8 {
                    return Err(error);
                }
            }
            Err(_) => return Ok(vec![v4]),
        }
    }
}

/// Serves `router` on every listener until `stop` resolves, then lets each finish its requests;
/// a listener that fails ends it with the error.
pub(crate) async fn serve_on(
    listeners: Vec<tokio::net::TcpListener>,
    router: Router,
    stop: impl Future<Output = ()>,
) -> std::io::Result<()> {
    use futures_util::future::{select, try_join_all, Either};
    use std::future::IntoFuture;
    let (stopped, watch) = tokio::sync::watch::channel(false);
    let servers = listeners.into_iter().map(|listener| {
        let mut watch = watch.clone();
        let stop = async move {
            let _ = watch.wait_for(|stopped| *stopped).await;
        };
        axum::serve(listener, router.clone()).with_graceful_shutdown(stop).into_future()
    });
    let servers = std::pin::pin!(try_join_all(servers));
    match select(std::pin::pin!(stop), servers).await {
        Either::Left(((), servers)) => {
            let _ = stopped.send(true);
            servers.await.map(drop)
        }
        Either::Right((served, _)) => served.map(drop),
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
struct Wrap<M>(Arc<Layer<M>>);

impl<M> Clone for Wrap<M> {
    fn clone(&self) -> Self {
        Wrap(self.0.clone())
    }
}

impl<M: Middleware> tower_layer::Layer<Route> for Wrap<M> {
    type Service = Wrapped<M>;

    fn layer(&self, route: Route) -> Wrapped<M> {
        Wrapped { layer: self.0.clone(), route }
    }
}

struct Wrapped<M> {
    layer: Arc<Layer<M>>,
    route: Route,
}

impl<M> Clone for Wrapped<M> {
    fn clone(&self) -> Self {
        Wrapped { layer: self.layer.clone(), route: self.route.clone() }
    }
}

impl<M: Middleware> tower_service::Service<Request> for Wrapped<M> {
    type Response = Response;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>>;

    // A route is always ready: it clones its service for each call.
    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let (layer, route) = (self.layer.clone(), self.route.clone());
        Box::pin(async move { Ok(layer.handle(req, route).await) })
    }
}

/// The rest of the request under the deadline and the panic catch: the app's middleware, then the
/// route and its handler.
pub(crate) struct Next<'a, M> {
    middleware: &'a M,
    route: Route,
}

impl<'a, M: Middleware> Next<'a, M> {
    // Not an `async fn`: a layer of its own would move the request once more.
    #[inline]
    pub(crate) fn run(self, req: Request) -> impl Future<Output = Response> + Send + 'a {
        self.middleware.call(req, Handle(self.route))
    }
}

/// The route, as the last step after the app's middleware.
struct Handle(Route);

impl crate::middleware::Next for Handle {
    #[inline]
    fn run(self, req: Request) -> impl Future<Output = Response> + Send {
        // The route's own future, not an async block around it, which would move the request
        // once more.
        let mut route = self.0;
        futures_util::FutureExt::map(tower_service::Service::call(&mut route, req), |res: Result<Response, Infallible>| match res
        {
            Ok(res) => res,
            Err(never) => match never {},
        })
    }
}

/// Everything `App` puts around a request, as one middleware: one boxed future per request,
/// where separate security, compression and error layers cost one each.
struct Layer<M> {
    shared: Arc<Shared>,
    deadline: Option<Duration>,
    policy: Applied,
    mounts: Vec<Mount>,
    own: Own,
    middleware: M,
}

impl<M: Middleware> Layer<M> {
    async fn handle(&self, req: Request, route: Route) -> Response {
        #[cfg(feature = "compress")]
        let coding = crate::compress::negotiate(req.headers());
        let res = self.secure(req, route).await;
        #[cfg(feature = "compress")]
        let res = crate::compress::finish(res, coding).await;
        res
    }

    /// The security policy around the request: the host and same-origin checks first, the
    /// headers last.
    async fn secure(&self, mut req: Request, route: Route) -> Response {
        // A request for another name (a DNS-rebound page) runs nothing of the app's: no error
        // page, no caller lookup.
        if !self.policy.answers(&req) {
            let mut res = crate::security::misdirected();
            self.policy.headers(&mut res, None);
            res.headers_mut().insert(header::CACHE_CONTROL, cache::PRIVATE);
            return res;
        }
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
        // A page's nonce is made only if its HTML asks for one; an API route never has one.
        let nonce = mount.is_none().then(NonceSlot::default);
        if let Some(nonce) = &nonce {
            req.extensions_mut().insert(nonce.clone());
        }
        let origins = mount.map_or(&[][..], |mount| mount.origins());
        if let Some(refused) = self.policy.refuse(req.method(), req.headers(), origins) {
            let mut refused = match mount {
                Some(_) => crate::api::json_failure(refused, false).await,
                None => match self.cross_site(&req) {
                    Some(asked) => page::error_page(&self.shared, asked, Failure::CrossSite).await,
                    None => refused,
                },
            };
            self.policy.headers(&mut refused, nonce.as_ref());
            refused.headers_mut().insert(header::CACHE_CONTROL, cache::PRIVATE);
            return refused;
        }
        let cors = mount.and_then(|mount| mount.allowed_origin(req.headers()));
        let credentials = cache::credentials(req.headers());
        let path = self.shared.dev.then(|| req.uri().path().to_string());
        let (mut res, caller, mode) = self.run(req, route, mount.is_some()).await;
        // First: the security headers reserve room for `Cache-Control` and `Vary` too.
        self.policy.headers(&mut res, nonce.as_ref());
        let seen = Seen {
            credentials,
            caller,
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

    /// A write another site sent, to a page route: the request for the error page when a browser
    /// sent it ([`Failure::CrossSite`]); otherwise the status line answers it.
    fn cross_site(&self, req: &Request) -> Option<Asked> {
        let mode = page::mode_of(req.headers());
        let browser = matches!(mode, Mode::Navigation | Mode::Action)
            || (mode == Mode::Document
                && req.headers().get(header::ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|v| v.contains("text/html")));
        browser.then(|| Asked::new(req, self.shared.clone(), Credentials::of(req.headers())))
    }

    /// The response, whether anything read who is asking, and the request's mode.
    async fn run(&self, mut req: Request, route: Route, api: bool) -> (Response, bool, Mode) {
        // Dropped with this future if the client leaves first; then the response's.
        let watch = Watch(Signal::default());
        req.extensions_mut().insert(watch.0.clone());
        req.extensions_mut().insert(self.shared.clone());
        let mode = page::mode_of(req.headers());
        let next = Next { middleware: &self.middleware, route };
        let res = page::render_errors(&self.shared, req, next, self.deadline, &watch.0, api, mode).await;
        let personal = watch.0.is_personal();
        (signal::follow(res, watch), personal, mode)
    }
}

/// `policy` with the names in `HOSTS` (comma separated, as given to [`Policy::host`]; `*` for any
/// host), which a deploy sets where it sets `HOST` and `PORT`.
fn hosts_from_env(mut policy: Policy) -> Policy {
    let Ok(hosts) = std::env::var("HOSTS") else { return policy };
    for name in hosts.split(',').map(str::trim).filter(|name| !name.is_empty()) {
        policy = if name == "*" { policy.any_host() } else { policy.host(name) };
    }
    policy
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
