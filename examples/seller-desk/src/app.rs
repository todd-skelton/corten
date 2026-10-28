//! A seller's desk: one screen with most of what client-heavy apps do, built on handlers.
//!
//! | Route | Piece |
//! |---|---|
//! | `GET /` | the inventory grid: selection, shift-click ranges, select all, filter, sort, inline price edits, a confirm dialog, live stock levels |
//! | `POST /items/{id}/price` | a price cell's form: the cell as a patch, or 422 with the error |
//! | `POST /archive` | the selected rows, archived once per submit key: a patch that removes their rows, and a status line |
//! | `GET /statement` | what the stock is worth, by set: an answer nothing may keep (`NoStore`) |
//! | `GET /feed/levels`, `/feed/levels/live` | stock levels, the feed the grid subscribes to |
//!
//! Every response says how long the server took (`Server-Timing`), from a middleware of the app's.
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use corten::{
    axum::{
        extract::Request,
        http::{HeaderValue, StatusCode},
        response::Response,
    },
    feed::{Feed, FeedResponse, Row, Since},
    i18n::Minor,
    prelude::*,
    view::Remove,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    text::{Msg, USD},
    views::{self, Line, PriceCell, RowId, StatusLine},
};

/// An item a seller lists.
#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub set: String,
    pub condition: &'static str,
    pub price_cents: u32,
    pub archived: bool,
}

/// A stock level, what the live feed sends.
#[derive(Clone, Serialize, JsonSchema)]
pub struct Level {
    pub id: String,
    pub qty: u32,
}

impl Row for Level {
    fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Clone)]
pub struct Desk {
    pub items: Arc<Mutex<Vec<Item>>>,
    pub levels: Feed<Level>,
    /// Each archive submit's key, with the ids it archived: a resend answers the same. A real
    /// store keeps them next to the write, in its transaction, and drops them after a day.
    pub archived: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

/// Cards of an invented game, the one card-market sells.
const NAMES: [&str; 8] = [
    "Ashen Drake",
    "Lantern Warden",
    "Salt Oracle",
    "Gravel Sprite",
    "Undertow Serpent",
    "Cinder Cartographer",
    "Moth Court Herald",
    "Pearl Diver",
];
const SETS: [&str; 5] = ["Hollow Crown", "Tidebreak", "Ember Reach", "Saltglass Coast", "Ironwood Vale"];
const CONDITIONS: [&str; 4] = ["Near Mint", "Lightly Played", "Moderately Played", "Damaged"];

impl Desk {
    /// `rows` items, the same every time.
    pub fn new(rows: usize) -> Self {
        let items: Vec<Item> = (0..rows)
            .map(|i| Item {
                id: format!("i{i:05}"),
                name: format!("{} #{}", NAMES[i % NAMES.len()], i / NAMES.len() + 1),
                set: SETS[i % SETS.len()].into(),
                condition: CONDITIONS[i % CONDITIONS.len()],
                price_cents: 99 + (i as u32 * 7919) % 50_000,
                archived: false,
            })
            .collect();
        let levels = Feed::new(items.iter().enumerate().map(|(i, it)| Level { id: it.id.clone(), qty: (i as u32 * 31) % 40 }));
        Desk { items: Arc::new(Mutex::new(items)), levels, archived: Arc::default() }
    }

    fn quantities(&self) -> HashMap<String, u32> {
        self.levels.read(|rows| rows.values().map(|l| (l.id.clone(), l.qty)).collect())
    }
}

pub fn app(desk: Desk) -> App<ServerTiming> {
    App::new(url::router(desk)).components(views::COMPONENTS).messages::<Msg>().middleware(ServerTiming)
}

/// Tells the browser's developer tools how long the server took to answer (`Server-Timing`): the
/// time to the response's headers, so a feed's stream counts until it starts.
pub struct ServerTiming;

impl Middleware for ServerTiming {
    async fn call(&self, req: Request, next: impl Next) -> Response {
        let start = Instant::now();
        let mut res = next.run(req).await;
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        if let Ok(value) = HeaderValue::from_str(&format!("app;dur={ms:.1}")) {
            res.headers_mut().insert("server-timing", value);
        }
        res
    }
}

/// Every URL the app serves and links to, with the handlers that serve it, which share the desk.
#[corten::routes(state = Desk)]
pub mod url {
    #[route("/", get = home)]
    pub struct Home;

