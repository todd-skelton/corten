//! [`App`]: an app's routes with the production pieces around them, configured by defaults.

use std::{
    any::{type_name, TypeId},
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use axum::{
    extract::Request,
    http::{Extensions, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures_util::{stream, StreamExt};

use crate::{
    assets::Assets,
    page::{self, default_error_page, default_shell, error_page_fn, IntoShell, Shared, ShellFn},
    pwa::Pwa,
    security::Policy,
    view::Render,
    Meta,
};

type ContextFuture = Pin<Box<dyn Future<Output = Box<dyn FnOnce(&mut Extensions) + Send>> + Send>>;
type ContextFn = Arc<dyn Fn(&HeaderMap) -> ContextFuture + Send + Sync>;
type ReadyFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

/// An app: its routes, plus everything around them that every production app needs, on by
/// default. `App::new(routes).serve().await` gives:
///
/// - security headers, a CSP with a per-request nonce, and a check that refuses cross-site writes
///   ([`Policy::new`]; change it with [`App::policy`]);
/// - the shell around every page ([`crate::default_shell`] until [`App::shell`]);
/// - error pages for failed requests and unknown URLs, in the request's mode, with the error
///   shown under `corten dev`;
/// - the files in `static/` (or `STATIC_DIR`), the scripts of the app's islands and its
///   components' stylesheets joined into one file ([`App::components`]), at content-versioned,
///   immutable URLs, each island's script preloaded on the pages that render it;
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
        }
    }

    /// The app's components with a script or a stylesheet (`#[island]`, `#[component(css)]`,
    /// `style::tokens!`), each listed by its constant: `.components(&[TOKENS, OfferList,
    /// ListingSort])`, or a library's list, `.components(ui::COMPONENTS)`. Call it once per list;
    /// the order is the stylesheets' order, so list tokens and libraries first.
    ///
    /// Scripts are served with the static files and preloaded on the pages that render them;
    /// the stylesheets are one file, `components.css`, linked from every document's `<head>`.
    /// A component listed twice counts once.
    pub fn components(mut self, components: &[crate::Component]) -> Self {
        self.components.extend_from_slice(components);
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

    /// Who is asking: `context(headers)` runs before every request (outside the error pages, so
    /// they see it too), and a `Some` value is put in the request extensions, where the shell's
    /// second parameter, `req.context::<C>()` and axum's `Extension<C>` find it. Typically the
    /// signed-in user, read from a session cookie.
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
                Box::new(move |extensions: &mut Extensions| {
                    if let Some(value) = value {
                        extensions.insert(value);
                    }
                }) as Box<dyn FnOnce(&mut Extensions) + Send>
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
        let assets = load_assets(self.static_dir, &self.components).map(Arc::new);
        let mut head = String::new();
        if let Some(href) = assets.as_ref().and_then(|a| a.stylesheet()) {
            head.push_str(&format!(r#"<link rel="stylesheet" href="{href}">"#));
        }
        let mut routes = self.routes.merge(crate::routes());
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
            routes = routes.merge(assets.routes());
        }
        let ready = self.ready.unwrap_or_else(|| Arc::new(|| Box::pin(async { true })));
        routes = routes.merge(crate::health(move || ready()));

        let error_page = self.error_page.unwrap_or_else(|| error_page_fn(default_error_page));
        let mut shared = Shared::new(self.shell, error_page, assets, head);
        shared.offline = offline;
        shared.locales = self.locales;
        shared.dev = dev;
        shared.components = self.components.iter().map(|c| c.name).collect();
        let shared = Arc::new(shared);
        let context = self.context.map(|(f, _, _)| f);
        let routes = routes.layer(middleware::from_fn(move |mut req: Request, next: Next| {
            let shared = shared.clone();
            let context = context.clone();
            async move {
                if let Some(context) = &context {
                    let insert = context(req.headers()).await;
                    insert(req.extensions_mut());
                }
                req.extensions_mut().insert(shared.clone());
                page::render_errors(shared, req, next).await
            }
        }));
        let routes = self.policy.apply(routes);
        #[cfg(feature = "compress")]
        let routes = crate::compress::apply(routes);
        routes
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

fn load_assets(dir: Option<PathBuf>, components: &[crate::Component]) -> Option<Assets> {
    let explicit = dir.is_some() || std::env::var_os("STATIC_DIR").is_some();
    let dir = dir.or_else(|| std::env::var_os("STATIC_DIR").map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("static"));
    if !dir.is_dir() {
        assert!(!explicit, "corten: the static directory {} does not exist", dir.display());
        if components.is_empty() {
            return None;
        }
    }
    let found = dir.is_dir().then_some(dir.as_path());
    match Assets::build(found, components, "/assets") {
        Ok(assets) => Some(assets),
        Err(error) => panic!("corten: could not load the static files in {} and the components: {error}", dir.display()),
    }
}
