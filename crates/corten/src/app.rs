//! [`App`]: an app's routes with the production pieces around them, configured by defaults.

use std::{
    convert::Infallible,
    future::Future,
    marker::PhantomData,
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
    task::{Finishing, Running, Task},
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
    sources: crate::security::Sources,
    middleware: Vec<Arc<dyn crate::middleware::Erased>>,
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

    /// A step of the area's own in each request at its prefix or under it (a write limit, an
    /// audit log): its routes, its nested areas' and an unknown URL there, as with its
    /// [`deadline`](Area::deadline) and [`policy`](Area::policy). It runs after the app's
    /// middleware ([`App::middleware`]) and before the handler: `.middleware(WriteLimit::new(20))`,
    /// or a tuple run in order. Given again, it runs after what was given before; an outer area's
    /// runs before an inner one's. A request anywhere else never meets it. It refuses with a
    /// [`crate::Refused`], answered as a [`crate::Caller`]'s refusal is; see
    /// [`crate::middleware`].
    pub fn middleware<M: Middleware>(mut self, middleware: M) -> Self {
        self.middleware.push(Arc::new(middleware));
        self
    }

    /// What the area's pages add to the app's [`Policy`] (its Content-Security-Policy, and a looser
    /// `Cross-Origin-Opener-Policy`): a payment provider's scripts and frames on the payment pages,
    /// style attributes on pages that show rich text written with them, and not on every other page
    /// (a size drawn from data needs none: it is a custom property, [`crate::style::Var`]). Only adding is possible, so a page in the
    /// area never allows less than the app; nested areas add up, outermost first. A stricter
    /// `Referrer-Policy` ([`crate::security::Sources::referrer`]) is the one way an area sends
    /// less, for every answer under it, an API's included.
    ///
    /// ```
    /// use corten::prelude::*;
    ///
    /// #[corten::routes(area = Payments)]
    /// mod url {
    ///     #[route("/pay")]
    ///     pub struct Payments;
    ///
    ///     #[route("/pay/{id}", get = pay)]
    ///     pub struct Pay<'a> {
    ///         pub id: &'a str,
    ///     }
    /// }
    /// # async fn pay(_: Params<url::Pay<'_>>) -> &'static str { "pay" }
    ///
    /// fn app(routes: Routes) -> App {
    ///     App::new(routes).area(|area| {
    ///         url::area(area).policy(|p| {
    ///             p.script_src("https://js.stripe.com").frame_src("https://js.stripe.com").connect_src("https://api.stripe.com")
    ///         })
    ///     })
    /// }
    /// # let _ = app(Routes::new()).router();
    /// ```
    ///
    /// Each area's header is built once, at startup. A document keeps the policy it loaded with,
    /// so a client navigation into or out of such an area loads the page in full (its shell key
    /// differs); a patch from the area's routes is applied under the policy of the page it lands
    /// on, and a page that answers a form from a page under another policy reloads that page. A
    /// request finds its area's policy once, by path, and only when some area adds something the
    /// app's policy lacks (a source, a looser `Cross-Origin-Opener-Policy` or a stricter
    /// `Referrer-Policy`). A policy for one route is an area of one page.
    ///
    /// What is the whole app's (the names it answers to, HSTS, other sites' writes) is not an
    /// area's to change:
    ///
    /// ```compile_fail,E0599
    /// # use corten::prelude::*;
    /// # #[derive(Route)]
    /// # #[route("/pay")]
    /// # struct Pay;
    /// let app = App::new(Routes::new()).area::<Pay>(|area| area.policy(|p| p.allow_origin("https://other.example")));
    /// ```
    pub fn policy(mut self, add: impl FnOnce(crate::security::Sources) -> crate::security::Sources) -> Self {
        self.sources = add(std::mem::take(&mut self.sources));
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
    sources: crate::security::Sources,
    middleware: Vec<Arc<dyn crate::middleware::Erased>>,
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
/// - the scripts of the app's components and its components' stylesheets joined into one file
///   ([`App::components`]), at content-versioned, immutable URLs, each script preloaded on the
///   pages whose handlers use it, and the same for the static files the app compiles in
///   ([`App::assets`]);
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
///     .pwa(Pwa::new(Manifest::new("Shop", "Shop").icon(Static::ICONS_192_PNG, "192x192")))
/// ```
pub struct App<M = ()> {
    routes: Router,
    /// The app's route patterns and whether each has a `GET` handler ([`crate::Req::serves`]);
    /// the areas' are added in `router`.
    pages: Vec<(&'static str, bool)>,
    error_page: Option<Box<page::ErrorPage>>,
    policy: Policy,
    /// The static directory `embed!` compiled in (`App::assets`): where it was read, and its files.
    static_files: Option<StaticFiles>,
    pwa: Option<Pwa>,
    ready: Option<ReadyFn>,
    locales: Option<&'static [&'static str]>,
    client_zone: bool,
    #[cfg(feature = "zones")]
    zoned_dates: bool,
    dev: Option<bool>,
    components: Vec<crate::Component>,
    areas: Vec<Placed>,
    deadline: Option<Duration>,
    apis: Vec<(Mount, Router)>,
    tasks: Vec<Task>,
    grace: Duration,
    /// Work its handlers finish whether or not they are dropped ([`Signal::finish`]), which
    /// `serve` waits for at shutdown.
    finishing: Arc<Finishing>,
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
            static_files: None,
            pwa: None,
            ready: None,
            locales: None,
            client_zone: false,
            #[cfg(feature = "zones")]
            zoned_dates: false,
            dev: None,
            components: Vec::new(),
            areas: Vec::new(),
            deadline: Some(Duration::from_secs(30)),
            apis: Vec::new(),
            tasks: Vec::new(),
            grace: Duration::from_secs(5),
            finishing: Arc::default(),
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
            static_files: self.static_files,
            pwa: self.pwa,
            ready: self.ready,
            locales: self.locales,
            client_zone: self.client_zone,
            #[cfg(feature = "zones")]
            zoned_dates: self.zoned_dates,
            dev: self.dev,
            components: self.components,
            areas: self.areas,
            deadline: self.deadline,
            apis: self.apis,
            tasks: self.tasks,
            grace: self.grace,
            finishing: self.finishing,
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
        let area = build(Area {
            routes: Router::new(),
            pages: Vec::new(),
            components: Vec::new(),
            deadline: None,
            sources: crate::security::Sources::default(),
            middleware: Vec::new(),
            types: PhantomData,
        });
        self.areas.push(Placed {
            prefix,
            routes: area.routes,
            pages: area.pages,
            components: area.components,
            deadline: area.deadline,
            sources: area.sources,
            middleware: area.middleware,
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
    /// Under the prefix every failure is an RFC 9457 Problem Details document
    /// (`application/problem+json`, `{"title","status","detail"}`), whatever the
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

    /// The app's components with a script or a stylesheet
    /// (`#[component(js)]`, `#[component(css)]`, `script!`, `style::tokens!`), each listed by its constant: `.components(&[TOKENS, OfferList,
    /// ListingSort])`, or a library's list, `.components(ui::COMPONENTS)`. Call it once per list;
    /// the order is the stylesheets' order, so list tokens and libraries first.
    ///
    /// Scripts are served with the static files and preloaded on the pages whose handlers use them;
    /// the stylesheets are one file, `components.css`, linked from every document's `<head>`.
    /// A component listed twice counts once. The typed events (`#[event]`) a component's script
    /// imports are served with it; an event only a file in the static directory imports is listed.
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
    /// loads it, so the requests after that (a form's post, the next page) carry it. With the
    /// `zones` feature, `req.zone()` gives it with its rules, and `App::zoned_dates` writes dates
    /// in it. Off unless set: an app that writes every date in UTC or one zone of its own sets no
    /// cookie.
    pub fn client_zone(mut self) -> Self {
        self.client_zone = true;
        self
    }

    /// Write dates in the visitor's zone (`zones` feature): while Corten renders a request's
    /// answer (a page, a patch, a live part's fragments), `t.date`, `t.time` and `t.date_time`
    /// with no `.zone(..)` of their own are written in the zone the browser reported, and the
    /// answer is private. Learns the zone ([`App::client_zone`]) too. A `Public` answer, the same
    /// for everyone, is written in UTC (corten-ui's `LocalTime` writes it again in the browser).
    ///
    /// The app says so itself, not the feature: a crate that turns the feature on (a library, the
    /// workspace's `--all-features`) changes no app's dates.
    #[cfg(feature = "zones")]
    pub fn zoned_dates(mut self) -> Self {
        self.client_zone = true;
        self.zoned_dates = true;
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

    /// The static directory, compiled into the program by [`crate::assets::embed!`]:
    /// `corten::assets::embed!(pub struct Static = "static");` and `.assets(Static)`. Pages name a
    /// file by its constant, `req.asset(Static::STYLE_CSS)`. Under `corten dev` the directory is
    /// read where the macro read it, so a file added since the build is served.
    pub fn assets<E: crate::assets::Embed>(mut self, _files: E) -> Self {
        self.static_files = Some(StaticFiles { dir: E::DIR, owner: E::OWNER, files: crate::assets::embedded::<E> });
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
    /// overlap. With `MissedTickBehavior::Skip` a run that overran a mark is followed by one late
    /// run, and then the marks again (tokio's default, `Burst`, runs once for each mark missed,
    /// back to back). `biased` puts the signal first, so no run starts once shutdown has begun; one
    /// in progress has until [`App::shutdown_grace`], and a run that can take longer is given the
    /// signal and stops its slow awaits there. A test aborts a task's signal as `serve` does with
    /// [`Signal::task`]. A mark on the clock (:15 past the hour, UTC) is where the interval starts,
    /// and the loop keeps what each job did for a status page:
    ///
    /// ```
    /// use corten::{prelude::*, Signal};
    /// use std::{
    ///     collections::BTreeMap,
    ///     sync::{Arc, Mutex},
    ///     time::{Duration, SystemTime, UNIX_EPOCH},
    /// };
    /// use tokio::time::{interval, interval_at, Instant, MissedTickBehavior};
    /// # #[derive(Clone)]
    /// # struct Models { runs: Arc<Mutex<Vec<(Job, Duration)>>>, start: Instant }
    /// # impl Models {
    /// #     fn ran(&self, job: Job) { self.runs.lock().unwrap().push((job, self.start.elapsed())) }
    /// #     async fn refresh(&self, signal: &Signal) -> Result<(), String> {
    /// #         self.ran(Job::Refresh);
    /// #         tokio::select! {
    /// #             biased;
    /// #             _ = signal.aborted() => return Err("stopped for shutdown".into()),
    /// #             _ = tokio::time::sleep(Duration::from_secs(11 * 60)) => {}
    /// #         }
    /// #         Err("the provider answered 503".into())
    /// #     }
    /// #     async fn recheck(&self) -> Result<(), String> { self.ran(Job::Recheck); Ok(()) }
    /// # }
    ///
    /// const HOUR: Duration = Duration::from_secs(3600);
    ///
    /// /// The next time the clock (UTC) is `past` a multiple of `period` (`past` under `period`).
    /// fn next_mark(past: Duration, period: Duration) -> Instant {
    ///     let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    ///     let into = (now + period - past).as_nanos() % period.as_nanos();
    ///     Instant::now() + period - Duration::from_nanos(into as u64)
    /// }
    ///
    /// #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    /// enum Job { Refresh, Recheck }
    ///
    /// /// A job's last run, for the app's status page.
    /// struct Ran { started: SystemTime, took: Duration, outcome: Result<(), String> }
    ///
    /// type Status = Arc<Mutex<BTreeMap<Job, Ran>>>;
    ///
    /// /// The models' refresh hourly at :15 and their recheck every five minutes, one at a time.
    /// async fn models(models: Models, status: Status, signal: Signal) {
    ///     let mut hourly = interval_at(next_mark(Duration::from_secs(15 * 60), HOUR), HOUR);
    ///     let mut recheck = interval(Duration::from_secs(5 * 60)); // the first tick is at once
    ///     hourly.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ///     recheck.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ///     loop {
    ///         let job = tokio::select! {
    ///             biased;
    ///             _ = signal.aborted() => return,
    ///             _ = hourly.tick() => Job::Refresh,
    ///             _ = recheck.tick() => Job::Recheck,
    ///         };
    ///         let (started, clock) = (SystemTime::now(), Instant::now());
    ///         let outcome = match job {
    ///             // Given the signal: its slow reads stop there, where its writes are whole.
    ///             Job::Refresh => models.refresh(&signal).await,
    ///             Job::Recheck => models.recheck().await,
    ///         };
    ///         let ran = Ran { started, took: clock.elapsed(), outcome };
    ///         status.lock().expect("never poisoned").insert(job, ran);
    ///     }
    /// }
    ///
    /// fn app(shared: Models, status: Status) -> App {
    ///     App::new(Routes::new()).task("models", move |signal| models(shared, status, signal))
    /// }
    /// # // Three hours on a paused clock, with a refresh that takes 11 minutes.
    /// # tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(async {
    /// #     let _ = app; // the app's own is served; this runs the loop alone
    /// #     let shared = Models { runs: Default::default(), start: Instant::now() };
    /// #     let status = Status::default();
    /// #     let wall = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    /// #     let (signal, shutdown) = Signal::task();
    /// #     let task = tokio::spawn(models(shared.clone(), status.clone(), signal));
    /// #     tokio::time::sleep(3 * HOUR).await;
    /// #     let runs = shared.runs.lock().unwrap().clone();
    /// #     let refreshes: Vec<_> = runs.iter().filter(|r| r.0 == Job::Refresh).map(|r| r.1).collect();
    /// #     assert_eq!(refreshes.len(), 3, "{runs:?}");
    /// #     assert!(refreshes.windows(2).all(|w| w[1] - w[0] == HOUR), "{refreshes:?}");
    /// #     let at = (wall + refreshes[0]).as_secs() % 3600;
    /// #     assert!((899..=901).contains(&at), "the first refresh ran {at} s past the hour");
    /// #     // One at a time, and an overrun runs the recheck once, not once per missed mark.
    /// #     assert!(runs.windows(2).all(|w| w[1].1 > w[0].1), "{runs:?}");
    /// #     for r in &refreshes {
    /// #         assert!(!runs.iter().any(|(_, at)| at > r && *at < *r + Duration::from_secs(11 * 60)), "{runs:?}");
    /// #     }
    /// #     let ran = status.lock().unwrap();
    /// #     let refresh = &ran[&Job::Refresh];
    /// #     assert!(refresh.took == Duration::from_secs(11 * 60) && refresh.outcome.is_err() && refresh.started <= SystemTime::now());
    /// #     assert_eq!(ran[&Job::Recheck].outcome, Ok(()));
    /// #     drop(ran);
    /// #     // Shutdown begins 2 minutes into the fourth refresh: the loop returns at once.
    /// #     tokio::time::sleep(refreshes[2] + HOUR + Duration::from_secs(120) - shared.start.elapsed()).await;
    /// #     let at = Instant::now();
    /// #     shutdown.begin();
    /// #     task.await.unwrap();
    /// #     assert_eq!(at.elapsed(), Duration::ZERO);
    /// #     assert_eq!(status.lock().unwrap()[&Job::Refresh].outcome, Err("stopped for shutdown".into()));
    /// # });
    /// ```
    ///
    /// The interval keeps time on the monotonic clock, which a change of the wall clock does not
    /// move and which some systems stop while asleep: after such a jump its marks are off the
    /// clock's until the interval is made again from `next_mark` (when a tick comes a period
    /// late, say). The next run's time, for the status page, is the app's to work out the same way.
    /// Daily at 03:30 UTC is `next_mark(HOUR * 7 / 2, 24 * HOUR)`; at 03:30 in a zone with
    /// daylight saving time takes a time-zone library, which is the app's. Jobs in separate tasks
    /// that must not overlap share a `tokio::sync::Mutex`.
    pub fn task<F, Fut>(mut self, name: &'static str, task: F) -> Self
    where
        F: FnOnce(Signal) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.tasks.push(Task { name, start: Box::new(move |signal| Box::pin(task(signal))) });
        self
    }

    /// How long [`serve`] waits, once shutdown begins, for the app's [tasks](App::task) to
    /// return, and for work its handlers finish ([`crate::Signal::finish`]) whose handler is gone
    /// (5 seconds unless set). Keep it under the time the platform waits before it kills
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
        let assets = load_assets(self.static_files, dev, &self.components, &sections).map(Arc::new);
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
        shared.finishing = self.finishing.clone();
        shared.offline = offline;
        shared.locales = self.locales;
        shared.client_zone = self.client_zone;
        #[cfg(feature = "zones")]
        {
            shared.zoned_dates = self.zoned_dates;
        }
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
        shared.plain_notice = !policy.sends_hsts();
        let area_sources: Vec<_> =
            self.areas.iter().filter(|area| !area.sources.is_empty()).map(|area| (area.prefix, &area.sources)).collect();
        let policy = Applied::new(policy, &shared.import_map, &area_sources);

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
        // Each area's middleware with every enclosing area's, the outermost first; the innermost
        // area first, so the first under a path decides. Empty unless an area has some.
        let mut area_middleware: Vec<(&'static str, crate::middleware::Stack)> = self
            .areas
            .iter()
            .filter_map(|area| {
                let mut around: Vec<&Placed> = self
                    .areas
                    .iter()
                    .filter(|other| !other.middleware.is_empty() && crate::api::under(other.prefix, area.prefix))
                    .collect();
                around.sort_by_key(|other| other.prefix.len());
                let stack: crate::middleware::Stack =
                    around.into_iter().flat_map(|other| other.middleware.iter().cloned()).collect();
                (!stack.is_empty()).then_some((area.prefix, stack))
            })
            .collect();
        area_middleware.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
        // An API under an area's prefix meets the area's middleware (and its enclosing areas')
        // before its own, as a nested area would.
        for mount in &mut mounts {
            if let Some((_, around)) = area_middleware.iter().find(|(prefix, _)| crate::api::under(prefix, &mount.prefix)) {
                mount.middleware = around.iter().chain(mount.middleware.iter()).cloned().collect();
            }
        }
        owned.extend(self.areas.into_iter().map(|area| area.routes));
        // The app's own routes are merged last: when two routers both have axum's default
        // fallback, a merge keeps the one merged in, and the app's answers unknown URLs.
        let mut router = Router::new();
        for routes in owned {
            router = router.merge(routes);
        }
        let router = router.merge(routes);
        // An app whose areas and APIs have no middleware of their own runs the app's alone.
        let plain = area_middleware.is_empty() && mounts.iter().all(|mount| mount.middleware.is_empty());
        macro_rules! wrap {
            ($steps:expr) => {
                router.layer(Wrap(Arc::new(Layer {
                    shared: Arc::new(shared),
                    deadline: self.deadline,
                    area_deadlines,
                    policy,
                    mounts,
                    own,
                    steps: $steps,
                })))
            };
        }
        let mut router =
            if plain { wrap!(Plain(self.middleware)) } else { wrap!(Scoped { app: self.middleware, areas: area_middleware }) };
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
/// the tasks and work handlers finish ([`crate::Signal::finish`]) have too (or
/// [`App::shutdown_grace`] has passed).
///
/// A tower layer of the app's own goes around it with [`App::layer`]. To serve it on listeners of
/// the app's own (a Unix socket, a socket a supervisor hands over) with the same shutdown, see
/// [`serve_on`]; for HTTPS, [`serve_wrapped`].
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
/// `port` when `PORT` is not set (`serve` passes 3000). For an app that serves HTTPS on them
/// ([`serve_wrapped`]), or whose own default port is not 3000
/// (`corten::serve_on(corten::listeners(8320).await?, app)`, with no `PORT` to set from the
/// program).
pub async fn listeners(port: u16) -> std::io::Result<Vec<tokio::net::TcpListener>> {
    let host = std::env::var("HOST").ok();
    let port: u16 = match std::env::var("PORT") {
        Ok(port) => port.parse().map_err(|_| std::io::Error::other(format!("PORT is not a port number: {port}")))?,
        Err(_) => port,
    };
    bind(host.as_deref(), port).await
}

/// [`serve`] on listeners of the app's own: anything that implements axum's
/// [`Listener`](axum::serve::Listener), such as a Unix socket or a socket a supervisor hands
/// over. Shutdown is `serve`'s, across all of them: on Ctrl-C or `SIGTERM` the app's tasks are
/// told, live streams end, every listener closes and finishes its requests, and it returns once
/// the tasks and work handlers finish ([`crate::Signal::finish`]) have finished too. HTTPS is [`serve_wrapped`], which runs the handshake on each
/// connection under the app's connection limits; a listener that ran it before handing the
/// connection over would have it outside them.
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
    run(listeners, plain, app, crate::shutdown_signal()).await
}

/// [`serve_on`] with a step of the app's own on each connection before HTTP, such as a TLS
/// handshake: `corten::serve_wrapped(corten::listeners(443).await?, move |tcp| acceptor.accept(tcp), app)`.
/// The step is held to what a connection is: it runs in the connection's own task, so a slow
/// client holds up no other; it counts against [`App::max_connections`] from before the
/// listener's accept; and [`App::header_timeout`] runs from the accept, so a client that stalls
/// before the step is done or after it, before its request head, is closed at the same time. A
/// step that fails closes the connection, and shutdown ends the steps in progress. The library
/// (rustls, native-tls) stays the app's, and so does its configuration: ALPN offers `http/1.1`,
/// what Corten speaks. A step whose future borrows from what it captures clones it first:
/// `move |tcp| { let acceptor = acceptor.clone(); async move { acceptor.accept(tcp).await } }`.
/// The step's error is not read: the connection closes.
pub async fn serve_wrapped<L, W, F, Io, E, M>(listeners: Vec<L>, wrap: W, app: App<M>) -> std::io::Result<()>
where
    L: axum::serve::Listener,
    L::Addr: std::fmt::Debug + 'static,
    W: Fn(L::Io) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Io, E>> + Send + 'static,
    Io: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    M: Middleware,
{
    #[cfg(feature = "log")]
    let _ = tracing_subscriber::fmt().with_target(false).try_init();
    if listeners.is_empty() {
        return Err(std::io::Error::other("corten::serve_wrapped: no listeners to serve on"));
    }
    run(listeners, wrap, app, crate::shutdown_signal()).await
}

/// The step on a connection that has none: the listener's IO itself.
pub(crate) fn plain<I>(io: I) -> std::future::Ready<Result<I, std::convert::Infallible>> {
    std::future::ready(Ok(io))
}

/// Serves `app` on `listeners` with its tasks running until `stop` resolves; then tells the tasks
/// and work handlers finish to stop, lets each listener finish its requests and waits for the
/// tasks, then the work, up to the app's grace from when `stop` resolved. `wrap` is each connection's step before HTTP
/// ([`serve_wrapped`]), [`plain`] for none.
pub(crate) async fn run<L, W, F, Io, E, M>(
    listeners: Vec<L>,
    wrap: W,
    mut app: App<M>,
    stop: impl Future<Output = ()>,
) -> std::io::Result<()>
where
    L: axum::serve::Listener,
    L::Addr: std::fmt::Debug + 'static,
    W: Fn(L::Io) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Io, E>> + Send + 'static,
    Io: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    M: Middleware,
{
    let (tasks, grace, connections) = (std::mem::take(&mut app.tasks), app.grace, app.connections);
    let finishing = app.finishing.clone();
    // Built first: a startup panic (a misplaced area) starts no task.
    let router = app.router();
    let running = Running::start(tasks, finishing.signal().clone());
    let stopped = std::sync::OnceLock::new();
    let served = serve_router(listeners, wrap, router, connections, async {
        stop.await;
        let _ = stopped.set(tokio::time::Instant::now());
        running.stop();
    })
    .await;
    // A listener that failed ends the tasks too, with the same grace.
    let began = stopped.get().copied().unwrap_or_else(tokio::time::Instant::now);
    running.finish(began + grace, grace).await;
    finishing.wait(began + grace, grace).await;
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

/// Serves `router` on every listener, each connection after its step `wrap`, until `stop`
/// resolves, then lets each finish its requests; a listener that fails ends it with the error.
pub(crate) async fn serve_router<L, W, F, Io, E>(
    listeners: Vec<L>,
    wrap: W,
    router: Router,
    connections: Connections,
    stop: impl Future<Output = ()>,
) -> std::io::Result<()>
where
    L: axum::serve::Listener,
    L::Addr: std::fmt::Debug + 'static,
    W: Fn(L::Io) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Io, E>> + Send + 'static,
    Io: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    use futures_util::future::{select, try_join_all, Either};
    let (stopped, watch) = tokio::sync::watch::channel(false);
    let open = connections.max.map(|max| Arc::new(tokio::sync::Semaphore::new(max)));
    let clock = connections.header_timeout.map(crate::watch::Clock::start);
    let wrap = Arc::new(wrap);
    let servers = listeners
        .into_iter()
        .map(|listener| serve_listener(listener, wrap.clone(), router.clone(), clock.clone(), open.clone(), watch.clone()));
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
/// once all have closed. Each connection's step `wrap` runs in its own task before HTTP. With
/// `clock`, a connection waits for a request's head (the step included) only as long as it
/// allows, and with `open`, a connection is accepted only once one of its permits is free.
async fn serve_listener<L, W, F, Io, E>(
    mut listener: L,
    wrap: Arc<W>,
    router: Router,
    clock: Option<Arc<crate::watch::Clock>>,
    open: Option<Arc<tokio::sync::Semaphore>>,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> std::io::Result<()>
where
    L: axum::serve::Listener,
    L::Addr: 'static,
    W: Fn(L::Io) -> F + Send + Sync + 'static,
    F: Future<Output = Result<Io, E>> + Send + 'static,
    Io: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
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
        let (alive, mut stop, wrap) = (alive.clone(), stop.clone(), wrap.clone());
        tokio::spawn(async move {
            let (_permit, _alive) = (permit, alive);
            // The step, until it is done, the clock expires the connection or shutdown begins.
            let io = {
                let mut step = std::pin::pin!(wrap(io));
                let mut stopping = std::pin::pin!(stop.wait_for(|stopped| *stopped));
                let stepped = std::future::poll_fn(|cx| {
                    if let std::task::Poll::Ready(done) = crate::watch::bounded(watch.as_deref(), step.as_mut(), cx) {
                        return std::task::Poll::Ready(done);
                    }
                    stopping.as_mut().poll(cx).map(|_| None)
                });
                match stepped.await {
                    Some(Ok(io)) => io,
                    _ => return,
                }
            };
            let io = hyper_util::rt::TokioIo::new(crate::watch::Watched { io, watch });
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
struct Wrap<S>(Arc<Layer<S>>);

impl<S> Clone for Wrap<S> {
    fn clone(&self) -> Self {
        Wrap(self.0.clone())
    }
}

impl<S: Steps> tower_layer::Layer<Route> for Wrap<S> {
    type Service = Wrapped<S>;

    fn layer(&self, route: Route) -> Wrapped<S> {
        Wrapped { layer: self.0.clone(), route }
    }
}

struct Wrapped<S> {
    layer: Arc<Layer<S>>,
    route: Route,
}

impl<S> Clone for Wrapped<S> {
    fn clone(&self) -> Self {
        Wrapped { layer: self.layer.clone(), route: self.route.clone() }
    }
}

impl<S: Steps> tower_service::Service<Request> for Wrapped<S> {
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
/// area's or API's, then the route and its handler.
pub(crate) struct Next<'a, S: Steps> {
    steps: &'a S,
    scope: S::Scope<'a>,
    route: Route,
}

impl<'a, S: Steps> Next<'a, S> {
    // Not an `async fn`: a layer of its own would move the request once more.
    #[inline]
    pub(crate) fn run(self, req: Request) -> impl Future<Output = Response> + Send + 'a {
        self.steps.run(req, self.route, self.scope)
    }
}

/// The middleware a request goes through: the app's, and an area's or API's where one has its
/// own. Chosen when the router is built, so in an app whose areas and APIs have none a request's
/// future is the app's middleware around the route, as if areas could have none.
pub(crate) trait Steps: Send + Sync + 'static {
    /// What a request carries to [`Steps::run`]: nothing, or the middleware of its area or API.
    type Scope<'a>: Send;

    fn scope<'a>(&'a self, mount: Option<&'a Mount>, own: bool, path: &str) -> Self::Scope<'a>;

    fn run<'a>(&'a self, req: Request, route: Route, scope: Self::Scope<'a>) -> impl Future<Output = Response> + Send + 'a;
}

/// The app's middleware alone: no area or API has any.
struct Plain<M>(M);

impl<M: Middleware> Steps for Plain<M> {
    type Scope<'a> = ();

    #[inline]
    fn scope<'a>(&'a self, _: Option<&'a Mount>, _: bool, _: &str) {}

    #[inline]
    fn run<'a>(&'a self, req: Request, route: Route, (): ()) -> impl Future<Output = Response> + Send + 'a {
        self.0.call(req, crate::middleware::Handle(route))
    }
}

/// The app's middleware, then the request's area's or API's.
struct Scoped<M> {
    app: M,
    /// [`Area::middleware`], with each area's enclosing areas', innermost prefix first.
    areas: Vec<(&'static str, crate::middleware::Stack)>,
}

impl<M: Middleware> Steps for Scoped<M> {
    type Scope<'a> = &'a [Arc<dyn crate::middleware::Erased>];

    fn scope<'a>(&'a self, mount: Option<&'a Mount>, own: bool, path: &str) -> Self::Scope<'a> {
        match mount {
            Some(mount) => &mount.middleware,
            None if own => &[],
            None => self.areas.iter().find(|(prefix, _)| crate::api::under(prefix, path)).map_or(&[], |(_, stack)| stack),
        }
    }

    #[inline]
    fn run<'a>(&'a self, req: Request, route: Route, stack: Self::Scope<'a>) -> impl Future<Output = Response> + Send + 'a {
        use futures_util::future::Either;
        match stack {
            [] => Either::Left(self.app.call(req, crate::middleware::Handle(route))),
            stack => Either::Right(self.app.call(req, crate::middleware::Rest { stack, route })),
        }
    }
}

/// Everything `App` puts around a request, as one middleware: one boxed future per request,
/// where separate security, compression and error layers cost one each.
struct Layer<S> {
    shared: Arc<Shared>,
    deadline: Option<Duration>,
    /// [`Area::deadline`]s, innermost prefix first; empty unless an area set one.
    area_deadlines: Vec<(&'static str, Option<Duration>)>,
    policy: Applied,
    mounts: Vec<Mount>,
    own: Own,
    /// The app's middleware, and the areas' and APIs' where some have their own.
    steps: S,
}

impl<S: Steps> Layer<S> {
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
            self.policy.headers(&mut res, None, &self.policy.csp);
            res.headers_mut().insert(header::CACHE_CONTROL, cache::PRIVATE);
            return res;
        }
        let path = req.uri().path();
        let own = self.own.contains(path);
        let mount = if own { None } else { self.mounts.iter().find(|mount| mount.contains(path)) };
        // The policy here, and the deadline: an area's when it sets one. An API in an area keeps
        // its own CSP but sends the area's referrer policy.
        let (csp, shell) = if own { (&self.policy.csp, 0) } else { self.policy.csp_for(path) };
        let deadline = match mount {
            Some(mount) => mount.deadline.unwrap_or(self.deadline),
            None if own || self.area_deadlines.is_empty() => self.deadline,
            None => {
                self.area_deadlines.iter().find(|(prefix, _)| crate::api::under(prefix, path)).map_or(self.deadline, |(_, d)| *d)
            }
        };
        // The middleware of the area or API the request is in, where one has some.
        let scope = self.steps.scope(mount, own, path);
        if let Some(mount) = mount {
            if let Some(mut res) = mount.preflight(req.method(), req.headers()) {
                self.policy.headers(&mut res, None, csp);
                res.headers_mut().insert(header::CACHE_CONTROL, cache::PRIVATE);
                return res;
            }
        }
        // A debug build holds a write's redirect to the page's `form-action`, as a browser does.
        #[cfg(debug_assertions)]
        if mount.is_none() {
            if let Some(targets) = self.policy.form_targets(req.method(), csp, req.headers()) {
                req.extensions_mut().insert(targets);
            }
        }
        // A page's nonce is made only if its HTML asks for one; an API route never has one.
        let nonce = mount.is_none().then(|| NonceSlot::new(shell));
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
            self.policy.headers(&mut refused, nonce.as_ref(), csp);
            refused.headers_mut().insert(header::CACHE_CONTROL, cache::PRIVATE);
            return refused;
        }
        let cors = mount.and_then(|mount| mount.allowed_origin(req.headers()));
        let credentials = cache::credentials(req.headers());
        let path = self.shared.dev.then(|| req.uri().path().to_string());
        let (mut res, caller, mode) = self.run(req, route, mount, deadline, scope).await;
        // First: the security headers reserve room for `Cache-Control` and `Vary` too.
        self.policy.headers(&mut res, nonce.as_ref(), csp);
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
    async fn run<'a>(
        &'a self,
        mut req: Request,
        route: Route,
        api: Option<&Mount>,
        deadline: Option<Duration>,
        scope: S::Scope<'a>,
    ) -> (Response, bool, Mode) {
        // Dropped with this future if the client leaves first; then the response's.
        // The zone the browser reported, read once: `req.zone()`, and with `zoned_dates` the
        // page and a live part's fragments are written in it (none for a `Public` answer,
        // `Signal::zone`).
        #[cfg(feature = "zones")]
        let signal = match self.shared.client_zone {
            true => Signal::for_app_in(self.shared.clone(), crate::client::ZoneName::of(req.headers())),
            false => Signal::for_app(self.shared.clone()),
        };
        #[cfg(not(feature = "zones"))]
        let signal = Signal::for_app(self.shared.clone());
        let watch = Watch(signal);
        req.extensions_mut().insert(watch.0.clone());
        let mode = page::mode_of(req.headers());
        let next = Next { steps: &self.steps, scope, route };
        let res = page::render_errors(&self.shared, req, next, deadline, &watch.0, api, mode).await;
        let personal = watch.0.is_personal();
        #[cfg(feature = "zones")]
        let zoned = self.shared.zoned_dates;
        #[cfg(not(feature = "zones"))]
        let zoned = false;
        (signal::follow(res, watch, zoned), personal, mode)
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
    files: Option<StaticFiles>,
    dev: bool,
    components: &[crate::Component],
    sections: &[(&str, &[crate::Component])],
) -> Option<Assets> {
    if files.is_none() && components.is_empty() && sections.is_empty() {
        return None;
    }
    let owner = files.map(|f| f.owner);
    let files = match files {
        // Under `corten dev`, the directory as it is now (a debug build knows where `embed!` read
        // it), and `corten dev` is told where it is, so a change to it alone restarts the app.
        Some(StaticFiles { dir, .. }) if dev && !dir.is_empty() && std::path::Path::new(dir).is_dir() => {
            if let Some(list) = std::env::var_os("CORTEN_DEV_ASSETS") {
                use std::io::Write;
                // Appended: a program with two apps lists both directories.
                let _ =
                    std::fs::OpenOptions::new().create(true).append(true).open(list).and_then(|mut file| writeln!(file, "{dir}"));
            }
            crate::assets::read_dir(std::path::Path::new(dir))
                .unwrap_or_else(|error| panic!("corten: could not read the static files in {dir}: {error}"))
        }
        Some(StaticFiles { files, .. }) => files(),
        None => Vec::new(),
    };
    match Assets::build_with(files, components, sections, "/assets") {
        Ok(mut assets) => {
            assets.owner = owner;
            Some(assets)
        }
        Err(error) => panic!("corten: could not serve the static files and the components: {error}"),
    }
}

/// The static directory `App::assets` was given.
#[derive(Clone, Copy)]
struct StaticFiles {
    /// Where `embed!` read it, in a debug build (empty in a release build).
    dir: &'static str,
    /// The `embed!` struct, which its files name.
    owner: &'static str,
    /// Its files, as compiled in.
    files: fn() -> Vec<(String, axum::body::Bytes)>,
}

#[cfg(test)]
mod tests {
    use super::{load_assets, StaticFiles};

    fn none() -> Vec<(String, axum::body::Bytes)> {
        Vec::new()
    }

    #[test]
    fn dev_reads_the_static_directory_from_disk_and_a_release_the_files_compiled_in() {
        let files =
            StaticFiles { dir: concat!(env!("CARGO_MANIFEST_DIR"), "/src/assets/fixture"), owner: "tests::Static", files: none };
        let dev = load_assets(Some(files), true, &[], &[]).expect("assets");
        assert!(dev.contains("style.css") && dev.contains("icons/192.png"));
        assert_eq!(dev.owner, Some("tests::Static"));
        let release = load_assets(Some(files), false, &[], &[]).expect("assets");
        assert!(!release.contains("style.css"), "compiled in: nothing here");
        for elsewhere in [concat!(env!("CARGO_MANIFEST_DIR"), "/src/assets/not-here"), ""] {
            let files = StaticFiles { dir: elsewhere, ..files };
            assert!(!load_assets(Some(files), true, &[], &[]).expect("assets").contains("style.css"), "{elsewhere}");
        }
    }
}
