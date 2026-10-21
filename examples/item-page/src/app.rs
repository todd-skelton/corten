//! A marketplace item detail page, with every production piece Corten offers switched on:
//! security policy (CSP nonce, CSRF check), versioned assets, error pages, health, PWA,
//! head tags, an i18n catalog, robots/sitemap, and the price-alert action as an MCP tool.
use std::sync::{Arc, OnceLock};

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::get,
    Form, Router,
};
use corten::{
    agent::{ToolError, Tools},
    assets::Assets,
    i18n::{Catalog, Locales},
    pwa::{Manifest, Pwa},
    security::Policy,
    seo::{self, SitemapUrl},
    Errors, Head, Meta, Mode, Page, Template,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    model::{alert_set, check_alert, sort_listings, AlertInput, ItemView, Listing, Offer},
    source::Source,
};

pub type Req = corten::Req<Shell>;

/// Static files are loaded once per process; the shell reads their versioned URLs from here.
static ASSETS: OnceLock<Arc<Assets>> = OnceLock::new();
static LOCALES: OnceLock<Locales> = OnceLock::new();

fn assets() -> &'static Arc<Assets> {
    ASSETS.get().expect("App::new loads the assets")
}

fn locales() -> &'static Locales {
    LOCALES.get_or_init(|| Locales::new(Catalog::from_json("en", include_str!("../locales/en.json")).expect("en.json")))
}

#[derive(Clone)]
pub struct App {
    source: Source,
    tokens: Arc<[String]>,
}

impl App {
    pub fn new(source: Source, static_dir: &str) -> std::io::Result<Self> {
        if ASSETS.get().is_none() {
            let _ = ASSETS.set(Arc::new(Assets::load(static_dir, "/assets")?));
        }
        let tokens = std::env::var("AGENT_TOKENS").unwrap_or_else(|_| "dev-token".into());
        Ok(App { source, tokens: tokens.split(',').map(str::to_string).collect() })
    }
}

pub fn router(app: App) -> Router {
    // Outermost, so it sees every response, error pages included. Event streams and images
    // pass through; clients that do not ask for compression (the load generator) get identity.
    corten::compress::apply(routes(app))
}

fn routes(app: App) -> Router {
    let assets = assets();
    let pwa = Pwa::new(
        Manifest::new("Card Market", "Card Market")
            .description("Buy and sell trading cards")
            .theme_color("#14213d")
            .background_color("#ffffff")
            .icon(&assets.url("icons/192.png"), "192x192", "image/png")
            .icon(&assets.url("icons/512.png"), "512x512", "image/png")
            .maskable_icon(&assets.url("icons/512.png"), "512x512", "image/png"),
    )
    .offline("/offline")
    .assets(assets)
    .import_script(&assets.url("sw-push.js"))
    .network_only("/mcp");
    let routes = Router::new()
        .route("/", get(home))
        .route("/items/{id}", get(show).post(create_alert))
        .route("/offline", get(offline))
        .route("/robots.txt", get(|| async { seo::robots(&["/mcp"], Some("/sitemap.xml")) }))
        .route("/sitemap.xml", get(sitemap))
        .route("/mcp", tools(app.clone()).route())
        .fallback(not_found)
        .with_state(app)
        .merge(assets.routes())
        .merge(pwa.routes())
        .merge(corten::health(|| async { true }))
        .merge(corten::routes());
    Policy::new().apply(corten::error_pages::<Shell, _>(routes))
}

pub struct Shell;

#[derive(Template)]
#[template(path = "shell.html")]
struct ShellTemplate<'a> {
    head: &'a Head<'a>,
    main: &'a str,
    style: String,
    script: String,
}

impl corten::Shell for Shell {
    type Context = ();

    const SERVICE_WORKER: Option<&'static str> = Some(corten::pwa::WORKER_PATH);

    fn islands_dir() -> &'static str {
        assets().islands_dir()
    }

    fn render(head: &Head<'_>, main: &str, _: Option<&()>) -> askama::Result<String> {
        ShellTemplate { head, main, style: assets().url("style.css"), script: assets().url("app.js") }.render()
    }
}

/// Offers shown before "Show all"; the rest ship in a <template> for the offer-list island.
const OFFERS_SHOWN: usize = 5;

/// The item page borrows the item: rendering copies no strings out of it.
#[derive(Template)]
#[template(path = "item.html")]
pub struct ItemPage<'a> {
    t: &'static Catalog,
    item: &'a ItemView,
    listings: Vec<&'a Listing>,
    message: String,
    form: AlertFormView<'a>,
}

