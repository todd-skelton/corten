//! An item detail page with every production piece Corten offers switched on: security
//! policy (CSP nonce, cross-site write check), versioned assets, error pages, health, PWA with
//! push, head tags, an i18n catalog, robots/sitemap, compression, and the price-alert action as
//! an MCP tool for agents, mounted as a JSON-only route (`App::api`).
use std::sync::Arc;

use corten::{
    Api, Error,
    agent::{ToolError, Tools},
    axum::http::StatusCode,
    prelude::*,
    pwa::{Manifest, Pwa},
    seo::{self, SitemapUrl},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    model::{AlertInput, ItemView, alert_set, check_alert},
    routes,
    source::Source,
    text::Msg,
    views::{self, AlertForm, AlertFormView, AlertMessage, Home, ItemPage},
};

/// The app, assembled: routes plus the shell, catalogs and PWA settings. Corten adds the rest.
pub fn app(source: Source) -> App {
    let tokens = std::env::var("AGENT_TOKENS").unwrap_or_else(|_| "dev-token".into());
    let tokens: Arc<[String]> = tokens.split(',').map(str::to_string).collect();
    let manifest = Manifest::new("Card Market", "Cards")
        .description("Buy and sell trading cards")
        .theme_color("#14213d")
        .background_color("#ffffff")
        .icon("icons/192.png", "192x192")
        .icon("icons/512.png", "512x512")
        .maskable_icon("icons/512.png", "512x512");
    // Agents in a browser on another site (AGENT_ORIGINS, comma separated) may call the tools.
    let mut mcp = Api::new(Router::new().route(routes::Mcp::PATH, tools(source.clone(), tokens).route()));
    for origin in std::env::var("AGENT_ORIGINS").unwrap_or_default().split(',').filter(|o| !o.is_empty()) {
        mcp = mcp.allow_origin(origin);
    }
    App::new(routes(source))
        .api(routes::Mcp::PATH, mcp)
        .shell(views::shell)
        .messages::<Msg>()
        .components(views::COMPONENTS)
        .pwa(Pwa::new(manifest).import_script("sw-push.js").network_only(routes::Mcp::PATH))
}

fn routes(source: Source) -> Router {
    Router::new()
        .route(routes::Home::PATH, get(|req: Req| async move { req.page("Card Market", Home { t: req.t() }) }))
        .route(routes::Item::PATH, get(show).post(create_alert))
        .route(routes::Category::PATH, get(category))
        .route(routes::Robots::PATH, get(|| async { seo::robots(&[routes::Mcp::PATH], Some(routes::Sitemap::PATH)) }))
        .route(routes::Sitemap::PATH, get(sitemap))
        .with_state(source)
}

async fn sitemap() -> corten::axum::response::Response {
    let urls: Vec<String> =
        (1..=50).map(|n| format!("https://cards.example.com{}", routes::Item { id: &format!("item_{n}") })).collect();
    seo::sitemap(urls.iter().map(|loc| SitemapUrl { loc, lastmod: None }))
}

fn not_found() -> Error {
    Error::status(StatusCode::NOT_FOUND)
}

async fn category(req: Req, Path(slug): Path<String>) -> corten::Result {
    let t: Msg = req.t();
    req.page(t.category_title(&slug).to_string(), views::category(t, &slug))
}

async fn show(req: Req, State(source): State<Source>, Path(id): Path<String>) -> corten::Result {
    let item = source.item(&id).await?.ok_or_else(not_found)?;
    let page = ItemPage::new(req.t(), &item, String::new());
    req.page(page.meta(), page.view())
}

