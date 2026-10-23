//! A seller's desk: one screen with most of what client-heavy apps do, built on handlers.
//!
//! | Route | Piece |
//! |---|---|
//! | `GET /` | the inventory grid: selection, shift-click ranges, select all, filter, sort, inline price edits, a confirm dialog, live stock levels |
//! | `POST /items/{id}/price` | a price cell's form: the cell as a patch, or 422 with the error |
//! | `POST /archive` | the selected rows, archived: a patch that removes their rows, and a status line |
//! | `GET /feed/levels`, `/feed/levels/live` | stock levels, the feed the grid subscribes to |
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use corten::{
    axum::http::StatusCode,
    feed::{Feed, Row, Since},
    prelude::*,
    view::Remove,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::views::{self, Line, PriceCell, RowId, StatusLine};

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
}

const NAMES: [&str; 8] = ["Pikachu", "Charizard", "Mewtwo", "Gengar", "Eevee", "Snorlax", "Lucario", "Gardevoir"];
const SETS: [&str; 5] = ["Base Set", "Jungle", "Fossil", "Team Rocket", "Neo Genesis"];
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
        Desk { items: Arc::new(Mutex::new(items)), levels }
    }

    fn quantities(&self) -> HashMap<String, u32> {
        self.levels.read(|rows| rows.values().map(|l| (l.id.clone(), l.qty)).collect())
    }
}

pub fn app(desk: Desk) -> App {
    App::new(routes(desk)).shell(views::shell).components(views::COMPONENTS)
}

/// Every URL the app serves and links to.
pub mod url {
    use corten::Route;

    #[derive(Route)]
    #[route("/")]
    pub struct Home;

    #[derive(Route)]
    #[route("/items/{id}/price")]
    pub struct SetPrice<'a> {
        pub id: &'a str,
    }

    #[derive(Route)]
    #[route("/archive")]
    pub struct Archive;

    #[derive(Route)]
    #[route("/feed/levels")]
    pub struct Levels;

    #[derive(Route)]
    #[route("/feed/levels/live")]
    pub struct LevelsLive;
}

fn routes(desk: Desk) -> Router {
    Router::new()
        .route(url::Home::PATH, get(home))
        .route(url::SetPrice::PATH, post(set_price))
        .route(url::Archive::PATH, post(archive))
        .route(url::Levels::PATH, get(|State(d): State<Desk>| async move { d.levels.snapshot() }))
        .route(url::LevelsLive::PATH, get(|State(d): State<Desk>, since: Since| async move { d.levels.live(since) }))
        .with_state(desk)
}

async fn home(req: Req, State(desk): State<Desk>) -> corten::Result {
    let qty = desk.quantities();
    let items = desk.items.lock().expect("items").clone();
    let lines: Vec<Line> = items.iter().filter(|i| !i.archived).map(|item| Line { item, qty: qty[&item.id] }).collect();
    req.page(Meta::new("Inventory"), views::desk(&lines))
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

async fn set_price(req: Req, State(desk): State<Desk>, Path(id): Path<String>, Form(input): Form<PriceInput>) -> corten::Result {
    let f = PriceInput::FIELDS;
    let Some(cents) = parse_price(&input.price) else {
        let mut errors = Errors::new();
        errors.add(f.price, "Enter a price above zero, like 12.50.");
        let cell = PriceCell { id: &id, cents: None, typed: &input.price, errors: &errors };
        return req.invalid(&errors, cell, || async { req.redirect(&url::Home) }).await;
    };
    let mut items = desk.items.lock().expect("items");
    let item = items.iter_mut().find(|i| i.id == id).ok_or_else(|| corten::Error::status(StatusCode::NOT_FOUND))?;
    item.price_cents = cents;
    drop(items);
    if req.mode == Mode::Action {
        return req.patch(PriceCell { id: &id, cents: Some(cents), typed: "", errors: &Errors::new() });
    }
    req.redirect(&url::Home)
}

#[derive(Deserialize, Fields)]
pub struct ArchiveInput {
    /// The selected rows' ids, comma-separated: the grid's handler writes them.
    pub ids: String,
}

async fn archive(req: Req, State(desk): State<Desk>, Form(input): Form<ArchiveInput>) -> corten::Result {
    let ids: Vec<&str> = input.ids.split(',').filter(|s| !s.is_empty()).collect();
    let mut items = desk.items.lock().expect("items");
    let mut done = Vec::new();
    for item in items.iter_mut().filter(|i| ids.contains(&i.id.as_str()) && !i.archived) {
        item.archived = true;
        done.push(item.id.clone());
    }
    drop(items);
    if req.mode != Mode::Action {
        return req.redirect(&url::Home);
    }
    req.patch(html! {
        for id in &done { {Remove(RowId(id))} }
        <StatusLine text={&format!("Archived {} item{}.", done.len(), if done.len() == 1 { "" } else { "s" })} />
    })
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
        let res = app(desk.clone()).router().oneshot(req).await.unwrap();
        let status = res.status();
        (status, String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap())
    }

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
        let (_, html) = send(&desk, post("/archive", "ids=i00001%2Ci00003")).await;
        assert!(
            html.contains(
                r#"<template id="row-i00001" data-remove></template><template id="row-i00003" data-remove></template>"#
            ),
            "{html}"
        );
        assert!(html.contains("Archived 2 items."), "{html}");
        let (_, page) = send(&desk, Request::get("/").body(Body::empty()).unwrap()).await;
        assert_eq!(page.matches(r#"<tr data-key=""#).count(), 3);
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
