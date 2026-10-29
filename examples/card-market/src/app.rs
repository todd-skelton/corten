//! An item detail page with every production piece Corten offers switched on: security
//! policy (CSP nonce, cross-site write check), versioned assets, error pages, health, PWA with
//! push, head tags, an i18n catalog, robots/sitemap, compression, the home and category pages
//! cached by CDNs (`Public`), an item as JSON for other services and the price-alert
//! action as an MCP tool for agents, both mounted as JSON-only routes (`App::api`). Partners'
//! services set alerts with an API key (`Caller<Partner>`), and writes are limited per client
//! address by a middleware of the app's (`WriteLimit`, `ClientIp`; `PROXIES` behind a load
//! balancer).
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use corten::{
    ClientIp, Error, ForAll, HtmlResponse,
    agent::{ToolError, Tools},
    axum::{
        extract::{FromRequestParts, Request},
        http::{HeaderMap, HeaderValue, Method, StatusCode, header, request::Parts},
        response::{IntoResponse, Response},
    },
    i18n::Minor,
    prelude::*,
    pwa::{Manifest, Pwa},
    security::Policy,
    seo::{self, SitemapUrl},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    api, mcp,
    model::{AlertInput, ItemView, alert_set, check_alert},
    routes,
    source::Source,
    text::Msg,
    views::{self, AlertForm, AlertFormView, AlertMessage, ItemList, ItemPage, Layout},
};

/// The app, assembled: routes plus the error page, catalogs and PWA settings. Corten adds the rest.
pub fn app(source: Source) -> App<WriteLimit> {
    let writes = match std::env::var("WRITES_PER_MINUTE") {
        Ok(n) => n.trim().parse().ok().filter(|&n| n > 0).expect("WRITES_PER_MINUTE is a whole number above zero"),
        Err(_) => WRITES_PER_MINUTE,
    };
    let keys = |name: &str| std::env::var(name).unwrap_or_default();
    app_with(source, Keys { partners: Partners::parse(&keys("API_KEYS")), agents: &keys("AGENT_TOKENS") }, writes)
}

/// Who may call the JSON routes: partners by API key, agents by token (`AGENT_TOKENS`, comma
/// separated). Neither has a default, so a deploy that leaves them out lets no one in.
struct Keys<'a> {
    partners: Partners,
    agents: &'a str,
}

/// The app with the keys and the writes a minute given, for tests.
fn app_with(source: Source, keys: Keys, writes: u32) -> App<WriteLimit> {
    let Keys { partners, agents } = keys;
    let tokens: Arc<[String]> = agents.split(',').map(str::trim).filter(|t| !t.is_empty()).map(str::to_string).collect();
    let limit = WriteLimit::new(writes, partners.clone());
    let manifest = Manifest::new("Card Market", "Cards")
        .description("Buy and sell trading cards")
        .theme_color("#14213d")
        .background_color("#ffffff")
        .icon("icons/192.png", "192x192")
        .icon("icons/512.png", "512x512")
        .maskable_icon("icons/512.png", "512x512");
    // Agents in a browser on another site (AGENT_ORIGINS, comma separated) may call the tools.
    let origins = std::env::var("AGENT_ORIGINS").unwrap_or_default();
    let tools = tools(source.clone(), tokens).route();
    // Served over HTTPS in production (`HTTPS=1`): browsers are told to use it from then on.
    let policy = if std::env::var_os("HTTPS").is_some() { Policy::new().hsts() } else { Policy::new() };
    let ready = source.clone();
    App::new(routes::router(source.clone()))
        .assets(crate::views::Static)
        .api(|routes| api::api(routes, ApiState { source, partners }))
        .api(|routes| {
            let routes = mcp::api(routes).route::<mcp::Mcp>(tools);
            origins.split(',').filter(|o| !o.is_empty()).fold(routes, |routes, origin| routes.allow_origin(origin))
        })
        .error_page(views::error_page, ())
        .messages::<Msg>()
        .components(views::COMPONENTS)
        .pwa(Pwa::new(manifest).import_script("sw-push.js").network_only(mcp::Mcp::PATH))
        .policy(policy)
        // An item page waits on the item API: past 10 seconds the visitor gets the 503 page.
        .deadline(Duration::from_secs(10))
        // Not ready while the item API is unreachable, so a load balancer sends no traffic here.
        .ready(move || {
            let source = ready.clone();
            async move { source.reachable().await }
        })
        .middleware(limit)
}

/// What the JSON routes share: the items, and the partners that may set alerts.
#[derive(Clone, FromRef)]
pub struct ApiState {
    source: Source,
    #[from_ref(skip)]
    partners: Partners,
}

/// The services that may set alerts for their users, by API key: `API_KEYS` is
/// `name=key,name=key`, and without it no partner may. A real app keeps a hash of each key in
/// its database.
#[derive(Clone)]
pub struct Partners(Arc<HashMap<String, Arc<str>>>);

impl Partners {
    /// `name=key` pairs, comma separated; a pair without a name or a key is left out.
    fn parse(keys: &str) -> Self {
        let pairs = keys.split(',').filter_map(|pair| pair.split_once('=')).map(|(name, key)| (name.trim(), key.trim()));
        let pairs = pairs.filter(|(name, key)| !name.is_empty() && !key.is_empty());
        Partners(Arc::new(pairs.map(|(name, key)| (key.to_string(), Arc::from(name))).collect()))
    }