async fn create_alert(
    req: Req,
    State(source): State<Source>,
    Path(id): Path<String>,
    Form(form): Form<AlertInput>,
) -> corten::Result {
    let t = req.t();
    let price = match check_alert(t, &form.email, &form.target_price) {
        Ok(price) => price,
        Err(errors) => {
            let view =
                AlertFormView { id: &id, title: &form.title, email: &form.email, target_price: &form.target_price, errors };
            // Without JavaScript the whole page comes back, with the form showing the errors.
            let page = || async {
                let item = source.item(&id).await?.ok_or_else(not_found)?;
                let page = ItemPage { form: view.clone(), ..ItemPage::new(t, &item, String::new()) };
                req.page(page.meta(), page.view())
            };
            return req.invalid(&view.errors, AlertForm { t, form: view.clone() }, page).await;
        }
    };
    let message = alert_set(&form.title, &form.email, price);
    if req.mode == Mode::Action {
        return req.patch(AlertMessage { message: &message });
    }
    // Without JavaScript the browser posted a plain form: answer with the whole page.
    let item = source.item(&id).await?.ok_or_else(not_found)?;
    let page = ItemPage::new(t, &item, message);
    req.page(page.meta(), page.view())
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
fn tools(source: Source, tokens: Arc<[String]>) -> Tools {
    let read_source = source.clone();
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
                let source = read_source.clone();
                async move {
                    let item = source.item(&input.id).await.map_err(ToolError::internal)?;
                    let item = item.ok_or_else(|| ToolError::Invalid(format!("no item {}", input.id)))?;
                    Ok(ItemView::clone(&item))
                }
            },
        )
        .tool(
            "alerts.create",
            "Email the user when the item's lowest price drops below a target.",
            move |(), input: CreateAlert| {
                let source = source.clone();
                async move {
                    let price = check_alert(Msg::default(), &input.email, &input.target_price)?;
                    let item = source.item(&input.id).await.map_err(ToolError::internal)?;
                    let item = item.ok_or_else(|| ToolError::Invalid(format!("no item {}", input.id)))?;
                    Ok(AlertSet { message: alert_set(&item.title, &input.email, price) })
                }
            },
        )
}

