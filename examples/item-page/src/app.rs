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
    Head, Meta, Mode, Page, Template,
};
use serde::Deserialize;
use serde_json::json;
use tower_http::compression::CompressionLayer;

use crate::{
    model::{alert_message, sort_listings, ItemView, Listing, Offer},
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
    // Compression skips event streams and images by default; responses to clients that do not
    // ask for it (the load generator) are sent as is.
    Policy::new().apply(corten::error_pages::<Shell, _>(routes)).layer(CompressionLayer::new())
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
    const SERVICE_WORKER: Option<&'static str> = Some(corten::pwa::WORKER_PATH);

    fn islands_dir() -> &'static str {
        assets().islands_dir()
    }

    fn render(head: &Head<'_>, main: &str) -> askama::Result<String> {
        ShellTemplate { head, main, style: assets().url("style.css"), script: assets().url("app.js") }.render()
    }
}

/// Offers shown before "Show all"; the rest ship in a <template> for the offer-list island.
const OFFERS_SHOWN: usize = 5;

#[derive(Template)]
#[template(path = "item.html")]
pub struct ItemPage {
    t: &'static Catalog,
    item: ItemView,
    listings: Vec<Listing>,
    message: String,
}

impl ItemPage {
    pub fn new(t: &'static Catalog, item: ItemView, message: String) -> Self {
        let listings = sort_listings(&item.listings, false);
        ItemPage { t, item, listings, message }
    }

    fn offers_shown(&self) -> &[Offer] {
        &self.item.offers[..self.item.offers.len().min(OFFERS_SHOWN)]
    }

    fn offers_rest(&self) -> &[Offer] {
        &self.item.offers[self.item.offers.len().min(OFFERS_SHOWN)..]
    }
}

impl Page for ItemPage {
    fn title(&self) -> String {
        format!("{} | Card Market", self.item.title)
    }

    fn head(&self) -> Meta {
        Meta::new().description(&self.item.description).canonical(&format!("/items/{}", self.item.id))
    }
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
        Some(item) => req.page(&ItemPage::new(catalog(&headers), item, String::new())),
        None => req.page_with_status(StatusCode::NOT_FOUND, &NotFound),
    }
}

#[derive(Deserialize)]
pub struct AlertForm {
    title: String,
    email: String,
    target_price: String,
}

async fn create_alert(
    req: Req,
    headers: HeaderMap,
    State(app): State<App>,
    Path(id): Path<String>,
    Form(form): Form<AlertForm>,
) -> corten::Result {
    let message = alert_message(&form.title, &form.email, &form.target_price);
    if req.mode == Mode::Action {
        return req.patch(&AlertMessage { message: &message });
    }
    // Without JavaScript the browser posted a plain form: answer with the whole page.
    match app.source.item(&id).await? {
        Some(item) => req.page(&ItemPage::new(catalog(&headers), item, message)),
        None => req.page_with_status(StatusCode::NOT_FOUND, &NotFound),
    }
}

/// The same read and write the page offers, as MCP tools. Agents authenticate with a bearer
/// token (`AGENT_TOKENS`, comma separated), never with the browser's cookies.
fn tools(app: App) -> Tools {
    let authorized = {
        let tokens = app.tokens.clone();
        move |headers: &HeaderMap| {
            let token = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer "));
            token.is_some_and(|t| tokens.iter().any(|known| known == t))
        }
    };
    let read_auth = authorized.clone();
    let read_app = app.clone();
    Tools::new("card-market-item-page", env!("CARGO_PKG_VERSION"))
        .read_tool(
            "items.get",
            "Item detail: title, market summary, listings sorted by price, and buy offers.",
            json!({ "type": "object", "properties": { "id": { "type": "string", "description": "Item id, like item_17" } }, "required": ["id"] }),
            move |headers, args| {
                let (app, authorized) = (read_app.clone(), read_auth.clone());
                async move {
                    if !authorized(&headers) {
                        return Err(ToolError::Unauthorized);
                    }
                    let id = args["id"].as_str().ok_or_else(|| ToolError::Invalid("id must be a string".into()))?;
                    let item = app.source.item(id).await.map_err(|e| ToolError::Internal(e.into()))?;
                    let item = item.ok_or_else(|| ToolError::Invalid(format!("no item {id}")))?;
                    Ok(serde_json::to_value(&item)?)
                }
            },
        )
        .tool(
            "alerts.create",
            "Email the user when the item's lowest price drops below a target.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "email": { "type": "string", "format": "email" },
                    "target_price": { "type": "string", "description": "Decimal dollars, like 4.50" }
                },
                "required": ["id", "email", "target_price"]
            }),
            move |headers, args| {
                let (app, authorized) = (app.clone(), authorized.clone());
                async move {
                    if !authorized(&headers) {
                        return Err(ToolError::Unauthorized);
                    }
                    #[derive(Deserialize)]
                    struct Input {
                        id: String,
                        email: String,
                        target_price: String,
                    }
                    let input: Input = serde_json::from_value(args)?;
                    let item = app.source.item(&input.id).await.map_err(|e| ToolError::Internal(e.into()))?;
                    let item = item.ok_or_else(|| ToolError::Invalid(format!("no item {}", input.id)))?;
                    Ok(json!({ "message": alert_message(&item.title, &input.email, &input.target_price) }))
                }
            },
        )
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
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
    async fn pwa_offline_health_and_seo_routes_answer() {
        let (_, headers, manifest) = send(Request::get("/manifest.webmanifest").body(Body::empty()).unwrap()).await;
        assert_eq!(headers["content-type"], "application/manifest+json");
        assert!(manifest.contains(r#""purpose":"maskable""#));
        let (_, _, worker) = send(Request::get("/service-worker.js").body(Body::empty()).unwrap()).await;
        assert!(worker.contains(r#"const OFFLINE = "/offline";"#) && worker.contains(&assets().url("style.css")));
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
        let read = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "items.get", "arguments": { "id": "item_9" } } });
        let (_, _, body) = send(call(Some("dev-token"), read)).await;
        let res: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(res["result"]["structuredContent"]["listings"].as_array().unwrap().len(), 2);
    }
}
