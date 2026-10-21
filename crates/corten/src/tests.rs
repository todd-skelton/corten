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
}

#[derive(Template)]
#[template(source = "<p id=\"msg\">{{ text }}</p>", ext = "html")]
struct Msg {
    text: &'static str,
}

struct TestShell;
impl Shell for TestShell {
    fn render(head: &Head<'_>, main: &str) -> askama::Result<String> {
        Ok(format!("<html><head>{head}</head><body><main>{main}</main></body></html>"))
    }
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
    assert!(body.starts_with("<html><head><title>A &amp; B</title>"));
    assert!(body.contains(r#"<link rel="modulepreload" href="/static/islands/sort-table.js">"#));
    assert!(body.contains("<style>sort-table{display:contents}</style>"));
    assert!(!body.contains("islands/corten-live.js"), "built-in elements are not preloaded");
    assert!(body.contains("<p>&#60;script&#62;</p>"), "askama escapes page text");
}

#[tokio::test]
async fn navigation_returns_title_and_page_only() {
    let (_, headers, body) = call(page_router(), "GET", Some("nav")).await;
    assert_eq!(headers["corten-page"], "1");
    assert!(body.starts_with("<title>A &amp; B</title><p>"));
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
    assert!(body.contains(
        r#"<script type="importmap">{"imports":{"corten":"/corten/corten.js","corten/solid":"/corten/solid.js"}}</script>"#
    ));
    let res = routes::<()>().oneshot(Request::get(SOLID_PATH).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "text/javascript; charset=utf-8");
}
