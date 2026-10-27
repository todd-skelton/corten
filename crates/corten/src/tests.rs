use super::*;
use crate::cache::Cache;
use crate::view::{html, Render};
use axum::{body::Body, extract::Path, http::Request, routing::get, Router};
use http_body_util::BodyExt;
use std::time::Duration;
use tower::ServiceExt;

fn demo(text: &str) -> impl Render + '_ {
    html! {
        <p>{text}</p>
        <sort-table><b>"x"</b></sort-table>
        <corten-live src="/x"></corten-live>
    }
}

/// Typed routes with no parameters, for the tests' areas and APIs: `paths! { Api = "/api" }`.
macro_rules! paths {
    ($($name:ident = $path:tt),* $(,)?) => {$(
        #[derive(crate::Route)]
        #[route($path)]
        struct $name;
    )*};
}

paths! {
    ApiRoot = "/api",
    ApiMissing = "/api/missing",
    ApiDown = "/api/down",
    ApiTeapot = "/api/teapot",
    ApiSlow = "/api/slow",
    ApiPost = "/api/post",
    ApiRates = "/api/rates",
    ApiSignUp = "/api/sign-up",
    ApiItems = "/api/items",
    Items = "/items",
    Admin = "/admin",
}

std::thread_local! {
    /// How many times this test's thread looked up a [`Viewer`] (each `#[tokio::test]` runs on
    /// its own thread).
    static LOOKUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The signed-in user, found by the `viewer` header or, as an app finds its session, the
/// `viewer` cookie.
#[derive(Clone, Hash, PartialEq, Debug)]
struct Viewer(&'static str);

impl<S: Sync> crate::Identify<S> for Viewer {
    async fn identify(parts: &axum::http::request::Parts, _: &S) -> std::result::Result<Self, crate::Refused> {
        LOOKUPS.with(|n| n.set(n.get() + 1));
        let header = parts.headers.get("viewer").and_then(|v| v.to_str().ok());
        match header.or_else(|| crate::security::cookie::get(&parts.headers, "viewer")) {
            None => Err(crate::Refused::sign_in(&caller_url::SignIn)),
            Some("down") => Err(crate::Refused::unavailable("the session store is down")),
            Some(name) => Ok(Viewer(name.to_owned().leak())),
        }
    }
}

/// Sends every request as `viewer`, by the header [`Viewer`] reads.
struct AsViewer(Option<&'static str>);

impl crate::Middleware for AsViewer {
    async fn call(&self, mut req: axum::extract::Request, next: impl crate::Next) -> Response {
        if let Some(name) = self.0 {
            req.headers_mut().insert("viewer", axum::http::HeaderValue::from_static(name));
        }
        next.run(req).await
    }
}

/// The tests' layout: the page's title and head tags, and a header that marks the current page.
#[crate::component]
pub(crate) fn Shell(req: &Req, title: &str, head: impl Render, children: impl Render) -> impl Document {
    html! {
        <!DOCTYPE html>
        <html lang={req.lang()}>
            <head>
                <meta charset="utf-8">
                <title>{title}</title>
                {head}
                <Corten />
            </head>
            <body>
                <header><a href={Root} aria-current={req.current(&Root)}>"home"</a></header>
                <main>{children}</main>
            </body>
        </html>
    }
}

/// `req.page` in the tests' layout with no head tags of the page's own.
pub(crate) trait TestPage {
    fn test_page(&self, title: &str, main: impl Render) -> HtmlResult;
}

impl TestPage for Req {
    fn test_page(&self, title: &str, main: impl Render) -> HtmlResult {
        self.page(Shell { req: self, title, head: "", children: main })
    }
}

/// The tests' error page, in their layout, for who is asking, with the error under `corten dev`.
async fn test_error_page(failure: Failure, req: Req, viewer: Option<crate::Caller<Viewer>>) -> HtmlResult {
    let viewer = viewer.map(|crate::Caller(Viewer(name))| name);
    req.page(test_error_document(&req, failure, viewer))
}

fn test_error_document<'a>(req: &'a Req, failure: Failure, viewer: Option<&'static str>) -> impl Document + 'a {
    let title = match (&failure, failure.status()) {
        (Failure::Unidentified { .. }, _) => "Who you are cannot be checked",
        (_, StatusCode::NOT_FOUND) => "Not found",
        (_, StatusCode::FORBIDDEN) => "Not allowed",
        _ => "Something went wrong",
    };
    let detail = failure.detail().map(str::to_owned);
    Shell {
        req,
        title,
        head: "",
        children: html! {
            <h1>{title}</h1>
            if let Some(viewer) = viewer {
                <p>"Signed in as "{viewer}</p>
            }
            if let Some(detail) = &detail {
                <pre>{detail}</pre>
            }
        },
    }
}

#[derive(crate::Route)]
#[route("/")]
struct Root;

#[derive(crate::Route)]
#[route("/done")]
struct Done;

fn meta_content<'a>(html: &'a str, name: &str) -> &'a str {
    let prefix = format!(r#"<meta name="{name}" content=""#);
    let start = html.find(&prefix).expect("meta tag") + prefix.len();
    &html[start..start + html[start..].find('"').unwrap()]
}

async fn call(router: Router, method: &str, mode: Option<&str>) -> (StatusCode, axum::http::HeaderMap, String) {
    call_with(router, method, "/", mode, None).await
}

async fn call_with(
    router: Router,
    method: &str,
    uri: &str,
    mode: Option<&str>,
    accept: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(mode) = mode {
        req = req.header("corten", mode);
    }
    if let Some(accept) = accept {
        req = req.header("accept", accept);
    }
    let res = router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let (parts, body) = res.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    (parts.status, parts.headers, String::from_utf8(bytes.to_vec()).unwrap())
}

fn routes() -> Router {
    Router::new().route(
        "/",
        get(|req: Req| async move {
            let head = html! { <meta name="description" content="About A"> };
            req.page(Shell { req: &req, title: "A & B", head, children: demo("<script>") })
        })
        .post(|req: Req| async move {
            if req.mode == Mode::Action {
                req.patch(html! { <p id={MsgId}>"saved"</p> })
            } else {
                req.redirect(&Done)
            }
        }),
    )
}

/// The app as `App` assembles it, with the tests' error page, every request sent as `viewer`.
fn app(routes: Router, viewer: Option<&'static str>) -> Router {
    App::new(Routes::from(routes)).error_page(test_error_page, ()).middleware(AsViewer(viewer)).router()
}

/// The status and body of `GET uri` with `Host: host` (none when empty), plus `extra` headers.
async fn for_host(router: &Router, uri: &str, host: &str, extra: &[(&str, &str)]) -> (StatusCode, String) {
    let mut req = Request::get(uri);
    if !host.is_empty() {
        req = req.header("host", host);
    }
    for (name, value) in extra {
        req = req.header(*name, *value);
    }
    let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    let status = res.status();
    (status, String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
}

/// A DNS-rebound page names a host the app is not: every route, in every mode, answers 421 and
/// runs nothing of the app's, not even its error page.
#[tokio::test]
async fn a_request_for_another_host_is_misdirected_in_every_mode() {
    let routes = Router::new().route("/", get(|req: Req| async move { req.test_page("Home", html! { <p>"home"</p> }) }));
    let router = App::new(Routes::from(routes))
        .error_page(test_error_page, ())
        .api::<ApiRoot>(|api| api.route::<ApiRates>(crate::Methods(get(|| async { "rates" }))))
        .router();
    let rebound = "rebound.attacker.example:3917";
    for (uri, extra) in [
        ("/", &[][..]),
        ("/", &[("corten", "nav")][..]),
        ("/", &[("accept", "application/json")][..]),
        ("/missing", &[("accept", "text/html")][..]),
        ("/api/rates", &[][..]),
        ("/health/live", &[][..]),
    ] {
        let (status, body) = for_host(&router, uri, rebound, extra).await;
        assert_eq!((status, body.as_str()), (StatusCode::MISDIRECTED_REQUEST, "Misdirected request."), "{uri} {extra:?}");
    }
    for host in ["localhost:3917", "127.0.0.1:3917", "[::1]:3917", ""] {
        assert_eq!(for_host(&router, "/", host, &[]).await.0, StatusCode::OK, "{host:?}");
    }
    let named = App::new(Routes::from(Router::new().route("/", get(|| async { "home" }))))
        .policy(crate::security::Policy::new().host("shop.example"))
        .router();
    assert_eq!(for_host(&named, "/", "shop.example", &[]).await.0, StatusCode::OK);
    assert_eq!(for_host(&named, "/", "www.shop.example", &[]).await.0, StatusCode::MISDIRECTED_REQUEST);
    let any = App::new(Routes::from(Router::new().route("/", get(|| async { "home" }))))
        .policy(crate::security::Policy::new().any_host())
        .router();
    assert_eq!(for_host(&any, "/", rebound, &[]).await.0, StatusCode::OK);
}

#[tokio::test]
async fn a_live_stream_that_finishes_tells_the_element_to_stop() {
    let router = Router::new()
        .route("/", get(|| async { crate::live(futures_util::stream::iter([String::from(r#"<p id="late">Ready</p>"#)])) }));
    let (status, headers, body) = call(router, "GET", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "text/event-stream");
    assert_eq!(body, "event: patch\ndata: <p id=\"late\">Ready</p>\n\nevent: end\ndata: end\n\n");
}

#[tokio::test]
async fn a_live_fragment_carries_its_cursor_and_a_reconnect_reads_it_back() {
    use crate::{Fragment, LiveSince};
    let fragments = || {
        futures_util::stream::iter([
            Fragment::new(r#"<p id="n">7</p>"#.into()).cursor("v7"),
            Fragment::new(r#"<p id="n">8</p>"#.into()),
        ])
    };
    let router = Router::new().route("/", get(move || async move { crate::live(fragments()) }));
    let (_, _, body) = call(router, "GET", None).await;
    // The id carries over in the browser to a fragment that has none, as SSE defines.
    assert_eq!(
        body,
        "event: patch\nid: v7\ndata: <p id=\"n\">7</p>\n\nevent: patch\ndata: <p id=\"n\">8</p>\n\nevent: end\ndata: end\n\n"
    );

    let since = |uri: &str, header: Option<&str>| {
        let mut req = Request::builder().uri(uri);
        if let Some(header) = header {
            req = req.header("last-event-id", header);
        }
        let (mut parts, ()) = req.body(()).unwrap().into_parts();
        async move { <LiveSince as axum::extract::FromRequestParts<()>>::from_request_parts(&mut parts, &()).await.unwrap().0 }
    };
    assert_eq!(since("/live", None).await, None, "a first connection without since");
    assert_eq!(since("/live?since=a%20b&x=1", None).await.as_deref(), Some("a b"), "the page's cursor");
    assert_eq!(since("/live?since=page", Some("v9")).await.as_deref(), Some("v9"), "a reconnect's cursor wins");
    assert_eq!(since("/live?since=page", Some("")).await.as_deref(), Some("page"), "an empty Last-Event-ID is none");
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "line break")]
fn a_cursor_cannot_end_its_event() {
    let _ = crate::Fragment::new(String::new()).cursor("a\nevent: patch");
}

#[test]
#[cfg(not(debug_assertions))]
fn a_cursor_that_would_end_its_event_is_left_off_in_release() {
    assert_eq!(crate::Fragment::new(String::new()).cursor("a\nevent: patch").cursor, None);
}

#[tokio::test]
async fn a_bare_router_renders_the_page_s_document_without_a_policy() {
    let (status, headers, body) = call(routes(), "GET", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["vary"], "corten, accept");
    assert!(body.starts_with(r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">"#), "{body}");
    assert!(body.contains(r#"<title>A &amp; B</title><meta name="description" content="About A" data-corten-head>"#));
    assert!(body.contains(&format!(r#"<script type="module" src="{}"></script>"#, runtime_path())));
    assert!(!body.contains("modulepreload"), "a page without handlers preloads no scripts");
    assert!(body.contains("<p>&lt;script&gt;</p>"), "page text is escaped");
    assert!(!body.contains("nonce"), "no policy, no nonce");
}

#[tokio::test]
async fn navigation_returns_title_head_tags_and_page_only() {
    let (_, headers, body) = call(app(routes(), None), "GET", Some("nav")).await;
    assert_eq!(headers["corten-page"], "1");
    assert!(body.starts_with(r#"<title>A &amp; B</title><meta name="description" content="About A" data-corten-head><p>"#));
    assert!(!body.contains("<html"), "{body}");
}

#[tokio::test]
async fn action_returns_patch_and_plain_post_redirects() {
    let (_, headers, body) = call(app(routes(), None), "POST", Some("action")).await;
    assert_eq!(headers["corten-patch"], "1");
    assert_eq!(body, r#"<p id="msg">saved</p>"#);
    let (status, headers, _) = call(app(routes(), None), "POST", None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers["location"], "/done");
}

#[cfg(feature = "json")]
#[derive(crate::json::ToJson)]
struct Data {
    text: &'static str,
}

#[cfg(feature = "json")]
#[tokio::test]
async fn screen_is_html_for_browsers_and_json_for_data_clients() {
    let router = app(
        Router::new().route(
            "/",
            get(|req: Req| async move {
                let data = Data { text: "<b>" };
                req.screen(&data, Shell { req: &req, title: "A & B", head: "", children: demo(data.text) })
            }),
        ),
        None,
    );
    let browser = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
    let (_, headers, body) = call_with(router.clone(), "GET", "/", None, Some(browser)).await;
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(body.starts_with("<!DOCTYPE html>"));
    let (_, headers, body) = call_with(router.clone(), "GET", "/", Some("nav"), Some("application/json")).await;
    assert_eq!(headers["corten-page"], "1", "the runtime's mode header wins over Accept");
    assert!(body.starts_with("<title>"));
    let (status, headers, body) = call_with(router, "GET", "/", None, Some("application/json; charset=utf-8")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(headers["vary"], "corten, accept");
    assert_eq!(body, r#"{"title":"A & B","data":{"text":"<b>"}}"#);
}

#[tokio::test]
async fn page_answers_data_requests_with_the_document() {
    let (_, headers, body) = call_with(app(routes(), None), "GET", "/", None, Some("application/json")).await;
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(body.starts_with("<!DOCTYPE html>"));
}

#[tokio::test]
async fn runtime_is_served_at_a_versioned_immutable_url() {
    assert!(runtime_path().starts_with(&format!("/corten/{}/", runtime_version())));
    let res = app(routes(), None).oneshot(Request::get(runtime_path()).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["cache-control"], "public, max-age=31536000, immutable");
}

/// A layout with an inline script of its own, which takes the request's nonce.
#[crate::component]
fn Inline<'a>(req: &'a Req) -> impl Document + 'a {
    html! {
        <html>
            <head>
                <title>"Inline"</title>
                <Corten />
                <script nonce={req.nonce()}>"1"</script>
            </head>
            <body><main></main></body>
        </html>
    }
}

/// A page in the `Inline` layout, marked for shared caches.
async fn inline_page(req: Req) -> Public<HtmlResult> {
    Public::new(std::time::Duration::from_secs(60), req.page(Inline { req: &req }))
}

#[tokio::test]
async fn the_csp_allows_the_import_map_by_hash_and_a_nonce_only_when_the_shell_asks() {
    let (_, headers, body) = call(app(routes(), None), "GET", None).await;
    let csp = headers["content-security-policy"].to_str().unwrap();
    let import_map = security::hash_source(&crate::import_map(None, None));
    assert!(csp.contains(&format!("script-src 'self' {import_map};")) && !csp.contains("nonce"), "{csp}");
    assert!(body.contains(&format!(r#"<script type="importmap">{}</script>"#, crate::import_map(None, None))));
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["cross-origin-opener-policy"], "same-origin");
    let (_, again, same) = call(app(routes(), None), "GET", None).await;
    assert_eq!(
        (again["content-security-policy"].clone(), same),
        (headers["content-security-policy"].clone(), body),
        "the same bytes"
    );

    let inline = || App::new(Routes::from(Router::new().route("/", get(inline_page)))).router();
    let (_, headers, body) = call(inline(), "GET", None).await;
    let csp = headers["content-security-policy"].to_str().unwrap();
    let nonce = csp.split("'nonce-").nth(1).unwrap().split('\'').next().unwrap();
    assert!(body.contains(&format!(r#"<script nonce="{nonce}">1</script>"#)), "{body}");
    let (_, again, _) = call(inline(), "GET", None).await;
    assert_ne!(again["content-security-policy"], headers["content-security-policy"], "a fresh nonce per request");
    let popups = App::new(Routes::from(routes())).policy(security::Policy::new().allow_popups()).router();
    let (_, popups, _) = call(popups, "GET", None).await;
    assert_eq!(popups["cross-origin-opener-policy"], "same-origin-allow-popups");
}

#[tokio::test]
async fn the_app_refuses_cross_site_writes() {
    let router = app(routes(), None);
    let req = |site: &'static str| {
        Request::post("/").header("sec-fetch-site", site).header("corten", "action").body(Body::empty()).unwrap()
    };
    assert_eq!(router.clone().oneshot(req("cross-site")).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(router.oneshot(req("same-origin")).await.unwrap().status(), StatusCode::OK);
}

fn failing() -> Router {
    Router::new()
        .route("/", get(|| async { Err::<Response, _>(Error::new("database is down")) }))
        .route("/missing", get(|| async { Err::<Response, _>(Error::status(StatusCode::NOT_FOUND)) }))
        .route("/forbidden", get(|| async { Err::<Response, _>(Error::status(StatusCode::FORBIDDEN)) }))
}

#[tokio::test]
async fn errors_render_in_the_shell_for_each_mode_without_leaking_the_message() {
    let (status, headers, body) = call(app(failing(), None), "GET", None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(body.contains("<title>Something went wrong</title>"), "{body}");
    assert_eq!(headers["x-robots-tag"], "noindex", "search engines skip every error page");
    assert!(!body.contains("database"), "the error message stays in the log");
    let (_, headers, body) = call(app(failing(), None), "GET", Some("nav")).await;
    assert_eq!(headers["corten-page"], "1");
    assert!(body.starts_with("<title>Something went wrong</title>"));
    let (_, headers, body) = call_with(app(failing(), None), "GET", "/", None, Some("application/json")).await;
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(body, r#"{"error":{"status":500,"message":"Internal Server Error"}}"#, "the message stays in the log");
    let (status, _, body) = call_with(app(failing(), None), "GET", "/missing", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("<title>Not found</title>"));
}

#[tokio::test]
async fn an_extractor_rejection_gets_the_error_page_in_a_browser_and_its_text_in_dev() {
    let routes =
        || Router::new().route("/n/{n}", get(|axum::extract::Path(n): axum::extract::Path<u32>| async move { n.to_string() }));
    let html = Some("text/html,application/xhtml+xml");
    let (status, headers, body) = call_with(app(routes(), None), "GET", "/n/abc", None, html).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(body.contains("<title>Something went wrong</title>") && !body.contains("parse"), "{body}");
    let (_, headers, body) = call_with(app(routes(), None), "GET", "/n/abc", Some("nav"), None).await;
    assert_eq!(headers["corten-page"], "1", "a navigation swaps the error page in: {body}");
    let (_, _, body) = call_with(App::new(Routes::from(routes())).dev(true).router(), "GET", "/n/abc", None, html).await;
    assert!(body.contains("<pre>Invalid URL"), "the default error page shows it: {body}");
    let (status, headers, body) = call_with(app(routes(), None), "GET", "/n/abc", None, Some("application/json")).await;
    assert_eq!((status, headers["content-type"].to_str().unwrap()), (StatusCode::BAD_REQUEST, "application/json"));
    assert!(body.starts_with(r#"{"error":{"status":400,"message":"Invalid URL"#), "a data client gets the text as JSON: {body}");
    let (_, _, body) = call_with(app(routes(), None), "GET", "/n/abc", None, None).await;
    assert!(body.starts_with("Invalid URL"), "so does a client that does not ask for HTML: {body}");
    let (_, _, body) = call_with(app(routes(), None), "GET", "/n/7", None, None).await;
    assert_eq!(body, "7", "a plain-text success is left alone");
}

#[tokio::test]
async fn dev_mode_shows_errors_and_reloads_pages_when_the_server_restarts() {
    let dev = || {
        App::new(Routes::from(failing().route("/ok", get(|req: Req| async move { req.test_page("ok", html! { "ok" }) }))))
            .dev(true)
    };
    let (_, _, body) = call(dev().router(), "GET", None).await;
    assert!(body.contains("<pre>database is down</pre>"), "{body}");

    let router = dev().router();
    let (_, _, page) = call_with(router.clone(), "GET", "/ok", None, None).await;
    let boot = meta_content(&page, "corten-dev").to_string();
    let res = router.oneshot(Request::get("/corten/dev").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.headers()["content-type"], "text/event-stream");
    let mut body = res.into_body();
    let mut events = String::new();
    while !events.contains("\n\n") {
        events.push_str(std::str::from_utf8(&body.frame().await.unwrap().unwrap().into_data().unwrap()).unwrap());
    }
    // Each server start has its own boot id; the page compares it to the one it was served with.
    assert!(!boot.is_empty() && events.contains(&format!("data: {boot}\n")), "{events}");

    let prod = App::new(Routes::from(failing())).dev(false).router();
    let (_, _, body) = call(prod.clone(), "GET", None).await;
    assert!(!body.contains("database") && !body.contains("corten-dev"));
    let (status, _, _) = call_with(prod, "GET", "/corten/dev", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unknown_urls_and_methods_get_the_error_page_and_the_app_can_replace_it() {
    let (status, _, body) = call_with(app(routes(), Some("ada")), "GET", "/nope", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("<title>Not found</title>") && body.contains(">home</a>"), "in the shell: {body}");
    let (status, headers, body) = call_with(app(routes(), None), "DELETE", "/", None, None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(headers.contains_key("allow") && body.contains("Something went wrong"));
    let custom = App::new(Routes::from(routes())).error_page(oops, ()).router();
    let (_, _, body) = call_with(custom, "GET", "/nope", None, None).await;
    assert!(body.contains("<title>Oops 404</title>") && body.contains("<h1>Lost?</h1>"));
}

/// An error page of the app's own, with no arguments but the failure and the request.
async fn oops(failure: Failure, req: Req) -> HtmlResult {
    let code = failure.status().as_u16();
    req.page(html! {
        <html>
            <head>
                <title>"Oops "{code}</title>
                <Corten />
            </head>
            <body><main><h1>"Lost?"</h1></main></body>
        </html>
    })
}

#[tokio::test]
async fn a_handler_that_panics_gets_the_error_page_and_the_server_keeps_serving() {
    let routes = Router::new().route("/", get(|req: Req| async move { req.test_page("Home", html! { <p>"home"</p> }) })).route(
        "/panic",
        get(|| async {
            if true {
                panic!("index out of bounds")
            } else {
                "never"
            }
        }),
    );
    let router = app(routes, None);
    let (status, headers, body) = call_with(router.clone(), "GET", "/panic", None, None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(body.contains("<title>Something went wrong</title>") && !body.contains("bounds"), "{body}");
    let (_, headers, body) = call_with(router.clone(), "GET", "/panic", Some("nav"), None).await;
    assert_eq!(headers["corten-page"], "1", "a navigation swaps the error page in: {body}");
    let (status, _, body) = call_with(router.clone(), "GET", "/panic", None, Some("application/json")).await;
    assert_eq!(
        (status, &body[..]),
        (StatusCode::INTERNAL_SERVER_ERROR, r#"{"error":{"status":500,"message":"Internal Server Error"}}"#)
    );
    let (status, _, _) = call(router, "GET", None).await;
    assert_eq!(status, StatusCode::OK, "the next request is served");
    let (_, _, body) = call_with(
        App::new(Routes::from(Router::new().route(
            "/",
            get(|| async {
                if true {
                    panic!("index out of bounds")
                } else {
                    "never"
                }
            }),
        )))
        .dev(true)
        .router(),
        "GET",
        "/",
        None,
        None,
    )
    .await;
    assert!(body.contains("index out of bounds"), "under corten dev the page shows the panic: {body}");
}

#[tokio::test]
async fn health_reports_liveness_and_readiness() {
    let router = App::new(Routes::new()).ready(|| async { false }).router();
    let get = |uri: &'static str| router.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap());
    assert_eq!(get("/health/live").await.unwrap().status(), StatusCode::OK);
    let ready = get("/health/ready").await.unwrap();
    assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(ready.headers()["cache-control"], "no-store");
}

/// A page that shows who is asking, or that no one is.
fn who_routes() -> Router {
    Router::new().route(
        "/",
        get(|req: Req, viewer: Option<Caller<Viewer>>| async move {
            let who = viewer.map_or("guest", |Caller(v)| v.0);
            req.test_page("Who", html! { <p>{who}</p> })
        }),
    )
}

#[tokio::test]
async fn the_page_key_follows_the_caller_the_handler_read() {
    let (_, headers, guest) = call(app(who_routes(), None), "GET", None).await;
    assert!(guest.contains("<p>guest</p>"), "{guest}");
    let key = meta_content(&guest, "corten-shell");
    assert_eq!(headers["corten-shell"], key, "the document and its header agree");

    let (_, _, ada) = call(app(who_routes(), Some("ada")), "GET", None).await;
    assert!(ada.contains("<p>ada</p>"));
    assert_ne!(meta_content(&ada, "corten-shell"), key, "signing in changes the key");
    let (_, _, bob) = call(app(who_routes(), Some("bob")), "GET", None).await;
    assert_ne!(meta_content(&bob, "corten-shell"), meta_content(&ada, "corten-shell"), "so does someone else");

    let (_, nav, body) = call(app(who_routes(), Some("ada")), "GET", Some("nav")).await;
    assert_eq!(nav["corten-shell"], meta_content(&ada, "corten-shell"), "navigations carry the key");
    assert_eq!(body, "<title>Who</title><p>ada</p>", "a navigation sends only the page");

    let (_, _, anonymous) = call(app(routes(), Some("ada")), "GET", None).await;
    assert_ne!(meta_content(&anonymous, "corten-shell"), meta_content(&ada, "corten-shell"), "a page that reads no caller");
}

#[test]
fn current_links_match_the_page_or_its_section() {
    use crate::page::is_current_for_tests as is_current;
    assert!(is_current("/sell", "/sell", false));
    assert!(is_current("/sell", "/sell?tab=2", false));
    assert!(!is_current("/sell/listings", "/sell", false));
    assert!(is_current("/sell/listings", "/sell", true));
    assert!(is_current("/sell/listings", "/sell/", true));
    assert!(!is_current("/seller", "/sell", true));
}

#[derive(crate::view::Id)]
struct SignUpId;

#[derive(crate::view::Id)]
struct MsgId;

fn alert_form(errors: &Errors) -> impl crate::Patch + '_ {
    let f = SignUp::FIELDS;
    let e = errors.within(SignUpId);
    html! { <form id={SignUpId}><input name={f.email} {..e.aria(f.email)}>{e.message(f.email)}</form> }
}

fn invalid_routes() -> Router {
    Router::new().route(
        "/",
        get(|req: Req| async move {
            let mut errors = Errors::new();
            errors.add(SignUp::FIELDS.email, "Enter an email address.");
            let page = || async { req.test_page("Alert", html! { <p>"page with errors"</p> }) };
            req.invalid(&errors, alert_form(&errors), page).await
        }),
    )
}

#[tokio::test]
async fn invalid_input_answers_422_in_each_mode() {
    let (status, headers, body) = call(app(invalid_routes(), None), "GET", Some("action")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(headers["corten-patch"], "1");
    assert_eq!(
        body,
        r#"<form id="sign-up"><input name="email" aria-invalid="true" aria-describedby="sign-up-email-error"><p class="field-error" id="sign-up-email-error">Enter an email address.</p></form>"#
    );

    let (status, _, body) = call(app(invalid_routes(), None), "GET", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("<p>page with errors</p>") && body.contains("<html"), "no JavaScript: the whole page");

    #[cfg(feature = "json")]
    {
        let (status, _, body) = call_with(app(invalid_routes(), None), "GET", "/", None, Some("application/json")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body,
            r#"{"error":{"status":422,"message":"email: Enter an email address.","fields":{"email":"Enter an email address."}}}"#
        );
    }
}

#[tokio::test]
async fn a_page_that_reads_again_says_so_in_its_head_on_every_surface() {
    use std::time::Duration;
    let routes = Router::new().route(
        "/",
        get(|req: Req| async move {
            req.page(Shell {
                req: &req,
                title: "Saving",
                head: ReadAgain { after: Duration::from_secs(2) },
                children: html! { <p>"Saving"</p> },
            })
        }),
    );
    let tag = r#"<noscript data-corten-read-again="2" data-corten-head><meta http-equiv="refresh" content="2"></noscript>"#;
    let (_, headers, document) = call(app(routes.clone(), None), "GET", None).await;
    assert!(
        document.contains(&format!("<title>Saving</title>{tag}")),
        "in <head>, where the browser reads it without JavaScript: {document}"
    );
    assert!(!headers.contains_key("refresh"), "not the header, which outlives the runtime's navigations");
    let (_, _, swap) = call(app(routes, None), "GET", Some("nav")).await;
    assert!(swap.starts_with(&format!("<title>Saving</title>{tag}<p>")), "a navigation's head tags: {swap}");
}

#[test]
fn read_again_is_whole_seconds_and_works_without_javascript() {
    use std::time::Duration;
    let tag = |after| ReadAgain { after }.to_html();
    assert_eq!(
        tag(Duration::from_secs(2)),
        r#"<noscript data-corten-read-again="2"><meta http-equiv="refresh" content="2"></noscript>"#
    );
    assert!(tag(Duration::from_millis(1500)).contains(r#"read-again="2""#), "rounded up");
    assert!(tag(Duration::ZERO).contains(r#"read-again="1""#), "never sooner than a second");
}

/// Browsers keep a full load's `Refresh` after the runtime navigates away, then load it again.
#[cfg(debug_assertions)]
#[tokio::test]
async fn a_page_with_a_refresh_header_is_an_error_in_debug_builds() {
    let routes = Router::new().route(
        "/",
        get(|req: Req| async move {
            let mut res = req.test_page("Saving", html! { <p>"Saving"</p> })?;
            res.headers_mut().insert("refresh", axum::http::HeaderValue::from_static("2"));
            Ok::<_, crate::Error>(res)
        }),
    );
    for mode in [None, Some("nav")] {
        let res = app(routes.clone(), None).oneshot(request_in(mode)).await.unwrap();
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR, "{mode:?}");
        assert!(!res.headers().contains_key("refresh"), "{mode:?}");
    }
    let api = Router::new().route("/", get(|| async { ([("refresh", "2")], "ok") }));
    assert_eq!(call(app(api, None), "GET", None).await.0, StatusCode::OK, "only pages are checked");
}

fn request_in(mode: Option<&str>) -> Request<Body> {
    let req = Request::builder().uri("/").header("accept", "text/html");
    match mode {
        Some(mode) => req.header("corten", mode),
        None => req,
    }
    .body(Body::empty())
    .unwrap()
}

#[tokio::test]
async fn a_page_that_moved_answers_301_in_every_mode_and_a_form_navigates() {
    #[derive(crate::Route)]
    #[route("/items/{slug}")]
    struct Item<'a> {
        slug: &'a str,
    }
    let routes = Router::new().route("/", get(|req: Req| async move { req.moved(&Item { slug: "pika chu" }) }));
    for (mode, accept) in [(None, Some("text/html")), (Some("nav"), None), (None, Some("application/json"))] {
        let (status, headers, _) = call_with(app(routes.clone(), None), "GET", "/", mode, accept).await;
        assert_eq!(status, StatusCode::MOVED_PERMANENTLY, "{mode:?} {accept:?}");
        assert_eq!(headers["location"], "/items/pika%20chu");
        assert_eq!(headers["cache-control"], "private, max-age=86400", "the browser keeps it, no shared cache");
    }
    let (status, headers, _) = call(app(routes, None), "GET", Some("action")).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "a data-patch form is sent on, as by redirect");
    assert_eq!(headers["corten-location"], "/items/pika%20chu");
}

#[tokio::test]
async fn a_redirect_to_a_place_keeps_its_fragment_in_every_mode() {
    #[derive(crate::Route)]
    #[route("/items/{slug}")]
    struct Item<'a> {
        slug: &'a str,
    }
    #[derive(crate::view::Id)]
    struct PlaceId<'a>(&'a str);
    let routes = Router::new()
        .route("/", get(|req: Req| async move { req.redirect(&Item { slug: "pika chu" }.at(PlaceId("7 & 8"))) }))
        .route("/old", get(|req: Req| async move { req.moved(&Item { slug: "pika" }.at(PlaceId("7"))) }));
    let place = "/items/pika%20chu#place-7%20%26%208";
    let (status, headers, _) = call(app(routes.clone(), None), "GET", None).await;
    assert_eq!((status, &headers["location"]), (StatusCode::SEE_OTHER, &place.parse().unwrap()), "a full load follows it");
    // fetch would follow a 303 and drop the fragment, so the runtime is told where to go.
    for mode in ["nav", "action"] {
        let (status, headers, _) = call(app(routes.clone(), None), "GET", Some(mode)).await;
        assert_eq!((status, &headers["corten-location"]), (StatusCode::NO_CONTENT, &place.parse().unwrap()), "{mode}");
    }
    let (status, headers, _) = call_with(app(routes.clone(), None), "GET", "/old", Some("nav"), None).await;
    assert_eq!((status, &headers["corten-location"]), (StatusCode::NO_CONTENT, &"/items/pika#place-7".parse().unwrap()));
    let (status, headers, _) = call_with(app(routes, None), "GET", "/old", None, Some("text/html")).await;
    assert_eq!((status, &headers["location"]), (StatusCode::MOVED_PERMANENTLY, &"/items/pika#place-7".parse().unwrap()));
}

#[cfg(debug_assertions)]
#[tokio::test]
#[should_panic(expected = "top-level text \"saved\" outside an element")]
async fn a_live_fragment_the_browser_would_drop_panics_in_debug_builds() {
    let router = Router::new().route("/", get(|| async { crate::live(futures_util::stream::iter([String::from("saved")])) }));
    let _ = call(router, "GET", None).await;
}

#[derive(Fields)]
#[allow(dead_code)]
struct SignUp {
    email: String,
    #[serde(rename = "pw")]
    password: String,
    #[serde(skip)]
    internal: u8,
}

#[derive(Fields)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[allow(dead_code)]
pub(crate) struct Shipping {
    pub street_line: String,
    postal_code: Option<Vec<(u8, String)>>,
    r#type: std::collections::HashMap<String, fn(u8) -> u8>,
}

#[test]
fn field_names_follow_serde_and_reach_markup() {
    assert_eq!((SignUp::FIELDS.email.as_str(), SignUp::FIELDS.password.as_str()), ("email", "pw"));
    let f = Shipping::FIELDS;
    assert_eq!((f.street_line.as_str(), f.postal_code.as_str(), f.r#type.as_str()), ("streetLine", "postalCode", "type"));
    let mut errors = Errors::new();
    errors.add(SignUp::FIELDS.email, "Enter an email address.");
    let f = SignUp::FIELDS;
    let e = errors.within(SignUpId);
    let html = html! { <input name={f.email} {..e.aria(f.email)}> }.to_html();
    assert_eq!(html, r#"<input name="email" aria-invalid="true" aria-describedby="sign-up-email-error">"#);
}

#[tokio::test]
async fn error_pages_render_in_the_shell() {
    let (status, _, body) = call_with(app(failing(), Some("ada")), "GET", "/forbidden", None, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains(">home</a>") && body.contains("Not allowed"));
}

#[tokio::test]
async fn the_pwa_links_its_manifest_and_corten_answers_offline() {
    let pwa = crate::pwa::Pwa::new(crate::pwa::Manifest::new("A", "A").theme_color("#123"));
    let router = App::new(Routes::from(routes())).pwa(pwa).router();
    let (_, _, body) = call(router.clone(), "GET", None).await;
    assert!(body.contains(r##"<link rel="manifest" href="/manifest.webmanifest"><meta name="theme-color" content="#123">"##));
    assert!(body.contains(r#"<meta name="corten-sw" content="/service-worker.js">"#));
    let (status, headers, body) = call_with(router.clone(), "GET", "/offline", None, None).await;
    assert_eq!((status, &headers["x-robots-tag"]), (StatusCode::OK, &"noindex".parse().unwrap()));
    assert!(body.contains("Check your connection"), "{body}");
    let (status, _, _) = call_with(router, "GET", "/manifest.webmanifest", None, None).await;
    assert_eq!(status, StatusCode::OK);
}

// ---- App::api: routes that answer only JSON ----

#[derive(crate::Route)]
#[route("/api/items/{id}")]
struct ApiItem<'a> {
    id: &'a str,
}

/// A page, and API routes for each way an API request can fail; `settings` adds the API's.
fn api_app(settings: impl FnOnce(crate::Api<ApiRoot>) -> crate::Api<ApiRoot>) -> Router {
    App::new(Routes::from(
        Router::new().route("/", get(|req: Req| async move { req.test_page("Home", html! { <p>"home"</p> }) })),
    ))
    .error_page(test_error_page, ())
    .deadline(std::time::Duration::from_millis(50))
    .api::<ApiRoot>(|api| {
        let api = api
            .route::<ApiItem>(crate::Methods(get(|axum::extract::Path(id): axum::extract::Path<u32>| async move {
                format!("item {id}")
            })))
            .route::<ApiMissing>(crate::Methods(get(|| async {
                Err::<Response, _>(Error::new("no item 7").with_status(StatusCode::NOT_FOUND))
            })))
            .route::<ApiDown>(crate::Methods(get(|| async { Err::<Response, _>(Error::new("database password wrong")) })))
            .route::<ApiTeapot>(crate::Methods(get(|| async {
                (StatusCode::IM_A_TEAPOT, [("content-type", "application/json")], r#"{"mine":1}"#)
            })))
            .route::<ApiSlow>(crate::Methods(get(std::future::pending::<&'static str>)))
            .route::<ApiPost>(crate::Methods(axum::routing::post(|| async { "posted" })));
        settings(api)
    })
    .router()
}

/// What most HTTP clients send: `*/*`, or no `Accept` at all, or a browser's own.
const ACCEPTS: [Option<&str>; 4] = [Some("*/*"), None, Some("text/html,application/xhtml+xml"), Some("application/json")];

#[tokio::test]
async fn api_routes_answer_every_failure_as_json_whatever_the_client_accepts() {
    let router = api_app(|api| api);
    for accept in ACCEPTS {
        let get = |uri: &'static str| call_with(router.clone(), "GET", uri, None, accept);
        let (status, headers, body) = get("/api/nope").await;
        assert_eq!(
            (status, &body[..]),
            (StatusCode::NOT_FOUND, r#"{"error":{"status":404,"message":"Not Found"}}"#),
            "{accept:?}"
        );
        assert_eq!(headers["content-type"], "application/json");
        assert!(!headers.contains_key("corten-page") && !headers.contains_key("corten-shell"), "no shell: {headers:?}");
        let (_, _, body) = get("/api/missing").await;
        assert_eq!(body, r#"{"error":{"status":404,"message":"no item 7"}}"#, "a 4xx error's message is for the client");
        let (status, _, body) = get("/api/down").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, r#"{"error":{"status":500,"message":"Internal Server Error"}}"#, "a 5xx message stays in the log");
        let (status, _, body) = get("/api/items/abc").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(body.starts_with(r#"{"error":{"status":400,"message":"Invalid URL"#), "an extractor's rejection: {body}");
        let (status, headers, body) = call_with(router.clone(), "DELETE", "/api/post", None, accept).await;
        assert_eq!((status, &headers["allow"]), (StatusCode::METHOD_NOT_ALLOWED, &"POST".parse().unwrap()));
        assert_eq!(body, r#"{"error":{"status":405,"message":"Method Not Allowed"}}"#);
        let (status, _, body) = get("/api/slow").await;
        assert_eq!(
            (status, &body[..]),
            (StatusCode::SERVICE_UNAVAILABLE, r#"{"error":{"status":503,"message":"Service Unavailable"}}"#)
        );
        let (status, _, body) = get("/api/teapot").await;
        assert_eq!((status, &body[..]), (StatusCode::IM_A_TEAPOT, r#"{"mine":1}"#), "the handler's own JSON passes through");
        let (status, _, body) = get("/api/items/7").await;
        assert_eq!((status, &body[..]), (StatusCode::OK, "item 7"), "a success is the handler's");
    }
    let (_, headers, body) = call_with(router, "GET", "/nope", None, Some("*/*")).await;
    assert!(
        headers["content-type"].to_str().unwrap().starts_with("text/html") && body.contains("Not found"),
        "pages keep the page"
    );
}

#[tokio::test]
async fn api_routes_have_their_own_csp_and_pages_get_a_nonce_only_when_they_use_one() {
    let router = api_app(|api| api);
    let (_, headers, _) = call_with(router.clone(), "GET", "/api/items/1", None, None).await;
    assert_eq!(headers["content-security-policy"], "default-src 'none'; frame-ancestors 'none'");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    let (_, headers, _) = call_with(router.clone(), "GET", "/", None, None).await;
    assert!(!headers["content-security-policy"].to_str().unwrap().contains("nonce"), "a document needs none");
    let (_, headers, _) = call_with(router.clone(), "GET", "/", Some("nav"), None).await;
    let csp = headers["content-security-policy"].to_str().unwrap();
    assert!(csp.starts_with("default-src 'self'; script-src 'self' 'sha256-") && !csp.contains("nonce"), "{csp}");
    let (_, headers, _) = call_with(router, "GET", runtime_path(), None, None).await;
    assert!(!headers["content-security-policy"].to_str().unwrap().contains("nonce"), "nor does a static file");
}

#[tokio::test]
async fn a_handler_that_takes_the_nonce_gets_the_one_in_the_csp() {
    let routes = Router::new().route(
        "/raw",
        get(|nonce: security::Nonce| async move { axum::response::Html(format!(r#"<script nonce="{}">1</script>"#, nonce.0)) }),
    );
    let (_, headers, body) = call_with(App::new(Routes::from(routes)).router(), "GET", "/raw", None, None).await;
    let nonce = body.split(r#"nonce=""#).nth(1).unwrap().split('"').next().unwrap();
    assert!(headers["content-security-policy"].to_str().unwrap().contains(&format!("'nonce-{nonce}'")));
}

#[tokio::test]
async fn a_caller_is_looked_up_only_by_the_handlers_that_read_it_and_once() {
    let routes = Router::new().route("/", get(|| async { "page" })).route(
        "/twice",
        get(|_: Caller<Viewer>, again: Option<Caller<Viewer>>| async move { again.map_or("", |Caller(v)| v.0) }),
    );
    let router = app(routes, Some("ada"));
    LOOKUPS.with(|n| n.set(0));
    for uri in ["/", runtime_path(), "/health/live"] {
        call_with(router.clone(), "GET", uri, None, None).await;
    }
    assert_eq!(LOOKUPS.with(|n| n.get()), 0, "a route that does not read who is asking never looks");
    let (_, _, body) = call_with(router, "GET", "/twice", None, None).await;
    assert_eq!((body.as_str(), LOOKUPS.with(|n| n.get())), ("ada", 1), "two readers, one lookup");
}

#[tokio::test]
async fn an_allowed_origin_may_call_the_api_from_the_browser_but_not_post_the_pages() {
    let router = api_app(|api| api.allow_origin("https://partner.example/"));
    let cross = |uri: &str, origin: &str| {
        Request::post(uri).header("origin", origin).header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap()
    };
    let res = router.clone().oneshot(cross("/api/post", "https://partner.example")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["access-control-allow-origin"], "https://partner.example");
    assert!(res.headers().get_all("vary").iter().any(|v| v == "origin"));
    assert!(!res.headers().contains_key("access-control-allow-credentials"), "no cookies cross over");

    let res = router.clone().oneshot(cross("/api/post", "https://evil.example")).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert!(!res.headers().contains_key("access-control-allow-origin"));
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body, r#"{"error":{"status":403,"message":"Cross-site request refused."}}"#);
    let res = router.clone().oneshot(cross("/", "https://partner.example")).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "the pages still refuse the partner's writes");
    let mut with_cookie = cross("/api/post", "https://partner.example");
    with_cookie.headers_mut().insert("cookie", "session=abc".parse().unwrap());
    let res = router.clone().oneshot(with_cookie).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "a partner's write never rides on the user's cookies");

    let preflight = |origin: &str| {
        Request::builder()
            .method("OPTIONS")
            .uri("/api/post")
            .header("origin", origin)
            .header("access-control-request-method", "POST")
            .header("access-control-request-headers", "authorization, content-type")
            .body(Body::empty())
            .unwrap()
    };
    let res = router.clone().oneshot(preflight("https://partner.example")).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert_eq!(res.headers()["access-control-allow-origin"], "https://partner.example");
    assert_eq!(res.headers()["access-control-allow-methods"], "POST");
    assert_eq!(res.headers()["access-control-allow-headers"], "authorization, content-type");
    let res = router.oneshot(preflight("https://evil.example")).await.unwrap();
    assert!(!res.headers().contains_key("access-control-allow-origin"), "another origin's preflight fails");
    assert_eq!(res.headers()["content-type"], "application/json");
}

#[tokio::test]
async fn an_api_at_the_root_makes_the_whole_app_json() {
    let router =
        App::new(Routes::new()).api::<Root>(|api| api.route::<Items>(crate::Methods(get(|| async { "items" })))).router();
    let (status, headers, body) = call_with(router.clone(), "GET", "/nope", None, Some("text/html")).await;
    assert_eq!((status, headers["content-type"].to_str().unwrap()), (StatusCode::NOT_FOUND, "application/json"));
    assert_eq!(body, r#"{"error":{"status":404,"message":"Not Found"}}"#);
    let (status, _, body) = call_with(router, "GET", "/health/ready", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "ok"), "Corten's own routes keep their behavior");
}

#[cfg(feature = "json")]
#[tokio::test]
async fn validation_errors_are_the_json_error_with_fields() {
    let router = App::new(Routes::new())
        .api::<ApiRoot>(|api| {
            api.route::<ApiSignUp>(crate::Methods(axum::routing::post(|| async {
                let mut errors = Errors::new();
                errors.add(SignUp::FIELDS.email, "Enter an email address.");
                Err::<&str, _>(errors)
            })))
        })
        .router();
    let (status, _, body) = call_with(router, "POST", "/api/sign-up", None, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        body,
        r#"{"error":{"status":422,"message":"email: Enter an email address.","fields":{"email":"Enter an email address."}}}"#
    );
}

#[cfg(feature = "json")]
#[tokio::test]
async fn a_json_body_that_does_not_parse_is_a_json_400() {
    let router = App::new(Routes::new())
        .api::<ApiRoot>(|api| {
            api.route::<ApiItems>(crate::Methods(axum::routing::post(
                |axum::Json(v): axum::Json<serde_json::Value>| async move { v.to_string() },
            )))
        })
        .router();
    let req = Request::post("/api/items").header("content-type", "application/json").body(Body::from("{nope")).unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(body.starts_with(r#"{"error":{"status":400,"message":"Failed to parse the request body as JSON"#), "{body}");
}

/// Pages marked for shared caches, one that sets a cookie, one that must never be stored, and one
/// that says nothing.
fn cached_routes() -> Router {
    let minute = std::time::Duration::from_secs(60);
    let home = || html! { <p>"home"</p> };
    Router::new()
        .route("/", get(move |req: Req| async move { Public::new(minute, req.test_page("Home", home())) }))
        .route(
            "/cookie",
            get(move |req: Req| async move {
                Ok::<_, Error>((Cache::public(minute), [(header::SET_COOKIE, "seen=1")], req.test_page("Home", home())?))
            }),
        )
        .route(
            "/catalog",
            get(move |req: Req| async move {
                Public::new(minute, req.test_page("Catalog", home())).shared(std::time::Duration::from_secs(86_400))
            }),
        )
        .route("/statement", get(move |req: Req| async move { NoStore(req.test_page("Statement", home())) }))
        .route("/plain", get(|req: Req| async move { req.test_page("Plain", html! { <p>"plain"</p> }) }))
}

async fn cache_control(router: &Router, uri: &str, extra: &[(&'static str, &'static str)]) -> String {
    let mut req = Request::get(uri);
    for (name, value) in extra {
        req = req.header(*name, *value);
    }
    let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    res.headers()[header::CACHE_CONTROL].to_str().unwrap().to_string()
}

#[tokio::test]
async fn what_the_app_renders_is_private_unless_it_says_otherwise() {
    let router = app(cached_routes(), None);
    assert_eq!(cache_control(&router, "/plain", &[]).await, "private, no-cache");
    assert_eq!(cache_control(&router, "/plain", &[("corten", "nav")]).await, "private, no-cache");
    assert_eq!(cache_control(&router, "/missing", &[]).await, "private, no-cache", "error pages too");
    assert_eq!(cache_control(&router, "/missing", &[("accept", "application/json")]).await, "private, no-cache");
    assert_eq!(cache_control(&router, runtime_path(), &[]).await, "public, max-age=31536000, immutable");
    assert_eq!(cache_control(&router, "/health/ready", &[]).await, "no-store", "Corten's own routes keep theirs");
    assert_eq!(cache_control(&router, "/statement", &[("cookie", "s=1")]).await, "no-store");
}

#[tokio::test]
async fn a_public_page_is_shared_only_when_nothing_makes_it_personal() {
    let router = app(cached_routes(), None);
    assert_eq!(cache_control(&router, "/", &[]).await, "public, max-age=60");
    assert_eq!(cache_control(&router, "/", &[("accept", "text/html,*/*")]).await, "public, max-age=60");
    for personal in [("cookie", "session=abc"), ("authorization", "Bearer t")] {
        assert_eq!(cache_control(&router, "/", &[personal]).await, "private, no-cache", "{personal:?}");
    }
    for partial in [("corten", "nav"), ("corten", "action"), ("accept", "application/json")] {
        assert_eq!(cache_control(&router, "/", &[partial]).await, "private, max-age=60", "only the document: {partial:?}");
    }
    assert_eq!(cache_control(&router, "/cookie", &[]).await, "private, no-cache", "it sets a cookie");
    assert_eq!(cache_control(&router, "/catalog", &[]).await, "public, max-age=60, s-maxage=86400");
    assert_eq!(cache_control(&router, "/catalog", &[("corten", "nav")]).await, "private, max-age=60", "the browser's lifetime");
    assert_eq!(cache_control(&router, "/catalog", &[("cookie", "session=abc")]).await, "private, no-cache");
    let inline = App::new(Routes::from(Router::new().route("/", get(inline_page)))).router();
    assert_eq!(cache_control(&inline, "/", &[]).await, "private, no-cache", "the layout took the nonce");

    let minute = std::time::Duration::from_secs(60);
    let api = App::new(Routes::new())
        .api::<ApiRoot>(|api| api.route::<ApiRates>(crate::Methods(get(move || async move { (Cache::public(minute), "rates") }))))
        .router();
    assert_eq!(cache_control(&api, "/api/rates", &[("accept", "application/json")]).await, "public, max-age=60");
    assert_eq!(cache_control(&api, "/api/rates", &[("authorization", "Bearer t")]).await, "private, no-cache");
}

#[tokio::test]
async fn only_a_swap_says_it_is_a_page_swap() {
    let (_, headers, _) = call(app(routes(), None), "GET", None).await;
    assert!(!headers.contains_key("corten-page"), "a document a cache hands to a navigation makes the runtime reload");
    let (_, headers, _) = call(app(routes(), None), "GET", Some("nav")).await;
    assert_eq!(headers["corten-page"], "1");
}

// ---- Callers: who is asking, read by the handlers that need it ----

std::thread_local! {
    /// How many handlers that write ran on this test's thread.
    static WROTE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Staff: someone signed in who may use the admin area.
#[derive(Clone, Hash, Debug)]
struct Staff(&'static str);

impl<S: Sync> crate::Identify<S> for Staff {
    async fn identify(parts: &axum::http::request::Parts, state: &S) -> std::result::Result<Self, crate::Refused> {
        let Viewer(name) = <Viewer as crate::Identify<S>>::identify(parts, state).await?;
        match name {
            "ada" | "bob" => Ok(Staff(name)),
            _ => Err(crate::Refused::forbidden()),
        }
    }
}

/// A partner, by its API key: there is nowhere to sign in.
#[derive(Clone, Hash, Debug)]
struct Partner(&'static str);

impl<S: Sync> crate::Identify<S> for Partner {
    async fn identify(parts: &axum::http::request::Parts, _: &S) -> std::result::Result<Self, crate::Refused> {
        match parts.headers.get("x-api-key").map(|v| v.as_bytes()) {
            Some(b"k1") => Ok(Partner("acme")),
            _ => Err(crate::Refused::unauthorized()),
        }
    }
}

#[crate::routes]
mod caller_url {
    #[route("/", get = caller_home)]
    pub struct Home;

    #[route("/account", get = account, post = save_account)]
    pub struct Account;

    #[route("/sign-in", get = sign_in)]
    pub struct SignIn;

    #[route("/catalog/{id}", get = catalog)]
    pub struct Catalog {
        pub id: u32,
    }
}

/// Every route here takes `Caller<Staff>`, or the crate does not compile.
#[crate::routes(area = Admin, caller = Staff)]
mod admin_url {
    #[route("/admin", get = admin_home, post = admin_save)]
    pub struct Admin;

    #[route("/admin/users/{id}", get = admin_user)]
    pub struct User {
        pub id: u32,
    }
}

async fn caller_home(req: Req) -> HtmlResult {
    req.test_page("Home", html! { <p>"home"</p> })
}

async fn account(req: Req, Caller(viewer): Caller<Viewer>) -> HtmlResult {
    req.test_page("Account", html! { <p>{viewer.0}</p> })
}

type Fields = crate::Form<std::collections::HashMap<String, String>>;

async fn save_account(Caller(_): Caller<Viewer>, crate::Form(_): Fields) -> &'static str {
    WROTE.with(|n| n.set(n.get() + 1));
    "saved"
}

async fn sign_in(req: Req) -> HtmlResult {
    req.test_page("Sign in", html! { <p>"sign in"</p> })
}

/// The same for everyone: it may read only what cannot tell who is asking.
async fn catalog(Path(id): Path<u32>) -> Public<String> {
    Public::new(std::time::Duration::from_secs(60), format!("item {id}"))
}

async fn admin_home(req: Req, Caller(staff): Caller<Staff>) -> HtmlResult {
    req.test_page("Admin", html! { <p>"admin "{staff.0}</p> })
}

async fn admin_save(Caller(_): Caller<Staff>, crate::Form(_): Fields) -> &'static str {
    WROTE.with(|n| n.set(n.get() + 1));
    "saved"
}

async fn admin_user(Path(id): Path<u32>, Caller(_): Caller<Staff>) -> String {
    format!("user {id}")
}

fn caller_app() -> Router {
    App::new(caller_url::router()).error_page(test_error_page, ()).area::<admin_url::Admin>(admin_url::area).router()
}

async fn guarded(
    router: &Router,
    method: &str,
    uri: &str,
    viewer: Option<&str>,
    headers: &[(&str, &str)],
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(viewer) = viewer {
        req = req.header("viewer", viewer);
    }
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let body = if method == "POST" { Body::from("field=1") } else { Body::empty() };
    let req = req.header("content-type", "application/x-www-form-urlencoded").body(body).unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    (parts.status, parts.headers, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn a_missing_caller_sends_a_browser_to_sign_in_in_its_mode() {
    let router = caller_app();
    WROTE.with(|n| n.set(0));
    for (method, uri, headers) in [
        ("GET", "/account", &[][..]),
        ("GET", "/account", &[("corten", "nav")][..]),
        ("POST", "/account", &[][..]),
        ("GET", "/admin", &[][..]),
        ("POST", "/admin", &[][..]),
        ("GET", "/admin/users/7", &[][..]),
    ] {
        let (status, res, _) = guarded(&router, method, uri, None, headers).await;
        assert_eq!(
            (status, &res["location"]),
            (StatusCode::SEE_OTHER, &"/sign-in".parse().unwrap()),
            "{method} {uri} {headers:?}"
        );
        assert_eq!(res["cache-control"], "private, no-cache");
    }
    let (status, res, _) = guarded(&router, "POST", "/account", None, &[("corten", "action")]).await;
    assert_eq!((status, &res["corten-location"]), (StatusCode::NO_CONTENT, &"/sign-in".parse().unwrap()));
    let (status, res, body) = guarded(&router, "GET", "/account", None, &[("accept", "application/json")]).await;
    assert_eq!(
        (status, &body[..]),
        (StatusCode::UNAUTHORIZED, r#"{"error":{"status":401,"message":"Unauthorized"}}"#),
        "a data client"
    );
    assert_eq!(res["content-type"], "application/json");
    assert_eq!(WROTE.with(|n| n.get()), 0, "no handler ran, and no form was read");
    for uri in ["/", "/sign-in"] {
        let (status, _, _) = guarded(&router, "GET", uri, None, &[]).await;
        assert_eq!(status, StatusCode::OK, "{uri} reads no caller");
    }
}

#[tokio::test]
async fn a_refused_caller_gets_the_error_page_in_its_mode() {
    let router = caller_app();
    WROTE.with(|n| n.set(0));
    let (status, res, body) = guarded(&router, "GET", "/admin", Some("eve"), &[("accept", "text/html")]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(res["content-type"].to_str().unwrap().starts_with("text/html") && body.contains("Not allowed"), "{body}");
    let (status, res, _) = guarded(&router, "POST", "/admin", Some("eve"), &[("corten", "action")]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(res.contains_key("corten-page"), "a form patch gets the error page to swap in");
    let (status, _, body) = guarded(&router, "GET", "/admin", Some("eve"), &[("accept", "application/json")]).await;
    assert_eq!((status, &body[..]), (StatusCode::FORBIDDEN, r#"{"error":{"status":403,"message":"Forbidden"}}"#));
    let (status, _, body) = guarded(&router, "GET", "/admin", Some("down"), &[("accept", "text/html")]).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "who they are cannot be known now");
    assert!(!body.contains("session store"), "the message stays in the log: {body}");
    assert!(body.contains("Who you are cannot be checked"), "Failure::Unidentified, not a handler's 503: {body}");
    assert_eq!(WROTE.with(|n| n.get()), 0);
}

#[tokio::test]
async fn the_error_page_shows_who_is_asking() {
    let router = caller_app();
    let html = [("accept", "text/html")];
    LOOKUPS.with(|n| n.set(0));
    let (status, _, body) = guarded(&router, "GET", "/nope", None, &[("cookie", "viewer=ada"), html[0]]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("Signed in as ada"), "an unknown URL, drawn for who is asking: {body}");
    assert_eq!(LOOKUPS.with(|n| n.get()), 1, "looked up once, by the error page");
    let (status, _, body) = guarded(&router, "GET", "/nope", None, &[html[0]]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!body.contains("Signed in"), "no one: {body}");
    let (status, _, body) = guarded(&router, "GET", "/admin", Some("eve"), &[("cookie", "viewer=eve"), html[0]]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains("Not allowed") && body.contains("Signed in as eve"), "a refusal, for who was refused: {body}");
    let (status, _, body) = guarded(&router, "GET", "/nope", None, &[("cookie", "viewer=down"), html[0]]).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "a lookup that fails draws the page signed out: {body}");
    assert!(body.contains("Not found") && !body.contains("Signed in"), "{body}");
    let (status, res, _) = guarded(&router, "GET", "/nope", None, &[("cookie", "viewer=ada"), ("corten", "nav")]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(res["x-robots-tag"], "noindex");
    assert!(res.contains_key("corten-page"), "in the request's mode");
}

#[tokio::test]
async fn the_error_page_after_a_handler_that_read_the_caller_keeps_the_pages_key() {
    let routes = Router::new()
        .route("/", get(|req: Req, _: Option<crate::Caller<Viewer>>| async move { req.test_page("Home", "") }))
        .route("/fail", get(|_: Option<crate::Caller<Viewer>>| async { Err::<Response, _>(Error::new("down")) }));
    let ada = [("cookie", "viewer=ada"), ("corten", "nav")];
    let (_, page, _) = guarded(&app(routes.clone(), None), "GET", "/", None, &ada).await;
    let (status, failed, body) = guarded(&app(routes, None), "GET", "/fail", None, &ada).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body.contains("Signed in as ada"), "{body}");
    // Marked by the handler and the error page alike, the caller would cancel out of the key.
    assert_eq!(failed["corten-shell"], page["corten-shell"], "the same shell, so the runtime swaps it in");
}

#[tokio::test]
async fn a_handlers_own_503_is_not_an_unknown_caller() {
    let routes = Router::new().route("/", get(|| async { Err::<Response, _>(Error::status(StatusCode::SERVICE_UNAVAILABLE)) }));
    let (status, _, body) = call(app(routes, None), "GET", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.contains("Something went wrong") && !body.contains("cannot be checked"), "Failure::Failed: {body}");
}

#[tokio::test]
async fn a_found_caller_reaches_the_handler() {
    let router = caller_app();
    let (status, _, body) = guarded(&router, "GET", "/admin", Some("bob"), &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<p>admin bob</p>"), "{body}");
    let (status, _, body) = guarded(&router, "POST", "/admin", Some("bob"), &[]).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "saved"));
    let (_, _, body) = guarded(&router, "GET", "/admin/users/7", Some("ada"), &[]).await;
    assert_eq!(body, "user 7");
    let (_, _, body) = guarded(&router, "GET", "/account", Some("eve"), &[]).await;
    assert!(body.contains("<p>eve</p>"), "anyone signed in has an account: {body}");
}

#[tokio::test]
async fn reading_a_caller_makes_the_answer_private_and_a_public_one_is_shared() {
    let router = caller_app();
    assert_eq!(cache_control(&router, "/catalog/1", &[]).await, "public, max-age=60");
    assert_eq!(cache_control(&router, "/catalog/1", &[("x-api-key", "k1")]).await, "public, max-age=60");
    assert_eq!(cache_control(&router, "/catalog/1", &[("cookie", "session=abc")]).await, "private, no-cache");
    assert_eq!(cache_control(&router, "/catalog/nope", &[]).await, "private, no-cache", "a failure is never shared");
    // A handler that reads a caller cannot return `Public` (that does not compile). A response that
    // says public anyway, by `Cache` or by a header written by hand (`HtmlResponse` derefs to the
    // response), is still private: the key could be anyone's.
    let minute = std::time::Duration::from_secs(60);
    let leaky = App::new(Routes::from(
        Router::new().route("/rates", get(move |Caller(p): Caller<Partner>| async move { (Cache::public(minute), p.0) })).route(
            "/by-hand",
            get(|Caller(p): Caller<Partner>| async move { ([(header::CACHE_CONTROL, "public, max-age=60")], p.0) }),
        ),
    ))
    .router();
    assert_eq!(cache_control(&leaky, "/rates", &[("x-api-key", "k1")]).await, "private, no-cache");
    assert_eq!(cache_control(&leaky, "/by-hand", &[("x-api-key", "k1")]).await, "private, no-cache");
}

#[tokio::test]
async fn a_caller_with_nowhere_to_sign_in_is_a_401_page() {
    let router = App::new(Routes::from(Router::new().route("/partner", get(|Caller(p): Caller<Partner>| async move { p.0 }))))
        .error_page(test_error_page, ())
        .router();
    let (status, res, body) = guarded(&router, "GET", "/partner", None, &[("accept", "text/html")]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!res.contains_key("location") && body.contains("Something went wrong"), "{body}");
    let (status, _, body) = guarded(&router, "GET", "/partner", None, &[("x-api-key", "k1")]).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "acme"));
}

#[tokio::test]
async fn a_sign_in_page_that_needs_a_caller_is_an_error_not_a_loop() {
    let router =
        App::new(Routes::from(Router::new().route("/sign-in", get(|Caller(v): Caller<Viewer>| async move { v.0 })))).router();
    let (status, res, body) = guarded(&router, "GET", "/sign-in", None, &[("accept", "text/html")]).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "not a redirect the browser follows forever");
    assert!(!res.contains_key("location") && !body.contains("itself"), "the message stays in the log: {body}");
}

#[cfg(feature = "json")]
#[crate::routes]
mod json_url {
    #[route("/api/price/{id}", get = price)]
    pub struct Price {
        pub id: u32,
    }
}

#[cfg(feature = "json")]
#[derive(crate::ToJson)]
struct Quote {
    id: u32,
    partner: &'static str,
}

#[cfg(feature = "json")]
async fn price(Path(id): Path<u32>, Caller(partner): Caller<Partner>) -> JsonResult<Quote> {
    Ok(Json(Quote { id, partner: partner.0 }))
}

#[cfg(feature = "json")]
#[tokio::test]
async fn a_json_route_answers_every_failure_as_json_whatever_the_client_accepts() {
    let router = App::new(json_url::router()).error_page(test_error_page, ()).router();
    for accept in ACCEPTS {
        let headers: &[(&str, &str)] = match accept {
            Some(accept) => &[("accept", accept)],
            None => &[],
        };
        let (status, res, body) = guarded(&router, "GET", "/api/price/7", None, headers).await;
        assert_eq!(
            (status, &body[..]),
            (StatusCode::UNAUTHORIZED, r#"{"error":{"status":401,"message":"Unauthorized"}}"#),
            "{accept:?}"
        );
        assert_eq!(res["content-type"], "application/json");
        let (status, _, body) = guarded(&router, "GET", "/api/price/abc", None, headers).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{accept:?}");
        assert!(body.starts_with(r#"{"error":{"status":400,"#), "a path that does not parse: {body}");
    }
    let (status, res, body) = guarded(&router, "GET", "/api/price/7", None, &[("x-api-key", "k1")]).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, r#"{"id":7,"partner":"acme"}"#));
    assert_eq!(res["cache-control"], "private, no-cache");
}

paths! { ApiAdmin = "/api/admin" }

#[test]
#[should_panic(expected = "the area \"/api/admin\" is under App::api(\"/api\")")]
fn an_area_under_an_api_prefix_fails_at_startup() {
    let _ = App::new(Routes::new()).api::<ApiRoot>(|api| api).area::<ApiAdmin>(|admin| admin).router();
}

#[tokio::test]
async fn the_app_answers_unknown_urls_under_an_area_or_api() {
    let fallback = || async { "the app's fallback" };
    let router = App::new(Routes::from(Router::new().route("/", get(|| async { "home" })).fallback(fallback)))
        .area::<Admin>(|admin| admin.route::<Admin>(crate::Methods(get(|| async { "admin" }))))
        .router();
    for uri in ["/nope", "/admin/nope"] {
        let (status, _, body) = call_with(router.clone(), "GET", uri, None, None).await;
        assert_eq!((status, &body[..]), (StatusCode::OK, "the app's fallback"), "{uri}");
    }
}

/// Whether this system has an IPv6 loopback address to listen on.
fn has_ipv6_loopback() -> bool {
    std::net::TcpListener::bind("[::1]:0").is_ok()
}

/// Without `HOST`, `localhost` is both loopback addresses on one port, so a browser that tries
/// `::1` first is answered at once, not after a fallback; stopping ends both.
#[tokio::test]
async fn without_a_host_the_app_listens_on_both_loopback_addresses() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listeners = crate::app::bind(None, 0).await.unwrap();
    let addrs: Vec<_> = listeners.iter().map(|l| l.local_addr().unwrap()).collect();
    assert!(addrs[0].is_ipv4() && addrs[0].ip().is_loopback(), "{addrs:?}");
    assert_eq!(addrs.len(), if has_ipv6_loopback() { 2 } else { 1 }, "{addrs:?}");
    assert!(addrs.iter().all(|a| a.ip().is_loopback() && a.port() == addrs[0].port()), "{addrs:?}");

    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let routes = Router::new().route("/", get(|| async { "hi" }));
    let server = tokio::spawn(crate::app::serve_on(listeners, routes, async {
        let _ = stopped.await;
    }));
    for addr in &addrs {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n").await.unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).await.unwrap();
        assert!(answer.starts_with("HTTP/1.1 200") && answer.ends_with("hi"), "{addr}: {answer}");
    }
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

/// A server for `app` on a free port of `127.0.0.1` until `stop` is sent.
async fn served<M: crate::Middleware>(
    app: App<M>,
) -> (tokio::sync::oneshot::Sender<()>, tokio::task::JoinHandle<std::io::Result<()>>) {
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let listeners = crate::app::bind(Some("127.0.0.1"), 0).await.unwrap();
    let server = tokio::spawn(crate::app::run(listeners, app, async {
        let _ = stopped.await;
    }));
    (stop, server)
}

/// A task of the app's is told when shutdown begins, and `serve` waits for it to finish what it
/// was writing before it returns.
#[tokio::test]
async fn shutdown_tells_the_tasks_and_waits_for_them() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let written = Arc::new(AtomicBool::new(false));
    let (reason_tx, reason) = tokio::sync::oneshot::channel();
    let journal = written.clone();
    let app = App::new(Routes::new()).task("journal", move |signal: Signal| async move {
        let why = signal.aborted().await;
        // The write in progress when the signal came.
        tokio::time::sleep(Duration::from_millis(100)).await;
        journal.store(true, Ordering::SeqCst);
        let _ = reason_tx.send(why);
    });
    let (stop, server) = served(app).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!server.is_finished() && !written.load(Ordering::SeqCst));
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
    assert!(written.load(Ordering::SeqCst), "serve returned before the task finished");
    assert_eq!(reason.await.unwrap(), crate::Abort::Shutdown);
}

/// A task that ignores the signal is ended at the grace, so it cannot hold the process; one that
/// panics ends alone, and the app keeps serving.
#[tokio::test]
async fn a_task_past_the_grace_is_ended_and_one_that_panics_ends_alone() {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let dropped = Arc::new(AtomicBool::new(false));
    let guard = Dropped(dropped.clone());
    let routes = Routes::from(Router::new().route("/", get(|| async { "hi" })));
    let app = App::new(routes)
        .shutdown_grace(Duration::from_millis(50))
        .task("stuck", move |_signal: Signal| async move {
            let _guard = guard;
            std::future::pending::<()>().await
        })
        .task("broken", |_signal: Signal| async move { panic!("the task failed") });
    let (stop, server) = served(app).await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!server.is_finished(), "a task that panicked took the server with it");
    let started = std::time::Instant::now();
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), server).await.expect("a stuck task held serve").unwrap().unwrap();
    assert!(started.elapsed() >= Duration::from_millis(50), "serve did not wait the grace");
    tokio::time::timeout(Duration::from_secs(1), async {
        while !dropped.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the stuck task was left running");
}

/// A `[::1]` port another process holds is an error, not a server only `127.0.0.1` reaches:
/// `http://localhost` would get the other process.
#[tokio::test]
async fn a_taken_ipv6_loopback_port_is_an_error() {
    if !has_ipv6_loopback() {
        return;
    }
    let taken = std::net::TcpListener::bind("[::1]:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let error = crate::app::bind(None, port).await.expect_err("the port is taken on ::1");
    assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
    assert_eq!(crate::app::bind(Some("127.0.0.1"), port).await.unwrap().len(), 1, "HOST names one address");
}
