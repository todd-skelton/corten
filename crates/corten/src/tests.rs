use super::*;
use crate::{
    page::Document,
    view::{html, Render},
};
use axum::{body::Body, http::Request, routing::get, Router};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn demo(text: &str) -> impl Render + '_ {
    html! { <p>{text}</p><sort-table><b>"x"</b></sort-table><corten-live src="/x"></corten-live> }
}

fn demo_meta() -> Meta {
    Meta::new("A & B").description("About A")
}

/// The signed-in user, as an app's context function would find it.
#[derive(Clone, Hash, PartialEq, Debug)]
struct Viewer(&'static str);

fn test_shell(doc: Document, viewer: Option<Viewer>) -> impl Render {
    let who = viewer.map_or("guest", |v| v.0);
    html! {
        <html><head>{doc.head}</head><body><header><a href="/" aria-current={doc.current("/")}>{who}</a></header><main>{doc.main}</main></body></html>
    }
}

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
        get(|req: Req| async move { req.page(demo_meta(), demo("<script>")) }).post(|req: Req| async move {
            if req.mode == Mode::Action {
                req.patch(html! { <p id="msg">"saved"</p> })
            } else {
                req.redirect("/done")
            }
        }),
    )
}

/// The app as `App` assembles it, with the test shell and a fixed viewer.
fn app(routes: Router, viewer: Option<&'static str>) -> Router {
    App::new(routes).shell(test_shell).context(move |_| async move { viewer.map(Viewer) }).router()
}

