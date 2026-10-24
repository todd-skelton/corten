use super::*;
use crate::{
    page::Document,
    view::{html, Render},
};
use axum::{body::Body, http::Request, routing::get, Router};
use http_body_util::BodyExt;
use tower::ServiceExt;

fn demo(text: &str) -> impl Render + '_ {
    html! {
        <p>{text}</p>
        <sort-table><b>"x"</b></sort-table>
        <corten-live src="/x"></corten-live>
    }
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
        <html>
            <head>{doc.head}</head>
            <body>
                <header><a href={Root} aria-current={doc.current(&Root)}>{who}</a></header>
                <main>{doc.main}</main>
            </body>
        </html>
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
        get(|req: Req| async move { req.page(demo_meta(), demo("<script>")) }).post(|req: Req| async move {
            if req.mode == Mode::Action {
                req.patch(html! { <p id="msg">"saved"</p> })
            } else {
                req.redirect(&Done)
            }
        }),
    )
}

/// The app as `App` assembles it, with the test shell and a fixed viewer.
fn app(routes: Router, viewer: Option<&'static str>) -> Router {
    App::new(routes).shell(test_shell).context(move |_| async move { viewer.map(Viewer) }).router()
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
async fn a_bare_router_renders_in_the_default_shell_without_a_policy() {
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

#[tokio::test]
async fn runtime_is_served_at_a_versioned_immutable_url() {
    assert!(runtime_path().starts_with(&format!("/corten/{}/", runtime_version())));
    let res = app(routes(), None).oneshot(Request::get(runtime_path()).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["cache-control"], "public, max-age=31536000, immutable");
}

/// A shell with an inline script of its own, which takes the request's nonce.
fn inline_shell(doc: Document) -> impl Render {
    html! {
        <html>
            <head>
                {doc.head}
                <script nonce={doc.nonce()}>"1"</script>
            </head>
            <body><main>{doc.main}</main></body>
        </html>
    }
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

    let inline = || App::new(routes()).shell(inline_shell).router();
    let (_, headers, body) = call(inline(), "GET", None).await;
    let csp = headers["content-security-policy"].to_str().unwrap();
    let nonce = csp.split("'nonce-").nth(1).unwrap().split('\'').next().unwrap();
    assert!(body.contains(&format!(r#"<script nonce="{nonce}">1</script>"#)), "{body}");
    let (_, again, _) = call(inline(), "GET", None).await;
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
    assert!(body.starts_with("<html><head><title>") && !body.contains("parse"), "{body}");
    let (_, headers, body) = call_with(app(routes(), None), "GET", "/n/abc", Some("nav"), None).await;
    assert_eq!(headers["corten-page"], "1", "a navigation swaps the error page in: {body}");
    let (_, _, body) = call_with(App::new(routes()).dev(true).router(), "GET", "/n/abc", None, html).await;
    assert!(body.contains(r#"<pre class="corten-error">Invalid URL"#), "{body}");
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
    let f = SignUp::FIELDS;
    html! { <form id="f"><input name={f.email} {..errors.aria(f.email)}>{errors.message(f.email)}</form> }
}

fn invalid_routes() -> Router {
    Router::new().route(
        "/",
        get(|req: Req| async move {
            let mut errors = Errors::new();
            errors.add(SignUp::FIELDS.email, "Enter an email address.");
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
        assert_eq!(
            body,
            r#"{"error":{"status":422,"message":"email: Enter an email address.","fields":{"email":"Enter an email address."}}}"#
        );
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
    assert_eq!((SignUp::FIELDS.email.as_str(), SignUp::FIELDS.password.as_str()), ("email", "pw"));
    let f = Shipping::FIELDS;
    assert_eq!((f.street_line.as_str(), f.postal_code.as_str(), f.r#type.as_str()), ("streetLine", "postalCode", "type"));
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

// ---- App::api: routes that answer only JSON ----

#[derive(crate::Route)]
#[route("/api/items/{id}")]
struct ApiItem<'a> {
    id: &'a str,
}

/// A page, and API routes for each way an API request can fail.
fn api_app(api: impl FnOnce(Router) -> crate::Api) -> Router {
    let routes = Router::new()
        .route(ApiItem::PATH, get(|axum::extract::Path(id): axum::extract::Path<u32>| async move { format!("item {id}") }))
        .route("/api/missing", get(|| async { Err::<Response, _>(Error::new("no item 7").with_status(StatusCode::NOT_FOUND)) }))
        .route("/api/down", get(|| async { Err::<Response, _>(Error::new("database password wrong")) }))
        .route(
            "/api/teapot",
            get(|| async { (StatusCode::IM_A_TEAPOT, [("content-type", "application/json")], r#"{"mine":1}"#) }),
        )
        .route("/api/slow", get(std::future::pending::<&'static str>))
        .route("/api/post", axum::routing::post(|| async { "posted" }));
    App::new(Router::new().route("/", get(|req: Req| async move { req.page("Home", html! { <p>"home"</p> }) })))
        .shell(test_shell)
        .context(|_| async { Some(Viewer("ada")) })
        .deadline(std::time::Duration::from_millis(50))
        .api("/api", api(routes))
        .router()
}

/// What most HTTP clients send: `*/*`, or no `Accept` at all, or a browser's own.
const ACCEPTS: [Option<&str>; 4] = [Some("*/*"), None, Some("text/html,application/xhtml+xml"), Some("application/json")];

#[tokio::test]
async fn api_routes_answer_every_failure_as_json_whatever_the_client_accepts() {
    let router = api_app(|r| r.into());
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
    let router = api_app(|r| r.into());
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
    let (_, headers, body) = call_with(App::new(routes).router(), "GET", "/raw", None, None).await;
    let nonce = body.split(r#"nonce=""#).nth(1).unwrap().split('"').next().unwrap();
    assert!(headers["content-security-policy"].to_str().unwrap().contains(&format!("'nonce-{nonce}'")));
}

#[tokio::test]
async fn the_context_runs_for_pages_and_only_for_api_routes_that_ask() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let counting = |api: crate::Api| {
        let calls = calls.clone();
        App::new(Router::new().route("/", get(|| async { "page" })))
            .context(move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Some(Viewer("ada")) }
            })
            .api("/api", api)
            .router()
    };
    let who = || {
        Router::new()
            .route("/api/who", get(|ctx: Option<axum::Extension<Viewer>>| async move { ctx.map_or("nobody", |v| v.0 .0) }))
    };
    let router = counting(who().into());
    let (_, _, body) = call_with(router.clone(), "GET", "/api/who", None, None).await;
    assert_eq!((body.as_str(), calls.load(Ordering::SeqCst)), ("nobody", 0), "an API route authenticates itself");
    for own in [runtime_path(), "/health/live"] {
        call_with(router.clone(), "GET", own, None, None).await;
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0, "Corten's own routes skip the context");
    call_with(router, "GET", "/", None, None).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let (_, _, body) = call_with(counting(crate::Api::new(who()).context()), "GET", "/api/who", None, None).await;
    assert_eq!((body.as_str(), calls.load(Ordering::SeqCst)), ("ada", 2), "unless it asks for it");
}

#[tokio::test]
async fn an_allowed_origin_may_call_the_api_from_the_browser_but_not_post_the_pages() {
    let router = api_app(|r| crate::Api::new(r).allow_origin("https://partner.example/"));
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
    let routes = Router::new().route("/items", get(|| async { "items" }));
    let router = App::new(Router::new()).api("/", routes).router();
    let (status, headers, body) = call_with(router.clone(), "GET", "/nope", None, Some("text/html")).await;
    assert_eq!((status, headers["content-type"].to_str().unwrap()), (StatusCode::NOT_FOUND, "application/json"));
    assert_eq!(body, r#"{"error":{"status":404,"message":"Not Found"}}"#);
    let (status, _, body) = call_with(router, "GET", "/health/ready", None, None).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "ok"), "Corten's own routes keep their behavior");
}

#[tokio::test]
async fn under_corten_dev_an_api_route_outside_its_prefix_says_so() {
    let routes = Router::new().route("/items", get(|| async { "items" }));
    let dev = App::new(Router::new()).api("/api", routes).dev(true).router();
    let (status, _, body) = call_with(dev, "GET", "/items", None, Some("text/html")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(body.contains("/items is routed by App::api(&quot;/api&quot;, ..) but does not start with"), "{body}");
}

#[cfg(feature = "json")]
#[tokio::test]
async fn validation_errors_are_the_json_error_with_fields() {
    let routes = Router::new().route(
        "/api/sign-up",
        axum::routing::post(|| async {
            let mut errors = Errors::new();
            errors.add(SignUp::FIELDS.email, "Enter an email address.");
            Err::<&str, _>(errors)
        }),
    );
    let router = App::new(Router::new()).api("/api", routes).router();
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
    let routes = Router::new()
        .route("/api/items", axum::routing::post(|axum::Json(v): axum::Json<serde_json::Value>| async move { v.to_string() }));
    let router = App::new(Router::new()).api("/api", routes).router();
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
        .route("/", get(move |req: Req| async move { Ok::<_, Error>((Cache::public(minute), req.page("Home", home())?)) }))
        .route(
            "/cookie",
            get(move |req: Req| async move {
                Ok::<_, Error>((Cache::public(minute), [(header::SET_COOKIE, "seen=1")], req.page("Home", home())?))
            }),
        )
        .route(
            "/statement",
            get(move |req: Req| async move { Ok::<_, Error>((Cache::no_store(), req.page("Statement", home())?)) }),
        )
        .route("/plain", get(|req: Req| async move { req.page("Plain", html! { <p>"plain"</p> }) }))
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
    let signed_in = app(cached_routes(), Some("ada"));
    assert_eq!(cache_control(&signed_in, "/", &[]).await, "private, no-cache", "the context found someone");
    let inline = App::new(cached_routes()).shell(inline_shell).router();
    assert_eq!(cache_control(&inline, "/", &[]).await, "private, no-cache", "the shell took the nonce");

    let minute = std::time::Duration::from_secs(60);
    let api = Router::new().route("/api/rates", get(move || async move { (Cache::public(minute), "rates") }));
    // The app has a context, but this mount does not run it.
    let api = App::new(Router::new()).context(|_| async { Some(Viewer("ada")) }).api("/api", api).router();
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

// ---- App::guard: routes only some callers may use ----

#[derive(crate::Route)]
#[route("/sign-in")]
struct SignIn;

/// Pages and an API under guards, the viewer named by the `viewer` header, and a count of the
/// handlers that ran.
fn guarded_app(dev: bool) -> (Router, Arc<std::sync::atomic::AtomicUsize>) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let ran = Arc::new(AtomicUsize::new(0));
    let count = |ran: &Arc<AtomicUsize>| {
        let ran = ran.clone();
        move |req: Req| {
            ran.fetch_add(1, Ordering::SeqCst);
            async move { req.page("Admin", html! { <p>"admin"</p> }) }
        }
    };
    let form = |ran: &Arc<AtomicUsize>| {
        let ran = ran.clone();
        move |crate::Form(_): crate::Form<std::collections::HashMap<String, String>>| {
            ran.fetch_add(1, Ordering::SeqCst);
            async { "saved" }
        }
    };
    let pages = Router::new()
        .route("/", get(|req: Req| async move { req.page("Home", html! { <p>"home"</p> }) }))
        .route("/admin", get(count(&ran)).post(form(&ran)))
        .route("/admin/billing", get(count(&ran)))
        .route("/administrators", get(count(&ran)));
    let api = Router::new().route("/api/admin/stats", get(count(&ran)));
    let viewer = |headers: &HeaderMap| headers.get("viewer").and_then(|v| v.to_str().ok()).map(|v| Viewer(v.to_owned().leak()));
    let router = App::new(pages)
        .shell(test_shell)
        .dev(dev)
        .context(move |headers| {
            let viewer = viewer(headers);
            async move { viewer }
        })
        .api("/api", Router::new().merge(api))
        .guard("/admin", |viewer: Option<&Viewer>, _| match viewer.map(|v| v.0) {
            None => Access::sign_in(&SignIn),
            Some("ada" | "bob") => Access::ALLOW,
            Some("down") => Access::refuse(Error::status(StatusCode::BAD_GATEWAY)),
            Some(_) => Access::refuse(Error::new("not staff").with_status(StatusCode::FORBIDDEN)),
        })
        .guard("/admin/billing", |viewer: Option<&Viewer>, req| match viewer.map(|v| v.0) {
            Some("ada") => Access::ALLOW,
            _ => Access::sign_in(&format!("/sign-in?next={}", req.uri().path()).parse::<axum::http::Uri>().unwrap()),
        })
        .guard("/api/admin", |viewer: Option<&Viewer>, _| match viewer {
            Some(_) => Access::ALLOW,
            None => Access::sign_in(&SignIn),
        })
        .router();
    (router, ran)
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
async fn a_guard_sends_someone_signed_out_to_sign_in_in_their_mode() {
    let (router, ran) = guarded_app(false);
    for (method, uri, headers) in [
        ("GET", "/admin", &[][..]),
        ("GET", "/admin", &[("corten", "nav")][..]),
        ("POST", "/admin", &[][..]),
        ("GET", "/admin/nope", &[][..]),
        ("DELETE", "/admin", &[][..]),
    ] {
        let (status, res, _) = guarded(&router, method, uri, None, headers).await;
        assert_eq!(
            (status, &res["location"]),
            (StatusCode::SEE_OTHER, &"/sign-in".parse().unwrap()),
            "{method} {uri} {headers:?}"
        );
        assert_eq!(res["cache-control"], "private, no-cache");
    }
    let (status, res, _) = guarded(&router, "POST", "/admin", None, &[("corten", "action")]).await;
    assert_eq!((status, &res["corten-location"]), (StatusCode::NO_CONTENT, &"/sign-in".parse().unwrap()));
    let unauthorized = r#"{"error":{"status":401,"message":"Unauthorized"}}"#;
    let (status, res, body) = guarded(&router, "GET", "/admin", None, &[("accept", "application/json")]).await;
    assert_eq!((status, &body[..]), (StatusCode::UNAUTHORIZED, unauthorized), "a data client");
    assert_eq!(res["content-type"], "application/json");
    for accept in ACCEPTS {
        let headers: &[(&str, &str)] = match accept {
            Some(accept) => &[("accept", accept)],
            None => &[],
        };
        let (status, _, body) = guarded(&router, "GET", "/api/admin/stats", None, headers).await;
        assert_eq!((status, &body[..]), (StatusCode::UNAUTHORIZED, unauthorized), "an API route, {accept:?}");
    }
    assert_eq!(ran.load(std::sync::atomic::Ordering::SeqCst), 0, "no handler ran, and no form was read");
    let (status, _, _) = guarded(&router, "GET", "/administrators", None, &[]).await;
    assert_eq!(status, StatusCode::OK, "a guard holds whole path segments");
    let (status, _, _) = guarded(&router, "GET", "/", None, &[]).await;
    assert_eq!(status, StatusCode::OK, "outside the guard");
}

#[tokio::test]
async fn a_guard_refuses_someone_signed_in_with_the_error_in_their_mode() {
    let (router, ran) = guarded_app(false);
    let (status, res, body) = guarded(&router, "GET", "/admin", Some("eve"), &[("accept", "text/html")]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(res["content-type"].to_str().unwrap().starts_with("text/html"));
    assert!(body.contains("<a href=\"/\">eve</a>"), "the error page is in the shell, which sees the viewer: {body}");
    assert!(!body.contains("not staff"), "the message stays in the log");
    let (status, res, _) = guarded(&router, "POST", "/admin", Some("eve"), &[("corten", "action")]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(res.contains_key("corten-page"), "a form patch gets the error page to swap in");
    let (status, _, body) = guarded(&router, "GET", "/admin", Some("eve"), &[("accept", "application/json")]).await;
    assert_eq!((status, &body[..]), (StatusCode::FORBIDDEN, r#"{"error":{"status":403,"message":"not staff"}}"#));
    let (status, _, _) = guarded(&router, "GET", "/admin", Some("down"), &[]).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "who they are cannot be known now");
    assert_eq!(ran.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn guards_nest_and_run_the_context_for_api_routes() {
    let (router, ran) = guarded_app(false);
    let (status, _, body) = guarded(&router, "GET", "/admin", Some("bob"), &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<p>admin</p>"));
    let (status, _, body) = guarded(&router, "POST", "/admin", Some("bob"), &[]).await;
    assert_eq!((status, &body[..]), (StatusCode::OK, "saved"));
    let (status, _, _) = guarded(&router, "GET", "/admin/billing", Some("ada"), &[]).await;
    assert_eq!(status, StatusCode::OK);
    let (status, res, _) = guarded(&router, "GET", "/admin/billing?x=1", Some("bob"), &[]).await;
    assert_eq!(
        (status, &res["location"]),
        (StatusCode::SEE_OTHER, &"/sign-in?next=/admin/billing".parse().unwrap()),
        "the inner guard, sending the caller back after signing in"
    );
    let (status, _, _) = guarded(&router, "GET", "/admin/billing", Some("eve"), &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the outer guard answers first");
    let (status, _, body) = guarded(&router, "GET", "/api/admin/stats", Some("eve"), &[]).await;
    assert_eq!(status, StatusCode::OK, "the API mount has no Api::context, but its guard reads the context: {body}");
    assert_eq!(ran.load(std::sync::atomic::Ordering::SeqCst), 4);
}

#[tokio::test]
async fn the_outer_guard_answers_first_whatever_the_order_they_are_given_in() {
    let routes = Router::new().route("/admin/billing", get(|| async { "billing" }));
    let router = App::new(routes)
        .context(|_| async { Some(Viewer("eve")) })
        .guard("/admin/billing", |_: Option<&Viewer>, _| Access::sign_in(&SignIn))
        .guard("/admin", |_: Option<&Viewer>, _| Access::refuse(Error::status(StatusCode::FORBIDDEN)))
        .router();
    let (status, _, _) = guarded(&router, "GET", "/admin/billing", None, &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn under_dev_a_refusal_names_its_guard() {
    let (router, _) = guarded_app(true);
    let (_, _, body) = guarded(&router, "GET", "/admin", Some("eve"), &[("accept", "text/html")]).await;
    assert!(body.contains(r#"not staff (refused by App::guard(&quot;/admin&quot;))"#), "{body}");
    let (_, _, body) = guarded(&router, "GET", "/api/admin/stats", None, &[]).await;
    assert_eq!(body, r#"{"error":{"status":401,"message":"Unauthorized (refused by App::guard(\"/api/admin\"))"}}"#);
}

#[test]
#[should_panic(expected = "App::guard(\"/admin\") takes Option<&")]
fn a_guard_of_another_type_than_the_context_fails_at_startup() {
    let _ = App::new(Router::new())
        .context(|_| async { Some(Viewer("ada")) })
        .guard("/admin", |_: Option<&String>, _| Access::ALLOW)
        .router();
}

#[test]
#[should_panic(expected = "the app has no App::context")]
fn a_guard_without_a_context_fails_at_startup() {
    let _ = App::new(Router::new()).guard("/admin", |_: Option<&Viewer>, _| Access::ALLOW).router();
}

#[test]
#[should_panic(expected = "a guard prefix is \"/\" or a path like")]
fn a_guard_prefix_is_a_plain_path() {
    let _ = App::new(Router::new()).guard("/admin/", |_: Option<&Viewer>, _| Access::ALLOW);
}

/// Reviews anyone may read and only a signed-in editor may write, at one URL.
fn reviews_app() -> Router {
    let routes = Router::new().route(
        "/reviews/{id}",
        get(|req: Req| async move { req.page("Review", html! { <p>"review"</p> }) })
            .post(|| async { "saved" })
            .delete(|| async { "deleted" }),
    );
    App::new(routes)
        .shell(test_shell)
        .context(|headers: &HeaderMap| {
            let viewer = headers.get("viewer").and_then(|v| v.to_str().ok()).map(|v| Viewer(v.to_owned().leak()));
            async move { viewer }
        })
        .guard("/reviews", |viewer: Option<&Viewer>, req| match viewer.map(|v| v.0) {
            _ if !req.writes() => Access::ALLOW,
            None => Access::sign_in(&SignIn),
            Some("editor") => Access::ALLOW,
            Some(_) => Access::refuse(Error::status(StatusCode::FORBIDDEN)),
        })
        .router()
}

#[tokio::test]
async fn a_guard_can_open_reads_and_guard_writes_at_one_url() {
    let router = reviews_app();
    for method in ["GET", "HEAD"] {
        let (status, _, _) = guarded(&router, method, "/reviews/1", None, &[]).await;
        assert_eq!(status, StatusCode::OK, "{method} is a read");
    }
    for method in ["POST", "DELETE"] {
        let (status, res, _) = guarded(&router, method, "/reviews/1", None, &[]).await;
        assert_eq!((status, &res["location"]), (StatusCode::SEE_OTHER, &"/sign-in".parse().unwrap()), "{method} writes");
        let (status, _, _) = guarded(&router, method, "/reviews/1", Some("reader"), &[]).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        let (status, _, _) = guarded(&router, method, "/reviews/1", Some("editor"), &[]).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, _, _) = guarded(&router, "PATCH", "/reviews/1", Some("reader"), &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a method nobody listed is a write, refused before the 405");
}

#[derive(crate::Route)]
#[route("/hooks/{name}")]
struct Hook<'a> {
    name: &'a str,
}

/// An app private everywhere: sign in, a webhook and Corten's own routes stay open.
fn private_app(open_sign_in: bool) -> Router {
    let page = |text: &'static str| get(move |req: Req| async move { req.page(text, html! { <p>{text}</p> }) });
    let routes = Router::new()
        .route("/", page("home"))
        .route(SignIn::PATH, page("sign in").post(|| async { "signed in" }))
        .route(Hook::PATH, axum::routing::post(|| async { "hooked" }));
    let mut app = App::new(routes)
        .shell(test_shell)
        .pwa(crate::pwa::Pwa::new(crate::pwa::Manifest::new("A", "A")))
        .context(|headers: &HeaderMap| {
            let viewer = headers.get("viewer").and_then(|v| v.to_str().ok()).map(|v| Viewer(v.to_owned().leak()));
            async move { viewer }
        })
        .api("/hooks", Router::new())
        .guard("/", |viewer: Option<&Viewer>, _| match viewer {
            Some(_) => Access::ALLOW,
            None => Access::sign_in(&SignIn),
        })
        .unguarded::<Hook>();
    if open_sign_in {
        app = app.unguarded::<SignIn>();
    }
    app.router()
}

#[tokio::test]
async fn a_guard_on_the_whole_app_leaves_its_unguarded_routes_and_cortens_own_open() {
    let router = private_app(true);
    let (status, res, _) = guarded(&router, "GET", "/", None, &[]).await;
    assert_eq!((status, &res["location"]), (StatusCode::SEE_OTHER, &"/sign-in".parse().unwrap()));
    let (status, res, _) = guarded(&router, "GET", "/nope", None, &[]).await;
    assert_eq!((status, &res["location"]), (StatusCode::SEE_OTHER, &"/sign-in".parse().unwrap()), "deny by default");
    for (method, uri) in [("GET", "/sign-in"), ("POST", "/sign-in"), ("POST", "/hooks/stripe")] {
        let (status, _, _) = guarded(&router, method, uri, None, &[]).await;
        assert_eq!(status, StatusCode::OK, "{method} {uri} is unguarded");
    }
    let (status, _, _) = guarded(&router, "POST", "/hooks", None, &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "only the route itself is open, not its prefix");
    for uri in ["/health/live", runtime_path(), "/manifest.webmanifest", "/service-worker.js", "/offline"] {
        let (status, _, _) = guarded(&router, "GET", uri, None, &[]).await;
        assert_eq!(status, StatusCode::OK, "{uri} is Corten's own");
    }
    let (status, _, _) = guarded(&router, "GET", "/", Some("ada"), &[]).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_sign_in_that_would_loop_is_an_error_that_says_so() {
    let router = private_app(false);
    let (status, res, body) = guarded(&router, "GET", "/", None, &[("accept", "text/html")]).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "not a redirect the browser follows forever");
    assert!(!res.contains_key("location") && !body.contains("unguarded"), "the message stays in the log");
    let dev = App::new(Router::new().route(SignIn::PATH, get(|| async { "sign in" })))
        .dev(true)
        .context(|_| async { None::<Viewer> })
        .guard("/", |_: Option<&Viewer>, _| Access::sign_in(&SignIn))
        .router();
    let (_, _, body) = guarded(&dev, "GET", "/sign-in", None, &[("accept", "text/html")]).await;
    assert!(body.contains("mark the sign-in route with App::unguarded"), "{body}");
}

#[tokio::test]
async fn a_guard_names_one_route_by_its_type() {
    let routes = Router::new().route(ApiItem::PATH, get(|| async { "item" })).route("/api/other", get(|| async { "other" }));
    let router = App::new(Router::new())
        .context(|_| async { None::<Viewer> })
        .api("/api", routes)
        .guard(
            "/api",
            |_: Option<&Viewer>, req| {
                if req.is::<ApiItem>() {
                    Access::refuse(Error::status(StatusCode::FORBIDDEN))
                } else {
                    Access::ALLOW
                }
            },
        )
        .router();
    assert_eq!(guarded(&router, "GET", "/api/items/7", None, &[]).await.0, StatusCode::FORBIDDEN);
    assert_eq!(guarded(&router, "GET", "/api/other", None, &[]).await.0, StatusCode::OK);
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
