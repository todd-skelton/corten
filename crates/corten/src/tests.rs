use super::*;
use crate::answer::ForOne;
use crate::cache::Cache;
use crate::security::cookie::SetCookie;
use crate::view::{html, Render};
use axum::{body::Body, http::Request, routing::get, Router};
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

/// A path of the test's own under the system's temp directory (`corten-{name}-{pid}`): any
/// leftover there is removed when it is made, the test creates what it writes, and the directory
/// is removed when dropped, a failing test's included, so a test run leaves nothing behind. Each
/// test uses a name no other test does.
pub(crate) struct Scratch(std::path::PathBuf);

impl Scratch {
    pub(crate) fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("corten-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        Scratch(path)
    }
}

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
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
    PayApi = "/pay/api",
    PayApiGo = "/pay/api/go",
    Items = "/items",
    Admin = "/admin",
    AdminSlow = "/admin/slow",
    AdminQuick = "/admin/quick",
    AdminQuickSlow = "/admin/quick/slow",
    AdminOther = "/admin/other",
    AdminOtherSlow = "/admin/other/slow",
    AdminHooks = "/admin/hooks",
    AdminHooksPing = "/admin/hooks/ping",
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

/// Stamps every answer with a request id and two `Server-Timing` values, and answers 429 with
/// `Retry-After` itself when asked to by `x-flood`.
struct Stamp;

impl crate::Middleware for Stamp {
    async fn call(&self, req: axum::extract::Request, next: impl crate::Next) -> Response {
        use axum::http::{header, HeaderValue};
        let mut res = if req.headers().contains_key("x-flood") {
            let mut res = crate::Error::status(StatusCode::TOO_MANY_REQUESTS).into_response();
            res.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static("7"));
            res
        } else {
            next.run(req).await
        };
        let headers = res.headers_mut();
        headers.insert("x-request-id", HeaderValue::from_static("r-1"));
        headers.append("server-timing", HeaderValue::from_static("app;dur=1"));
        headers.append("server-timing", HeaderValue::from_static("db;dur=2"));
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=600"));
        res
    }
}

