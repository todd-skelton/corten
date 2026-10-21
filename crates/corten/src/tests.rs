use super::*;
use axum::{body::Body, http::Request, routing::get, Router};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[derive(Template)]
#[template(source = "<p>{{ text }}</p><sort-table><b>x</b></sort-table><corten-live src=\"/x\"></corten-live>", ext = "html")]
#[cfg_attr(feature = "json", derive(serde::Serialize))]
struct Demo {
    text: String,
}

impl Page for Demo {
    fn title(&self) -> String {
        "A & B".into()
    }
    fn head(&self) -> Meta {
        Meta::new().description("About A")
    }
}

#[derive(Template)]
#[template(source = "<p id=\"msg\">{{ text }}</p>", ext = "html")]
struct Msg {
    text: &'static str,
}

/// The signed-in user, as an app's auth middleware would insert it.
#[derive(Clone, Hash)]
struct Viewer(&'static str);

struct TestShell;
impl Shell for TestShell {
    type Context = Viewer;
    fn render(head: &Head<'_>, main: &str, viewer: Option<&Viewer>) -> askama::Result<String> {
        let who = viewer.map_or("guest", |v| v.0);
        let current = if head.is_current("/", false) { " aria-current=\"page\"" } else { "" };
        Ok(format!(
            "<html><head>{head}</head><body><header><a href=\"/\"{current}>{who}</a></header><main>{main}</main></body></html>"
        ))
    }
}

/// `router` with `viewer` inserted into every request, as auth middleware would.
fn signed_in(router: Router, viewer: &'static str) -> Router {
    router.layer(axum::middleware::from_fn(move |mut req: Request<Body>, next: axum::middleware::Next| async move {
        req.extensions_mut().insert(Viewer(viewer));
        next.run(req).await
    }))
}

fn meta_content<'a>(html: &'a str, name: &str) -> &'a str {
    let prefix = format!(r#"<meta name="{name}" content=""#);
    let start = html.find(&prefix).expect("meta tag") + prefix.len();
    &html[start..start + html[start..].find('"').unwrap()]
}

async fn call(router: Router, method: &str, mode: Option<&str>) -> (StatusCode, axum::http::HeaderMap, String) {
    call_with(router, method, mode, None).await
}