impl<'a> ItemPage<'a> {
    pub fn new(t: &'static Catalog, item: &'a ItemView, message: String) -> Self {
        let listings = sort_listings(&item.listings, false);
        let form = AlertFormView { id: &item.id, title: &item.title, ..AlertFormView::default() };
        ItemPage { t, item, listings, message, form }
    }

    fn offers_shown(&self) -> &[Offer] {
        &self.item.offers[..self.item.offers.len().min(OFFERS_SHOWN)]
    }

    fn offers_rest(&self) -> &[Offer] {
        &self.item.offers[self.item.offers.len().min(OFFERS_SHOWN)..]
    }
}

impl Page for ItemPage<'_> {
    fn title(&self) -> String {
        format!("{} | Card Market", self.item.title)
    }

    fn head(&self) -> Meta {
        Meta::new().description(&self.item.description).canonical(&format!("/items/{}", self.item.id))
    }
}

/// The price-alert form's values and errors. The item page includes the form; a submit that
/// fails validation patches it on its own. Field names come from `AlertInput::FIELDS`.
#[derive(Clone, Default)]
pub struct AlertFormView<'a> {
    id: &'a str,
    title: &'a str,
    email: &'a str,
    target_price: &'a str,
    errors: Errors,
}

#[derive(Template)]
#[template(path = "alert_form.html")]
pub struct AlertForm<'a> {
    t: &'static Catalog,
    form: AlertFormView<'a>,
}

/// The price-alert result. Rendered inside the item page and on its own as a patch.
#[derive(Template)]
#[template(path = "alert_message.html")]
pub struct AlertMessage<'a> {
    message: &'a str,
}

#[derive(Template)]
#[template(path = "home.html")]
pub struct Home;

impl Page for Home {
    fn title(&self) -> String {
        "Card Market".into()
    }
}

#[derive(Template)]
#[template(source = "<h1>Not found</h1><p>Not found</p>", ext = "html")]
pub struct NotFound;

impl Page for NotFound {
    fn title(&self) -> String {
        "Not found".into()
    }
    fn head(&self) -> Meta {
        Meta::new().noindex()
    }
}

#[derive(Template)]
#[template(source = "<h1>You are offline</h1><p>Check your connection and try again.</p>", ext = "html")]
pub struct Offline;

impl Page for Offline {
    fn title(&self) -> String {
        "Offline | Card Market".into()
    }
    fn head(&self) -> Meta {
        Meta::new().noindex()
    }
}

fn catalog(headers: &HeaderMap) -> &'static Catalog {
    locales().negotiate(headers.get("accept-language").and_then(|v| v.to_str().ok()))
}

async fn home(req: Req) -> corten::Result {
    req.page(&Home)
}

async fn offline(req: Req) -> corten::Result {
    req.page(&Offline)
}

async fn not_found(req: Req) -> corten::Result {
    req.page_with_status(StatusCode::NOT_FOUND, &NotFound)
}

async fn sitemap() -> axum::response::Response {
    let urls: Vec<String> = (1..=50).map(|n| format!("https://example.com/items/item_{n}")).collect();
    seo::sitemap(urls.iter().map(|loc| SitemapUrl { loc, lastmod: None }))
}

async fn show(req: Req, headers: HeaderMap, State(app): State<App>, Path(id): Path<String>) -> corten::Result {
    match app.source.item(&id).await? {
        Some(item) => req.page(&ItemPage::new(catalog(&headers), &item, String::new())),
        None => req.page_with_status(StatusCode::NOT_FOUND, &NotFound),
    }
}

async fn create_alert(
    req: Req,
    headers: HeaderMap,
    State(app): State<App>,
    Path(id): Path<String>,
    Form(form): Form<AlertInput>,
) -> corten::Result {
    let t = catalog(&headers);
    let price = match check_alert(t, &form.email, &form.target_price) {
        Ok(price) => price,
        Err(errors) => {
            let view =
                AlertFormView { id: &id, title: &form.title, email: &form.email, target_price: &form.target_price, errors };
            let fragment = AlertForm { t, form: view.clone() };
            // Without JavaScript the whole page comes back, with the form showing the errors.
            let page = || async {
                let item = app.source.item(&id).await?.ok_or_else(|| corten::Error::status(StatusCode::NOT_FOUND))?;
                req.page(&ItemPage { form: view.clone(), ..ItemPage::new(t, &item, String::new()) })
            };
            return req.invalid(&view.errors, &fragment, page).await;
        }
    };
    let message = alert_set(&form.title, &form.email, price);
    if req.mode == Mode::Action {
        return req.patch(&AlertMessage { message: &message });
    }
    // Without JavaScript the browser posted a plain form: answer with the whole page.
    match app.source.item(&id).await? {
        Some(item) => req.page(&ItemPage::new(t, &item, message)),
        None => req.page_with_status(StatusCode::NOT_FOUND, &NotFound),
    }
}