    /// The partner whose key the request sends (`X-Api-Key`).
    fn named(&self, headers: &HeaderMap) -> Option<&Arc<str>> {
        self.0.get(headers.get("x-api-key")?.to_str().ok()?)
    }
}

/// A service that sets alerts for its users, known by the API key it sends (`X-Api-Key`). A
/// handler that takes `Caller<Partner>` never runs without one: the request is a JSON 401.
#[derive(Clone, Hash)]
pub(crate) struct Partner {
    pub name: Arc<str>,
}

impl Identify<ApiState> for Partner {
    async fn identify(parts: &Parts, state: &ApiState) -> Result<Self, Refused> {
        match state.partners.named(&parts.headers) {
            Some(name) => Ok(Partner { name: name.clone() }),
            None => Err(Refused::unauthorized()),
        }
    }
}

/// Writes one client may make a minute (`WRITES_PER_MINUTE` to change it): alerts send email,
/// so a script cannot set thousands.
const WRITES_PER_MINUTE: u32 = 20;

/// Alerts one partner may set a minute, for all its users together, from whatever addresses.
const PARTNER_WRITES_PER_MINUTE: u32 = 600;

/// Clients counted at most in a minute (a few megabytes); past it a new client waits for the
/// next minute, so memory stays bounded however many write. A flood from that many addresses
/// in one minute holds new writers off until the minute turns, and that is a botnet or under
/// two IPv6 /48s (65,536 /64s each, what one customer may be given): a site that faces one
/// limits at its edge (a CDN or load balancer) as well.
const ADDRESSES: usize = 100_000;

/// About `limit` writes a minute from one client: a form's post, or an alert with a key no
/// partner has. A client is its address (`ClientIp`: behind a load balancer, the one it
/// forwards, with `PROXIES` set), an IPv6 one by its /64, the block one host is given. A
/// partner's alerts are counted by the partner instead, whatever address its servers send from
/// (`PARTNER_WRITES_PER_MINUTE`). The count starts again each minute, so a burst across the
/// turn of a minute can reach twice the limit. A write past it is a 429 with `Retry-After`: the
/// error page for a browser, JSON for a partner. Reads pass straight through, so a page a CDN
/// keeps stays public; agents (`/mcp`, all POST, reads included) answer to their token instead;
/// and a request whose address cannot be known (a test without one) is not counted.
pub struct WriteLimit {
    limit: u32,
    partners: Partners,
    /// Clients counted at most in a minute ([`ADDRESSES`]).
    clients: usize,
    minute: Mutex<Minute>,
}

/// The writes of each client in the current minute.
struct Minute {
    started: Instant,
    writes: HashMap<Client, u32>,
}

/// Who a limit counts.
#[derive(PartialEq, Eq, Hash)]
enum Client {
    /// An IPv4 address, or an IPv6 address's /64.
    Address(IpAddr),
    Partner(Arc<str>),
}

impl Client {
    fn address(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(_) => Client::Address(ip),
            IpAddr::V6(v6) => Client::Address(IpAddr::V6((u128::from(v6) & !((1u128 << 64) - 1)).into())),
        }
    }
}

impl WriteLimit {
    fn new(limit: u32, partners: Partners) -> Self {
        let minute = Mutex::new(Minute { started: Instant::now(), writes: HashMap::new() });
        WriteLimit { limit, partners, clients: ADDRESSES, minute }
    }

    /// Counts a write from `client`: `None` when it may go ahead, else the seconds until it may.
    fn count(&self, client: Client, now: Instant) -> Option<u64> {
        let limit = if matches!(client, Client::Partner(_)) { PARTNER_WRITES_PER_MINUTE } else { self.limit };
        let mut minute = self.minute.lock().unwrap_or_else(|e| e.into_inner());
        let elapsed = now.duration_since(minute.started);
        if elapsed >= Duration::from_secs(60) {
            minute.started = now;
            minute.writes.clear();
        }
        let wait = 60 - now.duration_since(minute.started).as_secs().min(59);
        let full = minute.writes.len() >= self.clients;
        match minute.writes.get_mut(&client) {
            Some(writes) if *writes >= limit => Some(wait),
            Some(writes) => {
                *writes += 1;
                None
            }
            // Partners are the few `API_KEYS` names, so they need no room kept.
            None if full && !matches!(client, Client::Partner(_)) => Some(wait),
            None => {
                minute.writes.insert(client, 1);
                None
            }
        }
    }
}

impl Middleware for WriteLimit {
    async fn call(&self, req: Request, next: impl Next) -> Response {
        if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) || req.uri().path() == mcp::Mcp::PATH {
            return next.run(req).await;
        }
        let (mut parts, body) = req.into_parts();
        let partner = (parts.uri.path() == api::AlertsJson::PATH).then(|| self.partners.named(&parts.headers)).flatten();
        let client = match partner {
            Some(name) => Some(Client::Partner(name.clone())),
            None => Option::<ClientIp>::from_request_parts(&mut parts, &())
                .await
                .ok()
                .flatten()
                .map(|ClientIp(ip)| Client::address(ip)),
        };
        if let Some(wait) = client.and_then(|client| self.count(client, Instant::now())) {
            let mut res = Error::status(StatusCode::TOO_MANY_REQUESTS).into_response();
            res.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(wait));
            return res;
        }
        next.run(Request::from_parts(parts, body)).await
    }
}