#[tokio::test]
async fn a_bare_router_renders_in_the_default_shell_without_a_policy() {
    let (status, headers, body) = call(routes(), "GET", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["vary"], "corten, accept");
    assert!(body.starts_with(r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">"#), "{body}");
    assert!(body.contains(r#"<title>A &amp; B</title><meta name="description" content="About A" data-corten-head>"#));
    assert!(body.contains(&format!(r#"<script type="module" src="{}"></script>"#, runtime_path())));
    assert!(
        !body.contains("modulepreload") && !body.contains("display:contents"),
        "a custom element that is not one of the app's islands is left alone"
    );
    assert!(body.contains("<p>&lt;script&gt;</p>"), "page text is escaped");
    assert!(!body.contains("nonce"), "no policy, no nonce");
}

#[tokio::test]
async fn navigation_returns_title_head_tags_and_page_only() {
    let (_, headers, body) = call(app(routes(), None), "GET", Some("nav")).await;
    assert_eq!(headers["corten-page"], "1");
    assert!(body.starts_with(r#"<title>A &amp; B</title><meta name="description" content="About A" data-corten-head><p>"#));
    assert!(!body.contains("<html>"));
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
#[derive(serde::Serialize)]
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
                req.screen(demo_meta(), &data, demo(data.text))
            }),
        ),
        None,
    );
    let browser = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
    let (_, headers, body) = call_with(router.clone(), "GET", "/", None, Some(browser)).await;
    assert!(headers["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(body.starts_with("<html>"));
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
    assert!(body.starts_with("<html>"));
}

#[test]
fn finds_custom_elements_once_in_order() {
    let html = r#"<div><offer-list a="1"><tr></tr></offer-list><listing-sort></listing-sort><offer-list></offer-list><corten-live></corten-live><h2-x>"#;
    assert_eq!(find_islands(html), vec!["offer-list", "listing-sort", "h2-x"]);
    assert!(find_islands("<p>a &lt;b-c&gt; d</p><br><-x></x-y><p data-x-y=\"a-b\">well-known</p>").is_empty());
    assert_eq!(find_islands("<my-big-el data-x-y=\"a-b\"></my-big-el><x--><a-1>"), vec!["my-big-el", "x--", "a-1"]);
}

#[cfg(feature = "solid")]
#[tokio::test]
async fn solid_feature_adds_the_import_map_entry_and_route() {
    let (_, _, body) = call(routes(), "GET", None).await;
    assert!(body.contains(&format!(
        r#"<script type="importmap">{{"imports":{{"corten":"{}","corten/solid":"{}"}}}}</script>"#,
        runtime_path(),
        solid_path()
    )));
    let res = crate::routes::<()>().oneshot(Request::get(solid_path()).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "text/javascript; charset=utf-8");
}

#[tokio::test]
async fn runtime_is_served_at_a_versioned_immutable_url() {
    assert!(runtime_path().starts_with(&format!("/corten/{}/", runtime_version())));
    let res = app(routes(), None).oneshot(Request::get(runtime_path()).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["cache-control"], "public, max-age=31536000, immutable");
}

#[tokio::test]
async fn the_app_puts_the_nonce_on_inline_tags_and_in_the_csp() {
    let (_, headers, body) = call(app(routes(), None), "GET", None).await;
    let csp = headers["content-security-policy"].to_str().unwrap();
    let nonce = csp.split("'nonce-").nth(1).unwrap().split('\'').next().unwrap();
    assert!(body.contains(&format!(r#"<script type="importmap" nonce="{nonce}">"#)));
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["cross-origin-opener-policy"], "same-origin");
    let (_, again, _) = call(app(routes(), None), "GET", None).await;
    assert_ne!(again["content-security-policy"], headers["content-security-policy"], "a fresh nonce per request");
    let popups = App::new(routes()).policy(security::Policy::new().allow_popups()).router();
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
    assert!(body.starts_with("<html><head><title>Something went wrong</title>"), "{body}");
    assert!(body.contains(r#"<meta name="robots" content="noindex" data-corten-head>"#));
    assert!(!body.contains("database"), "the error message stays in the log");
    let (_, headers, body) = call(app(failing(), None), "GET", Some("nav")).await;
    assert_eq!(headers["corten-page"], "1");
    assert!(body.starts_with("<title>Something went wrong</title>"));
    let (_, headers, body) = call_with(app(failing(), None), "GET", "/", None, Some("application/json")).await;
    assert_eq!(headers["content-type"], "application/json");
    assert_eq!(body, r#"{"error":{"status":500}}"#);
    let (status, _, body) = call_with(app(failing(), None), "GET", "/missing", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("<title>Not found</title>"));
}

#[tokio::test]
async fn dev_mode_shows_errors_and_reloads_pages_when_the_server_restarts() {
    let dev = || App::new(failing().route("/ok", get(|req: Req| async move { req.page("ok", html! { "ok" }) }))).dev(true);
    let (_, _, body) = call(dev().router(), "GET", None).await;
    assert!(body.contains(r#"<pre class="corten-error">database is down</pre>"#), "{body}");

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

    let prod = App::new(failing()).dev(false).router();
    let (_, _, body) = call(prod.clone(), "GET", None).await;
    assert!(!body.contains("database") && !body.contains("corten-dev"));
    let (status, _, _) = call_with(prod, "GET", "/corten/dev", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unknown_urls_and_methods_get_the_error_page_and_the_app_can_replace_it() {
    let (status, _, body) = call_with(app(routes(), Some("ada")), "GET", "/nope", None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("<title>Not found</title>") && body.contains(">ada</a>"), "{body}");
    let (status, headers, body) = call_with(app(routes(), None), "DELETE", "/", None, None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert!(headers.contains_key("allow") && body.contains("Something went wrong"));
    let custom =
        App::new(routes()).error_page(|status| (format!("Oops {}", status.as_u16()), html! { <h1>"Lost?"</h1> })).router();
    let (_, _, body) = call_with(custom, "GET", "/nope", None, None).await;
    assert!(body.contains("<title>Oops 404</title>") && body.contains("<h1>Lost?</h1>"));
}

#[tokio::test]
async fn health_reports_liveness_and_readiness() {
    let router = App::new(Router::new()).ready(|| async { false }).router();
    let get = |uri: &'static str| router.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap());
    assert_eq!(get("/health/live").await.unwrap().status(), StatusCode::OK);
    let ready = get("/health/ready").await.unwrap();
    assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(ready.headers()["cache-control"], "no-store");
}

#[tokio::test]
async fn the_shell_sees_the_context_and_its_key_follows_it() {
    let (_, headers, guest) = call(app(routes(), None), "GET", None).await;
    assert!(guest.contains(r#"<a href="/" aria-current="page">guest</a>"#), "{guest}");
    let key = meta_content(&guest, "corten-shell");
    assert_eq!(headers["corten-shell"], key, "the document and its header agree");

    let (_, _, ada) = call(app(routes(), Some("ada")), "GET", None).await;
    assert!(ada.contains(">ada</a>"));
    assert_ne!(meta_content(&ada, "corten-shell"), key, "signing in changes the key");

    let (_, nav, body) = call(app(routes(), Some("ada")), "GET", Some("nav")).await;
    assert_eq!(nav["corten-shell"], meta_content(&ada, "corten-shell"), "navigations carry the key");
    assert!(!body.contains("<header>"), "a navigation sends only the page");
}

#[tokio::test]
async fn handlers_read_the_context() {
    // A view owns what it uses (references are copied in), so read from `req` first.
    let router = Router::new().route(
        "/",
        get(|req: Req| async move {
            let viewer = req.context::<Viewer>().map(|v| v.0);
            req.page("x", html! { {viewer} })
        }),
    );
    let (_, _, body) = call(app(router, Some("ada")), "GET", Some("nav")).await;
    assert_eq!(body, "<title>x</title>ada");
}

#[test]
#[should_panic(expected = "the shell takes Option<")]
fn a_context_of_another_type_than_the_shell_takes_fails_at_startup() {
    let _ = App::new(routes()).shell(test_shell).context(|_| async { Some(1u8) }).router();
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

fn alert_form(errors: &Errors) -> impl Render + '_ {
    html! { <form id="f"><input name="email" {..errors.aria("email")}>{errors.message("email")}</form> }
}

fn invalid_routes() -> Router {
    Router::new().route(
        "/",
        get(|req: Req| async move {
            let mut errors = Errors::new();
            errors.add("email", "Enter an email address.");
            let page = || async { req.page("Alert", html! { <p>"page with errors"</p> }) };
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
        r#"<form id="f"><input name="email" aria-invalid="true" aria-describedby="email-error"><p class="field-error" id="email-error">Enter an email address.</p></form>"#
    );

    let (status, _, body) = call(app(invalid_routes(), None), "GET", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("<p>page with errors</p>") && body.contains("<html>"), "no JavaScript: the whole page");

    #[cfg(feature = "json")]
    {
        let (status, _, body) = call_with(app(invalid_routes(), None), "GET", "/", None, Some("application/json")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body, r#"{"errors":{"email":"Enter an email address."}}"#);
    }
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
    assert_eq!((SignUp::FIELDS.email, SignUp::FIELDS.password), ("email", "pw"));
    assert_eq!(
        (Shipping::FIELDS.street_line, Shipping::FIELDS.postal_code, Shipping::FIELDS.r#type),
        ("streetLine", "postalCode", "type")
    );
    let mut errors = Errors::new();
    errors.add(SignUp::FIELDS.email, "Enter an email address.");
    let f = SignUp::FIELDS;
    let html = html! { <input name={f.email} {..errors.aria(f.email)}> }.to_html();
    assert_eq!(html, r#"<input name="email" aria-invalid="true" aria-describedby="email-error">"#);
}

#[tokio::test]
async fn error_pages_see_the_context() {
    let (status, _, body) = call_with(app(failing(), Some("ada")), "GET", "/forbidden", None, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains(">ada</a>") && body.contains("Not allowed"));
}

#[tokio::test]
async fn the_pwa_links_its_manifest_and_corten_answers_offline() {
    let pwa = crate::pwa::Pwa::new(crate::pwa::Manifest::new("A", "A").theme_color("#123"));
    let router = App::new(routes()).pwa(pwa).router();
    let (_, _, body) = call(router.clone(), "GET", None).await;
    assert!(body.contains(r##"<link rel="manifest" href="/manifest.webmanifest"><meta name="theme-color" content="#123">"##));
    assert!(body.contains(r#"<meta name="corten-sw" content="/service-worker.js">"#));
    let (status, _, body) = call_with(router.clone(), "GET", "/offline", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("You are offline") && body.contains("noindex"));
    let (status, _, _) = call_with(router, "GET", "/manifest.webmanifest", None, None).await;
    assert_eq!(status, StatusCode::OK);
}