/// Which item, for `items.get`.
#[derive(Deserialize, JsonSchema)]
struct ItemId {
    /// Item id, like item_17.
    id: String,
}

/// A price alert as an agent asks for it: the form's fields (same names, same rules) plus the item.
#[derive(Deserialize, JsonSchema)]
struct CreateAlert {
    /// Item id, like item_17.
    id: String,
    /// Where to send the alert.
    #[schemars(email)]
    email: String,
    /// Decimal dollars, like 4.50.
    target_price: String,
}

#[derive(Serialize, JsonSchema)]
struct AlertSet {
    message: String,
}

/// The same read and write the page offers, as MCP tools. Agents authenticate with a bearer
/// token (`AGENT_TOKENS`, comma separated), never with the browser's cookies. Input and output
/// schemas come from the structs above.
fn tools(app: App) -> Tools {
    let tokens = app.tokens.clone();
    let read_app = app.clone();
    Tools::new("card-market-item-page", env!("CARGO_PKG_VERSION"))
        .auth(move |headers| {
            let token = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
            match token {
                Some(token) if tokens.iter().any(|known| known == token) => Ok(()),
                _ => Err(ToolError::Unauthorized),
            }
        })
        .read_tool(
            "items.get",
            "Item detail: title, market summary, listings sorted by price, and buy offers.",
            move |(), input: ItemId| {
                let app = read_app.clone();
                async move {
                    let item = app.source.item(&input.id).await.map_err(ToolError::internal)?;
                    let item = item.ok_or_else(|| ToolError::Invalid(format!("no item {}", input.id)))?;
                    Ok(ItemView::clone(&item))
                }
            },
        )
        .tool(
            "alerts.create",
            "Email the user when the item's lowest price drops below a target.",
            move |(), input: CreateAlert| {
                let app = app.clone();
                async move {
                    let price = check_alert(locales().default_catalog(), &input.email, &input.target_price)?;
                    let item = app.source.item(&input.id).await.map_err(ToolError::internal)?;
                    let item = item.ok_or_else(|| ToolError::Invalid(format!("no item {}", input.id)))?;
                    Ok(AlertSet { message: alert_set(&item.title, &input.email, price) })
                }
            },
        )
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;

    fn app() -> Router {
        let offers: Vec<_> = (1..=7)
            .map(|n| json!({ "offer_id": format!("o{n}"), "buyer_display_name": format!("Buyer {n}"), "product_summary": "NM", "quantity_requested": 1, "price_amount": "1.00", "price_currency_code": "USD" }))
            .collect();
        let item = json!({
            "catalog_item_id": "item_9",
            "title": "Pika <Promo> & \"Friends\"",
            "description": "A card",
            "market_summary": { "lowest_price_amount": "0.50", "lowest_price_currency_code": "USD", "active_listing_count": 2, "total_visible_quantity": 3 },
            "market_listings": [
                { "listing_id": "l2", "seller_display_name": "B", "price_amount": "2.00", "price_currency_code": "USD", "visible_quantity": 1 },
                { "listing_id": "l1", "seller_display_name": "A", "seller_average_rating": "4.9", "seller_review_count": 12, "price_amount": "0.50", "price_currency_code": "USD", "visible_quantity": 2 }
            ],
            "offer_demand_matches": offers
        });
        let source = Source::fixture(&json!({ "item_9": item }).to_string()).unwrap();
        router(App::new(source, concat!(env!("CARGO_MANIFEST_DIR"), "/static")).unwrap())
    }

    async fn send(req: Request<Body>) -> (StatusCode, HeaderMap, String) {
        let res = app().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        (parts.status, parts.headers, String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap())
    }

    #[tokio::test]
    async fn item_page_renders_sorted_listings_split_offers_and_escaped_text() {
        let (status, headers, html) = send(Request::get("/items/item_9").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        assert!(headers.contains_key("content-security-policy"));
        assert!(html.find("A · 4.9 ★ (12)").unwrap() < html.find(">B<").unwrap(), "cheapest listing first");
        let (shown, rest) = html.split_once(r#"<template data-ref="rest">"#).unwrap();
        assert_eq!(shown.matches("Buyer ").count(), 5);
        assert_eq!(rest.matches("Buyer ").count(), 2);
        assert!(html.contains("Show all 7 offers"), "catalog interpolation");
        assert!(html.contains("<h1>Pika &#60;Promo&#62; &#38; &#34;Friends&#34;</h1>"));
        assert!(html.contains(r#"<link rel="canonical" href="/items/item_9" data-corten-head>"#));
        assert!(html.contains(&format!(r#"<link rel="modulepreload" href="{}listing-sort.js">"#, assets().islands_dir())));
        assert!(html.contains(r#"<link rel="manifest" href="/manifest.webmanifest">"#));
        assert!(!html.contains("[missing:"), "every catalog key exists");
    }

    #[tokio::test]
    async fn alert_patch_embeds_the_exact_page_markup() {
        let form = "title=Pika&email=a%40b.c&target_price=2";
        let req = Request::post("/items/item_9")
            .header("corten", "action")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("sec-fetch-site", "same-origin")
            .body(Body::from(form))
            .unwrap();
        let (_, headers, body) = send(req).await;
        assert_eq!(headers["corten-patch"], "1");
        assert_eq!(
            body,
            r#"<p class="alert-message" id="alert-message" aria-live="polite">Alert set: we will email a@b.c when Pika drops below $2.00.</p>"#
        );
        let cross = Request::post("/items/item_9")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("sec-fetch-site", "cross-site")
            .body(Body::from(form))
            .unwrap();
        assert_eq!(send(cross).await.0, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn an_invalid_alert_is_422_with_field_errors_on_every_surface() {
        let post = |mode: Option<&str>, accept: Option<&str>| {
            let mut req = Request::post("/items/item_9")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("sec-fetch-site", "same-origin");
            if let Some(mode) = mode {
                req = req.header("corten", mode);
            }
            if let Some(accept) = accept {
                req = req.header("accept", accept);
            }
            req.body(Body::from("title=Pika&email=a%40b.c&target_price=abc")).unwrap()
        };
        let (status, headers, body) = send(post(Some("action"), None)).await;
        assert_eq!((status, &headers["corten-patch"]), (StatusCode::UNPROCESSABLE_ENTITY, &"1".parse().unwrap()));
        assert!(body.starts_with(r#"<div class="alert-form" id="alert-form">"#), "{body}");
        assert!(body
            .contains(r#"value="abc" inputmode="decimal" required aria-invalid="true" aria-describedby="target_price-error">"#));
        assert!(body.contains(r#"<p class="field-error" id="target_price-error">Enter a target price above zero.</p>"#));
        assert!(body.contains(r#"value="a@b.c" required>"#), "a valid field keeps its value and no error");

        let (status, _, body) = send(post(None, Some("application/json"))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body, r#"{"errors":{"target_price":"Enter a target price above zero."}}"#);

        let (status, _, body) = send(post(None, None)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("<html") && body.contains("Show all 7 offers") && body.contains("target_price-error"));
    }

    /// Where the time for one item page goes. Release build, benchmark fixture:
    /// `CORTEN_FIXTURE=/path/items.json cargo test --release -p item-page -- --ignored render_cost --nocapture`
    #[tokio::test]
    #[ignore]
    async fn render_cost() {
        use std::time::Instant;
        let path = std::env::var("CORTEN_FIXTURE").expect("CORTEN_FIXTURE");
        let source = Source::fixture(&std::fs::read_to_string(path).unwrap()).unwrap();
        let ids: Vec<String> = (1..=500).map(|n| format!("item_{n}")).collect();
        let router = router(App::new(source.clone(), concat!(env!("CARGO_MANIFEST_DIR"), "/static")).unwrap());
        let t = locales().default_catalog();
        let rounds = 20;
        let n = (rounds * ids.len()) as f64;
        let per = |start: Instant| start.elapsed().as_secs_f64() * 1e6 / n;
        let mut items = Vec::new();
        for id in &ids {
            items.push(source.item(id).await.unwrap().unwrap());
        }
        let start = Instant::now();
        for _ in 0..rounds {
            for item in &items {
                std::hint::black_box(ItemPage::new(t, item, String::new()));
            }
        }
        let build = per(start);
        let start = Instant::now();
        let mut bytes = 0;
        for _ in 0..rounds {
            for item in &items {
                bytes += std::hint::black_box(ItemPage::new(t, item, String::new()).render().unwrap()).len();
            }
        }
        let render = per(start);
        let main = ItemPage::new(t, &items[0], String::new()).render().unwrap();
        let start = Instant::now();
        for _ in 0..rounds * ids.len() {
            std::hint::black_box(corten::find_islands(std::hint::black_box(&main)));
        }
        println!(
            "build view: {build:.1} us, build + render main: {render:.1} us ({} bytes avg), find islands: {:.1} us",
            bytes / (rounds * ids.len()),
            per(start)
        );
        for (label, mode, encoding) in [
            ("navigation, identity", Some("nav"), "identity"),
            ("document, identity", None, "identity"),
            ("document, zstd", None, "gzip, deflate, br, zstd"),
            ("document, br", None, "gzip, deflate, br"),
            ("document, gzip", None, "gzip"),
        ] {
            let start = Instant::now();
            let mut size = 0;
            for _ in 0..rounds {
                for id in &ids {
                    let mut req = Request::get(format!("/items/{id}")).header("accept-encoding", encoding);
                    if let Some(mode) = mode {
                        req = req.header("corten", mode);
                    }
                    let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
                    size += std::hint::black_box(res.into_body().collect().await.unwrap().to_bytes()).len();
                }
            }
            println!("full request, {label}: {:.1} us, {} bytes avg", per(start), size / (rounds * ids.len()));
        }
    }

    #[tokio::test]
    async fn pwa_offline_health_and_seo_routes_answer() {
        let (_, headers, manifest) = send(Request::get("/manifest.webmanifest").body(Body::empty()).unwrap()).await;
        assert_eq!(headers["content-type"], "application/manifest+json");
        assert!(manifest.contains(r#""purpose":"maskable""#));
        let (_, _, worker) = send(Request::get("/service-worker.js").body(Body::empty()).unwrap()).await;
        assert!(worker.contains(r#"const OFFLINE = "/offline";"#) && worker.contains(&assets().url("style.css")));
        assert!(
            worker.starts_with("// Generated")
                && worker.contains(&format!(r#"importScripts(...["{}"]);"#, assets().url("sw-push.js")))
        );
        let (status, _, offline) = send(Request::get("/offline").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        assert!(offline.contains("You are offline"));
        assert_eq!(send(Request::get("/health/ready").body(Body::empty()).unwrap()).await.0, StatusCode::OK);
        assert!(send(Request::get("/robots.txt").body(Body::empty()).unwrap()).await.2.contains("Disallow: /mcp"));
        let (status, _, body) = send(Request::get("/items/nope").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(body.contains(r#"content="noindex""#));
    }

    #[tokio::test]
    async fn agents_read_items_and_set_alerts_with_a_token() {
        let call = |token: Option<&str>, body: serde_json::Value| {
            let mut req = Request::post("/mcp").header("content-type", "application/json");
            if let Some(token) = token {
                req = req.header("authorization", format!("Bearer {token}"));
            }
            req.body(Body::from(body.to_string())).unwrap()
        };
        let alert = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "alerts.create", "arguments": { "id": "item_9", "email": "a@b.c", "target_price": "1" } } });
        let (_, _, body) = send(call(Some("dev-token"), alert.clone())).await;
        let res: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            res["result"]["structuredContent"]["message"],
            "Alert set: we will email a@b.c when Pika <Promo> & \"Friends\" drops below $1.00."
        );
        assert_eq!(send(call(Some("wrong"), alert)).await.0, StatusCode::UNAUTHORIZED);
        let bad = json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "alerts.create", "arguments": { "id": "item_9", "email": "nope", "target_price": "1" } } });
        let res: serde_json::Value = serde_json::from_str(&send(call(Some("dev-token"), bad)).await.2).unwrap();
        assert_eq!(res["result"]["isError"], true);
        assert_eq!(res["result"]["content"][0]["text"], "invalid arguments: email: Enter a valid email address.");
        let list = json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/list" });
        let res: serde_json::Value = serde_json::from_str(&send(call(None, list)).await.2).unwrap();
        let create = &res["result"]["tools"][0];
        assert_eq!(create["name"], "alerts.create");
        assert_eq!(create["inputSchema"]["properties"]["email"]["format"], "email");
        let fields: Vec<&str> =
            create["inputSchema"]["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert!(
            fields.contains(&AlertInput::FIELDS.email) && fields.contains(&AlertInput::FIELDS.target_price),
            "the tool takes the form's fields under the same names, so validation errors name them right: {fields:?}"
        );
        assert_eq!(res["result"]["tools"][1]["outputSchema"]["properties"]["listings"]["type"], "array");
        let read = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "items.get", "arguments": { "id": "item_9" } } });
        let (_, _, body) = send(call(Some("dev-token"), read)).await;
        let res: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(res["result"]["structuredContent"]["listings"].as_array().unwrap().len(), 2);
    }
}