async fn call_with(
    router: Router,
    method: &str,
    mode: Option<&str>,
    accept: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut req = Request::builder().method(method).uri("/");
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

fn page_router() -> Router {
    Router::new().route(
        "/",
        get(|req: Req<TestShell>| async move { req.page(&Demo { text: "<script>".into() }) }).post(
            |req: Req<TestShell>| async move {
                if req.mode == Mode::Action {
                    req.patch(&Msg { text: "saved" })
                } else {
                    req.redirect("/done")
                }
            },
        ),
    )
}

#[tokio::test]
async fn document_wraps_page_in_shell_with_island_preloads() {
    let (status, headers, body) = call(page_router(), "GET", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["vary"], "corten, accept");
    assert!(
        body.starts_with(r#"<html><head><title>A &amp; B</title><meta name="description" content="About A" data-corten-head>"#)
    );
    assert!(body.contains(&format!(r#"<script type="module" src="{}"></script>"#, runtime_path())));
    assert!(body.contains(r#"<link rel="modulepreload" href="/static/islands/sort-table.js">"#));
    assert!(body.contains("<style>sort-table{display:contents}</style>"));
    assert!(!body.contains("islands/corten-live.js"), "built-in elements are not preloaded");
    assert!(body.contains("<p>&#60;script&#62;</p>"), "askama escapes page text");
    assert!(!body.contains("nonce"), "no policy, no nonce");
}

#[tokio::test]
async fn navigation_returns_title_head_tags_and_page_only() {
    let (_, headers, body) = call(page_router(), "GET", Some("nav")).await;
    assert_eq!(headers["corten-page"], "1");
    assert!(body.starts_with(r#"<title>A &amp; B</title><meta name="description" content="About A" data-corten-head><p>"#));
    assert!(!body.contains("<html>"));
}

#[tokio::test]
async fn action_returns_patch_and_plain_post_redirects() {
    let (_, headers, body) = call(page_router(), "POST", Some("action")).await;
    assert_eq!(headers["corten-patch"], "1");
    assert_eq!(body, r#"<p id="msg">saved</p>"#);
    let (status, headers, _) = call(page_router(), "POST", None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers["location"], "/done");
}

#[cfg(feature = "json")]
#[tokio::test]
async fn screen_is_html_for_browsers_and_json_for_data_clients() {
    let router = Router::new().route("/", get(|req: Req<TestShell>| async move { req.screen(&Demo { text: "<b>".into() }) }));
    let browser = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
    let (_, headers, body) = call_with(router.clone(), "GET", None, Some(browser)).await;
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(body.starts_with("<html>"));
    let (_, headers, body) = call_with(router.clone(), "GET", Some("nav"), Some("application/json")).await;
    assert_eq!(headers["corten-page"], "1", "the runtime's mode header wins over Accept");
    assert!(body.starts_with("<title>"));
    let (status, headers, body) = call_with(router, "GET", None, Some("application/json; charset=utf-8")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(headers["vary"], "corten, accept");
    assert_eq!(body, r#"{"title":"A & B","data":{"text":"<b>"}}"#);
}

#[tokio::test]
async fn page_answers_data_requests_with_the_document() {
    let (_, headers, body) = call_with(page_router(), "GET", None, Some("application/json")).await;
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(body.starts_with("<html>"));
}

#[test]
fn finds_custom_elements_once_in_order() {
    let html = r#"<div><offer-list a="1"><tr></tr></offer-list><listing-sort></listing-sort><offer-list></offer-list><corten-live></corten-live><h2-x>"#;
    assert_eq!(find_islands(html), vec!["offer-list", "listing-sort", "h2-x"]);
    assert!(find_islands("<p>a &lt;b-c&gt; d</p><br><-x>").is_empty());
}

#[cfg(feature = "json")]
#[test]
fn props_script_cannot_be_closed_by_a_value() {
    let value = serde_json::json!({ "title": "</script><script>alert(1)</script>", "n": 1 });
    let tag = Props(&value).to_string();
    assert_eq!(
        tag,
        r#"<script type="application/json" data-props>{"n":1,"title":"\u003c/script>\u003cscript>alert(1)\u003c/script>"}</script>"#
    );
    let json = tag.strip_prefix(r#"<script type="application/json" data-props>"#).unwrap().strip_suffix("</script>").unwrap();
    assert_eq!(serde_json::from_str::<serde_json::Value>(json).unwrap(), value, "JSON.parse gets the original value back");
}

#[cfg(feature = "solid")]
#[tokio::test]
async fn solid_feature_adds_the_import_map_entry_and_route() {
    let (_, _, body) = call(page_router(), "GET", None).await;
    assert!(body.contains(&format!(
        r#"<script type="importmap">{{"imports":{{"corten":"{}","corten/solid":"{}"}}}}</script>"#,
        runtime_path(),
        solid_path()
    )));
    let res = routes::<()>().oneshot(Request::get(solid_path()).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "text/javascript; charset=utf-8");
}

#[tokio::test]
async fn runtime_is_served_at_a_versioned_immutable_url() {
    assert!(runtime_path().starts_with(&format!("/corten/{}/", runtime_version())));
    let res = routes::<()>().oneshot(Request::get(runtime_path()).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["cache-control"], "public, max-age=31536000, immutable");
}

#[tokio::test]
async fn policy_puts_the_nonce_on_inline_tags_and_in_the_csp() {
    let router = security::Policy::new().apply(page_router());
    let (_, headers, body) = call(router.clone(), "GET", None).await;
    let csp = headers["content-security-policy"].to_str().unwrap();
    let nonce = csp.split("'nonce-").nth(1).unwrap().split('\'').next().unwrap();
    assert!(body.contains(&format!(r#"<script type="importmap" nonce="{nonce}">"#)));
    assert!(body.contains(&format!(r#"<style nonce="{nonce}">sort-table"#)));
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["cross-origin-opener-policy"], "same-origin");
    let (_, again, _) = call(router, "GET", None).await;
    assert_ne!(again["content-security-policy"], headers["content-security-policy"], "a fresh nonce per request");
    let (_, popups, _) = call(security::Policy::new().allow_popups().apply(page_router()), "GET", None).await;
    assert_eq!(popups["cross-origin-opener-policy"], "same-origin-allow-popups");
}

#[tokio::test]
async fn policy_refuses_cross_site_writes() {
    let router = security::Policy::new().apply(page_router());
    let req = |site: &'static str| {
        Request::post("/").header("sec-fetch-site", site).header("corten", "action").body(Body::empty()).unwrap()
    };
    assert_eq!(router.clone().oneshot(req("cross-site")).await.unwrap().status(), StatusCode::FORBIDDEN);
    assert_eq!(router.oneshot(req("same-origin")).await.unwrap().status(), StatusCode::OK);
}

fn failing_router() -> Router {
    let router = Router::new()
        .route("/", get(|| async { Err::<Response, _>(Error::new("database is down")) }))
        .route("/missing", get(|| async { Err::<Response, _>(Error::status(StatusCode::NOT_FOUND)) }));
    error_pages::<TestShell, _>(router)
}

#[tokio::test]
async fn errors_render_in_the_shell_for_each_mode_without_leaking_the_message() {
    let (status, headers, body) = call(failing_router(), "GET", None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(body.starts_with("<html><head><title>Something went wrong</title>"));
    assert!(body.contains(r#"<meta name="robots" content="noindex" data-corten-head>"#));
    assert!(!body.contains("database"), "the error message stays in the log");
    let (_, headers, body) = call(failing_router(), "GET", Some("nav")).await;
    assert_eq!(headers["corten-page"], "1");
    assert!(body.starts_with("<title>Something went wrong</title>"));
    let (_, headers, body) = call_with(failing_router(), "GET", None, Some("application/json")).await;
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(body, r#"{"error":{"status":500}}"#);
    let res = failing_router().oneshot(Request::get("/missing").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    assert!(String::from_utf8_lossy(&body).contains("<title>Not found</title>"));
}

#[tokio::test]
async fn health_reports_liveness_and_readiness() {
    let router: Router = health(|| async { false });
    let get = |uri: &'static str| router.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap());
    assert_eq!(get("/health/live").await.unwrap().status(), StatusCode::OK);
    let ready = get("/health/ready").await.unwrap();
    assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(ready.headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn the_shell_sees_the_context_and_its_key_follows_it() {
    let (_, headers, guest) = call(page_router(), "GET", None).await;
    assert!(guest.contains(r#"<a href="/" aria-current="page">guest</a>"#));
    let key = meta_content(&guest, "corten-shell");
    assert_eq!(headers["corten-shell"], key, "the document and its header agree");

    let (_, _, ada) = call(signed_in(page_router(), "ada"), "GET", None).await;
    assert!(ada.contains(">ada</a>"));
    assert_ne!(meta_content(&ada, "corten-shell"), key, "signing in changes the key");

    let (_, nav, body) = call(signed_in(page_router(), "ada"), "GET", Some("nav")).await;
    assert_eq!(nav["corten-shell"], meta_content(&ada, "corten-shell"), "navigations carry the key");
    assert!(!body.contains("<header>"), "a navigation sends only the page");
}

#[test]
fn current_links_match_the_page_or_its_section() {
    assert!(is_current("/sell", "/sell", false));
    assert!(is_current("/sell", "/sell?tab=2", false));
    assert!(!is_current("/sell/listings", "/sell", false));
    assert!(is_current("/sell/listings", "/sell", true));
    assert!(is_current("/sell/listings", "/sell/", true));
    assert!(!is_current("/seller", "/sell", true));
}

#[derive(Template)]
#[template(source = r#"<form id="f"><input name="email"{{ errors.aria("email")|safe }}></form>"#, ext = "html")]
struct AlertForm<'a> {
    errors: &'a Errors,
}

fn invalid_router() -> Router {
    Router::new().route(
        "/",
        get(|req: Req<TestShell>| async move {
            let mut errors = Errors::new();
            errors.add("email", "Enter an email address.");
            req.invalid(&errors, &AlertForm { errors: &errors }, || async { Ok(Demo { text: "page with errors".into() }) }).await
        }),
    )
}

#[tokio::test]
async fn invalid_input_answers_422_in_each_mode() {
    let (status, headers, body) = call(invalid_router(), "GET", Some("action")).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(headers["corten-patch"], "1");
    assert_eq!(body, r#"<form id="f"><input name="email" aria-invalid="true" aria-describedby="email-error"></form>"#);

    let (status, _, body) = call(invalid_router(), "GET", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("<p>page with errors</p>") && body.contains("<html>"), "no JavaScript: the whole page");

    #[cfg(feature = "json")]
    {
        let (status, _, body) = call_with(invalid_router(), "GET", None, Some("application/json")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body, r#"{"errors":{"email":"Enter an email address."}}"#);
    }
}

#[tokio::test]
async fn error_pages_see_the_context() {
    let router = signed_in(
        error_pages::<TestShell, _>(
            Router::new().route("/", get(|| async { Err::<Response, _>(Error::status(StatusCode::FORBIDDEN)) })),
        ),
        "ada",
    );
    let (status, _, body) = call(router, "GET", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains(">ada</a>") && body.contains("Not allowed"));
}