#[cfg(test)]
mod tests {
    use corten::axum::{
        body::Body,
        http::{HeaderMap, Request},
    };
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
        super::app(source).router()
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
        assert!(html.contains("<h1>Pika &lt;Promo&gt; &amp; &quot;Friends&quot;</h1>"));
        assert!(html.contains(r#"<link rel="canonical" href="/items/item_9" data-corten-head>"#));
        assert!(html.contains(r#"/scripts/listing_sort.js">"#), "the scripts the page's handlers use are preloaded");
        assert!(html.contains(r#"<link rel="manifest" href="/manifest.webmanifest">"#));
    }

    #[tokio::test]
    async fn styles_are_scoped_to_components_and_served_as_one_file_after_the_app_stylesheet() {
        let (_, _, html) = send(Request::get("/items/item_9").body(Body::empty()).unwrap()).await;
        assert!(
            html.contains(r#"<section class="item-detail__panel item-detail__summary gap-5 direction-column sm:direction-row">"#),
            "{html}"
        );
        assert!(html.contains(r#"<td class="price__amount">"#));
        let link = |name: &str| html.find(&format!(r#"/{name}">"#)).unwrap_or_else(|| panic!("no {name} link"));
        assert!(link("style.css") < link("components.css"), "components come after the app's element defaults");
        let end = link("components.css") + "/components.css".len();
        let href = html[..end].rsplit_once('"').unwrap().1.to_string();
        assert!(href.starts_with("/assets/"), "{href}");
        let (status, headers, css) = send(Request::get(&href).body(Body::empty()).unwrap()).await;
        assert_eq!((status, &headers["content-type"]), (StatusCode::OK, &"text/css; charset=utf-8".parse().unwrap()));
        assert!(css.starts_with(":root{--color-ink:#1d1b26;"), "tokens first: {css}");
        assert!(css.contains(r#"[data-theme="dark"]{--color-ink:#ecebf2;"#) && css.contains("@media (min-width:40rem){"));
        assert!(css.contains(".sm\\:direction-row{flex-direction:row}"));
        assert!(css.contains(".item-detail__crumbs a{color:var(--color-muted);") && css.contains(".alert-form__message{"));
        assert_eq!(crate::theme::Space::_5.var(), "var(--space-5)");
    }

    #[tokio::test]
    async fn pages_follow_accept_language_and_links_come_from_routes() {
        let req = Request::get("/items/item_9").header("accept-language", "fr-CA, en;q=0.5").body(Body::empty()).unwrap();
        let (_, _, html) = send(req).await;
        assert!(html.contains(r#"<html lang="fr">"#) && html.contains("Voir les 7 offres") && html.contains("<h2>Annonces</h2>"));
        assert!(html.contains(r#"<form method="post" action="/items/item_9" data-patch>"#));
        assert!(html.contains(
            r#"<div data-scope="listing-sort"><h2>Annonces</h2><button type="button" data-on="click:listing_sort.toggle(@rows,@ascending,@descending)">"#
        ));
        let (status, _, body) = send(Request::get("/categories/promo").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("<h1>Category promo</h1>") && body.contains(r#"<a href="/">Back to popular items</a>"#));
    }

    /// The scripts are checked against these declarations by `npm run check`.
    #[test]
    fn script_types_match_the_server() {
        corten::types::Types::new().components(views::COMPONENTS).sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
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
            r#"<p class="alert-form__message" id="alert-message" aria-live="polite">Alert set: we will email a@b.c when Pika drops below $2.00.</p>"#
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
        assert!(body.starts_with(r#"<div class="alert-form__form" id="alert-form">"#), "{body}");
        assert!(
            body.contains(
                r#"value="abc" inputmode="decimal" required aria-invalid="true" aria-describedby="target_price-error">"#
            )
        );
        assert!(body.contains(r#"<p class="field-error" id="target_price-error">Enter a target price above zero.</p>"#));
        assert!(body.contains(r#"value="a@b.c" required>"#), "a valid field keeps its value and no error");

        let (status, _, body) = send(post(None, Some("application/json"))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["error"]["fields"], json!({ "target_price": "Enter a target price above zero." }));

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
        let router = super::app(source.clone()).router();
        let t = Msg::default();
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
                bytes += std::hint::black_box(ItemPage::new(t, item, String::new()).view().to_html()).len();
            }
        }
        let render = per(start);
        println!("build view: {build:.1} us, build + render main: {render:.1} us ({} bytes avg)", bytes / (rounds * ids.len()));
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
        // A versioned static URL, /assets/{12 hex}/{file}, somewhere in the worker's lists.
        let has = |text: &str, file: &str| {
            text.split(['"', '[', ']', ',']).any(|part| {
                part.starts_with("/assets/")
                    && part.ends_with(&format!("/{file}"))
                    && part.split('/').nth(2).is_some_and(|v| v.len() == 12)
            })
        };
        assert!(worker.contains(r#"const OFFLINE = "/offline";"#) && has(&worker, "style.css"), "{worker}");
        assert!(
            worker.starts_with("// Generated") && worker.contains("importScripts(...[\"/assets/") && has(&worker, "sw-push.js")
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
            fields.contains(&AlertInput::FIELDS.email.as_str()) && fields.contains(&AlertInput::FIELDS.target_price.as_str()),
            "the tool takes the form's fields under the same names, so validation errors name them right: {fields:?}"
        );
        assert_eq!(res["result"]["tools"][1]["outputSchema"]["properties"]["listings"]["type"], "array");
        let read = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "items.get", "arguments": { "id": "item_9" } } });
        let (_, _, body) = send(call(Some("dev-token"), read)).await;
        let res: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(res["result"]["structuredContent"]["listings"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn the_mcp_endpoint_is_json_only() {
        let (status, headers, body) = send(Request::get("/mcp").header("accept", "*/*").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(body, r#"{"error":{"status":405,"message":"Method Not Allowed"}}"#, "not the HTML error page");
        assert_eq!(headers["content-security-policy"], "default-src 'none'; frame-ancestors 'none'");
        let cross = Request::post("/mcp")
            .header("origin", "https://agent.example")
            .header("sec-fetch-site", "cross-site")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
            .unwrap();
        let (status, _, body) = send(cross).await;
        assert_eq!(
            (status, &body[..]),
            (StatusCode::FORBIDDEN, r#"{"error":{"status":403,"message":"Cross-site request refused."}}"#)
        );
    }
}
