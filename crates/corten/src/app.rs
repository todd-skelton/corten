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
    client::Peer,
    middleware::Middleware,
    page::{self, default_error_page, error_page_fn, Asked, Credentials, ErrorPageFn, Failure, Shared},
    pwa::Pwa,
    route::{Methods, Prefix, Route as TypedRoute, Routes},
    security::{Applied, NonceSlot, Policy},
    signal::{self, Watch},
    task::{Running, Task},
    Mode, Signal,
};

type ReadyFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

/// A part of an app under the prefix `P` (a [`Prefix`]: a route's path, or a prefix with no page), filled in by the
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
    pages: Vec<(&'static str, bool)>,
    components: Vec<crate::Component>,
    deadline: Option<Option<Duration>>,
    types: PhantomData<fn() -> P>,
}

impl<P: Prefix> Area<P> {
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
    ///
    /// A `Route` written by hand has a path of the shape `#[derive(Route)]` writes (each parameter
    /// a whole segment, the text characters a browser sends as they are, here not `|`), so
    /// [`crate::Req::serves`] knows which route the router picks:
    ///
    /// ```compile_fail,E0080
    /// # use corten::prelude::*;
    /// #[derive(Route)]
    /// #[route("/admin")]
    /// struct Admin;
    ///
    /// struct Item;
    /// impl corten::Route for Item {
    ///     const PATH: &'static str = "/admin/a|b";
    ///     fn write_href(&self, out: &mut String, _: bool) {
    ///         out.push_str("/admin/a|b");
    ///     }
    /// }
    ///
    /// let app = App::new(Routes::new()).area::<Admin>(|admin| admin.route::<Item>(get(|| async { "item" })));
    /// # let _ = app.router();
    /// ```
    pub fn route<R: TypedRoute>(mut self, methods: Methods) -> Self {
        const {
            assert!(
                crate::api::under_const(P::PREFIX, R::PATH),
                "corten: a route of an area is at or under its prefix (the note below names the area's prefix and the route)"
            );
            assert!(
                crate::route::route_shaped(R::PATH),
                "corten: a route's path has the shape #[derive(Route)] writes: whole-segment parameters like /items/{{id}} and plain text (the note below names the route)"
            );
        };
        self.routes = self.routes.route(R::PATH, methods.0);
        self.pages.push((R::PATH, methods.1));
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

    /// How long a request at the prefix or under it may take to start its answer, in place of
    /// [`App::deadline`]: `.deadline(Duration::from_secs(120))` for reports that take a while, so
    /// every other page keeps the app's. Past it the answer is the 503 error page and the
    /// request's [`crate::Signal`] aborts with [`crate::Abort::Deadline`], as at the app's. In
    /// nested areas the innermost that sets one decides. `None` removes it, for routes that bound
    /// themselves.
    pub fn deadline(mut self, deadline: impl Into<Option<Duration>>) -> Self {
        self.deadline = Some(deadline.into());
        self
    }
}

/// An [`Area`] given to [`App::area`], without its type.
struct Placed {
    prefix: &'static str,
    routes: Router,
    pages: Vec<(&'static str, bool)>,
    components: Vec<crate::Component>,
    deadline: Option<Option<Duration>>,
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
///     .area(|area| admin::area(area).components(&[SalesChart]))
///     .pwa(Pwa::new(Manifest::new("Shop", "Shop").icon("icons/192.png", "192x192")))
/// ```
pub struct App<M = ()> {
    routes: Router,
    /// The app's route patterns and whether each has a `GET` handler ([`crate::Req::serves`]);
    /// the areas' are added in `router`.
    pages: Vec<(&'static str, bool)>,
    error_page: Option<Box<page::ErrorPage>>,
    policy: Policy,
    static_dir: Option<PathBuf>,
    pwa: Option<Pwa>,
    ready: Option<ReadyFn>,
    locales: Option<&'static [&'static str]>,
    client_zone: bool,
    dev: Option<bool>,
    components: Vec<crate::Component>,
    areas: Vec<Placed>,
    deadline: Option<Duration>,
    apis: Vec<(Mount, Router)>,
    tasks: Vec<Task>,
    grace: Duration,
    connections: Connections,
    layers: Vec<Box<dyn FnOnce(Router) -> Router + Send>>,
    middleware: M,
}

/// How [`serve`] holds connections: how long a request's head may take, and how many may be
/// open at once.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Connections {
    pub header_timeout: Option<Duration>,
    pub max: Option<usize>,
}

impl Default for Connections {
    fn default() -> Self {
        Connections { header_timeout: Some(Duration::from_secs(30)), max: None }
    }
}

impl App {
    /// The app with these routes (a `#[corten::routes]` module's `router()`) and every default.
    pub fn new(routes: Routes) -> Self {
        App {
            routes: routes.0,
            pages: routes.1,
            error_page: None,
            policy: Policy::new(),
            static_dir: None,
            pwa: None,
            ready: None,
            locales: None,
            client_zone: false,
            dev: None,
            components: Vec::new(),
            areas: Vec::new(),
            deadline: Some(Duration::from_secs(30)),
            apis: Vec::new(),
            tasks: Vec::new(),
            grace: Duration::from_secs(5),
            connections: Connections::default(),
            layers: Vec::new(),
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
            pages: self.pages,
            error_page: self.error_page,
            policy: self.policy,
            static_dir: self.static_dir,
            pwa: self.pwa,
            ready: self.ready,
            locales: self.locales,
            client_zone: self.client_zone,
            dev: self.dev,
            components: self.components,
            areas: self.areas,
            deadline: self.deadline,
            apis: self.apis,
            tasks: self.tasks,
            grace: self.grace,
            connections: self.connections,
            layers: self.layers,
            middleware,
        }
    }
}

impl<M: Middleware> App<M> {
    /// A part of the app under `P::PREFIX`, with its own routes and, if it needs them, its own
    /// stylesheet, given by `build`: `.area(admin::area)` for a `#[corten::routes(area = Admin)]`
    /// module (`P` is inferred from it), with `.area(|area| admin::area(area).components(&[..]))`
    /// for its stylesheet, or by hand, `.area::<Admin>(|admin| admin.route::<AdminUser>(get(user)))`.
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
    pub fn area<P: Prefix>(mut self, build: impl FnOnce(Area<P>) -> Area<P>) -> Self {
        const {
            assert!(
                crate::api::plain_prefix(P::PREFIX, false),
                "corten: an area's prefix is a path like \"/admin\": letters, digits, -, _, . and ~ between slashes, without a trailing slash or parameters (the note below names the prefix's route)"
            )
        };
        let prefix = P::PREFIX;
        assert!(!self.areas.iter().any(|placed| placed.prefix == prefix), "corten: the area {prefix:?} is given twice");
        let area =
            build(Area { routes: Router::new(), pages: Vec::new(), components: Vec::new(), deadline: None, types: PhantomData });
        self.areas.push(Placed {
            prefix,
            routes: area.routes,
            pages: area.pages,
            components: area.components,
            deadline: area.deadline,
        });
        self
    }