    #[route("/items/{id}/price", post = set_price)]
    pub struct SetPrice<'a> {
        pub id: &'a str,
    }

    #[route("/archive", post = archive)]
    pub struct Archive;

    #[route("/statement", get = statement)]
    pub struct Statement;

    #[route("/feed/levels", get = levels)]
    pub struct Levels;

    #[route("/feed/levels/live", get = levels_live)]
    pub struct LevelsLive;
}

/// The stock levels feed: a snapshot, then live changes from a cursor.
async fn levels(State(desk): State<Desk>) -> FeedResponse {
    desk.levels.snapshot()
}

async fn levels_live(State(desk): State<Desk>, since: Since) -> FeedResponse {
    desk.levels.live(since)
}

async fn home(req: Req, State(desk): State<Desk>) -> HtmlResult {
    let qty = desk.quantities();
    let items = desk.items.lock().expect("items").clone();
    let lines: Vec<Line> = items.iter().filter(|i| !i.archived).map(|item| Line { item, qty: qty[&item.id] }).collect();
    let view = views::desk(&lines);
    req.page(html! { <views::Layout req={&req} title="Inventory">{view}</views::Layout> })
}

#[derive(Deserialize, Fields)]
pub struct PriceInput {
    pub price: String,
}

/// Cents from "12.50" or "$12.50", above zero.
fn parse_price(text: &str) -> Option<u32> {
    let text = text.trim().trim_start_matches('$');
    let (whole, cents) = text.split_once('.').unwrap_or((text, "0"));
    if whole.is_empty() || cents.len() > 2 || !whole.bytes().chain(cents.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let cents: u32 = format!("{cents:0<2}").parse().ok()?;
    let value = whole.parse::<u32>().ok()?.checked_mul(100)?.checked_add(cents)?;
    (value > 0).then_some(value)
}

async fn set_price(req: Req, State(desk): State<Desk>, Path(id): Path<String>, Form(input): Form<PriceInput>) -> HtmlResult {
    let f = PriceInput::FIELDS;
    let Some(cents) = parse_price(&input.price) else {
        let mut errors = Errors::new();
        errors.add(f.price, "Enter a price above zero, like 12.50.");
        let cell = PriceCell { id: &id, cents: None, typed: &input.price, errors: &errors };
        return req.invalid(&errors, cell, || async { req.redirect(&url::Home) }).await;
    };
    {
        let mut items = desk.items.lock().expect("items");
        let item = items.iter_mut().find(|i| i.id == id).ok_or_else(|| corten::Error::status(StatusCode::NOT_FOUND))?;
        item.price_cents = cents;
    }
    let none = Errors::new();
    let cell = PriceCell { id: &id, cents: Some(cents), typed: "", errors: &none };
    req.patch_or(cell, || async { req.redirect(&url::Home) }).await
}

#[derive(Deserialize, Fields)]
pub struct ArchiveInput {
    /// The checked rows' ids, one `ids` per checkbox: each row's box belongs to the archive form.
    #[serde(default)]
    pub ids: Vec<String>,
    /// This submit, so one sent again (its answer lost on the way) archives once.
    pub submit: SubmitKey,
}

async fn archive(req: Req, State(desk): State<Desk>, Form(input): Form<ArchiveInput>) -> HtmlResult {
    // Checked and recorded under one lock, so two copies of a submit cannot both apply.
    let done = {
        let mut archived = desk.archived.lock().expect("archived");
        match archived.get(input.submit.as_str()) {
            Some(done) => done.clone(),
            None => {
                let mut items = desk.items.lock().expect("items");
                let mut done = Vec::new();
                for item in items.iter_mut().filter(|i| input.ids.contains(&i.id) && !i.archived) {
                    item.archived = true;
                    done.push(item.id.clone());
                }
                archived.insert(input.submit.as_str().to_string(), done.clone());
                done
            }
        }
    };
    // A view renders only when it is sent, so the redirect does not format the status.
    let removed = html! {
        for id in &done { {Remove(RowId(id))} }
        <StatusLine text={&format!("Archived {} item{}.", done.len(), if done.len() == 1 { "" } else { "s" })} />
    };
    req.patch_or(removed, || async { req.redirect(&url::Home) }).await
}

/// What the listed stock is worth, by set. Amounts a seller would not want left in a shared
/// computer's cache or restored by its back button, so nothing may keep the page.
async fn statement(req: Req, State(desk): State<Desk>) -> NoStore<HtmlResult> {
    let mut sets: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    let qty = desk.quantities();
    for item in desk.items.lock().expect("items").iter().filter(|i| !i.archived) {
        let set = sets.entry(item.set.clone()).or_default();
        set.0 += 1;
        set.1 += u64::from(item.price_cents) * u64::from(qty[&item.id]);
    }
    let t: Msg = req.t();
    NoStore(req.page(html! {
        <views::Layout req={&req} title={t.statement_title()}>
            <h1>{t.statement_title()}</h1>
            <table>
                <thead>
                    <tr>
                        <th>{t.statement_set()}</th>
                        <th>{t.statement_items()}</th>
                        <th>{t.statement_value()}</th>
                    </tr>
                </thead>
                <tbody>
                    for (name, (items, cents)) in &sets {
                        <tr>
                            <td>{name}</td>
                            <td>{t.number(*items)}</td>
                            <td>{t.money(Minor(*cents), USD)}</td>
                        </tr>
                    }
                </tbody>
            </table>
        </views::Layout>
    }))
}

/// Sells and restocks a few items every 250 ms, so the grid's live levels have something to show.
pub async fn simulate_sales(desk: Desk) {
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    let count = desk.items.lock().expect("items").len().max(1);
    let mut n = 0usize;
    loop {
        tick.tick().await;
        desk.levels.write(|tx| {
            for k in 0..4 {
                let id = format!("i{:05}", (n * 7 + k * 131) % count);
                if let Some(level) = tx.get_mut(&id) {
                    level.qty = if level.qty == 0 { 12 } else { level.qty - 1 };
                }
            }
        });
        n = n.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use corten::axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    async fn send(desk: &Desk, req: Request<Body>) -> (StatusCode, String) {
        let (status, _, body) = send_full(desk, req).await;
        (status, body)
    }

    async fn send_full(desk: &Desk, req: Request<Body>) -> (StatusCode, corten::axum::http::HeaderMap, String) {
        let res = app(desk.clone()).router().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        (parts.status, parts.headers, String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap())
    }

    /// Two submit keys, as the page draws them.
    const KEY: &str = "3f2504e0-4f89-41d3-9a0c-0305e82c3301";
    const OTHER: &str = "9b2c7f0e-1d2a-4c3b-8e4f-5a6b7c8d9e0f";

    fn post(uri: &str, body: &str) -> Request<Body> {
        Request::post(uri)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("corten", "action")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[test]
    fn prices_parse_like_people_type_them() {
        assert_eq!(parse_price("12.5"), Some(1250));
        assert_eq!(parse_price("$3"), Some(300));
        assert_eq!(parse_price("0"), None);
        assert_eq!(parse_price("1.234"), None);
        assert_eq!(parse_price("abc"), None);
    }

    /// The scripts' data is checked against these declarations by `npm run check`.
    #[test]
    fn script_types_match_the_server() {
        corten::types::Types::new()
            .add::<Level>()
            .components(views::COMPONENTS)
            .sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
    }

    #[tokio::test]
    async fn the_desk_renders_every_row_with_its_handlers() {
        let desk = Desk::new(20);
        let (status, html) = send(&desk, Request::get("/").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(html.matches(r#"<tr data-key=""#).count(), 20, "every row is an InventoryRow, keyed by its item: {html}");
        assert!(html.contains("/scripts/inventory_grid.js\">") && html.contains("/scripts/price_cell.js\">"), "{html}");
        let sel = "{body:@body,all:@all,count:@count,bar:@bar}";
        assert!(
            html.contains(&format!(r#"data-on="click:inventory_grid.toggle($e,^select,$c,{sel})""#)),
            "one handler for every row: {html}"
        );
        assert!(html.contains(r#"<tr data-key="i00003" id="row-i00003"><td><input data-ref="select""#), "{html}");
        assert!(html.contains(r#"data-on="connect:inventory_grid.live(@body,"#), "{html}");
        assert!(
            html.contains("inventory_grid.priceEdited($e,$c,@body,@unsent)"),
            "a cell's typed event reaches the grid: {html}"
        );
    }

    #[tokio::test]
    async fn a_price_edit_patches_its_cell_or_says_what_is_wrong() {
        let desk = Desk::new(3);
        let (status, html) = send(&desk, post("/items/i00001/price", "price=12.5")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.starts_with(r#"<form data-scope="price-cell" id="price-i00001""#), "{html}");
        assert!(html.contains(r#"value="12.50""#), "{html}");
        let (status, html) = send(&desk, post("/items/i00001/price", "price=abc")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(html.contains(r#"value="abc""#) && html.contains(r#"aria-invalid="true""#), "{html}");
    }

    #[tokio::test]
    async fn archiving_hides_the_rows_and_says_how_many() {
        let desk = Desk::new(5);
        let (_, html) = send(&desk, post("/archive", &format!("ids=i00001&ids=i00003&submit={KEY}"))).await;
        assert!(
            html.contains(
                r#"<template id="row-i00001" data-remove></template><template id="row-i00003" data-remove></template>"#
            ),
            "{html}"
        );
        assert!(html.contains("Archived 2 items."), "{html}");
        let (_, page) = send(&desk, Request::get("/").body(Body::empty()).unwrap()).await;
        assert_eq!(page.matches(r#"<tr data-key=""#).count(), 3);
        assert!(page.contains(r#"<input type="hidden" name="submit" data-submit value=""#), "the form carries a key: {page}");
    }

    #[tokio::test]
    async fn an_archive_sent_again_with_its_key_applies_once_and_answers_the_same() {
        let desk = Desk::new(5);
        let first = send(&desk, post("/archive", &format!("ids=i00001&submit={KEY}"))).await;
        // The answer was lost; the runtime sends the same submit again, now with another row checked.
        let again = send(&desk, post("/archive", &format!("ids=i00001&ids=i00002&submit={KEY}"))).await;
        assert_eq!(first, again, "a resend is answered with what the first did");
        assert!(again.1.contains("Archived 1 item."), "{}", again.1);
        let (_, page) = send(&desk, Request::get("/").body(Body::empty()).unwrap()).await;
        assert_eq!(page.matches(r#"<tr data-key=""#).count(), 4, "i00002 is still listed");
        let (_, next) = send(&desk, post("/archive", &format!("ids=i00002&submit={OTHER}"))).await;
        assert!(next.contains("Archived 1 item."), "a new key is a new submit: {next}");
        let (status, _) = send(&desk, post("/archive", "ids=i00003")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "a submit without its key is refused");
    }

    #[tokio::test]
    async fn the_statement_is_kept_nowhere_and_a_page_says_how_long_it_took() {
        let desk = Desk::new(10);
        let (status, headers, html) = send_full(&desk, Request::get("/statement").body(Body::empty()).unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers["cache-control"], "no-store");
        assert!(html.contains("<td>Hollow Crown</td><td>2</td><td>$"), "the stock value in the locale's money format: {html}");
        let (_, headers, _) = send_full(&desk, Request::get("/").body(Body::empty()).unwrap()).await;
        let timing = headers["server-timing"].to_str().unwrap();
        assert!(timing.starts_with("app;dur="), "{timing}");
    }

    /// Server cost of the desk at a realistic size: `cargo test -p seller-desk --release -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn render_cost() {
        for rows in [100, 500, 2000] {
            let desk = Desk::new(rows);
            let qty = desk.quantities();
            let items = desk.items.lock().unwrap().clone();
            let lines: Vec<Line> = items.iter().map(|item| Line { item, qty: qty[&item.id] }).collect();
            let html = views::desk(&lines).to_html();
            let rounds = 200;
            let start = std::time::Instant::now();
            for _ in 0..rounds {
                std::hint::black_box(views::desk(std::hint::black_box(&lines)).to_html());
            }
            let us = start.elapsed().as_secs_f64() * 1e6 / rounds as f64;
            let handlers: usize = html.matches(" data-on=\"").count();
            let on_bytes: usize = html.split(" data-on=\"").skip(1).map(|s| s.find('"').unwrap_or(0) + 10).sum();
            let ref_bytes: usize = html.split(" data-ref=\"").skip(1).map(|s| s.find('"').unwrap_or(0) + 11).sum();
            let scope_bytes: usize = html.split(" data-scope=\"").skip(1).map(|s| s.find('"').unwrap_or(0) + 13).sum();
            println!(
                "{rows} rows: render {us:.0} us, {} bytes ({} per row); handler markup {on_bytes} B in {handlers} data-on, refs {ref_bytes} B, scopes {scope_bytes} B",
                html.len(),
                html.len() / rows
            );
        }
    }
}