/// A failure that becomes the error page keeps what the handler and the middleware said besides
/// the body (a request id, `Retry-After`, every `Server-Timing` value), in every mode; the error
/// page's own type and `Cache-Control` win.
#[tokio::test]
async fn the_error_page_keeps_the_headers_of_the_failure_it_replaces() {
    use axum::http::header;
    let routes = Router::new().route(
        "/busy",
        get(|| async { (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, "30"), (header::CONTENT_TYPE, "text/x-old")]) }),
    );
    let router = App::new(Routes::from(routes)).error_page(test_error_page, ()).middleware(Stamp).router();
    for mode in [None, Some("nav")] {
        for (uri, flood, status, retry) in
            [("/nope", false, 404, None), ("/busy", false, 429, Some("30")), ("/nope", true, 429, Some("7"))]
        {
            let mut req = Request::get(uri).header("accept", "text/html");
            if let Some(mode) = mode {
                req = req.header("corten", mode);
            }
            if flood {
                req = req.header("x-flood", "1");
            }
            let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
            let at = format!("{mode:?} {uri} flood={flood}");
            assert_eq!(res.status().as_u16(), status, "{at}");
            let headers = res.headers();
            assert_eq!(headers["x-request-id"], "r-1", "{at}");
            let timing: Vec<_> = headers.get_all("server-timing").iter().collect();
            assert_eq!(timing, ["app;dur=1", "db;dur=2"], "{at}");
            assert_eq!(headers.get(header::RETRY_AFTER).map(|v| v.to_str().unwrap()), retry, "{at}");
            assert!(headers[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/html"), "{at}: {headers:?}");
            assert_eq!(headers[header::CACHE_CONTROL], "private, no-cache", "{at}: an error page is never shared");
            let body = res.into_body().collect().await.unwrap().to_bytes();
            assert!(String::from_utf8_lossy(&body).contains("<h1>"), "{at}: the error page");
        }
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
                <header><a {..req.nav(&Root)}>"home"</a></header>
                <main>{children}</main>
            </body>
        </html>
    }
}

/// `req.page` in the tests' layout with no head tags of the page's own.
pub(crate) trait TestPage {
    fn test_page(&self, title: &str, main: impl Render) -> HtmlResult {
        self.test_page_for(title, main)
    }

    /// The page for whoever its handler answers: inside a `Public`, everyone.
    fn test_page_for<For>(&self, title: &str, main: impl Render) -> crate::Result<HtmlResponse<For>>;
}

impl TestPage for Req {
    fn test_page_for<For>(&self, title: &str, main: impl Render) -> crate::Result<HtmlResponse<For>> {
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
            req.page::<ForOne>(Shell { req: &req, title: "A & B", head, children: demo("<script>") })
        })
        .post(|req: Req| async move {
            req.patch_or(html! { <p id={MsgId}>"saved"</p> }, || async { req.redirect::<ForOne>(&Done) }).await
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
        .api::<ApiRoot>(|api| api.route::<ApiRates>(crate::Methods::reads(get(|| async { "rates" }))))
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

/// `live_watch` sends the current value only to a page that is behind, then each change, and
/// ends when the value's sender goes.
#[tokio::test]
async fn a_live_watch_sends_the_latest_value_from_where_the_page_is() {
    use crate::LiveSince;
    let (tx, rx) = tokio::sync::watch::channel(7u64);
    let stream = |since: Option<&str>| {
        let since = LiveSince(since.map(str::to_string));
        crate::live_watch(rx.clone(), since, |n| n.to_string(), |n| format!(r#"<p id="n">{n}</p>"#))
    };
    let (behind, current) = (stream(Some("6")), stream(Some("7")));
    let first = stream(None);
    tx.send_replace(8);
    tx.send_replace(9); // the browser only needs the latest: 8 is skipped
    drop(tx);
    let body = |res: LiveResponse| async move {
        String::from_utf8(res.into_response().into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    };
    let nine = "event: patch\nid: 9\ndata: <p id=\"n\">9</p>\n\nevent: end\ndata: end\n\n";
    assert_eq!(body(current).await, nine, "a page showing 7 gets the next change");
    assert_eq!(body(behind).await, nine, "a page behind gets the latest");
    assert_eq!(body(first).await, nine);
    let (tx, rx) = tokio::sync::watch::channel(7u64);
    let behind = crate::live_watch(rx, LiveSince(Some("6".into())), |n| n.to_string(), |n| format!(r#"<p id="n">{n}</p>"#));
    let read = tokio::spawn(body(behind));
    tokio::task::yield_now().await;
    drop(tx);
    assert_eq!(
        read.await.unwrap(),
        "event: patch\nid: 7\ndata: <p id=\"n\">7</p>\n\nevent: end\ndata: end\n\n",
        "the current one first"
    );

    // A receiver kept in the app's state never reads, so a clone of it has not seen 8: a page
    // showing 8 still gets nothing until the next change.
    let (tx, kept) = tokio::sync::watch::channel(7u64);
    tx.send_replace(8);
    let current = crate::live_watch(kept.clone(), LiveSince(Some("8".into())), |n| *n, |n| format!(r#"<p id="n">{n}</p>"#));
    drop(tx);
    assert_eq!(body(current).await, "event: end\ndata: end\n\n", "nothing to send");
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

/// `patch_or`'s page is built only for a submit without JavaScript, so a patch never loads what
/// only the page shows.
#[tokio::test]
async fn patch_or_builds_the_page_only_without_javascript() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static PAGES: AtomicUsize = AtomicUsize::new(0);
    let routes = || {
        Router::new().route(
            "/",
            axum::routing::post(|req: Req| async move {
                let page = || async {
                    PAGES.fetch_add(1, Ordering::Relaxed);
                    req.redirect::<ForOne>(&Done)
                };
                req.patch_or(html! { <p id={MsgId}>"saved"</p> }, page).await
            }),
        )
    };
    let (_, headers, _) = call(app(routes(), None), "POST", Some("action")).await;
    assert_eq!((headers["corten-patch"].to_str().unwrap(), PAGES.load(Ordering::Relaxed)), ("1", 0));
    let (status, _, _) = call(app(routes(), None), "POST", None).await;
    assert_eq!((status, PAGES.load(Ordering::Relaxed)), (StatusCode::SEE_OTHER, 1));
    // A data client gets the page's answer too, not a JSON error as from `invalid`.
    let (status, _, _) = call_with(app(routes(), None), "POST", "/", None, Some("application/json")).await;
    assert_eq!((status, PAGES.load(Ordering::Relaxed)), (StatusCode::SEE_OTHER, 2));
}

/// A view of references that are not `Sync` (`&Cell`) keeps the handler's future `Send`: the
/// patch is rendered before `patch_or`'s future holds anything.
#[tokio::test]
async fn patch_or_keeps_a_handler_send_with_a_view_of_cells() {
    let routes = Router::new().route(
        "/",
        axum::routing::post(|req: Req| async move {
            let count = std::cell::Cell::new(1);
            let count = &count;
            req.patch_or(html! { <p id={MsgId}>{count.get()}</p> }, || async { req.redirect::<ForOne>(&Done) }).await
        }),
    );
    let (_, _, body) = call(app(routes, None), "POST", Some("action")).await;
    assert_eq!(body, r#"<p id="msg">1</p>"#);
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
                req.screen::<ForOne, _>(&data, Shell { req: &req, title: "A & B", head: "", children: demo(data.text) })
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

/// A meter whose fill comes from data (`meter.css` registers `--fill`).
#[crate::component(css)]
fn Meter(used: f64) -> impl Render {
    html! { <div class={Meter} style={FILL.pct(used)}></div> }
}

fn meters() -> Router {
    Router::new().route(
        "/",
        get(|req: Req| async move {
            let meters = html! {
                <Meter used={42.5} />
                <Meter used={7.0} />
                <Meter used={42.5} />
                <Meter used={f64::NAN} />
            };
            req.test_page("Meters", meters)
        }),
    )
}

/// A full page's custom properties are one style block at `<Corten />`, allowed by its hash in
/// that response's CSP; a navigation's are left to the runtime, under the document's policy.
#[tokio::test]
async fn a_page_s_custom_properties_are_one_style_block_its_csp_allows_by_hash() {
    let (_, headers, body) = call(app(meters(), None), "GET", None).await;
    let rules = r#"[data-vars="--meter__fill:42.5%"]{--meter__fill:42.5%!important}[data-vars="--meter__fill:7%"]{--meter__fill:7%!important}"#;
    assert!(body.contains(&format!("<style>{rules}</style></head>")), "each value once, after the page's stylesheets: {body}");
    assert!(body.contains(r#"<div class="meter__meter" data-vars="--meter__fill:7%"></div><div class="meter__meter" data-vars="--meter__fill:42.5%"></div><div class="meter__meter"></div>"#), "a value that is not finite sets nothing: {body}");
    let csp = headers["content-security-policy"].to_str().unwrap();
    assert!(csp.contains(&format!("style-src 'self' {};", security::hash_source(rules))), "{csp}");
    assert!(!csp.contains("style-src-attr") && !csp.contains("unsafe"), "{csp}");
    let (_, again, same) = call(app(meters(), None), "GET", None).await;
    assert_eq!(
        (&again["content-security-policy"], &same),
        (&headers["content-security-policy"], &body),
        "the same bytes, so a cache can keep it"
    );

    let (_, headers, body) = call(app(meters(), None), "GET", Some("nav")).await;
    assert!(body.contains(r#"data-vars="--meter__fill:42.5%""#) && !body.contains("<style>"), "{body}");
    let csp = headers["content-security-policy"].to_str().unwrap();
    assert!(csp.contains("style-src 'self';"), "{csp}");
    let (_, headers, _) = call(app(routes(), None), "GET", None).await;
    assert!(headers["content-security-policy"].to_str().unwrap().contains("style-src 'self';"), "a page with none is as before");
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
    assert_eq!(headers["content-type"], "application/problem+json");
    assert_eq!(body, r#"{"title":"Internal Server Error","status":500}"#, "the message stays in the log");
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
    assert_eq!((status, headers["content-type"].to_str().unwrap()), (StatusCode::BAD_REQUEST, "application/problem+json"));
    assert!(
        body.starts_with(r#"{"title":"Bad Request","status":400,"detail":"Invalid URL"#),
        "a data client gets the text as JSON: {body}"
    );
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
    assert_eq!((status, &body[..]), (StatusCode::INTERNAL_SERVER_ERROR, r#"{"title":"Internal Server Error","status":500}"#));
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

/// A second layout, as a marketing page and an account page might have.
#[crate::component]
fn Plain(req: &Req, title: &str, children: impl Render) -> impl Document {
    html! {
        <html lang={req.lang()}>
            <head>
                <title>{title}</title>
                <Corten />
            </head>
            <body>
                <header>"Plain"</header>
                <main>{children}</main>
            </body>
        </html>
    }
}

/// The shell key names the layout that rendered the page: pages in one layout share it, so a
/// navigation between them swaps `<main>`; a page in another layout (or the default error
/// document) has another, so the runtime loads it in full rather than keep the first layout.
#[tokio::test]
async fn a_page_in_another_layout_has_another_shell_key() {
    let routes = Router::new()
        .route("/", get(|req: Req| async move { req.page::<ForOne>(Shell { req: &req, title: "A", head: "", children: "a" }) }))
        .route("/b", get(|req: Req| async move { req.page::<ForOne>(Shell { req: &req, title: "B", head: "", children: "b" }) }))
        .route("/plain", get(|req: Req| async move { req.page::<ForOne>(Plain { req: &req, title: "P", children: "p" }) }));
    let router = app(routes.clone(), None);
    let key = |uri: &'static str, mode: Option<&'static str>| {
        let router = router.clone();
        async move {
            let (status, headers, body) = call_with(router, "GET", uri, mode, None).await;
            assert_eq!(status, StatusCode::OK, "{uri}");
            let key = headers["corten-shell"].to_str().unwrap().to_string();
            if mode.is_none() {
                assert_eq!(meta_content(&body, "corten-shell"), key, "{uri}: the document and its header agree");
            }
            key
        }
    };
    let home = key("/", None).await;
    assert_eq!(key("/b", Some("nav")).await, home, "the same layout swaps in");
    let plain = key("/plain", Some("nav")).await;
    assert_ne!(plain, home, "another layout loads in full");
    assert_eq!(key("/plain", None).await, plain, "a layout's key is the same in every mode");

    // An app without an error page: an unknown URL is Corten's plain document, not the layout.
    let bare = App::new(Routes::from(routes)).router();
    let (_, nope, _) = call_with(bare.clone(), "GET", "/nope", Some("nav"), None).await;
    let (_, page, _) = call_with(bare, "GET", "/", Some("nav"), None).await;
    assert_ne!(nope["corten-shell"], page["corten-shell"], "the default error document is another layout");
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
fn a_nav_link_writes_its_route_once_and_says_when_it_is_the_page() {
    use crate::{page::Nav, view::Attrs};
    #[derive(crate::Route)]
    #[route("/sell")]
    struct Sell;
    #[derive(crate::Route)]
    #[route("/items/{id}")]
    struct Item<'a> {
        id: &'a str,
        tab: Option<&'a str>,
    }
    fn attrs<R: crate::Route>(nav: Nav<'_, R>) -> String {
        let mut out = String::new();
        nav.render_attrs(&mut out);
        out
    }
    assert_eq!(attrs(Nav { path: "/sell", to: &Sell, section: false }), r#" href="/sell" data-nav aria-current="page""#);
    assert_eq!(attrs(Nav { path: "/sell/listings", to: &Sell, section: false }), r#" href="/sell" data-nav"#);
    assert_eq!(
        attrs(Nav { path: "/sell/listings", to: &Sell, section: true }),
        r#" href="/sell" data-nav="section" aria-current="page""#
    );
    // A sibling that starts with the same text is not under the section.
    assert_eq!(attrs(Nav { path: "/seller", to: &Sell, section: true }), r#" href="/sell" data-nav="section""#);
    // A section written with a trailing slash holds the pages under it, and itself.
    #[derive(crate::Route)]
    #[route("/shop/")]
    struct Shop;
    assert_eq!(
        attrs(Nav { path: "/shop/boots", to: &Shop, section: true }),
        r#" href="/shop/" data-nav="section" aria-current="page""#
    );
    assert_eq!(
        attrs(Nav { path: "/shop/", to: &Shop, section: true }),
        r#" href="/shop/" data-nav="section" aria-current="page""#
    );
    // The path is percent-encoded, and the query is left out of the comparison.
    let item = Item { id: "a b", tab: Some("x&y") };
    assert_eq!(
        attrs(Nav { path: "/items/a%20b", to: &item, section: false }),
        r#" href="/items/a%20b?tab=x%26y" data-nav aria-current="page""#
    );
    // A route written by hand may write a path that escaping for the attribute changes; it is
    // compared unescaped.
    struct Terms;
    impl crate::Route for Terms {
        const PATH: &'static str = "/terms&privacy";
        fn write_href(&self, out: &mut String, html: bool) {
            out.push_str(if html { "/terms&amp;privacy" } else { "/terms&privacy" });
        }
    }
    assert_eq!(
        attrs(Nav { path: "/terms&privacy", to: &Terms, section: false }),
        r#" href="/terms&amp;privacy" data-nav aria-current="page""#
    );
}

/// A whole nav's links: of the entries that hold the page, only the deepest is current, as the
/// runtime judges a `<nav>` after a client navigation (#341).
#[test]
fn a_nav_marks_the_deepest_entry_that_holds_the_page() {
    use crate::{
        page::{nav_links, NavTo},
        view::Attrs,
        Local,
    };
    #[derive(crate::Route)]
    #[route("/")]
    struct Home;
    #[derive(crate::Route)]
    #[route("/account/desk")]
    struct Desk;
    #[derive(crate::Route)]
    #[route("/account/desk/money")]
    struct Money;
    let help = Local::parse("/help?topic=a&b").unwrap();
    let written = |nav: crate::page::NavLinks<()>| {
        nav.links()
            .map(|(link, ())| {
                let mut out = String::new();
                link.render_attrs(&mut out);
                out
            })
            .collect::<Vec<_>>()
    };
    let group = |links: &[String]| {
        let groups: Vec<_> =
            links.iter().map(|link| link.split(r#" data-nav-group=""#).nth(1).unwrap()[..8].to_string()).collect();
        assert!(groups.iter().all(|g| *g == groups[0] && g.bytes().all(|b| b.is_ascii_hexdigit())), "{groups:?}");
        groups[0].clone()
    };
    let marked = |path: &str| {
        let links = written(nav_links(
            path,
            [NavTo::page(&Home), NavTo::section(&Desk), NavTo::section(&Money), NavTo::page(&help)].map(|e| (e, ())),
        ));
        let g = format!(r#" data-nav-group="{}""#, group(&links));
        links.into_iter().map(|link| link.replace(&g, "")).collect::<Vec<_>>()
    };
    assert_eq!(
        marked("/account/desk/repricing"),
        [
            r#" href="/" data-nav"#,
            r#" href="/account/desk" data-nav="section" aria-current="page""#,
            r#" href="/account/desk/money" data-nav="section""#,
            r#" href="/help?topic=a&amp;b" data-nav"#,
        ]
    );
    let money = marked("/account/desk/money/payouts");
    assert!(!money[1].contains("aria-current") && money[2].ends_with(r#" aria-current="page""#), "{money:?}");
    assert!(marked("/")
        .iter()
        .filter(|link| link.contains("aria-current"))
        .eq([&r#" href="/" data-nav aria-current="page""#.to_string()]));
    assert!(marked("/help").last().unwrap().ends_with(r#" aria-current="page""#), "an entry from data takes part");
    assert!(marked("/elsewhere").iter().all(|link| !link.contains("aria-current")));

    // The group names the entries: the same nav twice is one group, another nav is another.
    let of = |path: &str, entries: [NavTo; 2]| written(nav_links(path, entries.map(|e| (e, ()))));
    let desk = of("/", [NavTo::section(&Desk), NavTo::page(&Money)]);
    assert_eq!(group(&desk), group(&of("/account", [NavTo::section(&Desk), NavTo::page(&Money)])));
    assert_ne!(group(&desk), group(&of("/", [NavTo::page(&Desk), NavTo::page(&Money)])));
    assert_ne!(group(&desk), group(&of("/", [NavTo::section(&Money), NavTo::page(&Desk)])));

    // Each link comes with its entry's item.
    let labels: Vec<_> =
        nav_links("/", [(NavTo::page(&Home), "Home"), (NavTo::section(&Desk), "Desk")]).links().map(|(_, l)| *l).collect();
    assert_eq!(labels, ["Home", "Desk"]);

    // A path from data is written as the browser asks for it, so the server compares what it is
    // sent: `"` and `|` are percent-encoded in the path (not in the query).
    let odd = Local::parse(r#"/a"b|c?q="|""#).unwrap();
    let links = written(nav_links("/a%22b%7Cc", [(NavTo::page(&odd), ())]));
    assert!(links[0].starts_with(r#" href="/a%22b%7Cc?q=&quot;|&quot;" data-nav data-nav-group=""#), "{links:?}");
    assert!(links[0].ends_with(r#" aria-current="page""#), "{links:?}");

    // A route that says `prefetch = false` keeps saying so in a nav, as in `href={..}`.
    #[derive(crate::Route)]
    #[route("/reports/suggested", prefetch = false)]
    struct Suggested;
    let links = written(nav_links("/", [(NavTo::page(&Suggested), ()), (NavTo::page(&Home), ())]));
    assert!(links[0].ends_with(" data-no-prefetch") && !links[1].contains("data-no-prefetch"), "{links:?}");
    let mut one = String::new();
    crate::page::Nav { path: "/reports/suggested", to: &Suggested, section: false }.render_attrs(&mut one);
    assert_eq!(one, r#" href="/reports/suggested" data-nav aria-current="page" data-no-prefetch"#);
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

fn sheet_form(errors: &Errors) -> impl crate::Patch + '_ {
    html! { <form id={SignUpId}>{errors.form_message()}</form> }
}

fn refused_routes() -> Router {
    refused_by(crate::Refusal::Conflict, "Someone saved first.")
}

fn refused_by(why: crate::Refusal, message: &'static str) -> Router {
    Router::new().route(
        "/",
        get(move |req: Req| async move {
            let mut errors = Errors::new();
            errors.refuse(why, message);
            let page = || async { req.test_page("Sheet", html! { <p>"page with the refusal"</p> }) };
            req.invalid(&errors, sheet_form(&errors), page).await
        }),
    )
}

#[tokio::test]
async fn a_stale_submit_answers_409_with_the_form_message_in_each_mode() {
    let (status, headers, body) = call(app(refused_routes(), None), "GET", Some("action")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(headers["corten-patch"], "1");
    assert_eq!(body, r#"<form id="sign-up"><p class="form-error" role="alert">Someone saved first.</p></form>"#);

    let (status, _, body) = call(app(refused_routes(), None), "GET", None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.contains("<p>page with the refusal</p>") && body.contains("<html"), "no JavaScript: the whole page");

    #[cfg(feature = "json")]
    {
        let (status, _, body) = call_with(app(refused_routes(), None), "GET", "/", None, Some("application/json")).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(
            body,
            r#"{"title":"Conflict","status":409,"detail":"Someone saved first.","form":"Someone saved first.","fields":{}}"#
        );
    }
}

#[tokio::test]
async fn a_failed_service_answers_502_with_the_form_message_in_each_mode() {
    let routes = || refused_by(crate::Refusal::Upstream, "The carrier did not answer. Try again.");
    let (status, headers, body) = call(app(routes(), None), "GET", Some("action")).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(headers["corten-patch"], "1", "the form, not the error page");
    assert_eq!(
        body,
        r#"<form id="sign-up"><p class="form-error" role="alert">The carrier did not answer. Try again.</p></form>"#
    );

    let (status, _, body) = call(app(routes(), None), "GET", None).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert!(
        body.contains("<p>page with the refusal</p>") && body.contains("<html"),
        "no JavaScript: the whole page, not the error page: {body}"
    );

    #[cfg(feature = "json")]
    {
        let (status, headers, body) = call_with(app(routes(), None), "GET", "/", None, Some("application/json")).await;
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(headers["content-type"], "application/problem+json");
        assert_eq!(
            body,
            r#"{"title":"Bad Gateway","status":502,"detail":"The carrier did not answer. Try again.","form":"The carrier did not answer. Try again.","fields":{}}"#
        );
    }
}

/// A refusal whose form needs a read, drawn per mode: `reads` counts the form side's reads and
/// `pages` the page side's runs.
fn lazily_refused_routes(
    reads: std::sync::Arc<std::sync::atomic::AtomicU32>,
    pages: std::sync::Arc<std::sync::atomic::AtomicU32>,
) -> Router {
    Router::new().route(
        "/",
        get(move |req: Req| async move {
            let mut errors = Errors::new();
            errors.refuse(crate::Refusal::Conflict, "Someone saved first.");
            let page = || async {
                pages.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                req.test_page("Sheet", html! { <p>"page with the refusal"</p> })
            };
            // Written in the call, as an app does: the closure's type comes from `invalid_with`.
            let shown = &errors;
            req.invalid_with(
                &errors,
                |form| async {
                    reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    form.draw(html! { <form id={SignUpId}>{shown.form_message()}"read now"</form> })
                },
                page,
            )
            .await
        }),
    )
}

#[tokio::test]
async fn a_refusal_whose_form_needs_a_read_reads_only_for_the_patch() {
    use std::sync::atomic::Ordering::Relaxed;
    use std::sync::{atomic::AtomicU32, Arc};
    let (reads, pages) = (Arc::new(AtomicU32::new(0)), Arc::new(AtomicU32::new(0)));
    let routes = || lazily_refused_routes(reads.clone(), pages.clone());
    let (status, headers, body) = call(app(routes(), None), "GET", Some("action")).await;
    assert_eq!((status, &headers["corten-patch"]), (StatusCode::CONFLICT, &"1".parse().unwrap()));
    assert!(
        body.starts_with(r#"<form id="sign-up">"#) && body.contains("Someone saved first.") && body.ends_with("read now</form>"),
        "{body}"
    );
    assert_eq!((reads.load(Relaxed), pages.load(Relaxed)), (1, 0), "an action draws only the patch");

    let (status, _, body) = call(app(routes(), None), "GET", None).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(body.contains("<p>page with the refusal</p>"), "no JavaScript: the whole page");
    assert_eq!((reads.load(Relaxed), pages.load(Relaxed)), (1, 1), "the page reads nothing for the patch");

    let (status, _, _) = call(app(routes(), None), "GET", Some("nav")).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!((reads.load(Relaxed), pages.load(Relaxed)), (1, 2), "a navigation gets the page");

    #[cfg(feature = "json")]
    {
        let (status, _, body) = call_with(app(routes(), None), "GET", "/", None, Some("application/json")).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(body.contains(r#""status":409"#), "{body}");
        assert_eq!((reads.load(Relaxed), pages.load(Relaxed)), (1, 2), "a data client runs neither side");
    }
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
            r#"{"title":"Unprocessable Entity","status":422,"detail":"email: Enter an email address.","fields":{"email":"Enter an email address."}}"#
        );
    }
}

#[tokio::test]
async fn a_page_that_reads_again_says_so_in_its_head_on_every_surface() {
    use std::time::Duration;
    let routes = Router::new().route(
        "/",
        get(|req: Req| async move {
            req.page::<ForOne>(Shell {
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

/// `ReadAgain::without_javascript` in the head (#319): the same refresh, without the mark the
/// runtime reads, on a full load and in a navigation's head tags.
#[tokio::test]
async fn a_page_that_reads_again_only_without_javascript_has_no_mark_in_its_head() {
    use std::time::Duration;
    let routes = Router::new().route(
        "/",
        get(|req: Req| async move {
            req.page::<ForOne>(Shell {
                req: &req,
                title: "Saving",
                head: ReadAgain { after: Duration::from_secs(2) }.without_javascript(),
                children: html! { <p>"Saving"</p> },
            })
        }),
    );
    let tag = r#"<noscript data-corten-head><meta http-equiv="refresh" content="2"></noscript>"#;
    let (_, headers, document) = call(app(routes.clone(), None), "GET", None).await;
    assert!(document.contains(&format!("<title>Saving</title>{tag}")), "{document}");
    assert!(!document.contains("data-corten-read-again"), "the runtime finds nothing to read again: {document}");
    assert!(!headers.contains_key("refresh"));
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

/// A write redirected to an origin the form's page does not allow in `form-action` is an error in
/// a debug build, as the browser stops it there; one the policy allows, a path, a page's own
/// host and a `GET` are not, and an area's policy or the sending page's counts.
#[cfg(debug_assertions)]
#[tokio::test]
async fn a_write_redirected_outside_form_action_is_an_error_in_debug_builds() {
    use crate::security::Policy;
    let to = |uri: &'static str| {
        axum::routing::post(move |req: Req| async move {
            let uri: axum::http::Uri = uri.parse().unwrap();
            req.redirect::<()>(&uri)
        })
    };
    let routes = Router::new()
        .route("/sibling", to("http://localhost:9999/account"))
        .route("/pay", to("https://pay.example/checkout?o=1"))
        .route("/sub", to("https://id.shop.example/login"))
        .route("/own", to("http://app.localhost/done"))
        .route("/path", to("/done"))
        .route("/public", to("https://app.localhost/done"))
        .route("/else", to("https://else.example/x"))
        .route(
            "/relay",
            axum::routing::post(|| async {
                let mut headers = axum::http::HeaderMap::new();
                headers.insert(axum::http::header::LOCATION, axum::http::HeaderValue::from_static("https://upstream.example/x"));
                crate::Relay::new(StatusCode::FOUND, headers, Body::empty())
            }),
        )
        .route(
            "/read",
            get(|req: Req| async move { req.redirect::<()>(&"https://else.example/".parse::<axum::http::Uri>().unwrap()) }),
        );
    let policy = Policy::new()
        .form_action("https://pay.example")
        .form_action("https://*.shop.example")
        .allow_origin("https://idp.example");
    let router = App::new(Routes::from(routes))
        .policy(policy)
        .area(|area| checkout_url::area(area).policy(|p| p.form_action("http://localhost:9999")))
        .router();
    let send = |path: &'static str, method: &'static str, headers: &'static [(&'static str, &'static str)]| {
        let router = router.clone();
        async move {
            let mut req = Request::builder().method(method).uri(path).header("accept", "text/html");
            for (name, value) in headers {
                req = req.header(*name, *value);
            }
            let res = router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
            let status = res.status();
            let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            (status, String::from_utf8(body.to_vec()).unwrap())
        }
    };
    for mode in [&[][..], &[("corten", "action")][..], &[("corten", "action"), ("sec-fetch-mode", "cors")][..]] {
        let (status, _) = send("/else", "POST", mode).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{mode:?}");
    }
    // Without a Referer the form may be on any of the app's pages: the checkout area allows this.
    let (status, _) = send("/sibling", "POST", &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "some page of the app allows it");
    // A script's own fetch is held to connect-src, not form-action.
    let (status, _) = send("/sibling", "POST", &[("sec-fetch-mode", "cors"), ("sec-fetch-site", "same-origin")]).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "a fetch");
    // A relayed answer is passed on as it came.
    let (status, _) = send("/relay", "POST", &[]).await;
    assert_eq!(status, StatusCode::FOUND, "a relay");
    // Behind a proxy that rewrites Host, the page's origin is the browser's Origin.
    const PROXIED: &[(&str, &str)] =
        &[("host", "127.0.0.1:3000"), ("origin", "https://app.localhost"), ("sec-fetch-site", "same-origin")];
    let (status, _) = send("/public", "POST", PROXIED).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "the page's own origin");
    const PLAIN: &[(&str, &str)] =
        &[("host", "app.localhost"), ("origin", "http://app.localhost"), ("sec-fetch-site", "same-origin")];
    let (status, _) = send("/public", "POST", PLAIN).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "an http page may go to its https origin");
    const SECURE: &[(&str, &str)] =
        &[("host", "app.localhost"), ("origin", "https://app.localhost"), ("sec-fetch-site", "same-origin")];
    let (status, _) = send("/own", "POST", SECURE).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "an https page's form may not go to http");
    for path in ["/pay", "/sub", "/path"] {
        let (status, _) = send(path, "POST", &[]).await;
        assert!(status == StatusCode::SEE_OTHER, "{path}: {status}");
    }
    let (status, _) = send("/own", "POST", &[("host", "app.localhost")]).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "the request's own host is 'self'");
    let (status, _) = send("/own", "POST", &[]).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "with no Host, an address is another origin");
    let (status, _) = send("/read", "GET", &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "a GET is not held to form-action");
    // An area's policy holds a write at its path, and the page the form was on counts too.
    let (status, _) = send("/checkout/go", "POST", &[]).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "the area allows it");
    const FROM_AREA: &[(&str, &str)] = &[("host", "app.localhost"), ("referer", "http://app.localhost/checkout?step=2")];
    let (status, _) = send("/sibling", "POST", FROM_AREA).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "the form's page, in the area, allows it");
    // A post from another site's page (an identity provider's form_post) is that page's policy's.
    const IDP: &[(&str, &str)] =
        &[("host", "app.localhost"), ("origin", "https://idp.example"), ("sec-fetch-site", "cross-site")];
    assert_eq!(send("/else", "POST", IDP).await.0, StatusCode::SEE_OTHER, "cross-site");
    const IDP_OLD: &[(&str, &str)] = &[("host", "app.localhost"), ("origin", "https://idp.example")];
    assert_eq!(send("/else", "POST", IDP_OLD).await.0, StatusCode::SEE_OTHER, "another origin, no Sec-Fetch-Site");
    // A test that names no host and sends the form's page is held to that page's policy.
    let (status, _) = send("/sibling", "POST", &[("referer", "http://app.localhost/news")]).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "the Referer alone says the page");
    const FROM_NEWS: &[(&str, &str)] = &[("host", "app.localhost"), ("referer", "http://app.localhost/news")];
    let (status, _) = send("/sibling", "POST", FROM_NEWS).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "the news page does not allow it");
    // A Referer of `/` is what any page under a referrer policy that sends only the origin sends
    // (one the page set itself included): it names no page.
    const ROOT: &[(&str, &str)] = &[("host", "app.localhost"), ("referer", "http://app.localhost/")];
    let (status, _) = send("/sibling", "POST", ROOT).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "a Referer of / may be any page");
    // Other ways a browser says the page is not one of the app's, and the bodies a form sends.
    for site in ["same-site", "none"] {
        let headers: &'static [(&'static str, &'static str)] = match site {
            // Allowed by `allow_origin`, else the write is refused before the check.
            "same-site" => &[("host", "app.localhost"), ("origin", "https://idp.example"), ("sec-fetch-site", "same-site")],
            _ => &[("host", "app.localhost"), ("origin", "http://app.localhost"), ("sec-fetch-site", "none")],
        };
        assert_eq!(send("/else", "POST", headers).await.0, StatusCode::SEE_OTHER, "{site}");
    }
    for body in ["multipart/form-data; boundary=x", "text/plain"] {
        let headers: &'static [(&'static str, &'static str)] = match body {
            "text/plain" => &[("content-type", "text/plain")],
            _ => &[("content-type", "multipart/form-data; boundary=x")],
        };
        assert_eq!(send("/else", "POST", headers).await.0, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    }
    // An https page's Origin and a Host that writes its default port are the same origin.
    const PORT: &[(&str, &str)] =
        &[("host", "app.localhost:443"), ("origin", "https://app.localhost"), ("sec-fetch-site", "same-origin")];
    assert_eq!(send("/public", "POST", PORT).await.0, StatusCode::SEE_OTHER, "Host with :443");
    // A browser that does not say the page's origin is not checked.
    const NULL: &[(&str, &str)] = &[("host", "127.0.0.1:3000"), ("origin", "null"), ("sec-fetch-site", "same-origin")];
    let (status, _) = send("/else", "POST", NULL).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "Origin: null");
    // A Host with its scheme's default port is the same host.
    let (status, _) = send("/own", "POST", &[("host", "app.localhost:80")]).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "Host with :80");
    // Only the form's page's policy counts, not the one at the path it posts to.
    const FROM_HOME: &[(&str, &str)] = &[
        ("host", "app.localhost"),
        ("origin", "http://app.localhost"),
        ("sec-fetch-site", "same-origin"),
        ("referer", "http://app.localhost/news"),
    ];
    let (status, _) = send("/checkout/go", "POST", FROM_HOME).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "the news page does not allow it");
    // A browser that sends no page (no-referrer) may be on any of the app's pages.
    const NO_PAGE: &[(&str, &str)] =
        &[("host", "app.localhost"), ("origin", "http://app.localhost"), ("sec-fetch-site", "same-origin")];
    let (status, _) = send("/sibling", "POST", NO_PAGE).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "some page of the app allows it");
    let (status, _) = send("/pay", "POST", NO_PAGE).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
}

/// The 500 says which origin and what allows it, under `corten dev`; a sign-in on another origin
/// is checked as a redirect is.
#[cfg(debug_assertions)]
#[tokio::test]
async fn a_redirect_outside_form_action_names_the_origin_and_a_sign_in_is_checked() {
    let routes = Router::new()
        .route(
            "/go",
            axum::routing::post(|req: Req| async move {
                req.redirect::<()>(&"https://else.example/x?y=1".parse::<axum::http::Uri>().unwrap())
            }),
        )
        .route("/account", axum::routing::post(|_: Caller<Elsewhere>| async { "saved" }))
        .route(
            "/cookie",
            axum::routing::post(|| async {
                use axum::{
                    http::{header, HeaderValue},
                    response::IntoResponse,
                };
                let mut res = StatusCode::SEE_OTHER.into_response();
                let headers = res.headers_mut();
                headers.insert(header::LOCATION, HeaderValue::from_static("//user:pw@else.example/secret?token=abc"));
                headers.append(header::SET_COOKIE, HeaderValue::from_static("a=1"));
                headers.append(header::SET_COOKIE, HeaderValue::from_static("b=2"));
                res
            }),
        );
    let dev = App::new(Routes::from(routes.clone())).dev(true).router();
    let (status, _, body) = call_with(dev.clone(), "POST", "/go", None, Some("text/html")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body.contains("https://else.example") && body.contains("Policy::form_action"), "{body}");
    assert!(!body.contains("/x?y=1"), "the origin alone: {body}");
    assert!(body.contains("send a Host in the test"), "no host is known: {body}");
    let (status, headers, body) = call_with(dev.clone(), "POST", "/cookie", None, Some("text/html")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let cookies: Vec<_> = headers.get_all("set-cookie").iter().collect();
    assert_eq!(cookies, ["a=1", "b=2"], "the handler's cookies are kept");
    assert!(!headers.contains_key("location"));
    assert!(body.contains("else.example") && !body.contains("token") && !body.contains("pw@"), "the origin alone: {body}");
    let (status, _, body) = call_with(dev, "POST", "/account", None, Some("text/html")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(body.contains("https://id.example") && body.contains("Policy::form_action"), "{body}");
    let allowed = App::new(Routes::from(routes.clone())).policy(crate::security::Policy::new().form_action("https://id.example"));
    let (status, ..) = call_with(allowed.router(), "POST", "/account", None, Some("text/html")).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "allowed, it is the sign-in redirect");
    let (status, _, body) =
        call_with(App::new(Routes::from(routes.clone())).router(), "POST", "/go", None, Some("text/html")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!body.contains("else.example"), "outside dev the page does not say: {body}");
    // A report-only policy stops nothing.
    let watching = App::new(Routes::from(routes)).policy(crate::security::Policy::new().report_only());
    let (status, ..) = call_with(watching.router(), "POST", "/go", None, Some("text/html")).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "report-only");
}

/// Only a form's post is checked (a form sends nothing else), a test's request is on the app's
/// own names, and a page whose policy sends only its origin as `Referer` may be any page.
#[cfg(debug_assertions)]
#[tokio::test]
async fn only_a_form_post_is_checked_against_form_action() {
    use crate::security::{Policy, Referrer};
    let to = |uri: &'static str| {
        move |req: Req| async move {
            let uri: axum::http::Uri = uri.parse().unwrap();
            req.redirect::<()>(&uri)
        }
    };
    let routes = Router::new()
        .route("/else", axum::routing::post(to("https://else.example/x")).put(to("https://else.example/x")))
        .route("/canonical", axum::routing::post(to("https://app.example/done")))
        .route("/sibling", axum::routing::post(to("http://localhost:9999/account")));
    let router = App::new(Routes::from(routes))
        .policy(Policy::new().host("app.example").referrer(Referrer::StrictOrigin))
        .area(|area| checkout_url::area(area).policy(|p| p.form_action("http://localhost:9999")))
        .router();
    let send = |method: &'static str, path: &'static str, headers: &'static [(&'static str, &'static str)]| {
        let router = router.clone();
        async move {
            let mut req = Request::builder().method(method).uri(path).header("accept", "text/html");
            for (name, value) in headers {
                req = req.header(*name, *value);
            }
            router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap().status()
        }
    };
    assert_eq!(send("POST", "/else", &[]).await, StatusCode::INTERNAL_SERVER_ERROR, "a form's post");
    const FORM: &[(&str, &str)] = &[("content-type", "application/x-www-form-urlencoded; charset=utf-8")];
    assert_eq!(send("POST", "/else", FORM).await, StatusCode::INTERNAL_SERVER_ERROR, "a form's body");
    assert_eq!(send("PUT", "/else", &[]).await, StatusCode::SEE_OTHER, "no form sends a PUT");
    const JSON: &[(&str, &str)] = &[("content-type", "application/json")];
    assert_eq!(send("POST", "/else", JSON).await, StatusCode::SEE_OTHER, "no form sends JSON");
    assert_eq!(send("POST", "/canonical", &[]).await, StatusCode::SEE_OTHER, "the app's own name, with no Host");
    // Under strict-origin every page's Referer is `/`: the form may be on the checkout page.
    const ROOT: &[(&str, &str)] = &[
        ("host", "app.example"),
        ("origin", "https://app.example"),
        ("sec-fetch-site", "same-origin"),
        ("referer", "https://app.example/"),
    ];
    assert_eq!(send("POST", "/sibling", ROOT).await, StatusCode::SEE_OTHER, "a Referer of / is not the page");
}

/// Someone signed in on another origin's sign-in page.
#[cfg(debug_assertions)]
#[derive(Clone, Hash)]
struct Elsewhere;

#[cfg(debug_assertions)]
impl<S: Sync> crate::Identify<S> for Elsewhere {
    async fn identify(_: &axum::http::request::Parts, _: &S) -> std::result::Result<Self, crate::Refused> {
        Err(crate::Refused::sign_in(&"https://id.example/sign-in".parse::<axum::http::Uri>().unwrap()))
    }
}

#[cfg(debug_assertions)]
#[crate::routes(area = Checkout)]
mod checkout_url {
    #[route("/checkout")]
    pub struct Checkout;

    #[route("/checkout/go", post = checkout_go)]
    pub struct Go;
}

#[cfg(debug_assertions)]
async fn checkout_go(req: Req) -> HtmlResult {
    req.redirect(&"http://localhost:9999/paid".parse::<axum::http::Uri>().unwrap())
}

/// A value only the layout shows is read for a full load, not for an answer the runtime swaps in.
#[tokio::test]
async fn a_layout_value_is_read_only_when_the_layout_renders() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static READS: AtomicU32 = AtomicU32::new(0);
    let routes = Router::new().route(
        "/",
        get(|req: Req| async move {
            let count = if req.mode.swaps() { None } else { Some(READS.fetch_add(1, Ordering::Relaxed) + 7) };
            req.page::<()>(html! {
                <html>
                    <head>
                        <title>"Shop"</title>
                        <Corten />
                    </head>
                    <body>
                        <header>"Cart "{count.unwrap_or(0)}</header>
                        <main>"Items"</main>
                    </body>
                </html>
            })
        }),
    );
    let (_, _, body) = call_with(app(routes.clone(), None), "GET", "/", None, Some("text/html")).await;
    assert!(body.contains("Cart 7"), "{body}");
    for mode in ["nav", "action"] {
        let (_, _, body) = call_with(app(routes.clone(), None), "GET", "/", Some(mode), Some("text/html")).await;
        assert!(!body.contains("Cart") && body.contains("Items"), "{mode}: {body}");
    }
    assert_eq!(READS.load(Ordering::Relaxed), 1, "read once, for the full load");
    assert!(
        [Mode::Navigation, Mode::Action].iter().all(|m| m.swaps()) && ![Mode::Document, Mode::Data].iter().any(|m| m.swaps())
    );
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
    let routes = Router::new().route("/", get(|req: Req| async move { req.moved::<ForOne>(&Item { slug: "pika chu" }) }));
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
        .route("/", get(|req: Req| async move { req.redirect::<ForOne>(&Item { slug: "pika chu" }.at(PlaceId("7 & 8"))) }))
        .route("/old", get(|req: Req| async move { req.moved::<ForOne>(&Item { slug: "pika" }.at(PlaceId("7"))) }));
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

/// A client navigation's `fetch` cannot follow a redirect to another origin (the CSP's
/// `connect-src`, else CORS), so it is told where to go and the browser goes there itself; a full
/// load still gets the 303 or 301.
#[tokio::test]
async fn a_redirect_to_another_origin_is_named_to_a_client_navigation() {
    use axum::http::Uri;
    let routes = Router::new()
        .route(
            "/away",
            get(|req: Req| async move { req.redirect::<ForOne>(&"https://shop.example/sales?x=1".parse::<Uri>().unwrap()) }),
        )
        .route("/moved", get(|req: Req| async move { req.moved::<ForOne>(&"//shop.example/sales".parse::<Uri>().unwrap()) }))
        .route("/here", get(|req: Req| async move { req.redirect::<ForOne>(&Done) }));
    for (uri, to, full) in [
        ("/away", "https://shop.example/sales?x=1", StatusCode::SEE_OTHER),
        ("/moved", "//shop.example/sales", StatusCode::MOVED_PERMANENTLY),
    ] {
        for mode in ["nav", "action"] {
            let (status, headers, _) = call_with(app(routes.clone(), None), "GET", uri, Some(mode), None).await;
            assert_eq!((status, &headers["corten-location"]), (StatusCode::NO_CONTENT, &to.parse().unwrap()), "{uri} {mode}");
            assert!(!headers.contains_key("location"), "{uri} {mode}: nothing for fetch to follow");
        }
        let (status, headers, _) = call_with(app(routes.clone(), None), "GET", uri, None, Some("text/html")).await;
        assert_eq!((status, &headers["location"]), (full, &to.parse().unwrap()), "{uri}: a full load follows it");
    }
    // A path on this origin is still a redirect fetch follows, in one round trip.
    let (status, headers, _) = call_with(app(routes, None), "GET", "/here", Some("nav"), None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(!headers.contains_key("corten-location"));
}

/// A place on a page outside the app keeps its `#fragment` through `moved` and `redirect`, which
/// an `axum::http::Uri` alone drops.
#[tokio::test]
async fn a_redirect_to_a_place_elsewhere_keeps_its_fragment() {
    use crate::route::Elsewhere;
    use axum::http::Uri;
    static COMPARE: std::sync::LazyLock<Uri> = std::sync::LazyLock::new(|| Uri::from_static("https://example.com/compare?x=1"));
    let routes = Router::new()
        .route("/old", get(|req: Req| async move { req.moved::<ForOne>(&Elsewhere::at(&COMPARE, "fee-calculator")) }))
        .route("/go", get(|req: Req| async move { req.redirect::<ForOne>(&Elsewhere::at(&COMPARE, "fees & taxes")) }));
    let place = "https://example.com/compare?x=1#fee-calculator";
    let (status, headers, _) = call_with(app(routes.clone(), None), "GET", "/old", None, Some("text/html")).await;
    assert_eq!((status, &headers["location"]), (StatusCode::MOVED_PERMANENTLY, &place.parse().unwrap()), "a full load");
    let (status, headers, _) = call_with(app(routes.clone(), None), "GET", "/old", Some("nav"), None).await;
    assert_eq!((status, &headers["corten-location"]), (StatusCode::NO_CONTENT, &place.parse().unwrap()), "a client navigation");
    let (status, headers, _) = call_with(app(routes, None), "GET", "/go", None, Some("text/html")).await;
    let to = "https://example.com/compare?x=1#fees%20&%20taxes";
    assert_eq!((status, &headers["location"]), (StatusCode::SEE_OTHER, &to.parse().unwrap()));
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "Elsewhere::at takes a full URL outside the app")]
fn a_place_elsewhere_on_a_path_panics_in_debug_builds() {
    let path = axum::http::Uri::from_static("/items/pika");
    let _ = crate::route::Elsewhere::at(&path, "place-7");
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, crate::UrlValue)]
#[serde(rename_all = "kebab-case")]
enum Domain {
    SellerPortal,
    Catalog,
}

#[derive(Debug, PartialEq, serde::Deserialize, Fields)]
struct Address {
    street: String,
    zip: Option<u32>,
}

#[derive(Debug, PartialEq, serde::Deserialize, Fields)]
struct DomainLimits {
    max_concurrent: u32,
    #[serde(default)]
    adaptive: bool,
    #[fields(group)]
    contact: Address,
}

#[derive(Debug, PartialEq, serde::Deserialize, Fields)]
struct Limits {
    name: String,
    #[fields(group)]
    ship_from: Address,
    #[fields(group)]
    ship_to: Address,
    #[fields(rows)]
    #[serde(default)]
    domains: std::collections::BTreeMap<Domain, DomainLimits>,
}

#[derive(crate::view::Id)]
struct LimitsFormId;

/// A group's fields are named under it and a row's under its key, so two groups of one type
/// and every row read into their own values, and each row's field has its own message.
#[test]
fn groups_and_rows_name_each_field_and_read_back_by_key() {
    let f = Limits::FIELDS;
    assert_eq!(f.name.to_string(), "name");
    assert_eq!(f.ship_to.street.to_string(), "ship_to.street");
    let portal = f.domains.at(&Domain::SellerPortal);
    assert_eq!(portal.max_concurrent.to_string(), "domains[seller-portal].max_concurrent");
    assert_eq!(portal.contact.zip.to_string(), "domains[seller-portal].contact.zip");
    let keyed = Limits2::FIELDS.lines.at(&"a b]\"c".to_string()).qty.to_string();
    assert_eq!(keyed, "lines[a%20b%5D%22c].qty");

    // Out of order, a row with its checkbox unchecked, the catalog row's zip blank.
    let body = "domains%5Bcatalog%5D.max_concurrent=4&ship_to.street=2+Elm&name=Main\
        &domains%5Bseller-portal%5D.adaptive=on&domains%5Bseller-portal%5D.contact.street=1+Oak\
        &ship_from.street=9+Ash&domains%5Bcatalog%5D.contact.street=3+Pine&ship_from.zip=55401\
        &domains%5Bseller-portal%5D.max_concurrent=2&domains%5Bcatalog%5D.contact.zip=&ship_to.zip=";
    let form: Limits = crate::form::from_bytes(body.as_bytes()).expect("reads");
    assert_eq!(form.name, "Main");
    assert_eq!(form.ship_from, Address { street: "9 Ash".into(), zip: Some(55401) });
    assert_eq!(form.ship_to, Address { street: "2 Elm".into(), zip: None });
    assert_eq!(
        form.domains[&Domain::SellerPortal],
        DomainLimits { max_concurrent: 2, adaptive: true, contact: Address { street: "1 Oak".into(), zip: None } }
    );
    assert_eq!(
        form.domains[&Domain::Catalog],
        DomainLimits { max_concurrent: 4, adaptive: false, contact: Address { street: "3 Pine".into(), zip: None } }
    );
    let keyed: Limits2 = crate::form::from_bytes(b"lines%5Ba%2520b%255D%2522c%5D.qty=3").expect("reads");
    assert_eq!(keyed.lines["a b]\"c"].qty, 3);

    // A row left out reads no row; nothing shifts.
    let one: Limits = crate::form::from_bytes(
        b"name=x&ship_from.street=a&ship_to.street=b&domains%5Bcatalog%5D.max_concurrent=1&domains%5Bcatalog%5D.contact.street=c",
    )
    .expect("reads");
    assert_eq!(one.domains.keys().collect::<Vec<_>>(), [&Domain::Catalog]);

    // Mistakes name the field as posted.
    let err = |body: &str| crate::form::from_bytes::<Limits>(body.as_bytes()).unwrap_err().to_string();
    let base = "name=x&ship_from.street=a&ship_to.street=b";
    assert_eq!(err("name=x&ship_to.street=b"), "missing field `ship_from`");
    assert_eq!(err("name=x&ship_from.zip=1&ship_to.street=b"), "ship_from: missing field `street`");
    assert!(
        err(&format!("{base}&domains%5Bcatalog%5D.max_concurrent=x&domains%5Bcatalog%5D.contact.street=c"))
            .starts_with("domains[catalog].max_concurrent: invalid"),
        "{}",
        err(&format!("{base}&domains%5Bcatalog%5D.max_concurrent=x&domains%5Bcatalog%5D.contact.street=c"))
    );
    assert!(
        err(&format!("{base}&domains%5Bnope%5D.max_concurrent=1")).starts_with("domains[nope]: key: unknown variant `nope`"),
        "{}",
        err(&format!("{base}&domains%5Bnope%5D.max_concurrent=1"))
    );

    // An error on one row marks that row's input only, with its own message id.
    let mut errors = Errors::new();
    errors.add(f.domains.at(&Domain::SellerPortal).max_concurrent, "At least 1.");
    errors.add(f.domains.at(&Domain::SellerPortal).max_concurrent, "dropped: the field has one");
    errors.add(f.domains.at(&Domain::Catalog).max_concurrent, "At most 8.");
    let e = &errors.within(LimitsFormId);
    let row = |d: Domain| {
        let field = f.domains.at(&d).max_concurrent;
        html! { <input name={field} {..e.aria(field)}>{e.message(field)} }.to_html()
    };
    assert_eq!(
        row(Domain::SellerPortal),
        r#"<input name="domains[seller-portal].max_concurrent" aria-invalid="true" aria-describedby="limits-form-domains[seller-portal].max_concurrent-error"><p class="field-error" id="limits-form-domains[seller-portal].max_concurrent-error">At least 1.</p>"#
    );
    assert!(row(Domain::Catalog).contains("At most 8."));
    assert!(!errors.has(f.domains.at(&Domain::SellerPortal).contact.street));
    assert_eq!(
        errors.to_string(),
        "domains[seller-portal].max_concurrent: At least 1.; domains[catalog].max_concurrent: At most 8."
    );
}

#[derive(Debug, serde::Deserialize, Fields)]
struct Line {
    qty: u32,
}

#[derive(Debug, serde::Deserialize, Fields)]
struct Limits2 {
    #[fields(rows)]
    #[serde(default)]
    lines: std::collections::BTreeMap<String, Line>,
}

#[derive(Debug, serde::Deserialize, Fields)]
struct Numbered {
    #[fields(rows)]
    #[serde(default)]
    lines: std::collections::BTreeMap<u32, Line>,
}

#[derive(Debug, serde::Deserialize, Fields)]
struct Counts {
    note: String,
    #[fields(map)]
    #[serde(default)]
    counts: std::collections::BTreeMap<u32, u32>,
}

/// One plain value per key (a count per SKU) is `map[key]`, read back by key, each with its own
/// error.
#[test]
fn a_map_names_each_keys_value() {
    let f = Counts::FIELDS;
    assert_eq!(f.counts.at(&7).to_string(), "counts[7]");
    let read: Counts = crate::form::from_bytes(b"counts%5B7%5D=3&note=x&counts%5B12%5D=0").expect("reads");
    assert_eq!(read.counts.into_iter().collect::<Vec<_>>(), [(7, 3), (12, 0)]);
    let err = crate::form::from_bytes::<Counts>(b"note=x&counts%5B7%5D=many").unwrap_err().to_string();
    assert!(err.starts_with("counts[7]: invalid value"), "{err}");
    let none: Counts = crate::form::from_bytes(b"note=x").expect("no keys: an empty map");
    assert!(none.counts.is_empty() && none.note == "x");

    let mut errors = Errors::new();
    errors.add(f.counts.at(&7), "Not that many.");
    let e = errors.within(LimitsFormId);
    let field = f.counts.at(&7);
    assert_eq!(
        html! { <input name={field} {..e.aria(field)}> }.to_html(),
        r#"<input name="counts[7]" aria-invalid="true" aria-describedby="limits-form-counts[7]-error">"#
    );
    assert!(!errors.has(f.counts.at(&12)));
}

#[derive(Debug, serde::Deserialize, Fields)]
struct Search {
    q: String,
    go: Option<String>,
}

#[derive(Debug, serde::Deserialize, Fields)]
struct Moded {
    q: String,
    #[serde(rename = "q-mode")]
    mode: String,
}

/// What a client sends is not trusted: a key sent another way than the form writes it is
/// refused, stray `x.y` keys beside a field sent as `x` are left alone, and many rows' errors
/// cost each row its own lookup.
#[test]
fn rows_read_only_their_own_keys_and_scale_with_their_count() {
    // `a` as `%61`, lowercase hex, a character the form would encode: each refused.
    for key in ["%2561", "a%252cb", "a.b"] {
        let body = format!("lines%5B{key}%5D.qty=1");
        let err = crate::form::from_bytes::<Limits2>(body.as_bytes()).unwrap_err().to_string();
        assert!(err.contains("the row's key is not"), "{key}: {err}");
    }
    let read: Limits2 = crate::form::from_bytes(b"lines%5Ba%252Cb%5D.qty=1").expect("the form's own encoding");
    assert_eq!(read.lines["a,b"].qty, 1);

    // An image button (`go.x`, `go.y`) and a stray `q.x` beside `q`: read as a plain form was.
    let search: Search = crate::form::from_bytes(b"q=cards&q.x=1&go.x=3&go.y=4").expect("reads");
    assert_eq!((search.q.as_str(), search.go), ("cards", None));
    // A key between `q` and `q.x` (`q-mode`) does not make `q.x` a group.
    let moded: Moded = crate::form::from_bytes(b"q=a&q-mode=b&q.x=1").expect("q is sent as itself");
    assert_eq!((moded.q.as_str(), moded.mode.as_str()), ("a", "b"));

    // An integer key is read only as it is written, so one row cannot be sent two ways.
    for key in ["07", "%2B7", "-0"] {
        let body = format!("lines%5B{key}%5D.qty=1");
        let err = crate::form::from_bytes::<Numbered>(body.as_bytes()).unwrap_err().to_string();
        assert!(err.contains("as the form writes it") || err.contains("the way the form writes it"), "{key}: {err}");
    }

    // Integer keys, written without allocating, and each row's errors found by its key.
    let f = Numbered::FIELDS;
    assert_eq!(f.lines.at(&7).qty.to_string(), "lines[7].qty");
    let read: Numbered = crate::form::from_bytes(b"lines%5B7%5D.qty=2&lines%5B12%5D.qty=5").expect("reads");
    assert_eq!((read.lines[&7].qty, read.lines[&12].qty), (2, 5));
    let mut errors = Errors::new();
    for row in 0..20_000u32 {
        errors.add(f.lines.at(&row).qty, "Too many.");
        errors.add(f.lines.at(&row).qty, "dropped: the row has one");
    }
    assert_eq!(errors.len(), 20_000);
    assert_eq!(errors.get(f.lines.at(&19_999).qty), Some("Too many."));
    assert!(!errors.has(f.lines.at(&20_000).qty));
    assert!(errors.iter().next().is_some_and(|(name, _)| name == "lines[0].qty"));

    // A group in a row: its own message id, and its key in the JSON clients get.
    let mut errors = Errors::new();
    let zip = Limits::FIELDS.domains.at(&Domain::SellerPortal).contact.zip;
    errors.add(zip, "Five digits.");
    let e = errors.within(LimitsFormId);
    assert_eq!(
        html! { {e.message(zip)} }.to_html(),
        r#"<p class="field-error" id="limits-form-domains[seller-portal].contact.zip-error">Five digits.</p>"#
    );
    #[cfg(feature = "json")]
    assert_eq!(serde_json::to_string(&errors).unwrap(), r#"{"domains[seller-portal].contact.zip":"Five digits."}"#);
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
    // A hint of its own: one aria-describedby naming the hint, then the error. The hint's id is a
    // temporary, which lives as long as the spread renders.
    let field = String::from("email");
    let e = errors.within(SignUpId);
    let html = html! { <input name={f.email} {..e.aria(f.email).hint(&FieldHintId(&field))}> }.to_html();
    assert_eq!(html, r#"<input name="email" aria-invalid="true" aria-describedby="field-hint-email sign-up-email-error">"#);
}

#[derive(crate::view::Id)]
struct FieldHintId<'a>(&'a str);

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
            .route::<ApiItem>(crate::Methods::reads(get(|axum::extract::Path(id): axum::extract::Path<u32>| async move {
                format!("item {id}")
            })))
            .route::<ApiMissing>(crate::Methods::reads(get(|| async {
                Err::<Response, _>(Error::new("no item 7").with_status(StatusCode::NOT_FOUND))
            })))
            .route::<ApiDown>(crate::Methods::reads(get(|| async { Err::<Response, _>(Error::new("database password wrong")) })))
            .route::<ApiTeapot>(crate::Methods::reads(get(|| async {
                (StatusCode::IM_A_TEAPOT, [("content-type", "application/json")], r#"{"mine":1}"#)
            })))
            .route::<ApiSlow>(crate::Methods::reads(get(std::future::pending::<&'static str>)))
            .route::<ApiPost>(crate::Methods::writes(axum::routing::post(|| async { "posted" })));
        settings(api)
    })
    .router()
}

/// An area or an API with a deadline of its own: its routes get it, and every other page keeps
/// the app's. In nested areas the innermost that sets one decides; one that sets none takes its
/// parent's.
#[tokio::test]
async fn an_area_or_an_api_sets_its_own_deadline() {
    let slow = || {
        crate::Methods::reads(get(|| async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            "done"
        }))
    };
    let router = App::new(Routes::from(Router::new().route("/slow", slow().0)))
        .error_page(test_error_page, ())
        .deadline(Duration::from_millis(50))
        .area::<Admin>(|admin| admin.route::<AdminSlow>(slow()).deadline(Duration::from_secs(10)))
        .area::<AdminQuick>(|quick| quick.route::<AdminQuickSlow>(slow()).deadline(Duration::from_millis(20)))
        .area::<AdminOther>(|other| other.route::<AdminOtherSlow>(slow()))
        .api::<ApiRoot>(|api| api.route::<ApiSlow>(slow()).deadline(None))
        .router();
    let status = |uri| {
        let router = router.clone();
        async move { call_with(router, "GET", uri, None, Some("text/html")).await.0 }
    };
    assert_eq!(status("/slow").await, StatusCode::SERVICE_UNAVAILABLE, "a page keeps the app's deadline");
    assert_eq!(status("/admin/slow").await, StatusCode::OK, "the area's own");
    assert_eq!(status("/admin/other/slow").await, StatusCode::OK, "a nested area without one takes its parent's");
    assert_eq!(status("/admin/quick/slow").await, StatusCode::SERVICE_UNAVAILABLE, "the innermost decides");
    assert_eq!(status("/api/slow").await, StatusCode::OK, "None removes it for the API");
}

#[cfg(feature = "json")]
paths! {
    Zone = "/zone",
    ZoneApi = "/zone/api",
    ZoneApiPing = "/zone/api/ping",
    ZoneMid = "/zone/api/mid",
    ZoneMidPost = "/zone/api/mid/post",
    ZoneIn = "/zone/api/mid/in",
    ZoneInPing = "/zone/api/mid/in/ping",
    Everything = "/",
    EverythingPing = "/ping",
}

#[cfg(feature = "json")]
#[derive(crate::Route)]
#[route("/{*rest}")]
struct EverythingRest<'a> {
    rest: &'a str,
}

/// A request with one more header, or several.
#[cfg(feature = "json")]
async fn call_with_header(
    router: Router,
    method: &str,
    path: &str,
    header: (&str, &str),
) -> (StatusCode, axum::http::HeaderMap, String) {
    call_with_headers(router, method, path, &[header]).await
}

#[cfg(feature = "json")]
async fn call_with_headers(
    router: Router,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut req = Request::builder().method(method).uri(path);
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let (parts, body) = router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap().into_parts();
    (parts.status, parts.headers, String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap())
}

/// Three APIs nested in an area, each taking what it leaves unset from the nearest around it and
/// keeping what it sets; their middleware runs area, outer, middle, inner (#539).
#[cfg(feature = "json")]
#[tokio::test]
async fn nested_apis_take_what_they_leave_unset_from_the_nearest() {
    let steps = || {
        crate::Methods::reads(get(|headers: axum::http::HeaderMap| async move {
            tokio::time::sleep(Duration::from_millis(if headers.contains_key("x-slow") { 200 } else { 0 })).await;
            headers.get("x-steps").map_or(String::new(), |v| v.to_str().unwrap().to_string())
        }))
    };
    let shape = |status: StatusCode, message: &str| ShapedError {
        r#type: "error",
        error: ShapedDetail { code: status.as_u16(), message: message.to_string() },
    };
    let shaped = |code: u16, message: &str| format!(r#"{{"type":"error","error":{{"code":{code},"message":{message:?}}}}}"#);
    let router = App::new(Routes::new())
        .middleware(Step("app", false))
        .api::<ZoneIn>(|inner| inner.route::<ZoneInPing>(steps()).middleware(Step("in", false)))
        .area::<Zone>(|zone| zone.middleware(Step("zone", false)).deadline(Duration::from_millis(50)))
        .api::<ZoneMid>(|mid| {
            mid.route::<ZoneMidPost>(crate::Methods::writes(axum::routing::post(|| async { "posted" })))
                .middleware(Step("mid", false))
                .allow_origin("https://mid.example")
        })
        .api::<ZoneApi>(|api| {
            api.route::<ZoneApiPing>(steps()).middleware(Step("api", false)).errors(shape).allow_origin("https://outer.example")
        })
        .router();
    let send = |method: &'static str, path: &'static str, headers: &'static [(&'static str, &'static str)]| {
        let router = router.clone();
        async move {
            let mut req = Request::builder().method(method).uri(path);
            for (name, value) in headers {
                req = req.header(*name, *value);
            }
            let (parts, body) = router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap().into_parts();
            (parts.status, parts.headers, String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap())
        }
    };
    assert_eq!(send("GET", "/zone/api/mid/in/ping", &[]).await.2, "app zone api mid in", "area, then outermost API first");
    assert_eq!(send("GET", "/zone/api/ping", &[]).await.2, "app zone api");
    let (status, _, body) = send("GET", "/zone/api/mid/in/ping", &[("x-slow", "1")]).await;
    assert_eq!((status.as_u16(), body), (503, shaped(503, "Service Unavailable")), "the area's deadline, the outer API's shape");
    let (status, _, body) = send("GET", "/zone/api/mid/in/nope", &[]).await;
    assert_eq!((status.as_u16(), body), (404, shaped(404, "Not Found")), "an unknown URL under the inner prefix");
    let preflight = |origin: &'static str| -> &'static [(&'static str, &'static str)] {
        match origin {
            "mid" => &[("origin", "https://mid.example"), ("access-control-request-method", "GET")],
            _ => &[("origin", "https://outer.example"), ("access-control-request-method", "GET")],
        }
    };
    // Origins are each API's own: the inner API names none, so neither the middle's nor the outer's
    // partner may call it, and the middle's list is not the outer's.
    for (path, origin) in [("/zone/api/mid/in/ping", "mid"), ("/zone/api/mid/in/ping", "outer"), ("/zone/api/mid/post", "outer")]
    {
        let (_, headers, _) = send("OPTIONS", path, preflight(origin)).await;
        assert!(!headers.contains_key("access-control-allow-origin"), "{path} from {origin}");
    }
    let (status, headers, _) = send("OPTIONS", "/zone/api/mid/post", preflight("mid")).await;
    assert_eq!(
        (status, headers.get("access-control-allow-origin").map(|v| v.to_str().unwrap())),
        (StatusCode::NO_CONTENT, Some("https://mid.example")),
        "an API's own origin"
    );
    let inner_cross: &[(&str, &str)] = &[("origin", "https://mid.example"), ("sec-fetch-site", "cross-site")];
    assert_eq!(
        send("POST", "/zone/api/mid/in/ping", inner_cross).await.0,
        StatusCode::FORBIDDEN,
        "nor its writes under the inner prefix"
    );
    let cross: &[(&str, &str)] = &[("origin", "https://mid.example"), ("sec-fetch-site", "cross-site")];
    assert_eq!(send("POST", "/zone/api/mid/post", cross).await.2, "posted", "a write from its own origin");
    let evil: &[(&str, &str)] = &[("origin", "https://outer.example"), ("sec-fetch-site", "cross-site")];
    let (status, _, body) = send("POST", "/zone/api/mid/post", evil).await;
    assert_eq!(
        (status.as_u16(), body),
        (403, shaped(403, "Cross-site request refused.")),
        "the outer's origin is not the middle's"
    );

    // A "/" API around another: the inner one's routes meet the outer's middleware first.
    let router = App::new(Routes::new())
        .api::<Everything>(|all| all.route::<EverythingPing>(steps()).middleware(Step("all", false)))
        .api::<ApiRoot>(|api| api.route::<ApiSlow>(steps()).middleware(Step("api", false)))
        .router();
    assert_eq!(call_with(router.clone(), "GET", "/api/slow", None, None).await.2, "all api");
    assert_eq!(call_with(router, "GET", "/ping", None, None).await.2, "all");
    // A "/" API's catch-all does not answer under another API's prefix.
    let router = App::new(Routes::new())
        .api::<Everything>(|all| all.route::<EverythingRest>(steps()))
        .api::<ApiRoot>(|api| api.route::<ApiSlow>(steps()))
        .router();
    assert_eq!(call_with(router.clone(), "GET", "/elsewhere", None, None).await.0, StatusCode::OK);
    assert_eq!(call_with(router, "GET", "/api/elsewhere", None, None).await.0, StatusCode::NOT_FOUND);
}

#[cfg(feature = "json")]
paths! {
    Outer = "/outer",
    OuterIn = "/outer/in",
    OuterInSlow = "/outer/in/slow",
    OuterInNone = "/outer/none",
    OuterInNoneSlow = "/outer/none/slow",
    Walled = "/walled",
    WalledApi = "/walled/api",
    WalledApiIn = "/walled/api/in",
    WalledApiInSlow = "/walled/api/in/slow",
}

#[cfg(feature = "json")]
#[derive(crate::Route)]
#[route("/outer/{id}")]
struct OuterAny<'a> {
    id: &'a str,
}

/// What an inner API sets is its own over the outer's (its error shape, its deadline, `None`
/// included), an outer API's deadline over an area's around both, and an outer parameter route
/// never answers a path under the inner prefix (#539).
#[cfg(feature = "json")]
#[tokio::test]
async fn an_inner_apis_settings_are_its_own() {
    let slow = || {
        crate::Methods::reads(get(|| async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            "done"
        }))
    };
    let outer_shape = |status: StatusCode, message: &str| ShapedError {
        r#type: "outer",
        error: ShapedDetail { code: status.as_u16(), message: message.to_string() },
    };
    let inner_shape = |status: StatusCode, message: &str| ShapedError {
        r#type: "inner",
        error: ShapedDetail { code: status.as_u16(), message: message.to_string() },
    };
    let page = Router::new().route("/{*rest}", get(|| async { "page" }));
    let router = App::new(Routes::from(page))
        .deadline(Duration::from_millis(50))
        .api::<Outer>(|outer| outer.route::<OuterAny>(crate::Methods::reads(get(|| async { "outer" }))).errors(outer_shape))
        .api::<OuterIn>(|inner| inner.route::<OuterInSlow>(slow()).errors(inner_shape).deadline(Duration::from_secs(10)))
        .api::<OuterInNone>(|none| none.route::<OuterInNoneSlow>(slow()).deadline(None))
        .area::<Walled>(|walled| walled.deadline(Duration::from_millis(20)))
        .api::<WalledApi>(|api| api.deadline(Duration::from_secs(10)))
        .api::<WalledApiIn>(|inner| inner.route::<WalledApiInSlow>(slow()))
        .router();
    let call = |path: &'static str| {
        let router = router.clone();
        async move {
            let (status, _, body) = call_with(router, "GET", path, None, None).await;
            (status.as_u16(), body)
        }
    };
    assert_eq!(call("/outer/7").await, (200, "outer".to_string()), "the outer parameter route outside the inner prefixes");
    let inner_404 = (404, r#"{"type":"inner","error":{"code":404,"message":"Not Found"}}"#.to_string());
    assert_eq!(call("/outer/in").await, inner_404, "not the outer /outer/{{id}}: the inner API's 404, in its shape");
    assert_eq!(call("/outer/in/other").await, inner_404);
    assert_eq!(call("/elsewhere").await, (200, "page".to_string()), "the app's catch-all page outside the APIs");
    assert_eq!(call("/outer/in/a/b").await, inner_404, "not the app's catch-all page either");
    assert_eq!(call("/outer/in/slow").await, (200, "done".to_string()), "the inner API's longer deadline");
    assert_eq!(call("/outer/none/slow").await, (200, "done".to_string()), "an inner None removes the outer's");
    let (status, body) = call("/outer/none/x").await;
    assert_eq!(
        (status, &body[..]),
        (404, r#"{"type":"outer","error":{"code":404,"message":"Not Found"}}"#),
        "the shape it left unset"
    );
    assert_eq!(call("/walled/api/in/slow").await, (200, "done".to_string()), "the nearest API's deadline over the area's");
}

#[cfg(feature = "json")]
paths! {
    Apis = "/apis",
    ApisPing = "/apis/ping",
    WalledOwn = "/walled/own",
    WalledOwnSlow = "/walled/own/slow",
}

/// `/apis` is beside `/api`, not in it: it takes nothing from it and its routes are its own; and
/// an API in an area that sets a deadline, in no API, takes the area's (#539).
#[cfg(feature = "json")]
#[tokio::test]
async fn an_api_beside_another_takes_nothing_from_it() {
    let slow = || {
        crate::Methods::reads(get(|| async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            "done"
        }))
    };
    let shape = |status: StatusCode, message: &str| ShapedError {
        r#type: "error",
        error: ShapedDetail { code: status.as_u16(), message: message.to_string() },
    };
    let router = App::new(Routes::new())
        .api::<ApiRoot>(|api| api.route::<ApiRest>(crate::Methods::reads(get(|| async { "api" }))).errors(shape))
        .api::<Apis>(|apis| apis.route::<ApisPing>(crate::Methods::reads(get(|| async { "apis" }))))
        .area::<Walled>(|walled| walled.deadline(Duration::from_millis(20)))
        .api::<WalledOwn>(|own| own.route::<WalledOwnSlow>(slow()))
        .router();
    let call = |path: &'static str| {
        let router = router.clone();
        async move {
            let (status, _, body) = call_with(router, "GET", path, None, None).await;
            (status.as_u16(), body)
        }
    };
    assert_eq!(call("/apis/ping").await, (200, "apis".to_string()));
    let (status, body) = call("/apis/nope").await;
    assert_eq!(status, 404);
    assert!(body.contains(r#""title":"Not Found""#), "its own Problem Details, not /api's shape: {body}");
    assert_eq!(call("/api/x").await, (200, "api".to_string()));
    assert_eq!(call("/walled/own/slow").await.0, 503, "the area's deadline");
}

/// Under a nested API, a WebSocket handshake from another site is held to that API's origins,
/// not the outer one's.
#[cfg(feature = "json")]
#[tokio::test]
async fn an_upgrade_under_a_nested_api_takes_its_origins() {
    use crate::upgrade::Upgrade;
    async fn nothing(_: crate::upgrade::Stream, _: crate::Signal) {}
    let router = App::new(Routes::new())
        .api::<ApiRoot>(|api| api.allow_origin("https://outer.example"))
        .api::<ApiItems>(|items| {
            items
                .route::<ApiItem>(crate::Methods::reads(get(|upgrade: Upgrade| async move { upgrade.websocket(nothing) })))
                .allow_origin("https://inner.example")
        })
        .router();
    let status = |origin: &'static str| {
        let router = router.clone();
        async move {
            let req = Request::get("/api/items/7")
                .header("origin", origin)
                .header("sec-fetch-site", "cross-site")
                .header("connection", "upgrade")
                .header("upgrade", "websocket")
                .header("sec-websocket-version", "13")
                .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
                .body(Body::empty())
                .unwrap();
            router.oneshot(req).await.unwrap().status()
        }
    };
    assert_eq!(status("https://inner.example").await, StatusCode::UPGRADE_REQUIRED, "allowed: a oneshot cannot switch");
    assert_eq!(status("https://outer.example").await, StatusCode::FORBIDDEN, "the outer API's origin is not the inner's");
    // An inner API that allows no site: the outer's partner may not switch under it either.
    let router = App::new(Routes::new())
        .api::<ApiRoot>(|api| api.allow_origin("https://outer.example"))
        .api::<ApiItems>(|items| {
            items.route::<ApiItem>(crate::Methods::reads(get(|upgrade: Upgrade| async move { upgrade.websocket(nothing) })))
        })
        .router();
    let req = Request::get("/api/items/7")
        .header("origin", "https://outer.example")
        .header("sec-fetch-site", "cross-site")
        .header("connection", "upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .body(Body::empty())
        .unwrap();
    assert_eq!(router.oneshot(req).await.unwrap().status(), StatusCode::FORBIDDEN);
}

/// A route an outer API writes under an inner API's prefix would answer with the inner one's
/// settings: the app does not start.
#[test]
#[should_panic(expected = "move it to that API")]
fn an_outer_apis_route_under_an_inner_prefix_does_not_start() {
    let reads = || crate::Methods::reads(get(|| async { "x" }));
    let _ = App::new(Routes::new()).api::<ApiRoot>(|api| api.route::<ApiItem>(reads())).api::<ApiItems>(|items| items).router();
}

/// Three deep, the message names the innermost API that holds the route.
#[cfg(feature = "json")]
#[test]
#[should_panic(expected = r#"is under App::api("/zone/api/mid/in")"#)]
fn the_route_is_named_with_the_innermost_api_that_holds_it() {
    let reads = || crate::Methods::reads(get(|| async { "x" }));
    let _ = App::new(Routes::new())
        .api::<ZoneApi>(|api| api.route::<ZoneInPing>(reads()))
        .api::<ZoneMid>(|mid| mid)
        .api::<ZoneIn>(|inner| inner)
        .router();
}

#[cfg(feature = "json")]
#[derive(crate::Route)]
#[route("/api/{*rest}")]
struct ApiRest<'a> {
    rest: &'a str,
}

/// An API inside another (a feature's `/api/items` in the app's `/api`, which relays the rest)
/// keeps its own routes, takes the settings it leaves unset from the outer one, and meets the
/// outer one's middleware first; a request under its prefix is its own, whichever route answers
/// it (#539).
#[cfg(feature = "json")]
#[tokio::test]
async fn an_api_nests_in_another() {
    let steps = || {
        crate::Methods::reads(get(|headers: axum::http::HeaderMap| async move {
            tokio::time::sleep(Duration::from_millis(if headers.contains_key("x-slow") { 200 } else { 0 })).await;
            headers.get("x-steps").map_or(String::new(), |v| v.to_str().unwrap().to_string())
        }))
    };
    let shape = |status: StatusCode, message: &str| ShapedError {
        r#type: "error",
        error: ShapedDetail { code: status.as_u16(), message: message.to_string() },
    };
    // Given inner first: the order they are given in does not matter.
    let router = App::new(Routes::new())
        .middleware(Step("app", false))
        .api::<ApiItems>(|items| items.route::<ApiItem>(steps()).middleware(Step("items", true)).middleware(Saw))
        .api::<ApiRoot>(|api| {
            api.route::<ApiRest>(steps())
                .route::<ApiSlow>(steps())
                .middleware(Step("api", false))
                .deadline(Duration::from_millis(50))
                .errors(shape)
                .allow_origin("https://partner.example")
        })
        .router();
    let get_with = |path: &'static str, header: Option<(&'static str, &'static str)>| {
        let router = router.clone();
        async move {
            let mut req = Request::get(path);
            if let Some((name, value)) = header {
                req = req.header(name, value);
            }
            let (parts, body) = router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap().into_parts();
            (parts.status, parts.headers, String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap())
        }
    };
    assert_eq!(get_with("/api/items/7", None).await.2, "app api items", "the outer API's middleware, then the inner's");
    assert_eq!(get_with("/api/slow", None).await.2, "app api", "the outer API's own routes meet its own alone");
    assert_eq!(get_with("/api/other/thing", None).await.2, "app api", "and its catch-all, outside the inner prefix");
    for path in ["/api/items/7/more", "/api/items"] {
        let (status, _, body) = get_with(path, None).await;
        assert_eq!(
            (status.as_u16(), &body[..]),
            (404, r#"{"type":"error","error":{"code":404,"message":"Not Found"}}"#),
            "{path}: a path under the inner prefix is the inner API's, never the outer catch-all's"
        );
        let (status, _, _) = get_with(path, Some(("x-flood", "1"))).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{path}: and meets the inner API's middleware first");
    }
    assert_eq!(get_with("/api/items/7", Some(("x-flood", "1"))).await.0, StatusCode::TOO_MANY_REQUESTS);
    // Every step runs for it, the app's first, and none is told a route matched it.
    let (_, headers, _) = get_with("/api/items/7/more", None).await;
    assert_eq!(
        (&headers["x-saw-steps"], headers.get("x-saw-route")),
        (&"app api items".parse::<axum::http::HeaderValue>().unwrap(), None)
    );
    let (_, headers, _) = get_with("/api/items/7", None).await;
    assert_eq!(headers.get("x-saw-route").map(|v| v.to_str().unwrap()), Some("/api/items/{id}"), "a route of its own is named");
    // A method the outer catch-all lacks (a 405 of the outer route's, before) is a 404 too, and
    // whatever answers it says no method is allowed: the inner middleware's 429 as well.
    let (status, headers, _) = call_with_header(router.clone(), "POST", "/api/items/7/more", ("x-flood", "1")).await;
    assert_eq!((status, headers.get("allow").map(|v| v.to_str().unwrap())), (StatusCode::TOO_MANY_REQUESTS, Some("")));
    // A preflight there is the inner API's, which allows no other site: the outer's partner is
    // not let in under the inner prefix, nor on the inner API's routes.
    for path in ["/api/items/7/more", "/api/items/7"] {
        let (_, headers, _) = call_with_headers(
            router.clone(),
            "OPTIONS",
            path,
            &[("origin", "https://partner.example"), ("access-control-request-method", "GET")],
        )
        .await;
        assert!(!headers.contains_key("access-control-allow-origin"), "{path}");
    }
    let (_, headers, _) = call_with_headers(
        router.clone(),
        "OPTIONS",
        "/api/other",
        &[("origin", "https://partner.example"), ("access-control-request-method", "GET")],
    )
    .await;
    assert_eq!(
        headers.get("access-control-allow-origin").map(|v| v.to_str().unwrap()),
        Some("https://partner.example"),
        "the outer API's own"
    );
    // And a HEAD: the inner API's 404, with no method allowed.
    for method in ["POST", "HEAD"] {
        let (status, headers, _) = call_with(router.clone(), method, "/api/items/7/more", None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method}");
        assert_eq!(headers.get("allow").map(|v| v.to_str().unwrap()), Some(""), "{method}: not the outer route's methods");
    }
    let (status, headers, body) = get_with("/api/items/7", Some(("x-slow", "1"))).await;
    assert_eq!(
        (status, &body[..]),
        (StatusCode::SERVICE_UNAVAILABLE, r#"{"type":"error","error":{"code":503,"message":"Service Unavailable"}}"#),
        "the outer API's deadline and error shape"
    );
    assert_eq!(headers["content-type"], "application/json");
    let (status, headers, _) = get_with("/api/items/7", Some(("origin", "https://partner.example"))).await;
    assert_eq!((status, headers.get("access-control-allow-origin")), (StatusCode::OK, None), "no partner of the outer's");
}

/// A layout's read started ahead by an extractor (`crate::Ahead`): under a path ending `/panics`
/// it panics, and `AHEAD_FINISHED` counts the reads that ran to their end.
struct StartedAhead(crate::Ahead<u32>);

static AHEAD_FINISHED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl<S: Sync> axum::extract::FromRequestParts<S> for StartedAhead {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, _: &S) -> Result<Self, Self::Rejection> {
        let panics = parts.uri.path().ends_with("/panics");
        Ok(StartedAhead(crate::Ahead::start(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if panics {
                panic!("the cart store");
            }
            AHEAD_FINISHED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            3
        })))
    }
}

/// A read started ahead fails the request where it is awaited, as the handler's own panic does,
/// and is stopped with the handler when the deadline passes.
#[tokio::test]
async fn a_read_started_ahead_is_the_requests() {
    let awaited = || {
        crate::Methods::reads(get(|ahead: StartedAhead| async move {
            ahead.0.await;
            "drawn"
        }))
    };
    let app = |deadline: Duration| {
        App::new(Routes::from(Router::new().route("/ahead/panics", awaited().0).route("/ahead/slow", awaited().0)))
            .error_page(test_error_page, ())
            .deadline(deadline)
            .router()
    };
    let (status, _, body) = call_with(app(Duration::from_secs(5)), "GET", "/ahead/panics", None, Some("text/html")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let before = AHEAD_FINISHED.load(std::sync::atomic::Ordering::SeqCst);
    let (status, _, _) = call_with(app(Duration::from_millis(50)), "GET", "/ahead/slow", None, Some("text/html")).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(AHEAD_FINISHED.load(std::sync::atomic::Ordering::SeqCst), before, "the deadline stopped the read");
}

/// Which routes outside an API can match a path under it, so only those APIs check per request.
#[test]
fn a_route_reaches_an_api_through_a_parameter_before_it() {
    use crate::api::reaches;
    for route in ["/{*rest}", "/{id}", "/api/{id}", "/api/{*rest}"] {
        assert!(reaches(route, "/api/items"), "{route}");
    }
    for route in ["/page", "/api/ping", "/v2/items/{id}", "/apis/{id}", "/api/items/{id}", "/api/items"] {
        assert!(!reaches(route, "/api/items"), "{route}");
    }
    assert!(!reaches("/{*rest}", "/"), "nothing is outside the root");
}

/// A top-level API owns its prefix too: a page's catch-all does not answer a path under it (#539).
#[cfg(feature = "json")]
#[tokio::test]
async fn a_page_catch_all_does_not_answer_under_an_api() {
    let everything = Routes::new().__route::<EverythingRest>(crate::Methods::reads(get(|| async { "page" })));
    let router = App::new(everything)
        .api::<ApiItems>(|items| items.route::<ApiItem>(crate::Methods::reads(get(|| async { "item" }))))
        .router();
    assert_eq!(call_with(router.clone(), "GET", "/elsewhere", None, None).await.2, "page");
    let (status, headers, body) = call_with(router.clone(), "GET", "/api/items/7/more", None, None).await;
    assert_eq!(
        (status, &headers["content-type"]),
        (StatusCode::NOT_FOUND, &"application/problem+json".parse::<axum::http::HeaderValue>().unwrap()),
        "{body}"
    );
    assert_eq!(call_with(router, "GET", "/api/items/7", None, None).await.2, "item");
}

/// Served under another router's `Router::nest`, an API's routes are still its own, and a path
/// outside them still gets its 404.
#[cfg(feature = "json")]
#[tokio::test]
async fn an_api_answers_under_router_nest() {
    let everything = Routes::new().__route::<EverythingRest>(crate::Methods::reads(get(|| async { "page" })));
    let app = App::new(everything)
        .api::<ApiRoot>(|api| api.route::<ApiRest>(crate::Methods::reads(get(|| async { "rest" }))))
        .api::<ApiItems>(|items| items.route::<ApiItem>(crate::Methods::reads(get(|| async { "item" }))))
        .router();
    let router = Router::new().nest("/outer", app);
    assert_eq!(call_with(router.clone(), "GET", "/outer/api/items/7", None, None).await.2, "item");
    assert_eq!(call_with(router.clone(), "GET", "/outer/api/other", None, None).await.2, "rest");
    assert_eq!(call_with(router, "GET", "/outer/api/items/7/more", None, None).await.0, StatusCode::NOT_FOUND);
    // Nested at a path that is also an API's prefix: the nest's part is taken off before the
    // matched route is compared, so a catch-all outside the API still does not answer under it.
    let app = App::new(Routes::new())
        .api::<Everything>(|root| root.route::<EverythingRest>(crate::Methods::reads(get(|| async { "root" }))))
        .api::<ApiRoot>(|api| api)
        .router();
    let router = Router::new().nest("/api", app);
    assert_eq!(call_with(router.clone(), "GET", "/api/elsewhere", None, None).await.2, "root");
    assert_eq!(call_with(router, "GET", "/api/api/other", None, None).await.0, StatusCode::NOT_FOUND);
}

/// Copies the steps the request met and the route axum matched (`MatchedPath`) onto the answer.
struct Saw;

impl crate::Middleware for Saw {
    async fn call(&self, req: Request<Body>, next: impl crate::Next) -> axum::response::Response {
        let steps = req.headers().get("x-steps").cloned();
        let route =
            req.extensions().get::<axum::extract::MatchedPath>().map(|m| m.as_str().parse::<axum::http::HeaderValue>().unwrap());
        let mut res = next.run(req).await;
        if let Some(steps) = steps {
            res.headers_mut().insert("x-saw-steps", steps);
        }
        if let Some(route) = route {
            res.headers_mut().insert("x-saw-route", route);
        }
        res
    }
}

/// Writes its name into `x-steps` on the way in, so a test reads the order middleware ran in;
/// with `refuse`, a request with `x-flood` is turned away with a 429.
struct Step(&'static str, bool);

impl crate::Middleware for Step {
    async fn call(&self, mut req: Request<Body>, next: impl crate::Next) -> axum::response::Response {
        use axum::response::IntoResponse;
        let steps = req.headers().get("x-steps").map(|v| format!("{} {}", v.to_str().unwrap(), self.0));
        let steps = steps.unwrap_or_else(|| self.0.to_string());
        req.headers_mut().insert("x-steps", steps.parse().unwrap());
        if self.1 && req.headers().contains_key("x-flood") {
            return crate::Refused::too_many(Duration::from_millis(7_500)).into_response();
        }
        next.run(req).await
    }
}

/// An area's or an API's middleware runs for its own routes only (and a nested area's), after
/// the app's, outermost first and each in the order given; a refusal from one is answered in the
/// request's mode, as a caller's is.
#[tokio::test]
async fn an_area_or_an_api_has_middleware_of_its_own() {
    let steps = || {
        crate::Methods::reads(get(|headers: axum::http::HeaderMap| async move {
            headers.get("x-steps").map_or(String::new(), |v| v.to_str().unwrap().to_string())
        }))
    };
    let router = App::new(Routes::from(Router::new().route("/slow", steps().0)))
        .error_page(test_error_page, ())
        .middleware(Step("app", false))
        .area::<AdminQuick>(|quick| quick.route::<AdminQuickSlow>(steps()).middleware(Step("quick", false)))
        .area::<Admin>(|admin| {
            admin
                .middleware((Step("admin", false), Step("limit", true)))
                .route::<AdminSlow>(steps())
                .middleware(Step("last", false))
        })
        .area::<AdminOther>(|other| other.route::<AdminOtherSlow>(steps()))
        .api::<ApiRoot>(|api| api.route::<ApiSlow>(steps()).middleware(Step("api", true)))
        .api::<AdminHooks>(|hooks| hooks.route::<AdminHooksPing>(steps()).middleware(Step("hooks", false)))
        .router();
    let body = |uri| {
        let router = router.clone();
        async move { call_with(router, "GET", uri, None, Some("text/html")).await.2 }
    };
    assert_eq!(body("/slow").await, "app", "a route outside every area meets the app's only");
    assert_eq!(
        body("/admin/slow").await,
        "app admin limit last",
        "the area's, in the order given, given before or after its routes"
    );
    assert_eq!(body("/admin/quick/slow").await, "app admin limit last quick", "an outer area's, then the inner one's");
    assert_eq!(body("/admin/other/slow").await, "app admin limit last", "a nested area without any has its parent's");
    assert_eq!(body("/api/slow").await, "app api");
    assert_eq!(body("/admin/hooks/ping").await, "app admin limit last hooks", "an API under the area meets the area's first");
    assert!(body("/admin/nope").await.contains("Not found"), "an unknown URL under the area is the app's 404");

    let flood = |uri: &'static str, accept: &'static str| {
        let router = router.clone();
        async move {
            let req = Request::get(uri).header("accept", accept).header("x-flood", "1").body(Body::empty()).unwrap();
            let (parts, body) = router.oneshot(req).await.unwrap().into_parts();
            (parts.status, parts.headers, String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap())
        }
    };
    let (status, headers, body) = flood("/admin/slow", "text/html").await;
    assert_eq!(
        (status, &headers["retry-after"]),
        (StatusCode::TOO_MANY_REQUESTS, &"8".parse::<axum::http::HeaderValue>().unwrap())
    );
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"), "a browser gets the error page: {headers:?}");
    let (status, headers, body2) = flood("/api/slow", "text/html").await;
    assert_eq!(
        (status, &headers["retry-after"]),
        (StatusCode::TOO_MANY_REQUESTS, &"8".parse::<axum::http::HeaderValue>().unwrap())
    );
    assert_eq!(body2, r#"{"title":"Too Many Requests","status":429}"#, "JSON under an API");
    let (status, _, _) = flood("/admin/nope", "text/html").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "an unknown URL under the area meets its middleware");
    let (status, _, body3) = flood("/admin/hooks/ping", "text/html").await;
    assert_eq!(
        (status, &body3[..]),
        (StatusCode::TOO_MANY_REQUESTS, r#"{"title":"Too Many Requests","status":429}"#),
        "and an API under it, in JSON"
    );
    let (status, _, _) = flood("/slow", "text/html").await;
    assert_eq!(status, StatusCode::OK, "the app's other routes never meet the limit");
    assert!(!body.is_empty());
}

/// A middleware reads the client's address as a handler would, and the answer is then private.
#[tokio::test]
async fn a_middleware_reads_the_client_address() {
    struct Seen;
    impl crate::Middleware for Seen {
        async fn call(&self, mut req: Request<Body>, next: impl crate::Next) -> axum::response::Response {
            let ip = crate::ClientIp::of(&mut req).map_or("none".to_string(), |crate::ClientIp(ip)| ip.to_string());
            req.headers_mut().insert("x-ip", ip.parse().unwrap());
            next.run(req).await
        }
    }
    let routes = Router::new().route(
        "/",
        get(|headers: axum::http::HeaderMap, ip: Option<crate::ClientIp>| async move {
            format!(
                "{} {}",
                headers["x-ip"].to_str().unwrap(),
                ip.map_or("none".to_string(), |crate::ClientIp(ip)| ip.to_string())
            )
        }),
    );
    let router = App::new(Routes::from(routes))
        .middleware(Seen)
        .router()
        .layer(axum::extract::connect_info::MockConnectInfo(std::net::SocketAddr::from(([203, 0, 113, 8], 1))));
    let (status, headers, body) = call_with(router, "GET", "/", None, Some("text/html")).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "203.0.113.8 203.0.113.8"), "the same address both ways");
    assert_eq!(headers["cache-control"], "private, no-cache", "an answer for one address is private");
}

/// What most HTTP clients send: `*/*`, or no `Accept` at all, or a browser's own.
const ACCEPTS: [Option<&str>; 4] = [Some("*/*"), None, Some("text/html,application/xhtml+xml"), Some("application/json")];

#[tokio::test]
async fn api_routes_answer_every_failure_as_json_whatever_the_client_accepts() {
    let router = api_app(|api| api);
    for accept in ACCEPTS {
        let get = |uri: &'static str| call_with(router.clone(), "GET", uri, None, accept);
        let (status, headers, body) = get("/api/nope").await;
        assert_eq!((status, &body[..]), (StatusCode::NOT_FOUND, r#"{"title":"Not Found","status":404}"#), "{accept:?}");
        assert_eq!(headers["content-type"], "application/problem+json");
        assert!(!headers.contains_key("corten-page") && !headers.contains_key("corten-shell"), "no shell: {headers:?}");
        let (_, _, body) = get("/api/missing").await;
        assert_eq!(body, r#"{"title":"Not Found","status":404,"detail":"no item 7"}"#, "a 4xx error's message is for the client");
        let (status, _, body) = get("/api/down").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, r#"{"title":"Internal Server Error","status":500}"#, "a 5xx message stays in the log");
        let (status, _, body) = get("/api/items/abc").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            body.starts_with(r#"{"title":"Bad Request","status":400,"detail":"Invalid URL"#),
            "an extractor's rejection: {body}"
        );
        let (status, headers, body) = call_with(router.clone(), "DELETE", "/api/post", None, accept).await;
        assert_eq!((status, &headers["allow"]), (StatusCode::METHOD_NOT_ALLOWED, &"POST".parse().unwrap()));
        assert_eq!(body, r#"{"title":"Method Not Allowed","status":405}"#);
        let (status, _, body) = get("/api/slow").await;
        assert_eq!((status, &body[..]), (StatusCode::SERVICE_UNAVAILABLE, r#"{"title":"Service Unavailable","status":503}"#));
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

#[cfg(feature = "json")]
#[derive(crate::ToJson)]
struct ShapedError {
    r#type: &'static str,
    error: ShapedDetail,
}

#[cfg(feature = "json")]
#[derive(crate::ToJson)]
struct ShapedDetail {
    code: u16,
    message: String,
}

/// `Api::errors` writes every failure Corten answers under the prefix in the API's own shape.
#[cfg(feature = "json")]
#[tokio::test]
async fn an_api_writes_the_failures_corten_answers_in_its_own_shape() {
    let router = api_app(|api| {
        api.errors(|status, message| ShapedError {
            r#type: "error",
            error: ShapedDetail { code: status.as_u16(), message: message.to_string() },
        })
    });
    let shaped = |code: u16, message: &str| format!(r#"{{"type":"error","error":{{"code":{code},"message":{message:?}}}}}"#);
    for (method, path, status, message) in [
        ("GET", "/api/nope", 404, "Not Found"),
        ("GET", "/api/post", 405, "Method Not Allowed"),
        ("GET", "/api/missing", 404, "no item 7"),
        ("GET", "/api/down", 500, "Internal Server Error"),
        ("GET", "/api/items/x", 400, "Invalid URL: Cannot parse `x` to a `u32`"),
        ("GET", "/api/slow", 503, "Service Unavailable"),
    ] {
        let (got, headers, body) = call_with(router.clone(), method, path, None, None).await;
        assert_eq!((got.as_u16(), &body[..]), (status, &shaped(status, message)[..]), "{method} {path}");
        assert_eq!(headers["content-type"], "application/json");
    }
    let (_, headers, _) = call_with(router.clone(), "GET", "/api/post", None, None).await;
    assert_eq!(headers["allow"], "POST", "what the failure says besides its body stays");
    let cross = Request::post("/api/post").header("origin", "https://evil.example").header("sec-fetch-site", "cross-site");
    let res = router.clone().oneshot(cross.body(Body::empty()).unwrap()).await.unwrap();
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body, shaped(403, "Cross-site request refused."));
    let (_, _, body) = call_with(router, "GET", "/api/teapot", None, None).await;
    assert_eq!(body, r#"{"mine":1}"#, "a handler's own JSON failure passes through");
}

/// A key check that says why it refused, and locks out a client with too many failed attempts.
#[derive(Clone, Hash, Debug)]
struct Keyed;

impl<S: Sync> crate::Identify<S> for Keyed {
    async fn identify(parts: &axum::http::request::Parts, _: &S) -> std::result::Result<Self, crate::Refused> {
        match parts.headers.get("x-api-key").map(|v| v.as_bytes()) {
            Some(b"k1") => Ok(Keyed),
            Some(b"locked") => Err(crate::Refused::too_many(std::time::Duration::from_millis(1500))
                .message("Too many failed attempts; try again later")),
            _ => Err(crate::Refused::unauthorized().message("Invalid API key")),
        }
    }
}

#[derive(crate::Route)]
#[route("/api/keyed")]
struct ApiKeyed;

/// A refusal says why, in the API's own shape, and a lockout is a 429 that says when to try again.
#[cfg(feature = "json")]
#[tokio::test]
async fn a_refusal_tells_the_client_why_and_when_to_try_again() {
    let router = api_app(|api| {
        api.route::<ApiKeyed>(crate::Methods::reads(get(|_: Caller<Keyed>| async { "ok" }))).errors(|status, message| {
            ShapedError { r#type: "error", error: ShapedDetail { code: status.as_u16(), message: message.to_string() } }
        })
    });
    let keyed = |key: Option<&'static str>| {
        let mut req = Request::get("/api/keyed");
        if let Some(key) = key {
            req = req.header("x-api-key", key);
        }
        let router = router.clone();
        async move {
            let res = router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
            let (status, retry) = (res.status().as_u16(), res.headers().get("retry-after").cloned());
            (status, retry, String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
        }
    };
    assert_eq!(keyed(Some("k1")).await, (200, None, "ok".to_string()));
    assert_eq!(
        keyed(None).await,
        (401, None, r#"{"type":"error","error":{"code":401,"message":"Invalid API key"}}"#.to_string())
    );
    let (status, retry, body) = keyed(Some("locked")).await;
    assert_eq!((status, retry.as_ref().map(|v| v.to_str().unwrap())), (429, Some("2")), "whole seconds, rounded up");
    assert_eq!(body, r#"{"type":"error","error":{"code":429,"message":"Too many failed attempts; try again later"}}"#);
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
    assert_eq!(body, r#"{"title":"Forbidden","status":403,"detail":"Cross-site request refused."}"#);
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
    assert_eq!(res.headers()["content-type"], "application/problem+json");
}

#[tokio::test]
async fn an_api_at_the_root_makes_the_whole_app_json() {
    let router =
        App::new(Routes::new()).api::<Root>(|api| api.route::<Items>(crate::Methods::reads(get(|| async { "items" })))).router();
    let (status, headers, body) = call_with(router.clone(), "GET", "/nope", None, Some("text/html")).await;
    assert_eq!((status, headers["content-type"].to_str().unwrap()), (StatusCode::NOT_FOUND, "application/problem+json"));
    assert_eq!(body, r#"{"title":"Not Found","status":404}"#);
    let (status, _, body) = call_with(router, "GET", "/health/ready", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "ok"), "Corten's own routes keep their behavior");
}

/// Query parameters a route carries as a set (`#[route(query)]`).
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, crate::Route)]
#[route(query)]
pub struct ParamsPaging {
    pub page: Option<u32>,
}

/// A set with each kind of parameter a route has: renamed, a list with no `#[serde(default)]`,
/// a prefix and a set of its own. It derives no `Deserialize`: `Params` reads it by its keys.
#[derive(Clone, Debug, Default, PartialEq, crate::Route)]
#[route(query)]
#[serde(rename_all = "camelCase")]
pub struct SetProbe {
    pub sort_by: Option<String>,
    pub tags: Vec<String>,
    #[route(prefix = "dimension.")]
    pub dims: Vec<(String, String)>,
    #[route(query)]
    pub paging: ParamsPaging,
}

#[derive(crate::Route)]
#[route("/probe")]
pub struct SetRoute {
    pub q: Option<String>,
    #[route(query)]
    pub set: SetProbe,
}

/// A set's fields with defaults of their own: a parameter the query leaves out is the field's.
#[derive(Debug, PartialEq, crate::Route)]
#[route(query)]
pub struct DefaultedSet {
    #[serde(default = "first_page")]
    pub page: u32,
    #[serde(default)]
    pub size: u32,
    #[route(prefix = "f.")]
    pub facets: std::collections::BTreeMap<String, String>,
}

fn first_page() -> u32 {
    1
}

/// A set with a type parameter.
#[derive(Debug, PartialEq, crate::Route)]
#[route(query)]
pub struct PickSet<T: std::fmt::Display> {
    pub pick: Option<T>,
}

#[derive(crate::Route)]
#[route("/defaulted")]
pub struct DefaultedRoute {
    #[route(query)]
    pub set: DefaultedSet,
    #[route(query)]
    pub pick: PickSet<u32>,
}

/// A set only links use: its field needs only `Display`.
#[derive(crate::Route)]
#[route(query)]
pub struct ShownSet {
    pub tab: Option<Shown>,
}

#[derive(crate::Route)]
#[route("/shown-set")]
struct ShownSetLink {
    #[route(query)]
    set: ShownSet,
}

#[test]
fn a_query_set_reads_back_what_its_links_write() {
    use crate::route::FromUrl;
    let set = SetProbe {
        sort_by: Some("price".into()),
        tags: vec!["a".into(), "b&c".into()],
        dims: vec![("form".into(), "graded".into()), ("grade".into(), "PSA 8".into())],
        paging: ParamsPaging { page: Some(3) },
    };
    let href = SetRoute { q: Some("x".into()), set: set.clone() }.href();
    assert_eq!(href, "/probe?q=x&sortBy=price&tags=a&tags=b%26c&dimension.form=graded&dimension.grade=PSA%208&page=3");
    let read = <SetRoute as FromUrl>::__read((), href.split_once('?').unwrap().1).unwrap();
    assert_eq!((read.q.as_deref(), &read.set), (Some("x"), &set), "the prefix field is read back, not left empty");
    let read = <SetRoute as FromUrl>::__read((), "").unwrap();
    assert_eq!(read.set, SetProbe::default(), "a missing list is empty, as on a route");
    for query in ["page=x", "sortBy=a&sortBy=b"] {
        assert!(<SetRoute as FromUrl>::__read((), query).is_err(), "{query}");
    }
    let read = <DefaultedRoute as FromUrl>::__read((), "size=5&f.kind=graded&pick=7").unwrap();
    let facets = std::collections::BTreeMap::from([("kind".to_string(), "graded".to_string())]);
    assert_eq!((&read.set, &read.pick), (&DefaultedSet { page: 1, size: 5, facets }, &PickSet { pick: Some(7) }));
    let empty = <DefaultedRoute as FromUrl>::__read((), "").unwrap().set;
    assert_eq!(empty, DefaultedSet { page: 1, size: 0, facets: Default::default() });
    let link = ShownSetLink { set: ShownSet { tab: Some(Shown("a b".into())) } };
    assert_eq!(link.href(), "/shown-set?tab=a%20b");
}

/// A set whose value is a rule over what was sent (`FromQuery`): a listing only when every one
/// sent agrees, and whether any option has a value.
#[derive(Debug, Default, PartialEq)]
pub struct Chosen {
    pub listing: Option<String>,
    pub options: Vec<(String, String)>,
}

impl Chosen {
    /// Any option has a value: a rule across keys.
    fn filtered(&self) -> bool {
        self.options.iter().any(|(_, v)| !v.is_empty())
    }
}

#[derive(crate::Route)]
#[route(query)]
pub struct ChosenLink<'a> {
    pub listing: Option<&'a str>,
    #[route(prefix = "dimension.")]
    pub options: &'a [(String, String)],
}

#[derive(crate::Route)]
#[route(query)]
pub struct ChosenRead {
    pub listing: Vec<String>,
    #[route(prefix = "dimension.")]
    pub options: Vec<(String, String)>,
}

impl crate::route::FromQuery for Chosen {
    type Link<'a> = ChosenLink<'a>;
    type Read = ChosenRead;

    fn link(&self) -> ChosenLink<'_> {
        ChosenLink { listing: self.listing.as_deref(), options: &self.options }
    }

    fn from_query(read: ChosenRead) -> Result<Self, crate::form::FormError> {
        let mut listings = read.listing.into_iter().filter(|l| !l.is_empty());
        let first = listings.next();
        if first.as_deref() == Some("refuse") {
            return Err(serde::de::Error::custom("listing: refused"));
        }
        let listing = if listings.all(|l| Some(&l) == first.as_ref()) { first } else { None };
        Ok(Chosen { listing, options: read.options })
    }
}

#[derive(crate::Route)]
#[route("/chosen")]
pub struct ChosenRoute {
    pub q: Option<String>,
    #[route(query)]
    pub chosen: Chosen,
}

#[test]
fn a_query_set_can_be_a_rule_over_what_was_sent() {
    use crate::route::FromUrl;
    let chosen = Chosen { listing: Some("l 1".into()), options: vec![("form".into(), "graded".into())] };
    let href = ChosenRoute { q: Some("x".into()), chosen }.href();
    assert_eq!(href, "/chosen?q=x&listing=l%201&dimension.form=graded");
    let read = <ChosenRoute as FromUrl>::__read((), href.split_once('?').unwrap().1).unwrap();
    assert_eq!(
        (read.q.as_deref(), &read.chosen),
        (Some("x"), &Chosen { listing: Some("l 1".into()), options: vec![("form".into(), "graded".into())] }),
        "a link reads back as itself"
    );
    assert!(read.chosen.filtered());
    let read = <ChosenRoute as FromUrl>::__read((), "listing=l1&listing=l1&zz=1").unwrap();
    assert_eq!(read.chosen.listing.as_deref(), Some("l1"), "a value sent twice is the set's to judge");
    let read = <ChosenRoute as FromUrl>::__read((), "listing=l1&listing=l2&dimension.form=").unwrap();
    assert_eq!((read.chosen.listing.as_deref(), read.chosen.filtered()), (None, false), "two listings are no choice");
    let err = <ChosenRoute as FromUrl>::__read((), "listing=refuse").err().expect("the set's failure refuses the page");
    assert!(err.to_string().contains("refused"), "{err}");
    assert!(<ChosenRoute as FromUrl>::__read((), "q=a&q=b").is_err(), "the route's own parameters are read as before");
}

/// A `FromQuery` set whose `Read` names its fields in another order than its `Link`, both
/// including a derived set: the names, not their order, must match.
#[derive(Debug, PartialEq)]
pub struct Reordered {
    pub listing: Option<String>,
    pub page: Option<u32>,
}

#[derive(crate::Route)]
#[route(query)]
pub struct ReorderedLink<'a> {
    pub listing: Option<&'a str>,
    #[route(query)]
    pub paging: ParamsPaging,
}

#[derive(crate::Route)]
#[route(query)]
pub struct ReorderedRead {
    #[route(query)]
    pub paging: ParamsPaging,
    pub listing: Vec<String>,
}

impl crate::route::FromQuery for Reordered {
    type Link<'a> = ReorderedLink<'a>;
    type Read = ReorderedRead;

    fn link(&self) -> ReorderedLink<'_> {
        ReorderedLink { listing: self.listing.as_deref(), paging: ParamsPaging { page: self.page } }
    }

    fn from_query(read: ReorderedRead) -> Result<Self, crate::form::FormError> {
        Ok(Reordered { listing: read.listing.into_iter().next(), page: read.paging.page })
    }
}

#[derive(crate::Route)]
#[route("/reordered")]
pub struct ReorderedRoute {
    #[route(query)]
    pub set: Reordered,
}

#[test]
fn a_query_set_that_is_a_rule_matches_names_not_their_order() {
    use crate::route::FromUrl;
    let set = Reordered { listing: Some("l1".into()), page: Some(3) };
    let href = ReorderedRoute { set }.href();
    assert_eq!(href, "/reordered?listing=l1&page=3");
    let read = <ReorderedRoute as FromUrl>::__read((), href.split_once('?').unwrap().1).unwrap();
    assert_eq!(read.set, Reordered { listing: Some("l1".into()), page: Some(3) });
}

/// A `FromQuery` set inside a derived one, beside the outer set's own names.
#[derive(crate::Route)]
#[route(query)]
pub struct ChosenOuter {
    pub sort: Option<String>,
    #[route(query)]
    pub chosen: Chosen,
}

#[derive(crate::Route)]
#[route("/chosen-outer")]
pub struct ChosenOuterRoute {
    #[route(query)]
    pub outer: ChosenOuter,
}

#[test]
fn a_query_set_that_is_a_rule_reads_inside_another() {
    use crate::route::FromUrl;
    let read = <ChosenOuterRoute as FromUrl>::__read((), "sort=price&listing=l1&dimension.form=graded").unwrap();
    assert_eq!(read.outer.sort.as_deref(), Some("price"));
    assert_eq!(read.outer.chosen, Chosen { listing: Some("l1".into()), options: vec![("form".into(), "graded".into())] });
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, crate::UrlValue)]
#[serde(rename_all = "snake_case")]
pub enum Filter {
    #[default]
    All,
    Due,
}

#[derive(Debug, PartialEq, serde::Deserialize, crate::Route)]
#[route(query)]
pub struct FilterSet {
    pub state: Option<Filter>,
}

#[derive(crate::Route)]
#[route("/filtered")]
pub struct Filtered {
    pub sort: Option<Filter>,
    #[serde(default)]
    pub shown: Filter,
    #[route(query)]
    pub set: FilterSet,
}

#[test]
fn a_query_word_the_app_no_longer_has_reads_as_missing() {
    use crate::route::FromUrl;
    let read = <Filtered as FromUrl>::__read((), "sort=due_now&shown=gone&state=old").unwrap();
    assert_eq!((read.sort, read.shown, read.set.state), (None, Filter::All, None));
    let read = <Filtered as FromUrl>::__read((), "sort=due&shown=due&state=due").unwrap();
    assert_eq!((read.sort, read.shown, read.set.state), (Some(Filter::Due), Filter::Due, Some(Filter::Due)));
    assert!(<Filtered as FromUrl>::__read((), "sort=due&sort=all").is_err(), "sent twice is still refused");
}

#[derive(crate::Route)]
#[route("/faceted")]
pub struct Faceted {
    #[route(prefix = "f.")]
    pub facets: std::collections::BTreeMap<Filter, Filter>,
    #[route(prefix = "n.")]
    pub counts: Vec<(Filter, u32)>,
}

#[test]
fn a_prefixed_entry_the_app_no_longer_has_is_left_out() {
    use crate::route::FromUrl;
    let read = <Faceted as FromUrl>::__read((), "f.due=all&f.gone=due&f.all=gone&n.due=2&n.old=3").unwrap();
    assert_eq!(read.facets, std::collections::BTreeMap::from([(Filter::Due, Filter::All)]));
    assert_eq!(read.counts, vec![(Filter::Due, 2)]);
    let err = <Faceted as FromUrl>::__read((), "n.due=two").err().expect("a value that is not a word is refused");
    assert!(err.to_string().contains("n.due: "), "the error names the parameter: {err}");
}

/// Every kind of parameter a route has, which its handler reads back as the link wrote it.
#[crate::routes]
mod params_url {
    #[route("/flags/{on}.txt", get = params_flag)]
    pub struct Flag {
        pub on: bool,
    }

    #[route("/shops/{shop}/items/{id}", get = params_item)]
    #[serde(rename_all = "camelCase")]
    pub struct Item<'a> {
        pub shop: &'a str,
        pub id: u32,
        pub sort_by: Option<&'a str>,
        pub tags: &'a [&'a str],
        #[serde(default)]
        pub mine: bool,
        #[route(query)]
        pub paging: super::ParamsPaging,
        #[route(prefix = "dim.")]
        pub dims: &'a std::collections::BTreeMap<String, String>,
    }

    #[route("/files/{*rest}", get = params_file)]
    pub struct File<'a> {
        pub rest: std::borrow::Cow<'a, str>,
    }

    /// A parameter followed by text in its segment: a sitemap's pages.
    #[route("/sitemap/{kind}/{page}.xml", get = params_sitemap)]
    pub struct SitemapPage<'a> {
        pub kind: &'a str,
        pub page: u32,
        pub gz: Option<bool>,
    }
}

paths! { Feeds = "/feeds" }

/// A route of an area with the same, added by hand (no `Params` in its handler).
#[derive(crate::Route)]
#[route("/feeds/{name}.rss")]
struct Rss<'a> {
    name: &'a str,
}

/// A value a link prints but nothing reads back: neither `Clone`, `Send` nor `Deserialize`.
pub struct Shown(std::rc::Rc<str>);

impl std::fmt::Display for Shown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A route used only for links still needs only `Display` of its fields.
#[derive(crate::Route)]
#[route("/shown/{value}")]
#[serde(deny_unknown_fields)]
struct LinkOnly<'a> {
    value: &'a Shown,
    #[serde(alias = "t")]
    tab: Option<&'a Shown>,
}

/// The same fields with no serde key: its hidden owned twin is written, and asks nothing of
/// `Shown` until a handler reads the route's `Params`.
#[derive(crate::Route)]
#[route("/shown-twin/{value}")]
struct LinkTwin<'a> {
    value: &'a Shown,
    tab: Option<&'a Shown>,
}

/// The same with text after the parameter: still only `Display` for a link.
#[derive(crate::Route)]
#[route("/shown-file/{value}.xml")]
struct LinkFile<'a> {
    value: &'a Shown,
}

async fn params_item(item: crate::Params<params_url::Item<'_>>) -> String {
    let (shop, id, sort, tags, mine, page, dims) =
        (&item.shop, item.id, &item.sort_by, &item.tags, item.mine, item.paging.page, &item.dims);
    format!("{shop}|{id}|{sort:?}|{tags:?}|{mine}|{page:?}|{dims:?}")
}

async fn params_file(file: crate::Params<params_url::File<'_>>) -> String {
    file.rest.clone()
}

async fn params_sitemap(page: crate::Params<params_url::SitemapPage<'_>>) -> String {
    format!("{}|{}|{:?}", page.kind, page.page, page.gz)
}

#[tokio::test]
async fn a_parameter_followed_by_text_is_read_without_it() {
    use crate::route::Route as _;
    assert_eq!(<params_url::SitemapPage<'_> as crate::Route>::PATH, "/sitemap/{kind}/{page}", "axum matches the segment");
    let link = params_url::SitemapPage { kind: "items", page: 3, gz: None };
    assert_eq!(link.href(), "/sitemap/items/3.xml");
    let router = App::new(params_url::router()).router();
    let (status, _, body) = call_with(router.clone(), "GET", &link.href(), None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "items|3|None"));
    let (_, _, body) = call_with(router.clone(), "GET", "/sitemap/items/12.xml?gz=true", None, None).await;
    assert_eq!(body, "items|12|Some(true)", "with a query");
    // The text is the route's: without it, or with nothing before it, no route matches.
    for path in ["/sitemap/items/3", "/sitemap/items/3.json", "/sitemap/items/.xml", "/sitemap/items/3.xml.bak"] {
        for method in ["GET", "DELETE"] {
            let (status, _, _) = call_with(router.clone(), method, path, None, None).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{method} {path}");
        }
    }
    let (status, _, _) = call_with(router.clone(), "DELETE", "/sitemap/items/3.xml", None, None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "a path the route serves, by another method");
    // A value that is not the parameter's type is refused as any path parameter is.
    let (status, _, body) = call_with(router.clone(), "GET", "/sitemap/items/x.xml", None, Some("text/plain")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body.starts_with("Invalid URL: Cannot parse `x` to a `u32`"), "{body}");

    // A route added to an area by hand is checked the same, with no `Params` to do it.
    let rss = crate::Methods::reads(get(|| async { "rss" }));
    let area = App::new(Routes::new()).area::<Feeds>(|feeds| feeds.route::<Rss>(rss));
    let router = area.router();
    let (status, _, body) = call_with(router.clone(), "GET", &Rss { name: "news" }.href(), None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "rss"));
    let (status, _, _) = call_with(router, "GET", "/feeds/news.atom", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // And one of an API, whose failures are JSON.
    let json = crate::Methods::reads(get(|| async { "json" }));
    let router = App::new(Routes::new()).api::<ApiRoot>(|api| api.route::<ApiFile>(json)).router();
    let (status, _, body) = call_with(router.clone(), "GET", &ApiFile { name: "rates" }.href(), None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "json"));
    let (status, headers, _) = call_with(router, "GET", "/api/files/rates.csv", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(headers[header::CONTENT_TYPE], "application/problem+json", "an API's 404");
}

#[derive(crate::Route)]
#[route("/api/files/{name}.json")]
struct ApiFile<'a> {
    name: &'a str,
}

#[derive(crate::Route)]
#[route("/feeds/{name}.atom")]
struct Atom<'a> {
    name: &'a str,
}

/// Two routes that differ only in the text after a parameter, in different places (the app's
/// routes and an area): axum would match them as one path, so the app does not start.
#[test]
#[should_panic(expected = "differ only in the text after a parameter")]
fn routes_that_differ_only_in_the_text_after_a_parameter_do_not_start() {
    let rss = crate::Methods::reads(get(|| async { "rss" }));
    let atom = crate::Methods::reads(axum::routing::post(|| async { "atom" }));
    let _ = App::new(Routes::new().__route::<Rss>(rss)).area::<Feeds>(|feeds| feeds.route::<Atom>(atom)).router();
}

async fn params_flag(at: crate::Params<params_url::Flag>) -> String {
    at.on.to_string()
}

/// A value before text reads as a path parameter does: a `bool` is `true` or `false`, not a
/// checkbox's `on`.
#[tokio::test]
async fn a_value_before_text_reads_as_a_path_parameter() {
    let router = App::new(params_url::router()).router();
    let (status, _, body) = call_with(router.clone(), "GET", "/flags/true.txt", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "true"));
    let (status, _, _) = call_with(router, "GET", "/flags/on.txt", None, None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[derive(crate::Route)]
#[route("/feeds/{kind}.json")]
struct FeedJson<'a> {
    kind: &'a str,
}

#[derive(crate::Route)]
#[route("/api/files/{name}.csv")]
struct ApiCsv<'a> {
    name: &'a str,
}

/// Two routes of one API that differ only in the text after a parameter.
#[test]
#[should_panic(expected = "differ only in the text after a parameter")]
fn routes_of_an_api_that_differ_only_in_the_text_after_a_parameter_do_not_start() {
    let json = crate::Methods::reads(get(|| async { "json" }));
    let csv = crate::Methods::reads(axum::routing::post(|| async { "csv" }));
    let _ = App::new(Routes::new()).api::<ApiRoot>(|api| api.route::<ApiFile>(json).route::<ApiCsv>(csv));
}

/// A page and an API route that differ only in the text after a parameter.
#[test]
#[should_panic(expected = "differ only in the text after a parameter")]
fn a_page_and_an_api_route_that_differ_only_in_the_text_after_a_parameter_do_not_start() {
    let json = crate::Methods::reads(get(|| async { "json" }));
    let csv = crate::Methods::reads(axum::routing::post(|| async { "csv" }));
    let _ = App::new(Routes::new().__route::<ApiCsv>(csv)).api::<ApiRoot>(|api| api.route::<ApiFile>(json)).router();
}

/// The same in one area, with other parameter names: refused by Corten before the router (which
/// would merge the two methods into one path, or refuse the names with a message about neither).
#[test]
#[should_panic(expected = "differ only in the text after a parameter")]
fn routes_of_an_area_that_differ_only_in_the_text_after_a_parameter_do_not_start() {
    let rss = crate::Methods::reads(get(|| async { "rss" }));
    let json = crate::Methods::reads(axum::routing::post(|| async { "json" }));
    let _ = App::new(Routes::new()).area::<Feeds>(|feeds| feeds.route::<Rss>(rss).route::<FeedJson>(json));
}

#[derive(crate::Route)]
#[route("/feeds/{*rest}")]
struct FeedsRest<'a> {
    rest: &'a str,
}

/// A route with text after a parameter where another, in another module or an area, has its
/// catch-all: the router cannot hold the two, and Corten names both as written (axum would name
/// `/feeds/{name}`), in either order.
#[test]
#[should_panic(expected = "have a parameter where the other has its catch-all")]
fn a_route_with_text_beside_a_catch_all_does_not_start() {
    let rest = crate::Methods::reads(get(|| async { "rest" }));
    let rss = crate::Methods::reads(get(|| async { "rss" }));
    let _ = App::new(Routes::new().__route::<FeedsRest>(rest)).area::<Feeds>(|feeds| feeds.route::<Rss>(rss)).router();
}

#[test]
#[should_panic(expected = "have a parameter where the other has its catch-all")]
fn a_catch_all_beside_a_route_with_text_in_an_area_does_not_start() {
    let rest = crate::Methods::reads(get(|| async { "rest" }));
    let rss = crate::Methods::reads(get(|| async { "rss" }));
    let _ = App::new(Routes::new()).area::<Feeds>(|feeds| feeds.route::<Rss>(rss).route::<FeedsRest>(rest));
}

#[derive(crate::Route)]
#[route("/api/files/{*rest}")]
struct ApiFilesRest<'a> {
    rest: &'a str,
}

#[test]
#[should_panic(expected = "have a parameter where the other has its catch-all")]
fn a_route_with_text_beside_a_catch_all_in_an_api_does_not_start() {
    let json = crate::Methods::reads(get(|| async { "json" }));
    let rest = crate::Methods::reads(get(|| async { "rest" }));
    let _ = App::new(Routes::new()).api::<ApiRoot>(|api| api.route::<ApiFile>(json).route::<ApiFilesRest>(rest));
}

#[tokio::test]
async fn a_handler_reads_its_routes_parameters_as_the_link_wrote_them() {
    let router = App::new(params_url::router()).router();
    let dims = std::collections::BTreeMap::from([
        ("form".to_string(), "graded".to_string()),
        ("grade".to_string(), "PSA 8".to_string()),
    ]);
    let link = params_url::Item {
        shop: "a b/c",
        id: 7,
        sort_by: Some("price"),
        tags: &["x", "y&z"],
        mine: true,
        paging: ParamsPaging { page: Some(2) },
        dims: &dims,
    };
    assert_eq!(
        link.href(),
        "/shops/a%20b%2Fc/items/7?sortBy=price&tags=x&tags=y%26z&mine=true&page=2&dim.form=graded&dim.grade=PSA%208"
    );
    let (status, _, body) = call_with(router.clone(), "GET", &link.href(), None, None).await;
    assert_eq!(
        (status, &body[..]),
        (StatusCode::OK, r#"a b/c|7|Some("price")|["x", "y&z"]|true|Some(2)|{"form": "graded", "grade": "PSA 8"}"#)
    );
    let empty = std::collections::BTreeMap::new();
    let plain = params_url::Item {
        shop: "s",
        id: 1,
        sort_by: None,
        tags: &[],
        mine: false,
        paging: ParamsPaging::default(),
        dims: &empty,
    };
    let (status, _, body) = call_with(router.clone(), "GET", "/shops/s/items/1", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "s|1|None|[]|false|None|{}"), "{}", plain.href());
    let (_, _, body) = call_with(router.clone(), "GET", "/shops/s/items/1?sortBy=&other=1", None, None).await;
    assert_eq!(body, "s|1|None|[]|false|None|{}", "a blank value is none, and a parameter the route does not have is ignored");
    for uri in
        ["/shops/s/items/seven", "/shops/s/items/1?mine=maybe", "/shops/s/items/1?page=x", "/shops/s/items/1?sortBy=a&sortBy=b"]
    {
        let (status, _, _) = call_with(router.clone(), "GET", uri, None, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
    }
    let shown = Shown("x y".into());
    assert_eq!(LinkOnly { value: &shown, tab: Some(&shown) }.href(), "/shown/x%20y?tab=x%20y");
    assert_eq!(LinkTwin { value: &shown, tab: None }.href(), "/shown-twin/x%20y");
    assert_eq!(LinkFile { value: &shown }.href(), "/shown-file/x%20y.xml");
    let file = params_url::File { rest: "docs/a b.txt".into() };
    let (_, _, body) = call_with(router, "GET", &file.href(), None, None).await;
    assert_eq!(body, "docs/a b.txt", "{}", file.href());
}

/// An API's routes in a module of their own, sharing the module's state; its prefix route names
/// no handler.
#[crate::routes(api = ModRoot, state = u32)]
mod api_module {
    #[route("/v1")]
    pub struct ModRoot;

    #[route("/v1/items/{id}", get = module_item)]
    pub struct ModItem {
        pub id: u32,
    }
}

async fn module_item(axum::extract::State(base): axum::extract::State<u32>, item: crate::Params<api_module::ModItem>) -> String {
    format!("item {}", base + item.id)
}

#[tokio::test]
async fn an_api_module_serves_its_routes_and_answers_its_failures_as_json() {
    let router = App::new(Routes::new()).api(|routes| api_module::api(routes, 100)).router();
    let (status, _, body) = call_with(router.clone(), "GET", "/v1/items/7", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "item 107"), "the handler gets the module's state");
    for (uri, status) in
        [("/v1/items/seven", StatusCode::BAD_REQUEST), ("/v1", StatusCode::NOT_FOUND), ("/v1/nope", StatusCode::NOT_FOUND)]
    {
        let (got, headers, body) = call_with(router.clone(), "GET", uri, None, Some("text/html")).await;
        assert_eq!((got, headers["content-type"].to_str().unwrap()), (status, "application/problem+json"), "{uri}");
        assert!(body.starts_with('{') && body.contains(&format!(r#""status":{}"#, status.as_u16())), "{uri}: {body}");
    }
    assert_eq!(api_module::ModItem { id: 7 }.to_string(), "/v1/items/7", "the structs still build links");
}

#[cfg(feature = "json")]
#[tokio::test]
async fn validation_errors_are_the_json_error_with_fields() {
    let router = App::new(Routes::new())
        .api::<ApiRoot>(|api| {
            api.route::<ApiSignUp>(crate::Methods::writes(axum::routing::post(|| async {
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
        r#"{"title":"Unprocessable Entity","status":422,"detail":"email: Enter an email address.","fields":{"email":"Enter an email address."}}"#
    );
}

/// A JSON route that checks its input and loads a record, each with `?`.
#[cfg(feature = "json")]
#[crate::routes(api = CheckedRoot)]
mod checked_api {
    #[route("/checked")]
    pub struct CheckedRoot;

    #[route("/checked/{n}", post = checked)]
    pub struct Checked {
        pub n: u32,
    }
}

#[cfg(feature = "json")]
async fn checked(checked: crate::Params<checked_api::Checked>) -> crate::JsonResult<u32> {
    let n = checked.n;
    let mut errors = Errors::new();
    if n == 0 {
        errors.add(SignUp::FIELDS.email, "Enter a number above zero.");
    }
    let n = errors.into_result(n)?;
    // A load that fails, next to the check.
    let n = n.checked_mul(1000).ok_or_else(|| Error::new("too large to load"))?;
    Ok(crate::Json(n))
}

/// JSON answers with a status or a cookie of their own, in a routes module and under an API.
#[cfg(feature = "json")]
#[crate::routes]
mod own_json_url {
    #[route("/started", post = own_started)]
    pub struct Started;

    #[route("/forecast", get = own_forecast)]
    pub struct Forecast;

    #[route("/session", post = own_session)]
    pub struct Session;

    #[route("/held", get = own_held)]
    pub struct Held;
}

#[cfg(feature = "json")]
#[crate::routes(api = OwnApiRoot)]
mod own_json_api {
    #[route("/v1")]
    pub struct OwnApiRoot;

    #[route("/v1/forecast", get = own_forecast)]
    pub struct ApiForecast;
}

#[cfg(feature = "json")]
#[derive(serde::Serialize, crate::ToJson)]
struct OwnStarted {
    ok: bool,
}

#[cfg(feature = "json")]
async fn own_started() -> Result<crate::JsonResponse, JsonError> {
    Ok(crate::Json(OwnStarted { ok: true }).with_status(StatusCode::ACCEPTED))
}

#[cfg(feature = "json")]
async fn own_forecast() -> Result<crate::JsonResponse, JsonError> {
    let problem = crate::json::Raw::new(r#"{"error":"No capacity forecast yet"}"#)?;
    Ok(crate::Json(problem).with_status(StatusCode::SERVICE_UNAVAILABLE))
}

#[cfg(feature = "json")]
async fn own_session() -> Result<crate::JsonResponse, JsonError> {
    let token = SetCookie::try_new("session", "abc")?;
    Ok(crate::Json(OwnStarted { ok: true }).with_cookie(token).with_cookie(SetCookie::remove("old")))
}

#[cfg(feature = "json")]
async fn own_held() -> crate::JsonResult<crate::json::Raw> {
    Ok(crate::Json(crate::json::Raw::new(r#"{"rows":[1,2]}"#)?))
}

#[cfg(feature = "json")]
#[tokio::test]
async fn a_json_answer_has_a_status_and_cookies_of_its_own() {
    let router = App::new(own_json_url::router()).api(own_json_api::api).router();
    let (status, headers, body) = call_with(router.clone(), "POST", "/started", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::ACCEPTED, r#"{"ok":true}"#));
    assert_eq!(headers["content-type"], "application/json");
    // The app's own 503 reaches the client as written, in a routes module and under an API.
    for uri in ["/forecast", "/v1/forecast"] {
        let (status, headers, body) = call_with(router.clone(), "GET", uri, None, Some("text/html")).await;
        assert_eq!((status, &body[..]), (StatusCode::SERVICE_UNAVAILABLE, r#"{"error":"No capacity forecast yet"}"#), "{uri}");
        assert_eq!(headers["content-type"], "application/json");
    }
    let (status, headers, body) = call_with(router.clone(), "POST", "/session", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, r#"{"ok":true}"#));
    let cookies: Vec<_> = headers.get_all("set-cookie").iter().map(|v| v.to_str().unwrap()).collect();
    assert_eq!(
        cookies,
        ["session=abc; Path=/; SameSite=Lax; HttpOnly; Secure", "old=; Path=/; SameSite=Lax; Max-Age=0; HttpOnly; Secure"]
    );
    assert!(headers["cache-control"].to_str().unwrap().contains("private"));
    let (status, _, body) = call_with(router, "GET", "/held", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, r#"{"rows":[1,2]}"#));
}

#[cfg(feature = "json")]
#[test]
fn a_held_document_is_written_as_it_is_inside_another_value() {
    use crate::json::ToJson;
    #[derive(crate::ToJson)]
    struct Page {
        id: u32,
        rows: crate::json::Raw,
    }
    let doc = crate::json::Raw::new(r#"{"rows":[1,2]}"#).unwrap();
    assert_eq!(vec![doc.clone(), doc.clone()].to_json(), r#"[{"rows":[1,2]},{"rows":[1,2]}]"#);
    assert_eq!(Page { id: 7, rows: doc }.to_json(), r#"{"id":7,"rows":{"rows":[1,2]}}"#);
}

#[cfg(feature = "json")]
#[test]
fn a_held_document_that_is_not_one_json_value_is_an_error() {
    use crate::json::Raw;
    for text in ["{\"rows\": [1,", "", "{} {}", "nul", "\u{feff}{}"] {
        let error = Raw::new(text).unwrap_err();
        assert!(error.to_string().starts_with("not one JSON value: "), "{text:?}: {error}");
    }
    // A string in another encoding (Windows-1252's é) is not JSON, though the parser skips it.
    let error = Raw::new(&b"{\"name\":\"caf\xE9\"}"[..]).unwrap_err();
    assert!(error.to_string().starts_with("not one JSON value: not UTF-8"), "{error}");
    assert_eq!(Raw::new(" [1] ").unwrap().as_bytes(), b" [1] ");
}

#[cfg(feature = "json")]
#[tokio::test]
async fn a_json_handler_question_marks_field_errors_into_a_422_with_the_fields() {
    let router = App::new(Routes::new()).api(checked_api::api).router();
    let (status, _, body) = call_with(router.clone(), "POST", "/checked/2", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "2000"));
    let (status, headers, body) = call_with(router.clone(), "POST", "/checked/0", None, Some("text/html")).await;
    assert_eq!(
        (status, headers["content-type"].to_str().unwrap()),
        (StatusCode::UNPROCESSABLE_ENTITY, "application/problem+json")
    );
    assert_eq!(
        body,
        r#"{"title":"Unprocessable Entity","status":422,"detail":"email: Enter a number above zero.","fields":{"email":"Enter a number above zero."}}"#
    );
    // Any other error is still a 500, whose message a client does not see.
    let (status, _, body) = call_with(router, "POST", "/checked/4294968", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::INTERNAL_SERVER_ERROR, r#"{"title":"Internal Server Error","status":500}"#));
    // An API with its own error shape writes the 500 in it; the fields keep Corten's shape.
    let shaped = App::new(Routes::new())
        .api(|api| {
            checked_api::api(api).errors(|status, message| ShapedError {
                r#type: "error",
                error: ShapedDetail { code: status.as_u16(), message: message.to_string() },
            })
        })
        .router();
    let (_, _, body) = call_with(shaped.clone(), "POST", "/checked/4294968", None, None).await;
    assert_eq!(body, r#"{"type":"error","error":{"code":500,"message":"Internal Server Error"}}"#);
    let (status, _, body) = call_with(shaped, "POST", "/checked/0", None, None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.ends_with(r#""fields":{"email":"Enter a number above zero."}}"#), "{body}");
}

#[cfg(feature = "json")]
#[tokio::test]
async fn a_json_body_that_does_not_parse_is_a_json_400() {
    let router = App::new(Routes::new())
        .api::<ApiRoot>(|api| {
            api.route::<ApiItems>(crate::Methods::writes(axum::routing::post(
                |axum::Json(v): axum::Json<serde_json::Value>| async move { v.to_string() },
            )))
        })
        .router();
    let req = Request::post("/api/items").header("content-type", "application/json").body(Body::from("{nope")).unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(
        body.starts_with(r#"{"title":"Bad Request","status":400,"detail":"Failed to parse the request body as JSON"#),
        "{body}"
    );
}

/// Pages marked for shared caches, one that sets a cookie, one that must never be stored, and one
/// that says nothing.
fn cached_routes() -> Router {
    let minute = std::time::Duration::from_secs(60);
    let home = || html! { <p>"home"</p> };
    Router::new()
        .route("/", get(move |req: Req| async move { Public::new(minute, req.test_page_for("Home", home())) }))
        .route(
            "/cookie",
            get(move |req: Req| async move {
                Ok::<_, Error>((Cache::public(minute), [(header::SET_COOKIE, "seen=1")], req.test_page("Home", home())?))
            }),
        )
        .route(
            "/catalog",
            get(move |req: Req| async move {
                Public::new(minute, req.test_page_for("Catalog", home())).shared(std::time::Duration::from_secs(86_400))
            }),
        )
        .route("/statement", get(move |req: Req| async move { NoStore(req.test_page("Statement", home())) }))
        // A one-time code that has expired, and a statement that is someone else's.
        .route(
            "/code",
            get(|| async { NoStore(Err::<StatusCode, Error>(Error::status(StatusCode::BAD_REQUEST))) })
                .post(|| async { NoStore(Err::<StatusCode, Error>(Error::status(StatusCode::BAD_REQUEST))) }),
        )
        // Two lines, as HTTP allows; the second says no-store.
        .route(
            "/lines",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "max-age=60"), (header::CACHE_CONTROL, "no-store")];
                (StatusCode::BAD_REQUEST, axum::response::AppendHeaders(lines), "")
            }),
        )
        .route("/statement/other", get(|| async { NoStore(Err::<StatusCode, Error>(Error::not_found())) }))
        // A no-store line with a byte that is not text.
        .route(
            "/odd",
            get(|| async {
                let line = HeaderValue::from_bytes(b"no-store, x=\"\xE9\"").unwrap();
                (StatusCode::BAD_REQUEST, [(header::CACHE_CONTROL, line)], "")
            }),
        )
        // Shared, with a limited private that still lets a shared cache keep the rest.
        .route(
            "/limited",
            get(|| async {
                let lines = [
                    (header::CACHE_CONTROL, "public, max-age=600"),
                    (header::CACHE_CONTROL, "private=\"set-cookie\""),
                    (header::SET_COOKIE, "seen=1"),
                ];
                (axum::response::AppendHeaders(lines), "limited")
            }),
        )
        .route(
            "/cookies",
            // `with_cookie` on a `Public` answer does not compile; a header written by hand still
            // makes it private when it is sent.
            get(move |req: Req| async move {
                let page = req.test_page_for("Home", home()).map(|mut page| {
                    page.headers_mut().append(header::SET_COOKIE, SetCookie::new("a", "1").header());
                    page
                });
                Public::new(minute, page)
            }),
        )
        .route("/plain", get(|req: Req| async move { req.test_page("Plain", html! { <p>"plain"</p> }) }))
        // A closure has no return type to read: `Public::new` finds its type from the answer.
        .route("/text", get(move || async move { Public::new(minute, "text") }))
        .route("/page", get(move |req: Req| async move { Public::new(minute, req.test_page_for("Page", home())) }))
}

async fn cache_control(router: &Router, uri: &str, extra: &[(&'static str, &'static str)]) -> String {
    let mut req = Request::get(uri);
    for (name, value) in extra {
        req = req.header(*name, *value);
    }
    let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    res.headers()[header::CACHE_CONTROL].to_str().unwrap().to_string()
}

/// Each `with_cookie` adds one `Set-Cookie`, in order, with the safe flags.
#[tokio::test]
async fn with_cookie_sets_each_cookie_in_order() {
    let routes = Router::new().route(
        "/",
        get(|req: Req| async move {
            Ok::<_, Error>(
                req.redirect::<ForOne>(&Done)?.with_cookie(SetCookie::new("a", "1")).with_cookie(SetCookie::remove("b")),
            )
        }),
    );
    let res = app(routes, None).oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
    let cookies: Vec<_> = res.headers().get_all(header::SET_COOKIE).iter().map(|c| c.to_str().unwrap()).collect();
    assert_eq!(cookies, ["a=1; Path=/; SameSite=Lax; HttpOnly; Secure", "b=; Path=/; SameSite=Lax; Max-Age=0; HttpOnly; Secure"]);
}

/// A redirect's notice rides in a cookie for the page it goes to: that page reads it once (the
/// answer removes it, and is private), a navigation's page that did not draw it hands it to the
/// runtime in `corten-notice`, and another page neither sees nor removes it.
#[tokio::test]
async fn a_redirect_notice_is_read_once_by_the_page_it_was_for() {
    let routes = Router::new()
        .route("/", get(|req: Req| async move { req.redirect_with_notice(&Done.at(MsgId), "Batch 3517 deleted; 2 left.") }))
        .route("/long", get(|req: Req| async move { req.redirect_with_notice(&Done, "é".repeat(400)) }))
        .route(
            "/done",
            get(|req: Req| async move {
                let notice = req.notice();
                req.test_page("Done", html! { <p>{notice}</p> })
            }),
        )
        .route("/other", get(|req: Req| async move { req.test_page("Other", "nothing") }));
    let router = app(routes, None);
    let set = |res: &Response| {
        res.headers().get_all(header::SET_COOKIE).iter().map(|c| c.to_str().unwrap().to_string()).collect::<Vec<_>>()
    };
    let res = router.clone().oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers()[header::LOCATION], "/done#msg");
    let key = |path: &str| crate::page::path_key(path);
    let cookie = &format!("__Host-corten-notice=p={}&m=Batch+3517+deleted%3B+2+left.", key("/done"));
    assert_eq!(set(&res), [format!("{cookie}; Path=/; SameSite=Lax; Max-Age=60; HttpOnly; Secure")]);
    // The page's path, escapes decoded: the link's `%20` and the browser's `%7B` for a `{` name
    // the same page as the characters.
    assert_eq!(key("/items/a%20b"), key("/items/a b"));
    assert_eq!(key("/a%7Bb%7d"), key("/a{b}"));
    assert_ne!(key("/done"), key("/done/"));
    // An escaped `/` is part of one segment, not a second one.
    assert_ne!(key("/a%2Fb"), key("/a/b"));
    // A broken escape is its bytes as they are.
    assert_ne!(key("/bad%zz"), key("/bad"));
    assert_ne!(key("/bad%4"), key("/bad"));
    let fetch = |uri: &str, mode: Option<&str>| {
        let mut req = Request::get(uri).header(header::COOKIE, format!("a=1; {cookie}"));
        if let Some(mode) = mode {
            req = req.header("corten", mode);
        }
        router.clone().oneshot(req.body(Body::empty()).unwrap())
    };
    let gone = "__Host-corten-notice=; Path=/; SameSite=Lax; Max-Age=0; HttpOnly; Secure";
    for mode in [None, Some("nav")] {
        let res = fetch("/done", mode).await.unwrap();
        assert_eq!(set(&res), [gone], "{mode:?}");
        assert_eq!(res.headers()[header::CACHE_CONTROL], "private, no-cache");
        // The page drew it: a navigation's answer says so (`&d`), so the runtime keeps it only for a
        // full load in its place.
        let header = res.headers().get("corten-notice").map(|v| v.to_str().unwrap().to_string());
        assert_eq!(header.as_deref(), mode.map(|_| "m=Batch+3517+deleted%3B+2+left.&d"), "{mode:?}");
        let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        assert!(body.contains("<p>Batch 3517 deleted; 2 left.</p>"), "{body}");
    }
    let res = fetch("/other", Some("nav")).await.unwrap();
    assert!(set(&res).is_empty() && res.headers().get("corten-notice").is_none(), "another page leaves it");
    // A HEAD request (a link preview) leaves it for the page's GET.
    let req = Request::head("/done").header(header::COOKIE, cookie.as_str()).body(Body::empty()).unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert!(set(&res).is_empty(), "HEAD leaves it");
    // Also when it ends on the error page.
    let missing = cookie.replace(&key("/done"), &key("/missing"));
    let req = Request::head("/missing").header(header::COOKIE, missing).body(Body::empty()).unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert!(set(&res).is_empty(), "HEAD to the error page leaves it");
    let req = Request::get("/missing")
        .header(header::COOKIE, cookie.replace(&key("/done"), &key("/missing")))
        .body(Body::empty())
        .unwrap();
    assert_eq!(set(&router.clone().oneshot(req).await.unwrap()), [gone], "the error page's GET reads it");
    // A navigation's page that does not draw it, or holds it in the layout around `<main>`, which
    // a navigation does not run: the runtime gets it. A full load draws it in the layout.
    let routes = Router::new().route("/done", get(|req: Req| async move { req.test_page("Done", "done") })).route(
        "/done/in-layout",
        get(|req: Req| async move {
            let notice = req.notice();
            req.page::<ForOne>(html! {
                <html>
                    <head>
                        <title>"Done"</title>
                        <Corten />
                    </head>
                    <body>
                        <p id="toast">{notice}</p>
                        <main>"done"</main>
                    </body>
                </html>
            })
        }),
    );
    let layouts = app(routes, None);
    for (path, mode) in [("/done", Some("nav")), ("/done/in-layout", Some("nav")), ("/done/in-layout", None)] {
        let cookie = cookie.replace(&key("/done"), &key(path));
        let mut req = Request::get(path).header(header::COOKIE, cookie);
        if let Some(mode) = mode {
            req = req.header("corten", mode);
        }
        let res = layouts.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(set(&res), [gone], "{path} {mode:?}");
        let header = res.headers().get("corten-notice").map(|v| v.to_str().unwrap().to_string());
        let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        if mode.is_some() {
            assert_eq!(header.as_deref(), Some("m=Batch+3517+deleted%3B+2+left."), "{path}");
            assert!(!body.contains("Batch 3517"), "{body}");
        } else {
            assert_eq!(header, None);
            assert!(body.contains(r#"<p id="toast">Batch 3517 deleted; 2 left.</p>"#), "{body}");
        }
    }
    // Cut at 500 bytes, at a character's end.
    let res = router.clone().oneshot(Request::get("/long").body(Body::empty()).unwrap()).await.unwrap();
    let value = set(&res)[0].clone();
    let value = value.split_once('=').unwrap().1.split(';').next().unwrap();
    let text: String = form_urlencoded::parse(value.as_bytes()).find(|(k, _)| k == "m").unwrap().1.into_owned();
    assert_eq!(text, "é".repeat(250));
}

/// Which origins a browser refuses a `Secure` cookie from: plain `http://` but for loopback.
#[test]
fn a_plain_origin_is_http_off_loopback() {
    let with = |origin: &str, host: &str, fetch_site: Option<&'static str>| {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_str(origin).unwrap());
        headers.insert(header::HOST, HeaderValue::from_str(host).unwrap());
        if let Some(site) = fetch_site {
            headers.insert("sec-fetch-site", HeaderValue::from_static(site));
        }
        crate::page::plain_origin(&headers)
    };
    let plain = |origin: &str| with(origin, origin.split_once("://").map_or("", |(_, host)| host), None);
    for origin in
        ["http://automation.lan:3001", "http://192.168.1.5", "http://localhost.lan", "http://[fe80::1]:80", "http://10.0.0.127"]
    {
        assert!(plain(origin), "{origin}");
    }
    for origin in [
        "https://automation.lan",
        "http://localhost",
        "http://localhost.:3000",
        "http://LOCALHOST:3000",
        "http://app.localhost:3000",
        "http://127.0.0.2:8080",
        "http://[::1]:3000",
        "http://[::1]",
        "null",
    ] {
        assert!(!plain(origin), "{origin}");
    }
    assert!(!crate::page::plain_origin(&HeaderMap::new()), "no Origin: not a browser's write");
    // A browser that counts the origin as secure (a policy for an internal tool) sends
    // `Sec-Fetch-Site`, takes a `Secure` cookie and is not read the plain one.
    assert!(!with("http://automation.lan:3001", "automation.lan:3001", Some("same-origin")));
    // Another origin's write (`Policy::allow_origin`): the response is this host's, not that one's.
    assert!(!with("http://admin.lan", "automation.lan:3001", None));
}

/// A write from a plain `http://` origin on the LAN, which a browser does not count as secure and so
/// refuses a `Secure` (and `__Host-`) cookie from, gets its notice in a plain `corten-notice`
/// cookie; the page reads that one only from a request without `Sec-Fetch-Site` (which a browser
/// sends to every secure origin, so on HTTPS another site on the domain cannot plant one) and
/// removes it as it was set.
#[tokio::test]
async fn a_notice_written_over_plain_http_on_the_lan_reaches_its_page() {
    let routes =
        Router::new().route("/", axum::routing::post(|req: Req| async move { req.redirect_with_notice(&Done, "Saved.") })).route(
            "/done",
            get(|req: Req| async move {
                let notice = req.notice();
                req.test_page("Done", html! { <p>{notice}</p> })
            }),
        );
    let router = app(routes, None);
    let set = |res: &Response| {
        res.headers().get_all(header::SET_COOKIE).iter().map(|c| c.to_str().unwrap().to_string()).collect::<Vec<_>>()
    };
    let value = format!("p={}&m=Saved.", crate::page::path_key("/done"));
    let write = |origin: &str, host: &str| {
        let req = Request::post("/").header("origin", origin).header("host", host);
        router.clone().oneshot(req.body(Body::empty()).unwrap())
    };
    let res = write("http://192.168.1.5:3001", "192.168.1.5:3001").await.unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(set(&res), [format!("corten-notice={value}; Path=/; SameSite=Lax; Max-Age=60; HttpOnly")]);
    // A secure origin, loopback over plain `http://` included, keeps the `__Host-` cookie.
    for (origin, host) in [
        ("https://192.168.1.5", "192.168.1.5"),
        ("http://localhost:3000", "localhost:3000"),
        ("http://127.0.0.1:3000", "127.0.0.1:3000"),
    ] {
        let res = write(origin, host).await.unwrap();
        assert_eq!(
            set(&res),
            [format!("__Host-corten-notice={value}; Path=/; SameSite=Lax; Max-Age=60; HttpOnly; Secure")],
            "{origin}"
        );
    }
    let read = |cookie: &str, secure: bool| {
        let mut req = Request::get("/done").header(header::COOKIE, cookie);
        if secure {
            req = req.header("sec-fetch-site", "same-origin");
        }
        router.clone().oneshot(req.body(Body::empty()).unwrap())
    };
    let plain = format!("a=1; corten-notice={value}");
    let res = read(&plain, false).await.unwrap();
    assert_eq!(set(&res), ["corten-notice=; Path=/; SameSite=Lax; Max-Age=0; HttpOnly"]);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "private, no-cache");
    let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(body.contains("<p>Saved.</p>"), "{body}");
    // From a secure origin the plain cookie is not the app's: another site on the domain could
    // have set it. It is neither shown nor removed.
    let res = read(&plain, true).await.unwrap();
    assert!(set(&res).is_empty());
    let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(body.contains("<p></p>"), "{body}");
    // With both, the `__Host-` one is the notice, and both go.
    let both = format!("corten-notice=p={}&m=Planted.; __Host-corten-notice={value}", crate::page::path_key("/done"));
    let res = read(&both, false).await.unwrap();
    assert_eq!(
        set(&res),
        [
            "__Host-corten-notice=; Path=/; SameSite=Lax; Max-Age=0; HttpOnly; Secure",
            "corten-notice=; Path=/; SameSite=Lax; Max-Age=0; HttpOnly"
        ]
    );
    let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(body.contains("<p>Saved.</p>"), "{body}");
    // A HEAD request leaves it for the GET.
    let req = Request::head("/done").header(header::COOKIE, plain.as_str()).body(Body::empty()).unwrap();
    assert!(set(&router.clone().oneshot(req).await.unwrap()).is_empty(), "HEAD leaves it");
    // A write that `Sec-Fetch-Site` says comes from a secure page keeps `__Host-`.
    let req = Request::post("/")
        .header("origin", "http://192.168.1.5")
        .header("host", "192.168.1.5")
        .header("sec-fetch-site", "same-origin");
    let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    assert!(set(&res)[0].starts_with("__Host-corten-notice="), "{:?}", set(&res));
    // An app that says it is HTTPS only (`Policy::hsts`) never sets nor reads the plain one.
    let routes =
        Router::new().route("/", axum::routing::post(|req: Req| async move { req.redirect_with_notice(&Done, "Saved.") })).route(
            "/done",
            get(|req: Req| async move {
                let notice = req.notice();
                req.test_page("Done", html! { <p>{notice}</p> })
            }),
        );
    let https = App::new(Routes::from(routes)).policy(crate::security::Policy::new().hsts()).router();
    let req = Request::post("/").header("origin", "http://192.168.1.5").header("host", "192.168.1.5");
    let res = https.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    assert!(set(&res)[0].starts_with("__Host-corten-notice="), "{:?}", set(&res));
    let req = Request::get("/done").header(header::COOKIE, plain.as_str()).body(Body::empty()).unwrap();
    let res = https.clone().oneshot(req).await.unwrap();
    assert!(set(&res).is_empty(), "{:?}", set(&res));
}

/// A `Public` page that reads a notice from the plain cookie is sent private, as for the
/// `__Host-` one: the removal is Corten's, not a cookie the app set by mistake.
#[tokio::test]
async fn a_public_page_reading_a_plain_notice_is_private() {
    let minute = std::time::Duration::from_secs(60);
    let routes = Router::new().route(
        "/done",
        get(move |req: Req| async move {
            let notice = req.notice();
            Public::new(minute, req.test_page_for("Done", html! { <p>{notice}</p> }))
        }),
    );
    let router = app(routes, None);
    for cookie in ["corten-notice", "__Host-corten-notice"] {
        let value = format!("{cookie}=p={}&m=Saved.", crate::page::path_key("/done"));
        let req = Request::get("/done").header(header::COOKIE, value).body(Body::empty()).unwrap();
        let res = router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.headers()[header::CACHE_CONTROL], "private, no-cache", "{cookie}");
        assert_eq!(res.headers().get_all(header::SET_COOKIE).iter().count(), 1, "{cookie}");
    }
}

/// Bytes as the app names their type: private by default, and a year in any cache when the URL
/// names the content (`Public::immutable`), cookies or not, unless a caller was read.
#[tokio::test]
async fn media_is_sent_as_its_type_and_kept_only_when_its_url_names_its_content() {
    let png: &'static [u8] = b"\x89PNG\r\n\x1a\n";
    let routes = Router::new()
        .route("/fresh", get(move || async move { crate::Media::new("image/png", png) }))
        .route("/kept", get(move || async move { Public::immutable(crate::Media::new("image/png", png)) }))
        .route("/gone", get(|| async { Public::immutable(Err::<crate::Media, _>(Error::not_found())) }));
    let router = app(routes, None);
    let res = router.clone().oneshot(Request::get("/kept").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(res.headers()["x-content-type-options"], "nosniff");
    assert_eq!(res.into_body().collect().await.unwrap().to_bytes(), png);
    assert_eq!(cache_control(&router, "/fresh", &[]).await, "private, no-cache");
    assert_eq!(cache_control(&router, "/kept", &[]).await, "public, max-age=31536000, immutable");
    let res =
        router.clone().oneshot(Request::get("/kept").header("cookie", "session=1").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
    assert!(
        res.headers().get(header::VARY).is_none_or(|vary| !vary.to_str().unwrap().contains("cookie")),
        "the URL alone says what it is"
    );
    assert_eq!(cache_control(&router, "/gone", &[]).await, "private, no-cache", "a failure is never kept");
}

/// A download names its file on a private answer, and is compressed as any text is; a public
/// one keeps the name too.
#[tokio::test]
async fn a_download_names_its_file_and_stays_private() {
    let csv = "id,name\n7,Pikachu\n".repeat(200);
    let routes = Router::new()
        .route("/export", get(move || async move { crate::Media::new("text/csv", csv).attachment("Pokémon.csv") }))
        .route(
            "/slip",
            get(move || async move {
                Public::immutable(crate::Media::new("application/pdf", &b"%PDF-1.7"[..]).attachment("x").inline("slip.pdf"))
            }),
        );
    let router = app(routes, None);
    let res = router
        .clone()
        .oneshot(Request::get("/export").header("accept-encoding", "gzip").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        res.headers()[header::CONTENT_DISPOSITION],
        "attachment; filename=\"Pok_mon.csv\"; filename*=UTF-8''Pok%C3%A9mon.csv"
    );
    assert_eq!(res.headers()[header::CACHE_CONTROL], "private, no-cache");
    #[cfg(feature = "compress")]
    assert_eq!(res.headers()[header::CONTENT_ENCODING], "gzip");
    let res = router.oneshot(Request::get("/slip").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.headers()[header::CONTENT_DISPOSITION], "inline; filename=\"slip.pdf\"", "the last one given");
    assert_eq!(res.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
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

/// `NoStore` holds for a failure too: the error page that replaces it is not kept either.
#[tokio::test]
async fn a_failure_under_no_store_is_not_stored() {
    let router = app(cached_routes(), None);
    for (path, status) in [("/code", StatusCode::BAD_REQUEST), ("/statement/other", StatusCode::NOT_FOUND)] {
        for extra in [&[][..], &[("accept", "text/html")], &[("corten", "nav")], &[("accept", "application/json")]] {
            let mut req = Request::get(path);
            for (name, value) in extra {
                req = req.header(*name, *value);
            }
            let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(res.status(), status, "{path} {extra:?}");
            assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store", "{path} {extra:?}");
        }
    }
    // A form sent to the expired code: the runtime's action, answered with the error page.
    let req = Request::post("/code").header("sec-fetch-site", "same-origin").header("corten", "action");
    let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store", "an action");
    // A header written by hand counts on any of its lines; the error page is `no-store` alone,
    // since the failure's other directives described its own answer.
    let res =
        router.clone().oneshot(Request::get("/lines").header("accept", "text/html").body(Body::empty()).unwrap()).await.unwrap();
    let lines: Vec<_> = res.headers().get_all(header::CACHE_CONTROL).iter().map(|v| v.to_str().unwrap().to_string()).collect();
    assert_eq!(lines, ["no-store"]);
    // Also when the error page read a caller.
    let res = app(cached_routes(), Some("ann"))
        .oneshot(Request::get("/lines").header("accept", "text/html").header("cookie", "s=1").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let lines: Vec<_> = res.headers().get_all(header::CACHE_CONTROL).iter().map(|v| v.to_str().unwrap().to_string()).collect();
    assert_eq!(lines, ["no-store"], "with a caller");
    // A line that is not text still counts.
    let res =
        router.clone().oneshot(Request::get("/odd").header("accept", "text/html").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
}

/// A `Cache-Control` whose `private` or `no-store` is not a plain directive (in a quoted value, a
/// line that is not text, `no-store=1`, with `must-understand`) is not trusted: an answer that sets
/// a cookie or read a caller goes out `private, no-cache`, or `no-store` when any reader could find
/// one in its header. A plain `no-store` or `private` is trusted as written only on one line of
/// plain text with no quoted value: otherwise a `no-store` goes out on one line (`no-store` alone
/// beside a quoted value or a byte that is not text), so a cache that reads only one line, or a
/// value another way, is told the same. `must-understand, no-store` on an answer nothing makes
/// personal is left as written, and a `private` beside them is not trusted.
#[tokio::test]
async fn a_cache_control_counts_only_for_what_it_plainly_says() {
    let router = Router::new()
        .route(
            "/valued",
            get(|| async {
                let line = [(header::CACHE_CONTROL, "public, max-age=600, no-store=1")];
                (StatusCode::BAD_REQUEST, line, "")
            }),
        )
        .route(
            "/second-line",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "public, max-age=600"), (header::CACHE_CONTROL, "no-store")];
                (axum::response::AppendHeaders(lines), "second")
            }),
        )
        .route(
            "/odd-line",
            get(|| async {
                let odd = HeaderValue::from_bytes(b"x=\"\xE9\"").unwrap();
                let lines = [
                    (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=600")),
                    (header::CACHE_CONTROL, odd),
                    (header::SET_COOKIE, HeaderValue::from_static("seen=1")),
                ];
                (axum::response::AppendHeaders(lines), "odd")
            }),
        )
        .route(
            "/quoted",
            get(|| async {
                let lines =
                    [(header::CACHE_CONTROL, "public, max-age=600, no-cache=\"a, no-store, b\""), (header::SET_COOKIE, "seen=1")];
                (axum::response::AppendHeaders(lines), "quoted")
            }),
        )
        .route(
            "/quoted-failure",
            get(|| async {
                let line = [(header::CACHE_CONTROL, "public, max-age=600, no-cache=\"a, no-store, b\"")];
                (StatusCode::BAD_REQUEST, line, "")
            }),
        )
        .route(
            "/escaped",
            get(|| async {
                let lines = [
                    (header::CACHE_CONTROL, "public, max-age=600, no-cache=\"a\\\", no-store, b\""),
                    (header::SET_COOKIE, "seen=1"),
                ];
                (axum::response::AppendHeaders(lines), "escaped")
            }),
        )
        .route(
            "/private-then-no-store",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "private, max-age=600"), (header::CACHE_CONTROL, "no-store")];
                (axum::response::AppendHeaders(lines), "both")
            }),
        )
        .route(
            "/later-private",
            get(|| async {
                let lines = [
                    (header::CACHE_CONTROL, "public, max-age=600"),
                    (header::CACHE_CONTROL, "private"),
                    (header::SET_COOKIE, "seen=1"),
                ];
                (axum::response::AppendHeaders(lines), "later")
            }),
        )
        .route(
            "/private-then-public",
            get(|| async {
                let lines = [
                    (header::CACHE_CONTROL, "private, max-age=600"),
                    (header::CACHE_CONTROL, "public, s-maxage=600"),
                    (header::SET_COOKIE, "seen=1"),
                ];
                (axum::response::AppendHeaders(lines), "both")
            }),
        )
        .route(
            "/no-store-and-more",
            get(|| async {
                let line = [(header::CACHE_CONTROL, "public, s-maxage=600, x=\"\\\"\", no-store")];
                (axum::response::AppendHeaders(line), "more")
            }),
        )
        .route(
            "/private-and-quoted",
            get(|| async {
                let lines =
                    [(header::CACHE_CONTROL, "public, s-maxage=600, x=\"\\\"\", private"), (header::SET_COOKIE, "seen=1")];
                (axum::response::AppendHeaders(lines), "quoted")
            }),
        )
        .route(
            "/must-understand",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "must-understand, no-store"), (header::SET_COOKIE, "seen=1")];
                (axum::response::AppendHeaders(lines), "kept by a cache that knows a 200")
            }),
        )
        .route(
            "/must-understand-shared",
            get(|| async {
                let line = [(header::CACHE_CONTROL, "max-age=60, must-understand, no-store")];
                (axum::response::AppendHeaders(line), "kept only by a cache that knows a 200")
            }),
        )
        .route(
            "/must-understand-valued",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "no-store, must-understand=1, max-age=600"), (header::SET_COOKIE, "seen=1")];
                (axum::response::AppendHeaders(lines), "valued")
            }),
        )
        .route(
            "/private-must-understand",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "private, no-store, must-understand"), (header::SET_COOKIE, "seen=1")];
                (axum::response::AppendHeaders(lines), "private")
            }),
        )
        .route(
            "/private-valued-must-understand",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "private, no-store=1, must-understand"), (header::SET_COOKIE, "seen=1")];
                (axum::response::AppendHeaders(lines), "private")
            }),
        )
        .route(
            "/upper-valued",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "public, max-age=600, No-Store=1"), (header::SET_COOKIE, "seen=1")];
                (axum::response::AppendHeaders(lines), "upper")
            }),
        )
        .route(
            "/must-understand-failure",
            get(|| async {
                let line = [(header::CACHE_CONTROL, "must-understand, no-store")];
                (StatusCode::BAD_REQUEST, line, "")
            }),
        )
        .route(
            "/not-text-no-store",
            get(|| async {
                let odd = HeaderValue::from_bytes(b"public, max-age=600, no-store, x=\xE9").unwrap();
                let lines = [(header::CACHE_CONTROL, odd), (header::SET_COOKIE, HeaderValue::from_static("seen=1"))];
                (axum::response::AppendHeaders(lines), "odd")
            }),
        )
        .route(
            "/not-text-private",
            get(|| async {
                let odd = HeaderValue::from_bytes(b"public, s-maxage=600, private, x=\xE9").unwrap();
                let lines = [(header::CACHE_CONTROL, odd), (header::SET_COOKIE, HeaderValue::from_static("seen=1"))];
                (axum::response::AppendHeaders(lines), "odd")
            }),
        )
        .route(
            "/unquoted-limited",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "public, max-age=600, private=set-cookie"), (header::SET_COOKIE, "seen=1")];
                (axum::response::AppendHeaders(lines), "limited")
            }),
        )
        .route(
            "/no-transform",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "no-transform"), (header::CACHE_CONTROL, "no-store")];
                (axum::response::AppendHeaders(lines), "both")
            }),
        )
        .route(
            "/limited-first",
            get(|| async {
                let lines = [(header::CACHE_CONTROL, "private=\"set-cookie\", max-age=600"), (header::SET_COOKIE, "seen=1")];
                (axum::response::AppendHeaders(lines), "limited")
            }),
        );
    let router = app(router, Some("ann"));
    for (path, want) in [
        ("/odd-line", "private, no-cache"),
        ("/quoted", "no-store"),
        ("/quoted-failure", "no-store"),
        ("/valued", "no-store"),
        ("/second-line", "public, max-age=600, no-store"),
        ("/escaped", "no-store"),
        ("/must-understand-shared", "max-age=60, must-understand, no-store"),
        ("/must-understand-valued", "no-store"),
        ("/private-must-understand", "no-store"),
        ("/private-valued-must-understand", "no-store"),
        ("/upper-valued", "no-store"),
        ("/private-then-no-store", "private, max-age=600, no-store"),
        ("/must-understand", "no-store"),
        ("/must-understand-failure", "no-store"),
        ("/not-text-no-store", "no-store"),
        ("/not-text-private", "private, no-cache"),
        ("/unquoted-limited", "private, no-cache"),
        ("/no-transform", "no-transform, no-store"),
        ("/later-private", "private, no-cache"),
        ("/private-then-public", "private, no-cache"),
        ("/no-store-and-more", "no-store"),
        ("/private-and-quoted", "private, no-cache"),
        ("/limited-first", "private, no-cache"),
    ] {
        let req = Request::get(path).header("accept", "text/html").header("cookie", "s=1");
        let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let lines: Vec<_> = res.headers().get_all(header::CACHE_CONTROL).iter().map(|v| v.as_bytes().to_vec()).collect();
        assert_eq!(lines, [want.as_bytes().to_vec()], "{path}");
    }
}

/// `private="set-cookie"` limits only that header: a shared cache may still keep the rest, so it
/// does not stand for a plain `private` on an answer that sets a cookie.
#[tokio::test]
async fn a_limited_private_is_not_private() {
    let res = app(cached_routes(), None).oneshot(Request::get("/limited").body(Body::empty()).unwrap()).await.unwrap();
    let lines: Vec<_> = res.headers().get_all(header::CACHE_CONTROL).iter().map(|v| v.to_str().unwrap().to_string()).collect();
    assert_eq!(lines, ["private, no-cache"]);
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
    assert_eq!(cache_control(&router, "/cookies", &[]).await, "private, no-cache", "it sets cookies");
    assert_eq!(cache_control(&router, "/text", &[]).await, "public, max-age=60", "a closure's Public");
    assert_eq!(cache_control(&router, "/page", &[]).await, "public, max-age=60", "a closure's Public page");
    assert_eq!(cache_control(&router, "/catalog", &[]).await, "public, max-age=60, s-maxage=86400");
    assert_eq!(cache_control(&router, "/catalog", &[("corten", "nav")]).await, "private, max-age=60", "the browser's lifetime");
    assert_eq!(cache_control(&router, "/catalog", &[("cookie", "session=abc")]).await, "private, no-cache");
    // App::client_zone's cookie says where the visitor is, not who: alone, it shares as before.
    assert_eq!(cache_control(&router, "/", &[("cookie", "corten-zone=America/Chicago")]).await, "public, max-age=60");
    assert_eq!(cache_control(&router, "/", &[("cookie", "corten-zone=America/Chicago; session=abc")]).await, "private, no-cache");
    let inline = App::new(Routes::from(Router::new().route("/", get(inline_page)))).router();
    assert_eq!(cache_control(&inline, "/", &[]).await, "private, no-cache", "the layout took the nonce");

    let minute = std::time::Duration::from_secs(60);
    let api = App::new(Routes::new())
        .api::<ApiRoot>(|api| {
            api.route::<ApiRates>(crate::Methods::reads(get(move || async move { (Cache::public(minute), "rates") })))
        })
        .router();
    assert_eq!(cache_control(&api, "/api/rates", &[("accept", "application/json")]).await, "public, max-age=60");
    assert_eq!(cache_control(&api, "/api/rates", &[("authorization", "Bearer t")]).await, "private, no-cache");
}

/// `App::client_zone` asks the runtime for the browser's time zone, and `Option<ClientZone>` reads
/// it: a name the browser could write, otherwise `None`. Taking it without the setting is a 500 in
/// a debug build.
#[tokio::test]
async fn a_handler_reads_the_visitors_time_zone_once_the_app_asks_for_it() {
    use crate::ClientZone;
    let routes = || {
        Routes::from(
            Router::new().route("/", get(|req: Req| async move { req.test_page("Home", html! { <p>"home"</p> }) })).route(
                "/zone",
                get(|zone: Option<ClientZone>| async move { zone.map_or_else(|| "none".to_string(), |ClientZone(zone)| zone) }),
            )
            .route(
                "/zone-page",
                get(|req: Req, zone: Option<ClientZone>| async move { req.test_page("Zone", html! {
                    <p>{format!("{zone:?}")}</p>
                }) }),
            ),
        )
    };
    let get_with = |router: Router, uri: &'static str, cookie: Option<&'static str>| async move {
        let mut req = Request::get(uri);
        if let Some(cookie) = cookie {
            req = req.header("cookie", cookie);
        }
        let res = router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let status = res.status();
        (status, String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
    };
    let router = App::new(routes()).client_zone().router();
    // Reading the zone keeps the answer private but not out of the layout: the page around
    // `<main>` is the same, so the runtime swaps it in rather than loading it in full.
    let shell = |uri: &'static str| {
        let router = router.clone();
        async move {
            let req = Request::get(uri).header("corten", "nav").header("cookie", "corten-zone=America/Chicago");
            let res = router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
            (res.headers().get("corten-shell").cloned(), res.headers().get("cache-control").cloned())
        }
    };
    let (home, zoned) = (shell("/").await, shell("/zone-page").await);
    assert!(home.0.is_some() && home.0 == zoned.0, "same layout: {home:?} {zoned:?}");
    assert!(zoned.1.as_ref().is_some_and(|v| v.to_str().unwrap().contains("private")), "{zoned:?}");
    let (_, page) = get_with(router.clone(), "/", None).await;
    assert!(page.contains(r#"<meta name="corten-zone">"#), "the runtime is asked to keep it: {page}");
    assert_eq!(get_with(router.clone(), "/zone", None).await.1, "none", "before the runtime has run");
    for (cookie, zone) in [
        ("corten-zone=America/Chicago", "America/Chicago"),
        ("session=abc; corten-zone=Etc/GMT+5", "Etc/GMT+5"),
        ("corten-zone=America/Argentina/Buenos_Aires", "America/Argentina/Buenos_Aires"),
        ("corten-zone=<script>", "none"),
        ("corten-zone=../../etc/passwd", "none"),
        ("corten-zone=", "none"),
    ] {
        assert_eq!(get_with(router.clone(), "/zone", Some(cookie)).await.1, zone, "{cookie}");
    }

    let without = App::new(routes()).router();
    let (_, page) = get_with(without.clone(), "/", None).await;
    assert!(!page.contains("corten-zone"), "no cookie unless the app asks");
    let (status, body) = get_with(without, "/zone", Some("corten-zone=America/Chicago")).await;
    if cfg!(debug_assertions) {
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "a debug build names the missing App::client_zone()");
    } else {
        assert_eq!((status, body.as_str()), (StatusCode::OK, "none"), "a release build logs it and reads none");
    }
}

/// With the `zones` feature and `App::zoned_dates`, a date is written in the visitor's zone while
/// Corten renders the request's answer, which is then private; the first page, before the browser has reported its
/// zone, is in UTC and says so (`zone_unknown`); `.zone(..)` still chooses. `req.zone()` is the
/// zone with its rules.
#[cfg(feature = "zones")]
#[tokio::test]
async fn dates_are_written_in_the_visitors_zone_while_corten_renders_the_answer() {
    use crate::i18n::{DateStyle, Messages as _, TimeStyle, Utc, Zone as _};
    use std::time::{Duration, UNIX_EPOCH};
    crate::i18n::messages! { struct ZoneMsg { "en" => "src/i18n/en.json" } }
    // 2026-03-22T15:04:05Z: Chicago is on daylight time (UTC-5) since March 8.
    let at = UNIX_EPOCH + Duration::from_secs(1_774_191_845);
    let routes = move || {
        Routes::from(
            Router::new()
                .route(
                    "/",
                    get(move |req: Req| async move {
                        let t: ZoneMsg = req.t();
                        let when = t.date_time(at, DateStyle::Medium, TimeStyle::Short);
                        // Asked while the answer renders, as LocalTime asks it.
                        req.test_page(
                            "Zone",
                            html! {
                                <p>{when}</p>
                                <p>{if when.zone_unknown() { "unknown" } else { "known" }}</p>
                                <p>{when.zone(Utc)}</p>
                            },
                        )
                    }),
                )
                .route("/offset", get(move |req: Req| async move { req.zone().offset(at).to_string() }))
                // A calendar day: the same day in every zone (#502).
                .route(
                    "/day",
                    get(move |req: Req| async move {
                        let t: ZoneMsg = req.t();
                        let day = crate::i18n::Day::new(2026, 3, 22).unwrap();
                        req.test_page("Day", html! { <p>{t.date(day, DateStyle::Medium)}</p> })
                    }),
                )
                // Shared by its own header: a date written for one visitor would make it private.
                .route(
                    "/shared",
                    get(move |req: Req| async move {
                        let t: ZoneMsg = req.t();
                        let page =
                            req.test_page("Shared", html! { <p>{t.date_time(at, DateStyle::Medium, TimeStyle::Short)}</p> })?;
                        Ok::<_, Error>(([(axum::http::header::CACHE_CONTROL, "public, max-age=60")], page))
                    }),
                )
                // Through Corten's handler check, which knows the answer is `Public`.
                .route(
                    "/public",
                    crate::route::get(move |req: Req| async move {
                        let t: ZoneMsg = req.t();
                        let when = t.date_time(at, DateStyle::Medium, TimeStyle::Short);
                        let offset = req.zone().offset(at);
                        let page = req.test_page_for(
                            "Shared",
                            html! {
                                <p>{when}</p>
                                <p>{if when.zone_unknown() { "unknown" } else { "known" }}</p>
                                <p>{offset}</p>
                            },
                        );
                        Public::new(std::time::Duration::from_secs(60), page)
                    })
                    .0,
                )
                .route(
                    "/public-live",
                    crate::route::get(move |req: Req| async move {
                        let t: ZoneMsg = req.t();
                        let live = crate::live(futures_util::StreamExt::map(futures_util::stream::iter([()]), move |()| {
                            let when = t.date_time(at, DateStyle::Medium, TimeStyle::Short);
                            crate::Fragment::new(format!(r#"<p id="n">{when}</p>"#))
                        }));
                        Public::new(std::time::Duration::from_secs(60), live)
                    })
                    .0,
                )
                .route(
                    "/live",
                    get(move |req: Req| async move {
                        let t: ZoneMsg = req.t();
                        // Rendered as the stream is read, after the handler returned.
                        crate::live(futures_util::StreamExt::map(futures_util::stream::iter([()]), move |()| {
                            let when = t.date_time(at, DateStyle::Medium, TimeStyle::Short);
                            crate::Fragment::new(format!(r#"<p id="n">{when}</p>"#))
                        }))
                    }),
                ),
        )
    };
    let get_with = |router: Router, uri: &'static str, cookie: Option<&'static str>| async move {
        let mut req = Request::get(uri);
        if let Some(cookie) = cookie {
            req = req.header("cookie", cookie);
        }
        let res = router.oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
        let cache = res.headers().get("cache-control").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
        (cache, String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
    };
    let utc = "<p>Mar 22, 2026, 3:04 PM UTC</p>";
    let zoned = App::new(routes()).zoned_dates().router();
    let (cache, page) = get_with(zoned.clone(), "/", Some("corten-zone=America/Chicago")).await;
    assert!(page.contains(&format!("<p>Mar 22, 2026, 10:04 AM GMT-5</p><p>known</p>{utc}")), "{page}");
    assert!(cache.contains("private"), "written for this visitor: {cache}");
    for cookie in [None, Some("corten-zone=Mars/Olympus")] {
        let (_, page) = get_with(zoned.clone(), "/", cookie).await;
        assert!(page.contains(&format!("{utc}<p>unknown</p>{utc}")), "{cookie:?}: {page}");
    }
    assert_eq!(get_with(zoned.clone(), "/offset", Some("corten-zone=America/Chicago")).await.1, "-18000");
    assert_eq!(get_with(zoned.clone(), "/offset", None).await.1, "0");
    for zone in ["corten-zone=America/Chicago", "corten-zone=Pacific/Kiritimati", "corten-zone=Pacific/Pago_Pago"] {
        let (_, page) = get_with(zoned.clone(), "/day", Some(zone)).await;
        assert!(page.contains("<p>Mar 22, 2026</p>"), "{zone}: {page}");
    }
    // A `Public` answer is the same for everyone: written as for a visitor whose zone is not
    // known (UTC, its LocalTimes written again in the browser), and it stays shared.
    let (cache, page) = get_with(zoned.clone(), "/public", Some("corten-zone=America/Chicago")).await;
    assert!(page.contains(&format!("{utc}<p>unknown</p><p>0</p>")), "{page}");
    assert!(cache.starts_with("public"), "{cache}");
    // Its live part's fragments too, as the stream is read after the handler returned.
    let (cache, live) = get_with(zoned.clone(), "/public-live", Some("corten-zone=America/Chicago")).await;
    assert!(live.contains("3:04 PM UTC") && cache.starts_with("public"), "{cache}: {live}");
    // A middleware that builds a response of its own around the answer (no extensions kept)
    // cannot lose that it is `Public`: the request's signal holds it.
    struct Rebuild;
    impl crate::Middleware for Rebuild {
        async fn call(&self, req: axum::extract::Request, next: impl crate::Next) -> Response {
            let (parts, body) = next.run(req).await.into_parts();
            let mut res = Response::new(body);
            *res.status_mut() = parts.status;
            *res.headers_mut() = parts.headers;
            res
        }
    }
    let rebuilt = App::new(routes()).zoned_dates().middleware(Rebuild).router();
    for uri in ["/public", "/public-live"] {
        let (cache, page) = get_with(rebuilt.clone(), uri, Some("corten-zone=America/Chicago")).await;
        assert!(page.contains("3:04 PM UTC") && cache.starts_with("public"), "{uri}: {cache}: {page}");
    }
    // A live part's fragments are written in the zone, as its page was.
    let (_, live) = get_with(zoned, "/live", Some("corten-zone=America/Chicago")).await;
    assert!(live.contains("10:04 AM GMT-5"), "{live}");
    // A `Public` route's error page is not the same for everyone: written in the zone.
    async fn dated_error(_: Failure, req: Req) -> HtmlResult {
        let t: ZoneMsg = req.t();
        let at = UNIX_EPOCH + Duration::from_secs(1_774_191_845);
        req.test_page("Failed", html! { <p>{t.date_time(at, DateStyle::Medium, TimeStyle::Short)}</p> })
    }
    let failing = Routes::from(
        Router::new().route(
            "/public-fails",
            crate::route::get(|| async {
                Public::new(std::time::Duration::from_secs(60), Err::<&'static str, _>(Error::not_found()))
            })
            .0,
        ),
    );
    let failing = App::new(failing).zoned_dates().error_page(dated_error, ()).router();
    let (cache, page) = get_with(failing, "/public-fails", Some("corten-zone=America/Chicago")).await;
    assert!(page.contains("10:04 AM GMT-5") && cache.contains("private"), "{cache}: {page}");
    // An app that does not learn zones writes UTC and knows it is meant to.
    let (plain, page) = get_with(App::new(routes()).router(), "/", Some("corten-zone=America/Chicago")).await;
    assert!(page.contains(&format!("{utc}<p>known</p>{utc}")), "{page}");
    // Nor does one that learns them for `ClientZone` and `req.zone()` alone: the feature, which
    // another crate may turn on, does not change an app's dates; `zoned_dates` does.
    let learns = App::new(routes()).client_zone().router();
    let (cache, page) = get_with(learns.clone(), "/", Some("corten-zone=America/Chicago")).await;
    assert!(page.contains(&format!("{utc}<p>known</p>{utc}")), "{page}");
    assert_eq!(cache, plain, "answered as the app that does not learn zones");
    let (_, page) = get_with(learns.clone(), "/", None).await;
    assert!(page.contains(&format!("{utc}<p>known</p>{utc}")), "no LocalTime script before the cookie: {page}");
    let (cache, page) = get_with(learns.clone(), "/shared", Some("corten-zone=America/Chicago")).await;
    assert!(page.contains(utc) && cache.starts_with("public"), "{cache}: {page}");
    let (cache, page) = get_with(App::new(routes()).zoned_dates().router(), "/shared", Some("corten-zone=America/Chicago")).await;
    assert!(page.contains("10:04 AM GMT-5") && cache.contains("private"), "{cache}: {page}");
    assert_eq!(get_with(learns, "/offset", Some("corten-zone=America/Chicago")).await.1, "-18000");
    // Outside a request's render: UTC.
    assert_eq!(
        ZoneMsg::for_locale("en").date_time(at, DateStyle::Medium, TimeStyle::Short).to_string(),
        "Mar 22, 2026, 3:04 PM UTC"
    );
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

/// An area with no page at its prefix: `Docs` is only the prefix, with no `href`.
#[crate::routes(area = Docs)]
mod docs_url {
    #[route("/docs")]
    pub struct Docs;

    #[route("/docs/{page}", get = doc_page)]
    pub struct DocPage<'a> {
        pub page: &'a str,
    }
}

async fn doc_page(doc: crate::Params<docs_url::DocPage<'_>>) -> String {
    format!("doc {}", doc.page)
}

#[tokio::test]
async fn an_area_may_have_no_page_at_its_prefix() {
    use crate::Prefix;
    assert_eq!(docs_url::Docs::PREFIX, "/docs");
    let router = App::new(Routes::new()).area(docs_url::area).router();
    let (status, _, body) = call_with(router.clone(), "GET", &docs_url::DocPage { page: "intro" }.href(), None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "doc intro"));
    let (status, _, _) = call_with(router, "GET", "/docs", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "nothing is served at the prefix, and the app's 404 answers it");
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
async fn catalog(item: crate::Params<caller_url::Catalog>) -> Public<String> {
    Public::new(std::time::Duration::from_secs(60), format!("item {}", item.id))
}

async fn admin_home(req: Req, Caller(staff): Caller<Staff>) -> HtmlResult {
    req.test_page("Admin", html! { <p>"admin "{staff.0}</p> })
}

async fn admin_save(Caller(_): Caller<Staff>, crate::Form(_): Fields) -> &'static str {
    WROTE.with(|n| n.set(n.get() + 1));
    "saved"
}

async fn admin_user(user: crate::Params<admin_url::User>, Caller(_): Caller<Staff>) -> String {
    format!("user {}", user.id)
}

fn caller_app() -> Router {
    App::new(caller_url::router()).error_page(test_error_page, ()).area::<admin_url::Admin>(admin_url::area).router()
}

/// An auditor: another permission than the staff's, for one page in the admin area.
#[derive(Clone, Hash, Debug)]
struct Auditor;

impl<S: Sync> crate::Identify<S> for Auditor {
    async fn identify(parts: &axum::http::request::Parts, state: &S) -> std::result::Result<Self, crate::Refused> {
        let Viewer(name) = <Viewer as crate::Identify<S>>::identify(parts, state).await?;
        match name {
            "ada" | "carol" => Ok(Auditor),
            _ => Err(crate::Refused::forbidden()),
        }
    }
}

/// A page in the admin area that its own caller may use: a second module in the same area.
#[crate::routes(area = super::admin_url::Admin, caller = Auditor)]
mod audit_url {
    #[route("/admin/users/{id}/audit", get = user_audit)]
    pub struct UserAudit {
        pub id: u32,
    }
}

async fn user_audit(crate::Params(at): crate::Params<audit_url::UserAudit>, Caller(_): Caller<Auditor>) -> String {
    format!("audit {}", at.id)
}

/// Two modules in one area, each holding its handlers to its own caller: a staff member who is
/// not an auditor reads the user and not the audit, and an auditor who is not staff the other
/// way round.
#[tokio::test]
async fn an_area_takes_modules_with_callers_of_their_own() {
    let router = App::new(caller_url::router())
        .error_page(test_error_page, ())
        .area(|area| audit_url::area(admin_url::area(area)))
        .router();
    for (uri, viewer, want) in [
        ("/admin/users/7", "bob", StatusCode::OK),
        ("/admin/users/7/audit", "bob", StatusCode::FORBIDDEN),
        ("/admin/users/7", "carol", StatusCode::FORBIDDEN),
        ("/admin/users/7/audit", "carol", StatusCode::OK),
        ("/admin/users/7/audit", "ada", StatusCode::OK),
    ] {
        let (status, _, body) = guarded(&router, "GET", uri, Some(viewer), &[]).await;
        assert_eq!(status, want, "{uri} as {viewer}: {body}");
    }
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
    assert_eq!((status, &body[..]), (StatusCode::UNAUTHORIZED, r#"{"title":"Unauthorized","status":401}"#), "a data client");
    assert_eq!(res["content-type"], "application/problem+json");
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
    assert_eq!((status, &body[..]), (StatusCode::FORBIDDEN, r#"{"title":"Forbidden","status":403}"#));
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
async fn price(price: crate::Params<json_url::Price>, Caller(partner): Caller<Partner>) -> JsonResult<Quote> {
    Ok(Json(Quote { id: price.id, partner: partner.0 }))
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
        assert_eq!((status, &body[..]), (StatusCode::UNAUTHORIZED, r#"{"title":"Unauthorized","status":401}"#), "{accept:?}");
        assert_eq!(res["content-type"], "application/problem+json");
        let (status, _, body) = guarded(&router, "GET", "/api/price/abc", None, headers).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{accept:?}");
        assert!(body.starts_with(r#"{"title":"Bad Request","status":400,"#), "a path that does not parse: {body}");
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
        .area::<Admin>(|admin| admin.route::<Admin>(crate::Methods::reads(get(|| async { "admin" }))))
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
    let server = tokio::spawn(crate::app::serve_router(listeners, crate::app::plain, routes, Default::default(), async {
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
    let server = tokio::spawn(crate::app::run(listeners, crate::app::plain, app, async {
        let _ = stopped.await;
    }));
    (stop, server)
}

/// One request on a fresh connection to `addr`, with `headers` (lines ending in `\r\n`); the
/// answer as text.
async fn raw_get(addr: std::net::SocketAddr, path: &str, headers: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n{headers}\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await.unwrap();
    answer
}

/// `App::header_timeout` closes a connection that never finishes its request head, and
/// `App::max_connections` accepts no more connections than it allows until one closes.
#[tokio::test]
async fn a_connection_is_held_only_as_long_and_as_many_as_the_app_allows() {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let serve = |app: App| async move {
        let listeners = crate::app::bind(Some("127.0.0.1"), 0).await.unwrap();
        let addr = listeners[0].local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(crate::app::run(listeners, crate::app::plain, app, async {
            let _ = stopped.await;
        }));
        (addr, stop, server)
    };
    let routes = || {
        Routes::from(Router::new().route("/", get(|| async { "hi" })).route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_millis(700)).await;
                "slow"
            }),
        ))
    };

    let (addr, stop, server) = serve(App::new(routes()).header_timeout(Duration::from_millis(300))).await;
    let mut slow = tokio::net::TcpStream::connect(addr).await.unwrap();
    slow.write_all(b"GET / HTTP/1.1\r\nhost: localhost\r\n").await.unwrap();
    let mut rest = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(3), slow.read_to_end(&mut rest)).await;
    assert!(closed.is_ok(), "an unfinished head is not held past the timeout");
    assert!(raw_get(addr, "/", "").await.ends_with("hi"), "a request that finishes its head is answered");
    assert!(raw_get(addr, "/slow", "").await.ends_with("slow"), "a handler may take longer than the head's timeout");
    let mut idle = tokio::net::TcpStream::connect(addr).await.unwrap();
    idle.write_all(b"GET / HTTP/1.1\r\nhost: localhost\r\n\r\n").await.unwrap();
    let mut answered = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(3), idle.read_to_end(&mut answered)).await;
    assert!(closed.is_ok(), "an idle keep-alive connection is closed after the timeout");
    assert!(String::from_utf8_lossy(&answered).ends_with("hi"), "after its answer");
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();

    let (addr, stop, server) = serve(App::new(routes()).header_timeout(None).max_connections(1)).await;
    let mut held = tokio::net::TcpStream::connect(addr).await.unwrap();
    held.write_all(b"GET / HTTP/1.1\r\nhost: localhost\r\n\r\n").await.unwrap();
    let mut first = [0u8; 12];
    held.read_exact(&mut first).await.unwrap();
    assert_eq!(&first, b"HTTP/1.1 200");
    let second = tokio::spawn(raw_get(addr, "/", ""));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!second.is_finished(), "a second connection waits while the first is open");
    drop(held);
    let answer = tokio::time::timeout(Duration::from_secs(3), second).await.expect("answered once the first closes").unwrap();
    assert!(answer.ends_with("hi"), "{answer}");
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

/// `corten::serve_wrapped`: the app's step on each connection before HTTP (a TLS handshake) runs
/// in the connection's own task, counts against `App::max_connections` and is closed at
/// `App::header_timeout` when it stalls; a step that fails closes the connection.
#[tokio::test]
async fn a_step_before_http_is_counted_and_timed_as_the_connection() {
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    // A stand-in for a handshake: the client sends "hello\n" first, anything else fails it.
    async fn hello(mut tcp: tokio::net::TcpStream) -> std::io::Result<tokio::net::TcpStream> {
        let mut greeting = [0u8; 6];
        tcp.read_exact(&mut greeting).await?;
        match &greeting {
            b"hello\n" => Ok(tcp),
            _ => Err(std::io::Error::other("not a greeting")),
        }
    }
    let listeners = crate::app::bind(Some("127.0.0.1"), 0).await.unwrap();
    let addr = listeners[0].local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let routes = Routes::from(Router::new().route("/", get(|| async { "hi" })));
    let app = App::new(routes).header_timeout(Duration::from_millis(300)).max_connections(1);
    let server = tokio::spawn(crate::app::run(listeners, hello, app, async {
        let _ = stopped.await;
    }));
    let request = |greeting: &'static [u8]| async move {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut sent = greeting.to_vec();
        sent.extend_from_slice(b"GET / HTTP/1.1\r\nhost: localhost\r\nconnection: close\r\n\r\n");
        // A failed step may close the connection before all of it is read.
        let _ = stream.write_all(&sent).await;
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer).await;
        answer
    };
    assert!(request(b"hello\n").await.ends_with("hi"), "after its step, the connection is HTTP");
    assert_eq!(request(b"howdy\n").await, "", "a step that fails closes the connection");

    // A client that opens a connection and never finishes its step holds the only slot, until
    // the header timeout closes it.
    let started = Instant::now();
    let mut stalled = tokio::net::TcpStream::connect(addr).await.unwrap();
    stalled.write_all(b"hel").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let next = tokio::spawn(request(b"hello\n"));
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!next.is_finished(), "a connection in its step counts against max_connections");
    let mut rest = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(3), stalled.read_to_end(&mut rest)).await;
    assert!(closed.is_ok(), "a stalled step is not held past the header timeout");
    assert!(started.elapsed() >= Duration::from_millis(250), "{:?}", started.elapsed());
    let answer = tokio::time::timeout(Duration::from_secs(3), next).await.expect("answered once it closes").unwrap();
    assert!(answer.ends_with("hi"), "{answer}");

    // One clock from the accept to the head: a step that takes most of the timeout leaves the
    // rest for the head, not the whole timeout again.
    let started = Instant::now();
    let mut slow = tokio::net::TcpStream::connect(addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    slow.write_all(b"hello\n").await.unwrap();
    let mut rest = Vec::new();
    let closed = tokio::time::timeout(Duration::from_secs(3), slow.read_to_end(&mut rest)).await;
    assert!(closed.is_ok(), "a connection with no head after its step is closed");
    assert!(started.elapsed() < Duration::from_millis(420), "at the header timeout from the accept: {:?}", started.elapsed());

    stop.send(()).unwrap();
    server.await.unwrap().unwrap();

    // Shutdown ends a step in progress, with no header timeout to end it otherwise.
    let listeners = crate::app::bind(Some("127.0.0.1"), 0).await.unwrap();
    let addr = listeners[0].local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let routes = Routes::from(Router::new().route("/", get(|| async { "hi" })));
    let server = tokio::spawn(crate::app::run(listeners, hello, App::new(routes).header_timeout(None), async {
        let _ = stopped.await;
    }));
    let mut stalled = tokio::net::TcpStream::connect(addr).await.unwrap();
    stalled.write_all(b"hel").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), server).await.expect("shutdown does not wait for a step").unwrap().unwrap();
    let mut rest = Vec::new();
    assert_eq!(stalled.read_to_end(&mut rest).await.map(|_| rest.len()).unwrap_or(0), 0, "and closes it");
}

/// A route that takes `Upgrade` switches the connection: a WebSocket handshake from the page's own
/// origin or from a client that is not a browser gets its 101 and then the bytes, one from another
/// site is refused as a cross-site write is, and a request that does not ask to switch is a 426,
/// or, where the route takes `Option<Upgrade>`, its other answer.
#[tokio::test]
async fn an_upgrade_route_switches_for_its_own_origin_and_refuses_other_sites() {
    use crate::upgrade::{Switch, SwitchOr, Upgrade};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn echo(mut stream: crate::upgrade::Stream, _: crate::Signal) {
        let mut buf = [0u8; 64];
        while let Ok(n @ 1..) = stream.read(&mut buf).await {
            if stream.write_all(&buf[..n]).await.is_err() {
                break;
            }
        }
    }
    let routes = Router::new()
        .route("/ws", get(|upgrade: Upgrade| async move { upgrade.websocket(echo) }))
        .route(
            "/tunnel",
            get(|upgrade: Upgrade| async move {
                let mut headers = axum::http::HeaderMap::new();
                headers.insert("x-upstream", "yes".parse().unwrap());
                Ok::<Switch, crate::Error>(upgrade.switch(headers, echo))
            }),
        )
        .route(
            "/either",
            get(|upgrade: Option<Upgrade>| async move {
                match upgrade {
                    Some(upgrade) => Ok::<_, crate::Error>(upgrade.websocket(echo)?.into()),
                    None => Ok(SwitchOr::Answer("not a socket")),
                }
            }),
        );
    let listeners = crate::app::bind(Some("127.0.0.1"), 0).await.unwrap();
    let addr = listeners[0].local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let app = App::new(Routes::from(routes)).api::<ApiRoot>(|api| {
        api.route::<ApiWs>(crate::Methods::reads(get(|upgrade: Upgrade| async move { upgrade.websocket(echo) })))
            .allow_origin("https://partner.example")
    });
    let server = tokio::spawn(crate::app::run(listeners, crate::app::plain, app, async {
        let _ = stopped.await;
    }));
    // The answer's head, and the connection for what follows it.
    let handshake = |path: &'static str, protocol: &'static str, extra: &'static str| async move {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let request = format!(
            "GET {path} HTTP/1.1\r\nhost: localhost\r\nconnection: keep-alive, Upgrade\r\nupgrade: {protocol}\r\nsec-websocket-version: 13\r\nsec-websocket-key: dGhlIHNhbXBsZSBub25jZQ==\r\n{extra}\r\n"
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            let mut byte = [0u8; 1];
            if stream.read(&mut byte).await.unwrap() == 0 {
                break;
            }
            head.push(byte[0]);
        }
        (String::from_utf8(head).unwrap().to_ascii_lowercase(), stream)
    };
    let echoes = |mut stream: tokio::net::TcpStream| async move {
        stream.write_all(b"ping").await.unwrap();
        let mut back = [0u8; 4];
        stream.read_exact(&mut back).await.unwrap();
        &back == b"ping"
    };

    let (head, stream) = handshake("/ws", "websocket", "origin: http://localhost\r\nsec-fetch-site: same-origin\r\n").await;
    assert!(head.starts_with("http/1.1 101"), "{head}");
    assert!(head.contains("sec-websocket-accept: s3pplmbitxaq9kygzzhzrbk+xoo="), "{head}");
    assert!(echoes(stream).await, "the switched connection carries bytes both ways");

    let (head, stream) = handshake("/ws", "websocket", "").await;
    assert!(head.starts_with("http/1.1 101"), "a client that is not a browser switches: {head}");
    assert!(echoes(stream).await);

    let (head, _) = handshake("/ws", "websocket", "origin: https://evil.example\r\nsec-fetch-site: cross-site\r\n").await;
    assert!(head.starts_with("http/1.1 403"), "another site's page cannot open it with the user's cookies: {head}");
    let (head, _) = handshake("/ws", "websocket", "origin: https://evil.example\r\n").await;
    assert!(head.starts_with("http/1.1 403"), "nor an older browser's, by its Origin alone: {head}");

    let (head, _) = handshake("/ws", "chat", "").await;
    assert!(head.starts_with("http/1.1 400"), "only a WebSocket handshake is accepted as one: {head}");

    let plain = raw_get(addr, "/ws", "").await;
    assert!(plain.starts_with("HTTP/1.1 426"), "{plain}");

    let (head, stream) = handshake("/either", "websocket", "").await;
    assert!(head.starts_with("http/1.1 101"), "{head}");
    assert!(echoes(stream).await);
    let plain = raw_get(addr, "/either", "").await;
    assert!(plain.starts_with("HTTP/1.1 200") && plain.ends_with("not a socket"), "{plain}");
    let (head, _) = handshake("/either", "websocket", "origin: https://evil.example\r\nsec-fetch-site: cross-site\r\n").await;
    assert!(head.starts_with("http/1.1 403"), "a switch it asks for is held to what Upgrade is: {head}");

    // An API's allowed site may open its sockets, as it may write to it, but not with cookies.
    let partner = "origin: https://partner.example\r\nsec-fetch-site: cross-site\r\n";
    let (head, stream) = handshake("/api/ws", "websocket", partner).await;
    assert!(head.starts_with("http/1.1 101"), "{head}");
    assert!(echoes(stream).await);
    let (head, _) = handshake(
        "/api/ws",
        "websocket",
        "origin: https://partner.example\r\nsec-fetch-site: cross-site\r\ncookie: session=abc\r\n",
    )
    .await;
    assert!(head.starts_with("http/1.1 403"), "{head}");
    let (head, _) = handshake("/ws", "websocket", partner).await;
    assert!(head.starts_with("http/1.1 403"), "the pages' sockets still refuse the partner: {head}");

    let (head, stream) = handshake("/tunnel", "remote", "").await;
    assert!(head.starts_with("http/1.1 101") && head.contains("x-upstream: yes") && head.contains("upgrade: remote"), "{head}");
    assert!(echoes(stream).await);

    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

#[derive(crate::Route)]
#[route("/api/ws")]
struct ApiWs;

/// A relayed answer goes through the whole app uncompressed, a page-sized body and an event stream
/// alike, so a long model stream holds no compression context (#205).
#[cfg(feature = "compress")]
#[tokio::test]
async fn a_relay_is_not_compressed_by_the_app() {
    fn upstream(content_type: &'static str, body: Body) -> crate::Relay {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("content-type", content_type.parse().unwrap());
        crate::Relay::new(StatusCode::OK, headers, body)
    }
    let page = "<p>relayed</p>".repeat(300);
    let events = "event: delta\ndata: {\"text\":\"hello\"}\n\n".repeat(100);
    let (p, e, own) = (page.clone(), events.clone(), page.clone());
    let router = App::new(Routes::from(
        Router::new()
            .route("/own", get(move || async move { axum::response::Html(own) }))
            .route("/page", get(move || async move { upstream("text/html; charset=utf-8", Body::from(p)) }))
            .route(
                "/stream",
                get(move || async move {
                    let chunk = Ok::<_, std::convert::Infallible>(axum::body::Bytes::from(e));
                    upstream("text/event-stream", Body::from_stream(futures_util::stream::iter([chunk])))
                }),
            ),
    ))
    .router();
    for (path, sent) in [("/page", &page), ("/stream", &events)] {
        let req = Request::get(path).header("accept-encoding", "zstd, br, gzip").body(Body::empty()).unwrap();
        let res = router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{path}");
        assert!(
            !res.headers().contains_key("content-encoding") && !res.headers().contains_key("vary"),
            "{path}: {:?}",
            res.headers()
        );
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], sent.as_bytes(), "{path}");
    }
    // The same body as the app's own answer is compressed, so the app does compress.
    let req = Request::get("/own").header("accept-encoding", "zstd, br, gzip").body(Body::empty()).unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.headers()["content-encoding"], "zstd");
}

#[cfg(feature = "json")]
#[crate::routes]
mod half_url {
    #[route("/half/{status}", get = half)]
    pub struct Half {
        pub status: u16,
    }
}

/// A JSON route that answers `200` itself and passes anything else on, as a route moved off
/// another service a part at a time does.
#[cfg(feature = "json")]
async fn half(crate::Params(at): crate::Params<half_url::Half>) -> Result<crate::JsonResponse, crate::JsonError> {
    if at.status == 200 {
        return Ok(Json(Quote { id: 7, partner: "own" }).into());
    }
    let mut headers = axum::http::HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    headers.insert("content-security-policy", "default-src 'self'".parse().unwrap());
    let body = format!(r#"{{"from":"upstream","pad":"{}"}}"#, "x".repeat(4000));
    let status = StatusCode::from_u16(at.status).unwrap();
    Ok(crate::Relay::new(status, headers, Body::from(body)).into())
}

/// A JSON route that passes some requests on answers the relay as a `JsonResponse`, and it stays
/// the upstream's answer whatever the client accepts: its failure body (never the JSON error or
/// the error page), its CSP, and not compressed; the route's own answer is still the app's (#541).
#[cfg(feature = "json")]
#[tokio::test]
async fn a_json_route_passes_a_relay_on_as_the_upstream_gave_it() {
    let router = App::new(half_url::router()).error_page(test_error_page, ()).router();
    for accept in ACCEPTS {
        for status in [StatusCode::BAD_REQUEST, StatusCode::BAD_GATEWAY] {
            let mut req = Request::get(format!("/half/{}", status.as_u16())).header("accept-encoding", "zstd, br, gzip");
            if let Some(accept) = accept {
                req = req.header("accept", accept);
            }
            let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
            assert_eq!(res.status(), status, "{accept:?}");
            assert_eq!(res.headers()["content-type"], "application/json", "{accept:?}");
            assert_eq!(res.headers()["content-security-policy"], "default-src 'self'", "{accept:?}");
            assert!(!res.headers().contains_key("content-encoding"), "{accept:?}: {:?}", res.headers());
            let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
            assert!(body.starts_with(br#"{"from":"upstream""#), "{accept:?} {status}");
        }
    }
    let (status, _, body) = call_with(router, "GET", "/half/200", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, r#"{"id":7,"partner":"own"}"#));
}

/// A relayed answer is passed on as the upstream gave it: its failure status and body, never the
/// error page or the JSON error, without the hop-by-hop headers.
#[tokio::test]
async fn a_relay_passes_the_upstreams_answer_on() {
    fn upstream(status: StatusCode) -> crate::Relay {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("content-type", "text/plain".parse().unwrap());
        headers.insert("connection", "keep-alive, x-hop".parse().unwrap());
        headers.insert("x-hop", "1".parse().unwrap());
        headers.insert("keep-alive", "timeout=5".parse().unwrap());
        headers.insert("transfer-encoding", "chunked".parse().unwrap());
        headers.insert("x-upstream", "kept".parse().unwrap());
        crate::Relay::new(status, headers, Body::from("upstream says no"))
    }
    let router = App::new(Routes::from(Router::new().route("/relay", get(|| async { upstream(StatusCode::BAD_REQUEST) }))))
        .error_page(test_error_page, ())
        .api::<ApiRoot>(|api| api.route::<ApiItem>(crate::Methods::reads(get(|| async { upstream(StatusCode::BAD_GATEWAY) }))))
        .router();
    for (path, status, accept) in [
        ("/relay", StatusCode::BAD_REQUEST, Some("text/html")),
        ("/relay", StatusCode::BAD_REQUEST, Some("application/json")),
        ("/api/items/7", StatusCode::BAD_GATEWAY, None),
    ] {
        let (got, headers, body) = call_with(router.clone(), "GET", path, None, accept).await;
        assert_eq!((got, &body[..]), (status, "upstream says no"), "{path} {accept:?}");
        assert_eq!(headers["x-upstream"], "kept");
        for hop in ["connection", "x-hop", "keep-alive", "transfer-encoding"] {
            assert!(!headers.contains_key(hop), "{path}: {hop} is the upstream connection's, not ours");
        }
        assert!(headers.contains_key("x-content-type-options"), "the app's security headers still apply");
    }

    // A relayed page keeps the upstream's own CSP, which its scripts were written for; without one
    // the app's applies.
    let page = |csp: Option<&'static str>| {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("content-type", "text/html".parse().unwrap());
        if let Some(csp) = csp {
            headers.insert("content-security-policy", csp.parse().unwrap());
        }
        crate::Relay::new(StatusCode::OK, headers, Body::from("<script nonce=upstream></script>"))
    };
    let router = App::new(Routes::from(
        Router::new()
            .route("/own", get(move || async move { page(Some("script-src 'nonce-upstream'")) }))
            .route("/none", get(move || async move { page(None) })),
    ))
    .router();
    let (_, headers, _) = call_with(router.clone(), "GET", "/own", None, Some("text/html")).await;
    assert_eq!(headers["content-security-policy"], "script-src 'nonce-upstream'");
    assert_eq!(headers["x-frame-options"], "DENY", "the app's other headers still apply");
    let (_, headers, _) = call_with(router, "GET", "/none", None, Some("text/html")).await;
    assert!(headers["content-security-policy"].to_str().unwrap().starts_with("default-src 'self'"));

    // An upstream's event stream reaches the client a piece at a time, not when it ends.
    let (send, receive) = tokio::sync::mpsc::channel::<Result<&'static str, std::io::Error>>(1);
    let stream = std::sync::Arc::new(std::sync::Mutex::new(Some(tokio_stream_of(receive))));
    let router = App::new(Routes::from(Router::new().route(
        "/events",
        get(move || {
            let body = Body::from_stream(stream.lock().unwrap().take().unwrap());
            async move {
                let mut headers = axum::http::HeaderMap::new();
                headers.insert("content-type", "text/event-stream".parse().unwrap());
                crate::Relay::new(StatusCode::OK, headers, body)
            }
        }),
    )))
    .router();
    let res = router.oneshot(Request::get("/events").body(Body::empty()).unwrap()).await.unwrap();
    let mut body = res.into_body();
    send.send(Ok("data: one\n\n")).await.unwrap();
    let first = tokio::time::timeout(std::time::Duration::from_secs(2), body.frame()).await;
    let first = first.expect("the first event arrives while the stream is open").unwrap().unwrap();
    assert_eq!(first.into_data().unwrap(), "data: one\n\n");
    drop(send);
}

/// A stream of the pieces `receive` gets, until its sender is dropped.
fn tokio_stream_of<T>(receive: tokio::sync::mpsc::Receiver<T>) -> impl futures_util::Stream<Item = T> {
    futures_util::stream::unfold(receive, |mut receive| async move { receive.recv().await.map(|item| (item, receive)) })
}

/// `corten::serve` gives each request its connection's peer, which `ClientIp` reads. A proxy's `X-Forwarded-For` is followed only when the
/// app trusts the peer, and the answer is private to the address.
#[tokio::test]
async fn the_client_ip_is_the_peer_or_what_a_trusted_proxy_forwards() {
    let app = |policy: crate::security::Policy| {
        let routes = Router::new().route("/ip", get(|crate::ClientIp(ip): crate::ClientIp| async move { ip.to_string() }));
        App::new(Routes::from(routes)).policy(policy)
    };
    let body = |answer: &str| answer.rsplit("\r\n").next().unwrap().to_string();
    let forwarded = "x-forwarded-for: 1.1.1.1, 198.51.100.7\r\n";

    let listeners = crate::app::bind(Some("127.0.0.1"), 0).await.unwrap();
    let addr = listeners[0].local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(crate::app::run(listeners, crate::app::plain, app(crate::security::Policy::new()), async {
        let _ = stopped.await;
    }));
    let answer = raw_get(addr, "/ip", forwarded).await;
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert_eq!(body(&answer), "127.0.0.1", "without a trusted proxy the header is anyone's");
    assert!(answer.to_ascii_lowercase().contains("cache-control: private"), "{answer}");
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();

    let listeners = crate::app::bind(Some("127.0.0.1"), 0).await.unwrap();
    let addr = listeners[0].local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let behind = app(crate::security::Policy::new().proxy("127.0.0.0/8"));
    let server = tokio::spawn(crate::app::run(listeners, crate::app::plain, behind, async {
        let _ = stopped.await;
    }));
    assert_eq!(body(&raw_get(addr, "/ip", forwarded).await), "198.51.100.7");
    assert_eq!(body(&raw_get(addr, "/ip", "").await), "127.0.0.1");
    stop.send(()).unwrap();
    server.await.unwrap().unwrap();
}

/// Without a connection (a test's `oneshot`), `ClientIp` is a 500 that says why and
/// `Option<ClientIp>` is `None`; axum's `MockConnectInfo` gives it one.
#[tokio::test]
async fn without_a_connection_there_is_no_client_ip() {
    use axum::extract::connect_info::MockConnectInfo;
    let routes = || {
        Router::new()
            .route("/ip", get(|crate::ClientIp(ip): crate::ClientIp| async move { ip.to_string() }))
            .route("/maybe", get(|ip: Option<crate::ClientIp>| async move { format!("{ip:?}") }))
    };
    let router = App::new(Routes::from(routes())).router();
    let (status, _, _) = call_with(router.clone(), "GET", "/ip", None, None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let (status, _, body) = call_with(router, "GET", "/maybe", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "None"));
    let mocked =
        App::new(Routes::from(routes())).router().layer(MockConnectInfo(std::net::SocketAddr::from(([203, 0, 113, 9], 4711))));
    let (status, _, body) = call_with(mocked, "GET", "/ip", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "203.0.113.9"));
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

#[crate::routes]
mod serves_url {
    #[route("/check", get = serves_check)]
    pub struct Check;

    #[route("/items/{id}", get = serves_item)]
    pub struct Item {
        pub id: u32,
    }

    #[route("/files/{*rest}", get = serves_files)]
    pub struct Files {
        pub rest: String,
    }

    #[route("/cart/add", post = serves_check)]
    pub struct AddToCart;

    #[route("/cart/{id}", get = serves_cart_line)]
    pub struct CartLine {
        pub id: u32,
    }

    #[route("/both", post = serves_check, get = serves_check)]
    pub struct Both;

    #[route("/go", get = serves_go)]
    pub struct Go;
}

/// Every GET is a page here, but an API, Corten's own paths and its health checks are not.
#[crate::routes]
mod serves_catch_url {
    #[route("/{*page}", get = serves_any)]
    pub struct Page {
        pub page: String,
    }
}

#[tokio::test]
async fn a_catch_all_page_does_not_serve_an_api_or_cortens_own_paths() {
    let pwa = crate::pwa::Pwa::new(crate::pwa::Manifest::new("A", "A"));
    let router = App::new(serves_catch_url::router())
        .api::<ApiRoot>(|api| api.route::<ApiRates>(crate::Methods::reads(get(|| async { "rates" }))))
        .pwa(pwa)
        .router();
    for (p, want) in [
        ("/about/team", "local served"),
        ("/api/rates", "local elsewhere"),
        ("/api", "local elsewhere"),
        ("/health/live", "local elsewhere"),
        ("/corten/anything", "local elsewhere"),
        ("/manifest.webmanifest", "local elsewhere"),
        ("/service-worker.js", "local elsewhere"),
        ("/apis", "local served"),
    ] {
        let uri = format!("/check?p={}", form_urlencoded::byte_serialize(p.as_bytes()).collect::<String>());
        let (status, _, body) = call_with(router.clone(), "GET", &uri, None, None).await;
        assert_eq!((status, body.as_str()), (StatusCode::OK, want), "{p:?}");
    }
}

/// Paths where matching by hand is easy to get wrong: an empty segment, a POST-only route the
/// router picks over a catch-all page, and a path routed by both the app and an area.
#[crate::routes]
mod serves_router_url {
    #[route("/check", get = serves_check)]
    pub struct Check;

    #[route("/items/{id}/edit", get = serves_edit)]
    pub struct Edit {
        pub id: String,
    }

    #[route("/x/{a}/y", post = serves_write)]
    pub struct Write {
        pub a: String,
    }

    #[route("/shop/cart", get = serves_page)]
    pub struct CartPage;

    #[route("/sitemap/{page}.xml", get = serves_sitemap)]
    pub struct Sitemap {
        pub page: u32,
    }

    #[route("/{*page}", get = serves_router_page)]
    pub struct Page {
        pub page: String,
    }
}

#[crate::routes(area = Shop)]
mod serves_shop_url {
    #[route("/shop")]
    pub struct Shop;

    #[route("/shop/cart", post = serves_page)]
    pub struct Cart;
}

#[derive(crate::Route)]
#[route("/shop/list")]
struct ShopList;

async fn serves_page() -> &'static str {
    "page"
}

// The routes above with a parameter in their path, each read by a handler that takes its `Params`.
async fn serves_edit(_: crate::Params<serves_router_url::Edit>) -> &'static str {
    serves_page().await
}

async fn serves_write(_: crate::Params<serves_router_url::Write>) -> &'static str {
    serves_page().await
}

async fn serves_sitemap(_: crate::Params<serves_router_url::Sitemap>) -> &'static str {
    serves_page().await
}

async fn serves_router_page(_: crate::Params<serves_router_url::Page>) -> &'static str {
    serves_page().await
}

async fn serves_item(_: crate::Params<serves_url::Item>, req: Req, uri: axum::http::Uri) -> String {
    serves_check(req, uri).await
}

async fn serves_files(_: crate::Params<serves_url::Files>, req: Req, uri: axum::http::Uri) -> String {
    serves_check(req, uri).await
}

async fn serves_cart_line(_: crate::Params<serves_url::CartLine>, req: Req, uri: axum::http::Uri) -> String {
    serves_check(req, uri).await
}

async fn serves_any(_: crate::Params<serves_catch_url::Page>, req: Req, uri: axum::http::Uri) -> String {
    serves_check(req, uri).await
}

/// `req.serves` says a path is served exactly when a GET to it reaches a page.
#[tokio::test]
async fn serves_agrees_with_the_router() {
    let router = App::new(serves_router_url::router())
        .area(|shop| serves_shop_url::area(shop).route::<ShopList>(crate::route::post(serves_page).get(serves_page)))
        .router();
    for (p, want) in [
        ("/items/7/edit", true),
        ("/items//edit", true),
        ("/items/7/", true),
        ("/x/1/y", false),
        ("/x//y", false),
        ("/x/1/z", true),
        ("/shop/cart", true),
        ("/shop/list", true),
        ("/about", true),
        ("/health/live", false),
        // The sitemap's route takes every path of its shape; one without its text is a 404.
        ("/sitemap/3.xml", true),
        ("/sitemap/about", false),
        ("/sitemap/.xml", false),
        ("/sitemap/3.xml/x", true),
    ] {
        let (status, _, body) = call_with(router.clone(), "GET", p, None, None).await;
        let routed = status == StatusCode::OK && body == "page";
        let uri = format!("/check?p={}", form_urlencoded::byte_serialize(p.as_bytes()).collect::<String>());
        let (_, _, says) = call_with(router.clone(), "GET", &uri, None, None).await;
        assert_eq!((routed, says == "local served"), (want, want), "{p:?}: {status} {body:?}, serves says {says:?}");
    }
}

/// A redirect to the return address in `?p=`, or to `/check` when it is not on this site.
async fn serves_go(req: Req, uri: axum::http::Uri) -> HtmlResult {
    let p = form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()).find(|(k, _)| k == "p").map(|(_, v)| v.into_owned());
    match p.as_deref().and_then(crate::Local::parse) {
        Some(local) => req.redirect(&local),
        None => req.redirect(&serves_url::Check),
    }
}

/// Whether the app serves the path in `?p=`: "local served", "local elsewhere" or "not local".
async fn serves_check(req: Req, uri: axum::http::Uri) -> String {
    let p = form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()).find(|(k, _)| k == "p").map(|(_, v)| v.into_owned());
    match p.as_deref().and_then(crate::route::Local::parse) {
        Some(local) if req.serves(&local) => "local served".into(),
        Some(_) => "local elsewhere".into(),
        None => "not local".into(),
    }
}

/// A return address the app serves is one of its pages, its areas' included; an API route, a
/// static file or Corten's own routes are not; and nothing that leads off the site is a `Local`.
#[tokio::test]
async fn an_app_says_whether_it_serves_a_return_address() {
    let router = App::new(serves_url::router())
        .area::<admin_url::Admin>(admin_url::area)
        .api::<ApiRoot>(|api| api.route::<ApiRates>(crate::Methods::reads(get(|| async { "rates" }))))
        .router();
    for (p, want) in [
        ("/check", "local served"),
        ("/check?x=1#top", "local served"),
        ("/items/7", "local served"),
        ("/items/anything", "local served"),
        ("/files/a/b/c.txt", "local served"),
        ("/admin", "local served"),
        ("/admin/users/3", "local served"),
        ("/items", "local elsewhere"),
        ("/items/", "local elsewhere"),
        ("/items/7/edit", "local elsewhere"),
        ("/files/", "local elsewhere"),
        ("/check/", "local elsewhere"),
        ("/api/rates", "local elsewhere"),
        ("/health/live", "local elsewhere"),
        ("/admin/users", "local elsewhere"),
        ("/market/cart", "local elsewhere"),
        ("/cart/add", "local elsewhere"),
        ("/cart/7", "local served"),
        ("/cart/%61dd", "not local"),
        ("/both", "local served"),
        ("/items/..", "not local"),
        ("/check/../admin", "not local"),
        ("//evil.example/check", "not local"),
        ("/\\evil.example", "not local"),
        ("/\t/evil.example", "not local"),
        ("https://evil.example/check", "not local"),
        ("check", "not local"),
    ] {
        let uri = format!("/check?p={}", form_urlencoded::byte_serialize(p.as_bytes()).collect::<String>());
        let (status, _, body) = call_with(router.clone(), "GET", &uri, None, None).await;
        assert_eq!((status, body.as_str()), (StatusCode::OK, want), "{p:?}");
    }
    for (p, location) in [("/items/7?tab=reviews", "/items/7?tab=reviews"), ("//evil.example", "/check")] {
        let uri = format!("/go?p={}", form_urlencoded::byte_serialize(p.as_bytes()).collect::<String>());
        let (status, headers, _) = call_with(router.clone(), "GET", &uri, None, None).await;
        assert_eq!((status, headers["location"].to_str().unwrap()), (StatusCode::SEE_OTHER, location), "{p:?}");
    }
}

/// A `Public` answer with a header of the app's keeps it, and is compressed and shared as one
/// without it; a failure does not get it.
#[tokio::test]
async fn a_public_answer_takes_a_header_and_is_still_compressed() {
    use crate::answer::{Link, XRobotsTag};
    use axum::http::HeaderValue;
    let text = "# Example\n".repeat(400);
    let body = text.clone();
    let router = App::new(Routes::from(
        Router::new()
            .route(
                "/llms.txt",
                get(move || async move {
                    Public::new(Duration::from_secs(60), body)
                        .header::<XRobotsTag>(HeaderValue::from_static("noindex, nofollow"))
                        .header::<Link>(HeaderValue::from_static("</a>; rel=\"a\""))
                        .header::<Link>(HeaderValue::from_static("</b>; rel=\"b\""))
                }),
            )
            .route(
                "/gone",
                get(|| async {
                    Public::new(Duration::from_secs(60), crate::Result::<String>::Err(crate::Error::not_found()))
                        .header::<Link>(HeaderValue::from_static("</gone>; rel=\"gone\""))
                }),
            )
            .route(
                "/old",
                get(|req: Req| async move {
                    Public::new(Duration::from_secs(60), req.moved(&serves_url::Check))
                        .header::<Link>(HeaderValue::from_static("</check>; rel=\"canonical\""))
                }),
            ),
    ))
    .router();
    let req = Request::get("/llms.txt").header("accept-encoding", "zstd, br, gzip").body(Body::empty()).unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let headers = res.headers();
    assert_eq!(headers["x-robots-tag"], "noindex, nofollow");
    assert_eq!(headers.get_all("link").iter().count(), 2, "{headers:?}");
    #[cfg(feature = "compress")]
    assert_eq!(headers["content-encoding"], "zstd", "{headers:?}");
    assert_eq!(headers["cache-control"], "public, max-age=60", "{headers:?}");
    // An error is the error page, without the answer's headers.
    let res = router.clone().oneshot(Request::get("/gone").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert!(res.headers().get("link").is_none(), "{:?}", res.headers());
    // A redirect keeps them.
    let res = router.oneshot(Request::get("/old").body(Body::empty()).unwrap()).await.unwrap();
    assert!(res.status().is_redirection(), "{}", res.status());
    assert_eq!(res.headers()["link"], "</check>; rel=\"canonical\"");
}

#[crate::routes]
mod policy_url {
    #[route("/", get = policy_page)]
    pub struct Home;
}

#[crate::routes(area = Pay)]
mod pay_url {
    #[route("/pay")]
    pub struct Pay;

    #[route("/pay/card", get = policy_page)]
    pub struct Card;
}

/// An area inside `/pay` that adds to it.
#[crate::routes(area = Charts)]
mod charts_url {
    #[route("/pay/charts")]
    pub struct Charts;

    #[route("/pay/charts/week", get = policy_page)]
    pub struct Week;
}

async fn policy_page(req: Req) -> HtmlResult {
    req.test_page("Page", html! { <p>"page"</p> })
}

fn policy_app() -> Router {
    App::new(policy_url::router())
        .policy(crate::security::Policy::new().img_src("https://img.example"))
        .area(|area| pay_url::area(area).policy(|p| p.script_src("https://js.stripe.com").frame_src("https://js.stripe.com")))
        .area(|area| charts_url::area(area).policy(|p| p.allow_style_attributes().script_src("https://js.stripe.com")))
        .router()
}

/// What a page answers: its CSP and its shell key.
async fn csp_and_shell(router: &Router, path: &str) -> (StatusCode, String, String) {
    let res = router.clone().oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
    let status = res.status();
    let csp = res.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap().to_string();
    let body = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    let shell = body.split(r#"name="corten-shell" content=""#).nth(1).map_or("", |rest| &rest[..16]).to_string();
    (status, csp, shell)
}

/// An area's policy adds to the app's on its pages only, nested areas add up, and a page whose
/// policy differs has another shell key, so a client navigation to it loads the document in full.
#[tokio::test]
async fn an_area_adds_to_the_policy_of_its_pages_only() {
    let router = policy_app();
    let (status, home, home_shell) = csp_and_shell(&router, "/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(home.contains("img-src 'self' data: https: https://img.example;") && home.contains("frame-src 'none';"), "{home}");
    assert!(!home.contains("stripe") && !home.contains("style-src-attr"), "{home}");
    let (_, card, card_shell) = csp_and_shell(&router, "/pay/card").await;
    assert!(card.contains(" https://js.stripe.com; style-src") && card.contains("frame-src https://js.stripe.com;"), "{card}");
    assert!(card.contains("https://img.example") && !card.contains("style-src-attr"), "the app's sources stay: {card}");
    let (_, week, week_shell) = csp_and_shell(&router, "/pay/charts/week").await;
    assert!(week.contains("frame-src https://js.stripe.com;") && week.ends_with("; style-src-attr 'unsafe-inline'"), "{week}");
    assert_eq!(week.matches("https://js.stripe.com").count(), 2, "a source both areas add is listed once: {week}");
    assert!(
        home_shell != card_shell && card_shell != week_shell && home_shell != week_shell,
        "{home_shell} {card_shell} {week_shell}"
    );
    // An unknown URL in the area is answered under its policy, as its pages are.
    let res = router.clone().oneshot(Request::get("/pay/nothing").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(res.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap(), card);
    // A write another site sent to the area is refused under the area's policy too.
    let cross = Request::post("/pay/card").header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
    let res = router.clone().oneshot(cross).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(res.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap(), card);
    // A page outside every area that adds keeps the shell key it had without any.
    let img = || crate::security::Policy::new().img_src("https://img.example");
    let plain = App::new(policy_url::router()).policy(img()).router();
    assert_eq!(csp_and_shell(&plain, "/").await, (StatusCode::OK, home.clone(), home_shell.clone()));
    // An area that adds only what the app has keeps the app's policy and shell key: a navigation
    // into it is not a full load.
    let same = App::new(policy_url::router())
        .policy(img())
        .area(|area| pay_url::area(area).policy(|p| p.img_src("https://img.example")))
        .router();
    assert_eq!(csp_and_shell(&same, "/pay/card").await, (StatusCode::OK, home, home_shell));
}

/// An area relaxes the opener policy on its pages only (Stripe's Connect embedded components
/// sign in through a popup that needs `unsafe-none`), and a page under another one has another
/// shell key, since a document keeps the opener policy it loaded with.
#[tokio::test]
async fn an_area_relaxes_the_opener_policy_of_its_pages_only() {
    async fn opener(router: &Router, path: &str) -> (StatusCode, String, String) {
        let res = router.clone().oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
        let opener = res.headers()["cross-origin-opener-policy"].to_str().unwrap().to_string();
        let (status, _, shell) = csp_and_shell(router, path).await;
        (status, opener, shell)
    }
    let router =
        App::new(policy_url::router()).area(|area| pay_url::area(area).policy(|p| p.allow_cross_origin_opener())).router();
    let (_, home, home_shell) = opener(&router, "/").await;
    assert_eq!(home, "same-origin");
    let (_, card, card_shell) = opener(&router, "/pay/card").await;
    assert_eq!(card, "unsafe-none");
    assert_ne!(home_shell, card_shell, "a navigation into the area loads the document in full");
    assert_eq!(csp_and_shell(&router, "/").await.1, csp_and_shell(&router, "/pay/card").await.1, "the CSP is the app's");
    // An unknown URL in the area is answered under its policy, as its pages are.
    assert_eq!(opener(&router, "/pay/nothing").await.1, "unsafe-none");
    // Only relaxing: an area's allow_popups under an app that allows any opener changes nothing,
    // and one the app already has keeps the app's shell key.
    let open = App::new(policy_url::router())
        .policy(crate::security::Policy::new().allow_cross_origin_opener())
        .area(|area| pay_url::area(area).policy(|p| p.allow_popups()))
        .router();
    let (_, open_home, open_shell) = opener(&open, "/").await;
    assert_eq!(
        (open_home.as_str(), opener(&open, "/pay/card").await),
        ("unsafe-none", (StatusCode::OK, "unsafe-none".into(), open_shell))
    );
    // Nested areas add up: one inside an area that loosens it keeps the looser value, and a
    // write another site sent there is refused under it too.
    let nested = App::new(policy_url::router())
        .area(|area| pay_url::area(area).policy(|p| p.allow_cross_origin_opener()))
        .area(|area| charts_url::area(area).policy(|p| p.allow_popups().script_src("https://js.stripe.com")))
        .router();
    assert_eq!(opener(&nested, "/pay/charts/week").await.1, "unsafe-none");
    let cross = Request::post("/pay/charts/week").header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
    let refused = nested.clone().oneshot(cross).await.unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert_eq!(refused.headers()["cross-origin-opener-policy"], "unsafe-none");
    let popups = App::new(policy_url::router())
        .policy(crate::security::Policy::new().allow_popups())
        .area(|area| pay_url::area(area).policy(|p| p.allow_popups()))
        .router();
    let (_, popups_home, popups_shell) = opener(&popups, "/").await;
    assert_eq!(popups_home, "same-origin-allow-popups");
    assert_eq!(opener(&popups, "/pay/card").await.2, popups_shell);
}

/// Work a handler finishes (`Signal::finish`) runs past the handler being dropped: its slow step
/// sees why it should stop, and what follows it (a release) runs, whether the client left, the
/// deadline passed or the server is shutting down, which waits for it.
#[tokio::test]
async fn finished_work_releases_what_it_holds_when_the_handler_is_dropped() {
    use std::{sync::Mutex, time::Duration};
    use tokio::io::AsyncWriteExt;
    let released: Arc<Mutex<Vec<(&'static str, crate::Abort)>>> = Arc::default();
    let routes = {
        let released = released.clone();
        Routes::from(Router::new().route(
            "/{name}",
            get(move |signal: crate::Signal, axum::extract::Path(name): axum::extract::Path<String>| {
                let released = released.clone();
                async move {
                    let name: &'static str = Box::leak(name.into_boxed_str());
                    signal
                        .finish(move |signal| async move {
                            // The slow call, which only the signal ends.
                            let reason = signal.aborted().await;
                            // A release that outlasts the connection: `/slow-release` takes 500 ms.
                            let release = if name == "gone" { 500 } else { 30 };
                            tokio::time::sleep(Duration::from_millis(release)).await;
                            released.lock().unwrap().push((name, reason));
                            "done"
                        })
                        .await
                }
            }),
        ))
    };
    // Long enough that only `/late` meets it on a loaded machine.
    let app = App::new(routes).deadline(Some(Duration::from_secs(1))).shutdown_grace(Duration::from_secs(2));
    let listeners = crate::app::bind(Some("127.0.0.1"), 0).await.unwrap();
    let addr = listeners[0].local_addr().unwrap();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(crate::app::run(listeners, crate::app::plain, app, async {
        let _ = stopped.await;
    }));
    let wait_for = |name: &'static str| {
        let released = released.clone();
        async move {
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if let Some((_, reason)) = released.lock().unwrap().iter().find(|(n, _)| *n == name) {
                        return *reason;
                    }
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap_or_else(|_| panic!("{name}: the release never ran"))
        }
    };

    // The client leaves mid-call.
    let mut left = tokio::net::TcpStream::connect(addr).await.unwrap();
    left.write_all(b"GET /left HTTP/1.1\r\nhost: localhost\r\n\r\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(left);
    assert_eq!(wait_for("left").await, crate::Abort::Disconnected);

    // The deadline passes: the client gets 503 and the work still releases.
    let answer = raw_get(addr, "/late", "").await;
    assert!(answer.starts_with("HTTP/1.1 503"), "{answer}");
    assert_eq!(wait_for("late").await, crate::Abort::Deadline);

    // Shutdown during two calls: one whose client is still there is told (`Shutdown`) and
    // answers; one whose client left is still releasing, and serve waits for it.
    let kept = tokio::spawn(raw_get(addr, "/kept", ""));
    let mut gone = tokio::net::TcpStream::connect(addr).await.unwrap();
    gone.write_all(b"GET /gone HTTP/1.1\r\nhost: localhost\r\n\r\n").await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(gone);
    let began = std::time::Instant::now();
    stop.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), server).await.expect("serve returned").unwrap().unwrap();
    assert!(began.elapsed() >= Duration::from_millis(400), "serve did not wait for the release: {:?}", began.elapsed());
    let done: Vec<_> = released.lock().unwrap().clone();
    // Its client left as shutdown began: either is the first reason, and either way it released.
    assert!(done.iter().any(|(name, _)| *name == "gone"), "serve returned before the work it had to finish: {done:?}");
    assert!(done.contains(&("kept", crate::Abort::Shutdown)), "{done:?}");
    assert!(kept.await.unwrap().ends_with("done"), "the client that stayed got its answer");
}

/// A panic in work a handler finishes is the handler's: the request answers 500 and the server
/// goes on.
#[tokio::test]
async fn a_panic_in_finished_work_is_the_handlers() {
    let router = App::new(Routes::from(Router::new().route(
        "/",
        get(|signal: crate::Signal| async move {
            signal.finish(|_signal| async move { panic!("the release failed") }).await;
            "never"
        }),
    )))
    .router();
    let (status, _, _) = call(router.clone(), "GET", None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let (status, _, _) = call(router, "GET", None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "the server goes on after it");
}

#[crate::routes]
mod referrer_url {
    #[route("/relayed/{policy}", get = relayed)]
    pub struct Relayed<'a> {
        pub policy: &'a str,
    }
}

/// An answer that brings a policy of its own, as a relayed service's does.
async fn relayed(Params(at): Params<referrer_url::Relayed<'_>>) -> crate::Result<crate::Relay> {
    let mut headers = axum::http::HeaderMap::new();
    // `|` between lines of their own.
    for line in at.policy.split('|') {
        headers.append(header::REFERRER_POLICY, axum::http::HeaderValue::from_str(line).unwrap());
    }
    headers.insert(header::LOCATION, axum::http::HeaderValue::from_static("https://elsewhere.example/cb?code=1"));
    Ok(crate::Relay::new(StatusCode::FOUND, headers, Body::empty()))
}

/// An area or the app makes `Referrer-Policy` stricter, never looser; an area that changes it has
/// its own shell key (a document keeps the policy it loaded with); and an answer that brings a
/// stricter one keeps it, while a looser one is replaced.
#[tokio::test]
async fn an_area_or_an_answer_makes_the_referrer_policy_stricter_only() {
    use crate::security::{Policy, Referrer};
    async fn referrer(router: &Router, path: &str) -> String {
        let res = router.clone().oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
        res.headers()[header::REFERRER_POLICY].to_str().unwrap().to_string()
    }
    let router =
        App::new(policy_url::router()).area(|area| pay_url::area(area).policy(|p| p.referrer(Referrer::NoReferrer))).router();
    assert_eq!(referrer(&router, "/").await, "strict-origin-when-cross-origin");
    assert_eq!(referrer(&router, "/pay/card").await, "no-referrer");
    assert_eq!(referrer(&router, "/pay/nothing").await, "no-referrer", "an unknown URL in the area too");
    assert_ne!(csp_and_shell(&router, "/").await.2, csp_and_shell(&router, "/pay/card").await.2);
    // An API inside the area keeps its own CSP and sends the area's referrer policy, an unknown URL
    // in it too.
    let api = App::new(policy_url::router())
        .area(|area| pay_url::area(area).policy(|p| p.referrer(Referrer::NoReferrer)))
        .api::<PayApi>(|api| api.route::<PayApiGo>(crate::Methods::reads(get(|| async { "go" }))))
        .router();
    assert_eq!(referrer(&api, "/pay/api/go").await, "no-referrer");
    assert_eq!(referrer(&api, "/pay/api/nope").await, "no-referrer");
    assert_eq!(referrer(&api, "/").await, "strict-origin-when-cross-origin");
    // Two that each send less somewhere send the least of both: strict-origin to this site and
    // nothing to others has no name, so it is nothing.
    let nested = App::new(policy_url::router())
        .policy(Policy::new().referrer(Referrer::StrictOrigin))
        .area(|area| pay_url::area(area).policy(|p| p.referrer(Referrer::SameOrigin)))
        .router();
    assert_eq!(referrer(&nested, "/").await, "strict-origin");
    assert_eq!(referrer(&nested, "/pay/card").await, "no-referrer");
    // Two policies that both send nothing are one: the area keeps the app's shell key.
    let nothing = App::new(policy_url::router())
        .policy(Policy::new().referrer(Referrer::StrictOrigin).referrer(Referrer::SameOrigin))
        .area(|area| pay_url::area(area).policy(|p| p.referrer(Referrer::NoReferrer)))
        .router();
    assert_eq!(referrer(&nothing, "/").await, "no-referrer");
    assert_eq!(csp_and_shell(&nothing, "/").await.2, csp_and_shell(&nothing, "/pay/card").await.2);
    // The same value the app has is no change, and keeps the app's shell key.
    let same = App::new(policy_url::router())
        .policy(Policy::new().referrer(Referrer::SameOrigin))
        .area(|area| pay_url::area(area).policy(|p| p.referrer(Referrer::SameOrigin)))
        .router();
    assert_eq!(csp_and_shell(&same, "/").await.2, csp_and_shell(&same, "/pay/card").await.2);
    // An answer's own policy is kept when it is stricter, and the stricter of both otherwise.
    let relay = App::new(referrer_url::router()).router();
    for (brought, sent) in [
        ("no-referrer", "no-referrer"),
        ("same-origin", "same-origin"),
        ("unsafe-url", "strict-origin-when-cross-origin"),
        ("x-unknown", "strict-origin-when-cross-origin"),
        // `origin` sends only the origin to this site too.
        ("origin", "strict-origin"),
        ("unsafe-url, strict-origin", "strict-origin"),
        // The browser skips a token it does not know and uses the last one it does.
        ("no-referrer, x-future-policy", "no-referrer"),
        // Several lines are one list.
        ("unsafe-url|no-referrer", "no-referrer"),
        ("same-origin|unsafe-url", "strict-origin-when-cross-origin"),
    ] {
        let path = referrer_url::Relayed { policy: brought }.href();
        assert_eq!(referrer(&relay, &path).await, sent, "{brought}");
    }
    let strict = App::new(referrer_url::router()).policy(Policy::new().referrer(Referrer::StrictOrigin)).router();
    let path = referrer_url::Relayed { policy: "same-origin" }.href();
    assert_eq!(referrer(&strict, &path).await, "no-referrer", "the least of the app's and the answer's");
}

#[crate::routes]
mod page_or_url {
    #[route("/runs/{id}", get = page_or_run)]
    pub struct Run<'a> {
        pub id: &'a str,
        pub said: Option<&'a str>,
    }

    #[route("/runs/{id}/download", post = page_or_download)]
    pub struct Download<'a> {
        pub id: &'a str,
    }
}

async fn page_or_run(_: Params<page_or_url::Run<'_>>) -> &'static str {
    "run"
}

/// The file for run 7, the run's page saying why for any other.
async fn page_or_download(req: Req, Params(at): Params<page_or_url::Download<'_>>) -> crate::Result<crate::PageOr<crate::Media>> {
    if at.id != "7" {
        return Ok(req.redirect(&page_or_url::Run { id: &at.id, said: Some("stale") })?.into());
    }
    Ok(crate::Media::new("text/csv", "sku,qty\n").into())
}

/// A route that sends a file answers a redirect or a page instead when it refuses, as a page is:
/// private, with the app's headers.
#[tokio::test]
async fn a_file_route_answers_a_page_when_it_refuses() {
    let router = App::new(page_or_url::router()).router();
    let (status, headers, body) = call_with(router.clone(), "POST", "/runs/7/download", None, None).await;
    assert_eq!(
        (status, headers[header::CONTENT_TYPE].to_str().unwrap(), body.as_str()),
        (StatusCode::OK, "text/csv", "sku,qty\n")
    );
    let (status, headers, _) = call_with(router, "POST", "/runs/8/download", None, None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/runs/8?said=stale");
    assert_eq!(headers[header::CACHE_CONTROL], "private, no-cache");
    assert!(headers.contains_key(header::CONTENT_SECURITY_POLICY));
}

#[derive(Default, crate::Route)]
#[route(query)]
#[serde(rename_all = "camelCase")]
pub struct KeptPaging {
    pub sort_by: Option<String>,
    pub page_no: Option<u32>,
}

#[derive(crate::Route)]
#[route("/kept")]
pub struct Kept {
    pub q: Option<String>,
    pub tags: Vec<String>,
    #[route(prefix = "dim.")]
    pub dims: std::collections::BTreeMap<String, String>,
    #[route(query)]
    pub paging: KeptPaging,
}

#[test]
fn keep_query_keeps_every_key_the_controls_do_not_name() {
    use crate::form::keep_query;
    let at = Kept {
        q: Some("a \"b\" <c>".into()),
        tags: vec!["x".into(), "y".into()],
        dims: [("color".to_string(), "red".to_string()), ("size".to_string(), "9".to_string())].into(),
        paging: KeptPaging { sort_by: Some("price".into()), page_no: Some(3) },
    };
    // A set's field, a prefixed entry and a list, each named by a control: none is kept.
    let html = keep_query(
        &at,
        html! {
            <select name="sortBy"><option>"price"</option></select>
            <input name="dim.color" value="blue">
            for tag in ["x", "z"] { <input type="checkbox" name="tags" value={tag}> }
        },
    )
    .to_html();
    assert!(!html.contains(r#"type="hidden" name="sortBy""#) && !html.contains(r#"type="hidden" name="tags""#), "{html}");
    assert!(!html.contains(r#"type="hidden" name="dim.color""#), "{html}");
    assert!(
        html.ends_with(concat!(
            r#"<input type="hidden" name="q" value="a &quot;b&quot; &lt;c&gt;">"#,
            r#"<input type="hidden" name="dim.size" value="9">"#,
            r#"<input type="hidden" name="pageNo" value="3">"#,
        )),
        "the rest, escaped, in the route's order: {html}"
    );
    // Text that only looks like a name is not one.
    let html = keep_query(&at, html! { <p>" name=\"q\""</p> }).to_html();
    assert!(html.contains(r#"type="hidden" name="q""#), "{html}");
    // A link given as `&dyn Href` (a form several pages share) works the same.
    let shared: &dyn crate::Href = &at;
    assert!(keep_query(shared, "").to_html().contains(r#"name="pageNo" value="3""#));
    // A route with no query keeps nothing.
    assert_eq!(
        keep_query(&Kept { q: None, tags: vec![], dims: Default::default(), paging: KeptPaging::default() }, "x").to_html(),
        "x"
    );
}