/// The site's address, for the absolute URLs search engines read (the sitemap, structured data).
const SITE: &str = "https://cards.example.com";

/// Crawlers may read everything but the agents' endpoint.
pub(crate) async fn robots() -> Public<seo::File> {
    seo::robots(&[mcp::Mcp::PATH], Some(routes::Sitemap::PATH))
}

pub(crate) async fn sitemap() -> Public<seo::File> {
    let urls: Vec<String> = (1..=50).map(|n| format!("{SITE}{}", routes::Item { id: &format!("item_{n}") })).collect();
    seo::sitemap(urls.iter().map(|loc| SitemapUrl { loc, lastmod: None }))
}

/// A page that is the same for every visitor: a CDN may serve it for a minute, and an older copy
/// for ten more while it fetches a fresh one. The page is made for everyone (`ForAll`), so it
/// cannot set a cookie.
fn shared(page: corten::Result<HtmlResponse<ForAll>>) -> Public<HtmlResult> {
    Public::new(Duration::from_secs(60), page).stale_while_revalidate(Duration::from_secs(600))
}

pub(crate) async fn home(req: Req, State(source): State<Source>) -> Public<HtmlResult> {
    let t: Msg = req.t();
    let req = &req;
    shared(req.page(html! {
        <Layout req={req} title={t.home_title()} head="">
            <h1>{t.home_title()}</h1>
            <ItemList req={req} t={t} shelf={source.shelf()} />
        </Layout>
    }))
}

/// The trail a search engine shows for a page (schema.org `BreadcrumbList`), as structured data.
#[derive(Serialize, ToJson)]
struct Breadcrumbs {
    #[serde(rename = "@context")]
    context: &'static str,
    #[serde(rename = "@type")]
    kind: &'static str,
    #[serde(rename = "itemListElement")]
    items: Vec<Crumb>,
}

#[derive(Serialize, ToJson)]
struct Crumb {
    #[serde(rename = "@type")]
    kind: &'static str,
    position: usize,
    name: String,
    item: String,
}

impl Breadcrumbs {
    /// Each page's name and path, outermost first.
    fn new(pages: impl IntoIterator<Item = (String, String)>) -> Self {
        let items = pages
            .into_iter()
            .enumerate()
            .map(|(i, (name, path))| Crumb { kind: "ListItem", position: i + 1, name, item: format!("{SITE}{path}") })
            .collect();
        Breadcrumbs { context: "https://schema.org", kind: "BreadcrumbList", items }
    }
}

/// A category changes less often than the home page: a CDN keeps it for an hour, while a browser
/// still checks again after a minute.
pub(crate) async fn category(req: Req, State(source): State<Source>, Path(slug): Path<String>) -> Public<HtmlResult> {
    let t: Msg = req.t();
    let items = source.in_category(&slug);
    // The category's own name, when an item is in it; else its slug, as "Category {slug}".
    let name = items.iter().flat_map(|item| &item.categories).find(|c| c.slug == slug).map(|c| c.name.clone());
    let title = name.unwrap_or_else(|| t.category_title(&slug).to_string());
    let here = routes::Category { slug: &slug }.to_string();
    let crumbs = Breadcrumbs::new([(t.home_title().to_string(), routes::Home.to_string()), (title.clone(), here)]);
    let head = JsonLd { value: &crumbs };
    let (title, items) = (title.as_str(), items.as_slice());
    let req = &req;
    let page = req.page(html! { <Layout req={req} title={title} head={head}>{views::category(req, t, title, items)}</Layout> });
    shared(page).shared(Duration::from_secs(3600))
}

/// The item's old address, moved for good.
pub(crate) async fn old_item(req: Req, Path(id): Path<String>) -> HtmlResult {
    req.moved(&routes::Item { id: &id })
}

pub(crate) async fn show(req: Req, State(source): State<Source>, Path(id): Path<String>) -> HtmlResult {
    let item = source.item(&id).await?.ok_or_else(Error::not_found)?;
    req.page(ItemPage::new(req.t(), &item, String::new()).document(&req))
}

/// The cached item, serialized where it is.
pub(crate) async fn item_json(State(source): State<Source>, Path(id): Path<String>) -> JsonResult<Arc<ItemView>> {
    Ok(Json(source.item(&id).await?.ok_or_else(Error::not_found)?))
}

/// A partner's alert for one of its users: the form's rules, as JSON. A field the rules refuse
/// is a 422 with the fields (`?` on the rules' `Errors`), next to the `?` of the item's load.
pub(crate) async fn alert_json(
    Caller(partner): Caller<Partner>,
    State(source): State<Source>,
    Json(input): Json<CreateAlert>,
) -> JsonResult<AlertSet> {
    let t = Msg::default();
    let price = check_alert(t, &input.email, &input.target_price)?;
    let item = source.item(&input.id).await?.ok_or_else(Error::not_found)?;
    let price = listed(t, &item, price)?;
    Ok(Json(AlertSet { message: alert_set(t, &item.title, &input.email, price), partner: Some(partner.name) }))
}