    /// Routes that answer only JSON (a webhook, an OAuth callback, a public API), under `P::PREFIX`,
    /// given by `build`: `.api(api::api)` for a `#[corten::routes(api = ApiRoot)]` module (`P` is
    /// inferred from it), with [`Api`]'s settings in the same chain,
    /// `.api(|routes| api::api(routes).allow_origin(..))`. The
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
    pub fn api<P: Prefix>(mut self, build: impl FnOnce(Api<P>) -> Api<P>) -> Self {
        const {
            assert!(
                crate::api::plain_prefix(P::PREFIX, true),
                "corten: an API's prefix is \"/\" or a path like \"/api\": letters, digits, -, _, . and ~ between slashes, without a trailing slash or parameters (the note below names the prefix's route)"
            )
        };
        let prefix = P::PREFIX;
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
    /// removes it, for an app whose handlers bound themselves. An area or an API whose routes
    /// need another sets its own ([`Area::deadline`], [`Api::deadline`]), so one slow route does
    /// not lift it for every page.
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

    /// Learn each visitor's time zone, for [`ClientZone`](crate::ClientZone): the runtime keeps
    /// the browser's (`America/Chicago`) in a cookie, `corten-zone`, set on the first page that
    /// loads it, so the requests after that (a form's post, the next page) carry it. Off unless
    /// set: an app that writes every date in UTC or one zone of its own sets no cookie.
    pub fn client_zone(mut self) -> Self {
        self.client_zone = true;
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

    /// Work that lives as long as the app (a refresh loop, a queue consumer, a supervised child
    /// process), started by [`serve`] once it listens. Its [`Signal`] aborts with
    /// [`crate::Abort::Shutdown`] when shutdown begins, and `serve` waits for it to return, up to
    /// [`App::shutdown_grace`], before it returns itself, so a write in progress is finished
    /// rather than cut off at exit. A task still running at the bound is ended and logged by
    /// name, and so is one that panics; restarting a task is the app's choice. [`App::router`]
    /// starts none, so a test of the app does not run them.
    ///
    /// Work on a schedule is a task with tokio's intervals. Two jobs in one task's loop never
    /// overlap; `MissedTickBehavior::Skip` makes a run that overran its mark skip the marks it
    /// missed (tokio's default, `Burst`, runs once for each, back to back); `biased` puts the
    /// signal first, so no job starts once shutdown has begun, and one in progress finishes. A
    /// mark on the clock (:15 past the hour, in UTC) is where the interval starts:
    ///
    /// ```
    /// use corten::{prelude::*, Signal};
    /// use std::time::{Duration, SystemTime, UNIX_EPOCH};
    /// use tokio::time::{interval, interval_at, Instant, MissedTickBehavior};
    /// # use std::sync::{Arc, Mutex};
    /// # #[derive(Clone)]
    /// # struct Models { runs: Arc<Mutex<Vec<(&'static str, Duration)>>>, start: Instant }
    /// # impl Models {
    /// #     fn ran(&self, job: &'static str) { self.runs.lock().unwrap().push((job, self.start.elapsed())) }
    /// #     async fn refresh(&self) { self.ran("refresh"); tokio::time::sleep(Duration::from_secs(11 * 60)).await }
    /// #     async fn recheck(&self) { self.ran("recheck") }
    /// # }
    ///
    /// const HOUR: Duration = Duration::from_secs(3600);
    ///
    /// /// The next time the clock (UTC) is `past` a multiple of `period`.
    /// fn next_mark(past: Duration, period: Duration) -> Instant {
    ///     let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    ///     let into = (now + period - past).as_nanos() % period.as_nanos();
    ///     Instant::now() + period - Duration::from_nanos(into as u64)
    /// }
    ///
    /// /// The models' refresh hourly at :15 and their recheck every five minutes, one at a time.
    /// async fn models(models: Models, signal: Signal) {
    ///     let mut hourly = interval_at(next_mark(Duration::from_secs(15 * 60), HOUR), HOUR);
    ///     let mut recheck = interval(Duration::from_secs(5 * 60));
    ///     hourly.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ///     recheck.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ///     loop {
    ///         tokio::select! {
    ///             biased;
    ///             _ = signal.aborted() => return,
    ///             _ = hourly.tick() => models.refresh().await,
    ///             _ = recheck.tick() => models.recheck().await,
    ///         }
    ///     }
    /// }
    ///
    /// fn app(shared: Models) -> App {
    ///     App::new(Routes::new()).task("models", move |signal| models(shared, signal))
    /// }
    /// # // Three hours on a paused clock, with a refresh that takes 11 minutes.
    /// # tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(async {
    /// #     use corten::axum::extract::FromRequestParts;
    /// #     let _ = app; // the app's own is served; this runs the loop alone, with a signal that never aborts
    /// #     let (mut parts, ()) = corten::axum::http::Request::new(()).into_parts();
    /// #     let signal = Signal::from_request_parts(&mut parts, &()).await.unwrap();
    /// #     let shared = Models { runs: Default::default(), start: Instant::now() };
    /// #     let wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    /// #     let _ = tokio::time::timeout(3 * HOUR, models(shared.clone(), signal)).await;
    /// #     let runs = shared.runs.lock().unwrap().clone();
    /// #     let refreshes: Vec<_> = runs.iter().filter(|r| r.0 == "refresh").map(|r| r.1).collect();
    /// #     assert_eq!(refreshes.len(), 3, "{runs:?}");
    /// #     assert!(refreshes.windows(2).all(|w| w[1] - w[0] == HOUR), "{refreshes:?}");
    /// #     let at = (wall + refreshes[0]).as_secs() % 3600;
    /// #     assert!((899..=901).contains(&at), "the first refresh ran {at} s past the hour");
    /// #     // One at a time, and an overrun runs the recheck once, not once per missed mark.
    /// #     assert!(runs.windows(2).all(|w| w[1].1 > w[0].1), "{runs:?}");
    /// #     for r in &refreshes {
    /// #         assert!(!runs.iter().any(|(_, at)| at > r && *at < *r + Duration::from_secs(11 * 60)), "{runs:?}");
    /// #     }
    /// # });
    /// ```
    pub fn task<F, Fut>(mut self, name: &'static str, task: F) -> Self
    where
        F: FnOnce(Signal) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.tasks.push(Task { name, start: Box::new(move |signal| Box::pin(task(signal))) });
        self
    }

    /// How long [`serve`] waits, once shutdown begins, for the app's [tasks](App::task) to
    /// return (5 seconds unless set). Keep it under the time the platform waits before it kills
    /// the process (`docker stop`: 10 seconds; Kubernetes: 30).
    pub fn shutdown_grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    /// How long a connection may take to send a request's head (the request line and headers)
    /// before [`serve`] closes it: 30 seconds unless set, from when the connection opens or its
    /// last answer was sent. A client that opens connections and never finishes a request
    /// cannot hold them, and an idle keep-alive connection is closed after it too. A request
    /// whose head has arrived is not bounded by it: a long upload, a live stream and a WebSocket
    /// are the [deadline](App::deadline)'s and their own. `None` removes it, for a server that
    /// only a proxy which buffers heads reaches.
    pub fn header_timeout(mut self, timeout: impl Into<Option<Duration>>) -> Self {
        self.connections.header_timeout = timeout.into();
        self
    }

    /// The most connections [`serve`] holds open at once, across its listeners (no limit unless
    /// set). At the limit it accepts no more until one closes; the operating system queues the
    /// rest. Bounds what many idle or slow clients can hold of the process's sockets and memory.
    pub fn max_connections(mut self, max: usize) -> Self {
        assert!(max > 0, "corten: max_connections is at least 1");
        self.connections.max = Some(max);
        self
    }

    /// A tower layer around the whole app, outside every stage of [`crate::middleware`], for what
    /// must see the raw request first: a cap on body size, a limit per address, a trace span.
    /// Layers added later go outside earlier ones. A step inside Corten's order (after its
    /// checks, with the error page around it) is a [`Middleware`] ([`App::middleware`]).
    pub fn layer<L>(mut self, layer: L) -> Self
    where
        L: tower_layer::Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: tower_service::Service<Request> + Clone + Send + Sync + 'static,
        <L::Service as tower_service::Service<Request>>::Response: axum::response::IntoResponse + 'static,
        <L::Service as tower_service::Service<Request>>::Error: Into<Infallible> + 'static,
        <L::Service as tower_service::Service<Request>>::Future: Send + 'static,
    {
        self.layers.push(Box::new(move |router: Router| router.layer(layer)));
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

    /// The finished router: the app's routes with everything around them, its
    /// [layers](App::layer) included. For tests (`app.router().oneshot(request)`) and for serving
    /// it another way; it starts none of the app's [tasks](App::task).
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
        if self.client_zone {
            head.push_str(r#"<meta name="corten-zone">"#);
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
        shared.client_zone = self.client_zone;
        shared.dev = dev;
        // The same path routed in two places (the app's routes and an area's) is one route to axum,
        // with the methods of both.
        let mut pages: Vec<(&'static str, bool)> = Vec::new();
        for &(pattern, get) in self.pages.iter().chain(self.areas.iter().flat_map(|area| &area.pages)) {
            match pages.iter_mut().find(|(p, _)| *p == pattern) {
                Some((_, has)) => *has |= get,
                None => pages.push((pattern, get)),
            }
        }
        shared.pages = pages.into();
        // What is not a page however the app's routes match it: Corten's own paths and the APIs.
        shared.not_pages = NotPages { own: own.clone(), apis: self.apis.iter().map(|(mount, _)| mount.prefix.clone()).collect() };
        shared.components = self.components.iter().chain(sections.iter().flat_map(|(_, c)| c.iter())).map(|c| c.name).collect();
        if let Some(assets) = &shared.assets {
            for section in assets.section_stylesheets() {
                let index = shared.sections.len();
                shared.section_of.extend(section.components.iter().map(|name| (*name, index)));
                let link = format!(r#"<link rel="stylesheet" href="{}" data-corten-style>"#, crate::escape(&section.href));
                shared.sections.push((section.prefixes.clone(), link));
            }
        }
        let policy = proxies_from_env(hosts_from_env(self.policy));
        shared.proxies = policy.proxies().to_vec();
        shared.allowed_origins = policy.allowed_origins().to_vec();
        let policy = Applied::new(policy, &shared.import_map);

        // The routes of the APIs and areas: each is under its prefix, which the compiler checked.
        let mut owned = Vec::with_capacity(self.apis.len() + self.areas.len());
        let mut mounts = Vec::with_capacity(self.apis.len());
        for (mount, api) in self.apis {
            if !mount.origins().is_empty() {
                shared.api_origins.push((mount.prefix.clone(), mount.origins().to_vec()));
            }
            owned.push(api);
            mounts.push(mount);
        }
        // The areas that set their own deadline, innermost first, so the first under a path decides.
        let mut area_deadlines: Vec<(&'static str, Option<Duration>)> =
            self.areas.iter().filter_map(|area| Some((area.prefix, area.deadline?))).collect();
        area_deadlines.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
        owned.extend(self.areas.into_iter().map(|area| area.routes));
        let layer = Arc::new(Layer {
            shared: Arc::new(shared),
            deadline: self.deadline,
            area_deadlines,
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
        let mut router = router.merge(routes).layer(Wrap(layer));
        for wrap in self.layers {
            router = wrap(router);
        }
        router
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
/// Shutdown, on the first Ctrl-C or `SIGTERM`: the app's [tasks](App::task) are told to stop,
/// live streams end, the listeners close, requests in progress finish, and `serve` returns once
/// the tasks have too (or [`App::shutdown_grace`] has passed).
///
/// A tower layer of the app's own goes around it with [`App::layer`]. To serve it on listeners of
/// the app's own (HTTPS, a Unix socket, a socket a supervisor hands over) with the same
/// shutdown, see [`serve_on`].
pub async fn serve<M: Middleware>(app: App<M>) -> std::io::Result<()> {
    let listeners = listeners(3000).await?;
    let shown = match std::env::var("HOST").ok().as_deref() {
        None | Some("0.0.0.0" | "::") => "localhost".to_string(),
        Some(host) => host.to_string(),
    };
    println!("corten: listening on http://{shown}:{}", listeners[0].local_addr()?.port());
    serve_on(listeners, app).await
}

/// The listeners [`serve`] uses: `HOST`:`PORT`, or without `HOST` both loopback addresses
/// (`127.0.0.1` and `[::1]`) on one port, so `localhost` answers at once however it resolves;
/// `port` when `PORT` is not set (`serve` passes 3000). For an app that wraps them (in TLS), or
/// whose own default port is not 3000 (`corten::serve_on(corten::listeners(8320).await?, app)`,
/// with no `PORT` to set from the program), served with [`serve_on`].
pub async fn listeners(port: u16) -> std::io::Result<Vec<tokio::net::TcpListener>> {
    let host = std::env::var("HOST").ok();
    let port: u16 = match std::env::var("PORT") {
        Ok(port) => port.parse().map_err(|_| std::io::Error::other(format!("PORT is not a port number: {port}")))?,
        Err(_) => port,
    };
    bind(host.as_deref(), port).await
}

/// [`serve`] on listeners of the app's own: anything that implements axum's
/// [`Listener`](axum::serve::Listener), such as a TLS acceptor around [`listeners`], a Unix
/// socket, or a socket a supervisor hands over. Shutdown is `serve`'s, across all of them: on
/// Ctrl-C or `SIGTERM` the app's tasks are told, live streams end, every listener closes and
/// finishes its requests, and it returns once the tasks have finished too. TLS stays the app's
/// choice of library; the guide (the app, Your own listeners) shows rustls.
///
/// A listener's `accept` is called in turn, one connection at a time, so a TLS handshake belongs
/// after it (per connection), where a slow client cannot hold the next one up.
pub async fn serve_on<L, M>(listeners: Vec<L>, app: App<M>) -> std::io::Result<()>
where
    L: axum::serve::Listener,
    L::Addr: std::fmt::Debug + 'static,
    M: Middleware,
{
    #[cfg(feature = "log")]
    let _ = tracing_subscriber::fmt().with_target(false).try_init();
    if listeners.is_empty() {
        return Err(std::io::Error::other("corten::serve_on: no listeners to serve on"));
    }
    run(listeners, app, crate::shutdown_signal()).await
}

/// Serves `app` on `listeners` with its tasks running until `stop` resolves; then tells the tasks
/// to stop, lets each listener finish its requests and waits for the tasks, up to the app's
/// grace from when `stop` resolved.
pub(crate) async fn run<L, M>(listeners: Vec<L>, mut app: App<M>, stop: impl Future<Output = ()>) -> std::io::Result<()>
where
    L: axum::serve::Listener,
    L::Addr: std::fmt::Debug + 'static,
    M: Middleware,
{
    let (tasks, grace, connections) = (std::mem::take(&mut app.tasks), app.grace, app.connections);
    // Built first: a startup panic (a misplaced area) starts no task.
    let router = app.router();
    let running = Running::start(tasks);
    let stopped = std::sync::OnceLock::new();
    let served = serve_router(listeners, router, connections, async {
        stop.await;
        let _ = stopped.set(tokio::time::Instant::now());
        running.stop();
    })
    .await;
    // A listener that failed ends the tasks too, with the same grace.
    let began = stopped.get().copied().unwrap_or_else(tokio::time::Instant::now);
    running.finish(began + grace, grace).await;
    served
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
pub(crate) async fn serve_router<L>(
    listeners: Vec<L>,
    router: Router,
    connections: Connections,
    stop: impl Future<Output = ()>,
) -> std::io::Result<()>
where
    L: axum::serve::Listener,
    L::Addr: std::fmt::Debug + 'static,
{
    use futures_util::future::{select, try_join_all, Either};
    let (stopped, watch) = tokio::sync::watch::channel(false);
    let open = connections.max.map(|max| Arc::new(tokio::sync::Semaphore::new(max)));
    let clock = connections.header_timeout.map(crate::watch::Clock::start);
    let servers = listeners
        .into_iter()
        .map(|listener| serve_listener(listener, router.clone(), clock.clone(), open.clone(), watch.clone()));
    let servers = std::pin::pin!(try_join_all(servers));
    match select(std::pin::pin!(stop), servers).await {
        Either::Left(((), servers)) => {
            let _ = stopped.send(true);
            servers.await.map(drop)
        }
        Either::Right((served, _)) => served.map(drop),
    }
}

/// Accepts connections on `listener` and serves each until `stop` turns true; then stops
/// accepting, lets each connection finish the request it is in (an idle one closes) and returns
/// once all have closed. With `clock`, a connection waits for a request's head only as long as
/// it allows, and with `open`, a connection is accepted only once one of its permits is free.
async fn serve_listener<L>(
    mut listener: L,
    router: Router,
    clock: Option<Arc<crate::watch::Clock>>,
    open: Option<Arc<tokio::sync::Semaphore>>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> std::io::Result<()>
where
    L: axum::serve::Listener,
    L::Addr: 'static,
{
    // Each connection holds a sender; the listener is done when every one has dropped.
    let (alive, mut closed) = tokio::sync::mpsc::channel::<std::convert::Infallible>(1);
    loop {
        let permit = match &open {
            None => None,
            Some(open) => tokio::select! {
                permit = open.clone().acquire_owned() => Some(permit.expect("the semaphore is never closed")),
                _ = stop.wait_for(|stopped| *stopped) => break,
            },
        };
        let (io, addr) = tokio::select! {
            accepted = listener.accept() => accepted,
            _ = stop.wait_for(|stopped| *stopped) => break,
        };
        let addr: &dyn std::any::Any = &addr;
        let peer = match addr.downcast_ref::<std::net::SocketAddr>() {
            Some(addr) => Peer::Ip(addr.ip()),
            #[cfg(unix)]
            None if addr.is::<tokio::net::unix::SocketAddr>() => Peer::Local,
            None => Peer::Unknown,
        };
        let watch = clock.as_ref().map(crate::watch::Clock::watch);
        let service =
            hyper_util::service::TowerToHyperService::new(Connection { router: router.clone(), peer, watch: watch.clone() });
        let io = hyper_util::rt::TokioIo::new(crate::watch::Watched { io, watch });
        let (alive, mut stop) = (alive.clone(), stop.clone());
        tokio::spawn(async move {
            let (_permit, _alive) = (permit, alive);
            let http = hyper::server::conn::http1::Builder::new();
            let mut conn = std::pin::pin!(http.serve_connection(io, service).with_upgrades());
            tokio::select! {
                _ = conn.as_mut() => return,
                _ = stop.wait_for(|stopped| *stopped) => conn.as_mut().graceful_shutdown(),
            }
            let _ = conn.await;
        });
    }
    drop(listener);
    drop(alive);
    let _ = closed.recv().await;
    Ok(())
}

/// One connection's service: the router, each of whose requests runs with the connection's
/// peer at hand for [`ClientIp`](crate::ClientIp). Not a request extension: `http`'s extensions
/// box each value, and one more cost every request 497 instructions whether anything read it or
/// not; this costs 44 (callgrind, keep-alive requests over TCP).
#[derive(Clone)]
struct Connection {
    router: Router,
    peer: Peer,
    /// For [`App::header_timeout`].
    watch: Option<Arc<crate::watch::Watch>>,
}

impl tower_service::Service<hyper::Request<hyper::body::Incoming>> for Connection {
    type Response = axum::http::Response<crate::watch::Answered>;
    type Error = std::convert::Infallible;
    type Future = crate::watch::Answer<crate::client::WithPeer<<Router as tower_service::Service<Request>>::Future>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        tower_service::Service::<Request>::poll_ready(&mut self.router, cx)
    }

    fn call(&mut self, req: hyper::Request<hyper::body::Incoming>) -> Self::Future {
        let req = req.map(axum::body::Body::new);
        let watch = self.watch.as_ref().map(|watch| (watch.clone(), watch.busy()));
        let inner = crate::client::WithPeer::new(self.peer, tower_service::Service::<Request>::call(&mut self.router, req));
        crate::watch::Answer { inner, watch }
    }
}

/// The paths of Corten's own routes.
#[derive(Clone, Default)]
pub(crate) struct Own {
    exact: Vec<&'static str>,
    prefixes: Vec<String>,
}

impl Own {
    fn contains(&self, path: &str) -> bool {
        self.exact.contains(&path) || self.prefixes.iter().any(|prefix| path.starts_with(prefix.as_str()))
    }
}

/// Paths no page is at, however the app's routes match them: Corten's own and the APIs'.
#[derive(Default)]
pub(crate) struct NotPages {
    own: Own,
    apis: Vec<String>,
}

impl NotPages {
    pub(crate) fn contains(&self, path: &str) -> bool {
        self.own.contains(path) || self.apis.iter().any(|prefix| crate::api::under(prefix, path))
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
    /// [`Area::deadline`]s, innermost prefix first; empty unless an area set one.
    area_deadlines: Vec<(&'static str, Option<Duration>)>,
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
        let deadline = match mount {
            Some(mount) => mount.deadline.unwrap_or(self.deadline),
            None if own || self.area_deadlines.is_empty() => self.deadline,
            None => {
                self.area_deadlines.iter().find(|(prefix, _)| crate::api::under(prefix, path)).map_or(self.deadline, |(_, d)| *d)
            }
        };
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
                Some(mount) => crate::api::json_failure(refused, false, mount.errors.as_ref()).await,
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
        let (mut res, caller, mode) = self.run(req, route, mount, deadline).await;
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
    async fn run(
        &self,
        mut req: Request,
        route: Route,
        api: Option<&Mount>,
        deadline: Option<Duration>,
    ) -> (Response, bool, Mode) {
        // Dropped with this future if the client leaves first; then the response's.
        let watch = Watch(Signal::for_app(self.shared.clone()));
        req.extensions_mut().insert(watch.0.clone());
        let mode = page::mode_of(req.headers());
        let next = Next { middleware: &self.middleware, route };
        let res = page::render_errors(&self.shared, req, next, deadline, &watch.0, api, mode).await;
        let personal = watch.0.is_personal();
        (signal::follow(res, watch), personal, mode)
    }
}

/// `policy` with the proxies in `PROXIES` (comma separated, as given to [`Policy::proxy`]).
fn proxies_from_env(mut policy: Policy) -> Policy {
    let Ok(proxies) = std::env::var("PROXIES") else { return policy };
    for network in proxies.split(',').map(str::trim).filter(|network| !network.is_empty()) {
        policy = policy.proxy(network);
    }
    policy
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