pub(crate) async fn create_alert(
    req: Req,
    State(source): State<Source>,
    Path(id): Path<String>,
    Form(form): Form<AlertInput>,
) -> HtmlResult {
    let t = req.t();
    // An alert on an item that does not exist is a 404, whatever its fields say.
    let item = source.item(&id).await?.ok_or_else(Error::not_found)?;
    let price = match check_alert(t, &form.email, &form.target_price).and_then(|price| listed(t, &item, price)) {
        Ok(price) => price,
        Err(errors) => {
            let view =
                AlertFormView { id: &id, title: &form.title, email: &form.email, target_price: &form.target_price, errors };
            // Without JavaScript the whole page comes back, with the form showing the errors.
            let page = || async {
                let page = ItemPage { form: view.clone(), ..ItemPage::new(t, &item, String::new()) };
                req.page(page.document(&req))
            };
            return req.invalid(&view.errors, AlertForm { t, form: view.clone() }, page).await;
        }
    };
    let message = alert_set(t, &form.title, &form.email, price);
    // Without JavaScript the browser posted a plain form: answer with the whole page.
    let page = || async { req.page(ItemPage::new(t, &item, message.clone()).document(&req)) };
    req.patch_or(AlertMessage { message: &message }, page).await
}

/// An alert watches the item's lowest price, so an item with no listings has none to watch. No
/// one field is wrong, so the message is the whole form's, at its top.
fn listed(t: Msg, item: &ItemView, price: Minor<u64>) -> Result<Minor<u64>, Errors> {
    if !item.listings.is_empty() {
        return Ok(price);
    }
    let mut errors = Errors::new();
    errors.add_form(t.alert_no_listings());
    Err(errors)
}

/// Which item, for `items.get`.
#[derive(Deserialize, JsonSchema)]
struct ItemId {
    /// Item id, like item_17.
    id: String,
}

/// A price alert as an agent or a partner asks for it: the form's fields (same names, same
/// rules) plus the item.
#[derive(Deserialize, JsonSchema, FromJson)]
pub(crate) struct CreateAlert {
    /// Item id, like item_17.
    id: String,
    /// Where to send the alert.
    #[schemars(email)]
    email: String,
    /// Decimal dollars, like 4.50.
    target_price: String,
}

#[derive(Serialize, JsonSchema, ToJson)]
pub(crate) struct AlertSet {
    message: String,
    /// The partner the alert was set for, when a partner set it; agents do not have one.
    #[serde(skip_serializing_if = "Option::is_none")]
    partner: Option<Arc<str>>,
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
                    // The cached item, serialized where it is: no copy.
                    item.ok_or_else(|| ToolError::Invalid(format!("no item {}", input.id)))
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
                    Ok(AlertSet { message: alert_set(Msg::default(), &item.title, &input.email, price), partner: None })
                }
            },
        )
}

#[cfg(test)]
mod tests {
    use corten::axum::{
        Router,
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
        let bare = json!({ "catalog_item_id": "item_10", "title": "Bare", "market_summary": null });
        let source = Source::fixture(&json!({ "item_9": item, "item_10": bare }).to_string()).unwrap();
        super::app_with(source, Keys { partners: Partners::parse("dev=dev-key"), agents: "dev-token" }, WRITES_PER_MINUTE)
            .router()
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
        assert!(html.contains(r#"<section class="item-detail__panel item-detail__summary gap-3 sm:gap-5">"#), "{html}");
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
    async fn the_home_page_is_shared_by_caches_and_item_pages_are_not() {
        let get = |uri: &str, header: Option<(&str, &str)>| {
            let mut req = Request::get(uri);
            if let Some((name, value)) = header {
                req = req.header(name, value);
            }
            req.body(Body::empty()).unwrap()
        };
        let (_, headers, _) = send(get("/", None)).await;
        assert_eq!(headers["cache-control"], "public, max-age=60, stale-while-revalidate=600");
        let vary: Vec<_> = headers.get_all("vary").iter().map(|v| v.to_str().unwrap()).collect();
        assert!(vary.contains(&"corten, accept, accept-language") && vary.contains(&"cookie"), "{vary:?}");
        let (_, headers, _) = send(get("/", Some(("cookie", "session=abc")))).await;
        assert_eq!(headers["cache-control"], "private, no-cache", "a signed-in visitor's copy is theirs");
        let (_, headers, _) = send(get("/", Some(("corten", "nav")))).await;
        assert_eq!(headers["cache-control"], "private, max-age=60, stale-while-revalidate=600", "only the document is shared");
        let (_, headers, _) = send(get("/items/item_9", None)).await;
        assert_eq!(headers["cache-control"], "private, no-cache");
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

    /// A French reader types the price as French writes money, and is answered in French.
    #[tokio::test]
    async fn an_alert_reads_the_price_in_the_readers_locale() {
        let post = |price: &str| {
            Request::post("/items/item_9")
                .header("corten", "action")
                .header("accept-language", "fr")
                .header("content-type", "application/x-www-form-urlencoded")
                .header("sec-fetch-site", "same-origin")
                .body(Body::from(format!("title=Pika&email=a%40b.c&target_price={price}")))
                .unwrap()
        };
        let (status, _, body) = send(post("2%2C50")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("Alerte créée : nous écrirons à a@b.c quand Pika passera sous 2,50\u{a0}$US."), "{body}");
        // En's decimal point is not a French price: refused, not read as 250 dollars.
        let (status, _, body) = send(post("2.50")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("Saisissez un prix cible supérieur à zéro."), "{body}");
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
        assert!(body.contains(
            r#"value="abc" inputmode="decimal" required aria-invalid="true" aria-describedby="alert-form-target_price-error">"#
        ));
        assert!(
            body.contains(r#"<p class="field-error" id="alert-form-target_price-error">Enter a target price above zero.</p>"#)
        );
        assert!(body.contains(r#"value="a@b.c" required>"#), "a valid field keeps its value and no error");

        let (status, _, body) = send(post(None, Some("application/json"))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["error"]["fields"], json!({ "target_price": "Enter a target price above zero." }));

        let (status, _, body) = send(post(None, None)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("<html") && body.contains("Show all 7 offers") && body.contains("alert-form-target_price-error"));
    }

    /// Where the time for one item page goes. Release build, benchmark fixture:
    /// `CORTEN_FIXTURE=/path/items.json cargo test --release -p card-market -- --ignored render_cost --nocapture`
    #[tokio::test]
    #[ignore]
    async fn render_cost() {
        use std::time::Instant;
        let path = std::env::var("CORTEN_FIXTURE").expect("CORTEN_FIXTURE");
        let source = Source::fixture(&std::fs::read_to_string(&path).unwrap()).unwrap();
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
        for item in &items {
            assert_eq!(
                corten::json::to_vec(item),
                serde_json::to_vec(item).unwrap(),
                "ToJson differs from serde_json on {}",
                item.id
            );
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
        let start = Instant::now();
        let mut bytes = 0;
        for _ in 0..rounds {
            for item in &items {
                bytes += std::hint::black_box(corten::json::to_vec(item)).len();
            }
        }
        println!("item as JSON: {:.1} us ({} bytes avg)", per(start), bytes / (rounds * ids.len()));
        // The item API's responses, read as the API source reads them: the same items as serde_json.
        let text = std::fs::read_to_string(&path).unwrap();
        let raw: std::collections::HashMap<String, &serde_json::value::RawValue> = serde_json::from_str(&text).unwrap();
        let responses: Vec<&str> = ids.iter().map(|id| raw[id].get()).collect();
        for (id, response) in ids.iter().zip(&responses) {
            let ours = ItemView::from(corten::json::from_str::<crate::model::ApiItem>(response).unwrap());
            let theirs = ItemView::from(serde_json::from_str::<crate::model::ApiItem>(response).unwrap());
            // Compared as text: a NaN price is not equal to itself.
            assert_eq!(format!("{ours:?}"), format!("{theirs:?}"), "FromJson reads {id} differently from serde_json");
        }
        let parse_rounds = 5;
        let parse = |read: &dyn Fn(&str) -> usize| {
            let start = Instant::now();
            for _ in 0..parse_rounds {
                for response in &responses {
                    std::hint::black_box(read(std::hint::black_box(response)));
                }
            }
            start.elapsed().as_secs_f64() * 1e6 / (parse_rounds * responses.len()) as f64
        };
        let serde = parse(&|r| serde_json::from_str::<crate::model::ApiItem>(r).is_ok() as usize);
        let ours = parse(&|r| corten::json::from_str::<crate::model::ApiItem>(r).is_ok() as usize);
        let size = responses.iter().map(|r| r.len()).sum::<usize>() / responses.len();
        println!("item API response read: serde_json {serde:.1} us, FromJson {ours:.1} us ({size} bytes avg)");
        for (label, prefix, mode, encoding) in [
            ("navigation, identity", "/items/", Some("nav"), "identity"),
            ("document, identity", "/items/", None, "identity"),
            ("document, zstd", "/items/", None, "gzip, deflate, br, zstd"),
            ("document, br", "/items/", None, "gzip, deflate, br"),
            ("document, gzip", "/items/", None, "gzip"),
            ("json, identity", "/api/items/", None, "identity"),
        ] {
            let start = Instant::now();
            let mut size = 0;
            for _ in 0..rounds {
                for id in &ids {
                    let mut req = Request::get(format!("{prefix}{id}")).header("accept-encoding", encoding);
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
    #[ignore]
    async fn route_loop() {
        use std::time::{Duration, Instant};
        let path = std::env::var("CORTEN_FIXTURE").expect("CORTEN_FIXTURE");
        let source = Source::fixture(&std::fs::read_to_string(path).unwrap()).unwrap();
        let target = std::env::var("LOOP_TARGET").unwrap_or_else(|_| "json".into());
        let seconds: u64 = std::env::var("LOOP_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
        let (router, prefix, mode) = match target.as_str() {
            "json" => (super::app(source.clone()).router(), "/api/items/", None),
            "bare-json" => {
                // Axum alone, without Corten around the handler: what Corten's layer costs.
                let bare = corten::axum::routing::get(item_json);
                (Router::new().route(api::ItemJson::PATH, bare).with_state(source.clone()), "/api/items/", None)
            }
            "page" => (super::app(source.clone()).router(), "/items/", Some("nav")),
            "document" => (super::app(source.clone()).router(), "/items/", None),
            other => panic!("unknown LOOP_TARGET {other}"),
        };
        let paths: Vec<String> = (1..=500).map(|n| format!("{prefix}item_{n}")).collect();
        let start = Instant::now();
        let mut count = 0u64;
        while start.elapsed() < Duration::from_secs(seconds) {
            for path in &paths {
                let mut req = Request::get(path.as_str()).header("accept-encoding", "identity");
                if let Some(mode) = mode {
                    req = req.header("corten", mode);
                }
                let res = router.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
                std::hint::black_box(res.into_body().collect().await.unwrap().to_bytes());
            }
            count += paths.len() as u64;
        }
        println!("{target}: {count} requests, {:.2} us per request", start.elapsed().as_secs_f64() * 1e6 / count as f64);
    }

    #[tokio::test]
    async fn an_alert_on_an_item_with_no_listings_is_a_message_for_the_whole_form() {
        let req = Request::post("/items/item_10")
            .header("corten", "action")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("sec-fetch-site", "same-origin")
            .body(Body::from("title=Bare&email=a%40b.c&target_price=2"))
            .unwrap();
        let (status, _, body) = send(req).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            body.contains(
                r#"<p class="form-error" role="alert">This item has no listings yet, so there is no price to watch.</p>"#
            ),
            "{body}"
        );
        assert!(!body.contains("aria-invalid"), "no field is marked: {body}");
    }

    #[tokio::test]
    async fn an_alert_on_an_item_that_does_not_exist_is_not_found() {
        let req = Request::post("/items/item_404")
            .header("corten", "action")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("sec-fetch-site", "same-origin")
            .body(Body::from("title=Gone&email=a%40b.c&target_price=2"))
            .unwrap();
        let (status, _, _) = send(req).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn an_old_item_address_moves_for_good_and_a_category_carries_its_trail() {
        let (status, headers, _) = send(Request::get("/item/item_9").body(Body::empty()).unwrap()).await;
        assert_eq!((status, &headers["location"]), (StatusCode::MOVED_PERMANENTLY, &"/items/item_9".parse().unwrap()));
        let (_, headers, html) = send(Request::get("/categories/promo").body(Body::empty()).unwrap()).await;
        let cache = headers["cache-control"].to_str().unwrap();
        assert!(cache.starts_with("public, max-age=60,") && cache.contains("s-maxage=3600"), "{cache}");
        assert!(
            html.contains(r#"<script type="application/ld+json" data-corten-head>{"@context":"https://schema.org","@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Popular items","item":"https://cards.example.com/"},{"@type":"ListItem","position":2,"name":"Category promo","item":"https://cards.example.com/categories/promo"}]}</script>"#),
            "{html}"
        );
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
        assert!(offline.contains("<h1>Not available</h1>"), "{offline}");
        assert_eq!(send(Request::get("/health/ready").body(Body::empty()).unwrap()).await.0, StatusCode::OK);
        assert!(send(Request::get("/robots.txt").body(Body::empty()).unwrap()).await.2.contains("Disallow: /mcp"));
        let (status, headers, body) = send(Request::get("/items/nope").body(Body::empty()).unwrap()).await;
        assert_eq!((status, &headers["x-robots-tag"]), (StatusCode::NOT_FOUND, &"noindex".parse().unwrap()));
        assert!(body.contains("<h1>Not found</h1>"), "{body}");
        assert!(body.contains(r#"<p><a href="/">Go to popular items</a></p>"#), "a way back from a missing page: {body}");
    }

    #[tokio::test]
    async fn the_demo_catalog_lists_its_cards_on_the_home_and_category_pages() {
        let app = super::app(Source::demo()).router();
        let get = |uri: &'static str| app.clone().oneshot(Request::get(uri).body(Body::empty()).unwrap());
        let body = |res: corten::axum::response::Response| async move {
            String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
        };
        let home = body(get("/").await.unwrap()).await;
        assert!(home.contains("<strong>Ashen Drake</strong>") && home.contains("From $42.50"), "{home}");
        assert!(home.find(r#"href="/items/item_2""#).unwrap() < home.find(r#"href="/items/item_10""#).unwrap(), "in id order");
        assert!(
            home.contains(r#"/cards/item_1.svg" width="250" height="350" alt="">"#)
                && home.contains(r#"/cards/item_6.svg" width="250" height="350" alt="" loading="lazy">"#),
            "each card shows its art, sized, and past the first row loaded near the screen: {home}"
        );
        let item = body(get("/items/item_1").await.unwrap()).await;
        assert!(item.contains(r#"/cards/item_1.svg" width="220" height="308" alt="Ashen Drake card art">"#), "{item}");
        let json = body(get("/api/items/item_1").await.unwrap()).await;
        assert!(!json.contains(r#""art""#) && !json.contains("cards/"), "the JSON is the API's, without the store's art: {json}");
        // The same responses from another source are another store's cards: no art.
        let other = super::app(Source::fixture(include_str!("../fixtures/demo.json")).unwrap()).router();
        let res = other.oneshot(Request::get("/items/item_1").body(Body::empty()).unwrap()).await.unwrap();
        assert!(!body(res).await.contains("cards/"), "art is the demo catalog's alone");
        let category = body(get("/categories/tidebreak").await.unwrap()).await;
        assert!(category.contains("<h1>Tidebreak</h1>") && category.contains("Salt Oracle"), "{category}");
        assert!(!category.contains("Ashen Drake"), "only the category's cards");
        let boxed = body(get("/items/item_12").await.unwrap()).await;
        assert!(boxed.contains("Lantern Warden"), "a sealed box lists what it contains: {boxed}");
    }

    /// The item API has no list: the home page links the first ids, in the visitor's language.
    #[tokio::test]
    async fn against_the_item_api_the_home_page_links_the_first_ids() {
        let source = Source::Api { client: reqwest::Client::new(), base: "http://127.0.0.1:9".into() };
        let app = super::app(source).router();
        let req = Request::get("/").header("accept-language", "fr").body(Body::empty()).unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let home = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        assert!(home.contains(r#"<a href="/items/item_5">Article 5</a>"#), "{home}");
        assert_eq!(home.matches("<li>").count(), 50);
        let res = app.oneshot(Request::get("/categories/x").body(Body::empty()).unwrap()).await.unwrap();
        let category = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        assert!(category.contains("<h1>Category x</h1>") && !category.contains("<ul"), "{category}");
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
    async fn other_services_read_an_item_as_json() {
        let (status, headers, body) = send(Request::get("/api/items/item_9").body(Body::empty()).unwrap()).await;
        assert_eq!((status, &headers["content-type"]), (StatusCode::OK, &"application/json".parse().unwrap()));
        let item: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!((item["id"].as_str(), item["listings"].as_array().map(Vec::len)), (Some("item_9"), Some(2)));
        // The derived writer gives serde_json's bytes, escapes included.
        let view: ItemView = serde_json::from_str(&body).unwrap();
        assert_eq!(body, serde_json::to_string(&view).unwrap());
        assert!(body.contains(r#""title":"Pika <Promo> & \"Friends\"""#), "{body}");
        let (status, _, body) = send(Request::get("/api/items/nope").header("accept", "*/*").body(Body::empty()).unwrap()).await;
        assert_eq!((status, body.as_str()), (StatusCode::NOT_FOUND, r#"{"error":{"status":404,"message":"Not Found"}}"#));
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

    /// A partner's JSON write, with the key `key`.
    fn partner(key: Option<&str>, body: &'static str) -> Request<Body> {
        let mut req = Request::post("/api/alerts").header("content-type", "application/json");
        if let Some(key) = key {
            req = req.header("x-api-key", key);
        }
        req.body(Body::from(body)).unwrap()
    }

    #[tokio::test]
    async fn a_partner_sets_an_alert_with_its_api_key() {
        let alert = r#"{"id":"item_9","email":"ada@example.com","target_price":"0.40"}"#;
        for key in [None, Some("wrong")] {
            let (status, _, body) = send(partner(key, alert)).await;
            assert_eq!(
                (status, body.as_str()),
                (StatusCode::UNAUTHORIZED, r#"{"error":{"status":401,"message":"Unauthorized"}}"#)
            );
        }
        let (status, headers, body) = send(partner(Some("dev-key"), alert)).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            body,
            r#"{"message":"Alert set: we will email ada@example.com when Pika <Promo> & \"Friends\" drops below $0.40.","partner":"dev"}"#
        );
        assert_eq!(headers["cache-control"], "private, no-cache", "an answer for one partner");
        // The form's rules, as fields; a missing item; an item with no price to watch.
        let (status, _, body) = send(partner(Some("dev-key"), r#"{"id":"item_9","email":"ada","target_price":"0"}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            body.ends_with(
                r#""fields":{"email":"Enter a valid email address.","target_price":"Enter a target price above zero."}}}"#
            ),
            "{body}"
        );
        let (status, _, _) = send(partner(Some("dev-key"), r#"{"id":"nope","email":"a@b.c","target_price":"1"}"#)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, body) = send(partner(Some("dev-key"), r#"{"id":"item_10","email":"a@b.c","target_price":"1"}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains(r#""form":"This item has no listings yet, so there is no price to watch.""#), "{body}");
    }

    /// The app at `ip`, limited to `writes` a minute.
    fn limited(writes: u32, ip: [u8; 4]) -> Router {
        use corten::axum::extract::connect_info::MockConnectInfo;
        let json = json!({ "item_9": { "catalog_item_id": "item_9", "title": "Pika", "market_summary": null } }).to_string();
        let keys = Keys { partners: Partners::parse("dev=dev-key"), agents: "dev-token" };
        let app = super::app_with(Source::fixture(&json).unwrap(), keys, writes).router();
        app.layer(MockConnectInfo(std::net::SocketAddr::from((ip, 4000))))
    }

    /// A browser's post of the alert form, with a price the rules refuse.
    fn form_post() -> Request<Body> {
        Request::post("/items/item_9")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("sec-fetch-site", "same-origin")
            .header("accept", "text/html")
            .body(Body::from("email=a%40b.c&target_price=abc"))
            .unwrap()
    }

    #[tokio::test]
    async fn writes_are_limited_per_client_address_and_reads_are_not() {
        let one = limited(2, [203, 0, 113, 7]);
        for _ in 0..2 {
            assert_eq!(one.clone().oneshot(form_post()).await.unwrap().status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
        // A browser past the limit gets the error page, told when to come back.
        let (parts, body) = one.clone().oneshot(form_post()).await.unwrap().into_parts();
        let wait: u64 = parts.headers["retry-after"].to_str().unwrap().parse().unwrap();
        assert_eq!(parts.status, StatusCode::TOO_MANY_REQUESTS);
        assert!((1..=60).contains(&wait), "{wait}");
        let body = String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap();
        assert!(body.contains("<html"), "{body}");
        // A key no partner has is counted by address too, so guessing keys is limited.
        let guess = one.clone().oneshot(partner(Some("guess"), "{}")).await.unwrap();
        assert_eq!(guess.status(), StatusCode::TOO_MANY_REQUESTS);
        let body = guess.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], br#"{"error":{"status":429,"message":"Too Many Requests"}}"#);
        // Reads, preflights and agents still go through; a read keeps its cache headers.
        let home = one.clone().oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(
            (home.status(), &home.headers()["cache-control"]),
            (StatusCode::OK, &"public, max-age=60, stale-while-revalidate=600".parse().unwrap())
        );
        let preflight = Request::options("/api/alerts").body(Body::empty()).unwrap();
        assert_eq!(
            one.clone().oneshot(preflight).await.unwrap().status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "no site may call it from a browser"
        );
        let ping = Request::post("/mcp")
            .header("content-type", "application/json")
            .header("authorization", "Bearer dev-token")
            .body(Body::from(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#))
            .unwrap();
        assert_eq!(one.clone().oneshot(ping).await.unwrap().status(), StatusCode::OK);
        // Another address has its own count.
        assert_eq!(limited(2, [203, 0, 113, 8]).oneshot(form_post()).await.unwrap().status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn a_partner_is_counted_as_itself_whatever_address_it_sends_from() {
        let one = limited(1, [203, 0, 113, 9]);
        let write = || partner(Some("dev-key"), r#"{"id":"nope","email":"a@b.c","target_price":"1"}"#);
        for _ in 0..3 {
            assert_eq!(one.clone().oneshot(write()).await.unwrap().status(), StatusCode::NOT_FOUND, "past the address's 1");
        }
        let limit = WriteLimit::new(1, Partners::parse("dev=dev-key"));
        let now = Instant::now();
        for _ in 0..PARTNER_WRITES_PER_MINUTE {
            assert_eq!(limit.count(Client::Partner("dev".into()), now), None);
        }
        assert!(limit.count(Client::Partner("dev".into()), now).is_some(), "a partner has its own limit");
    }

    #[tokio::test]
    async fn a_write_whose_address_is_not_known_is_not_counted() {
        let json = json!({ "item_9": { "catalog_item_id": "item_9", "title": "Pika", "market_summary": null } }).to_string();
        let keys = Keys { partners: Partners::parse(""), agents: "" };
        let app = super::app_with(Source::fixture(&json).unwrap(), keys, 1).router();
        for _ in 0..3 {
            assert_eq!(app.clone().oneshot(form_post()).await.unwrap().status(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    fn address(ip: &str) -> Client {
        Client::address(ip.parse().unwrap())
    }

    #[test]
    fn a_new_minute_starts_the_count_again() {
        let limit = WriteLimit::new(2, Partners::parse(""));
        let start = Instant::now();
        let ip = || address("198.51.100.1");
        assert_eq!((limit.count(ip(), start), limit.count(ip(), start)), (None, None));
        assert_eq!(limit.count(ip(), start + Duration::from_secs(15)), Some(45), "seconds left in the minute");
        assert_eq!(limit.count(address("198.51.100.2"), start), None, "another address has its own count");
        assert_eq!(limit.count(ip(), start + Duration::from_secs(60)), None);
    }

    #[test]
    fn past_the_cap_a_new_client_waits_for_the_next_minute_and_counted_ones_do_not() {
        let limit = WriteLimit { clients: 2, ..WriteLimit::new(5, Partners::parse("")) };
        let (start, ip) = (Instant::now(), |n: u8| Client::address(IpAddr::from([198, 51, 100, n])));
        assert_eq!((limit.count(ip(1), start), limit.count(ip(2), start)), (None, None));
        assert_eq!(limit.count(ip(3), start + Duration::from_secs(20)), Some(40), "a third client waits");
        assert_eq!(limit.count(ip(1), start), None, "a counted client goes on");
        assert_eq!(limit.count(Client::Partner("dev".into()), start), None, "a partner is not a stranger");
        assert_eq!(limit.count(ip(3), start + Duration::from_secs(60)), None, "and the new minute has room");
    }

    #[test]
    fn an_ipv6_client_is_counted_by_its_64() {
        let limit = WriteLimit::new(1, Partners::parse(""));
        let now = Instant::now();
        assert_eq!(limit.count(address("2001:db8:1:2::1"), now), None);
        assert!(limit.count(address("2001:db8:1:2:ffff::9"), now).is_some(), "the same /64 is the same client");
        assert_eq!(limit.count(address("2001:db8:1:3::1"), now), None, "another /64 is another");
    }

    #[test]
    fn api_keys_without_a_name_or_a_key_are_left_out() {
        let partners = Partners::parse(" shop = k1 ,=k2,blank=, nokey");
        assert_eq!(partners.0.len(), 1);
        assert_eq!(partners.0.get("k1").map(|name| &**name), Some("shop"));
        assert!(Partners::parse("").0.is_empty(), "no API_KEYS, no partners");
    }
}
