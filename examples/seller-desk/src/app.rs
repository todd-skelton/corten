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
    axum::{extract::Request, http::HeaderValue, response::Response},
    feed::{Feed, FeedResponse, Row, Since},
    prelude::*,
    view::Remove,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    text::{Msg, USD},
    views::{self, ItemCount, Line, PriceCell, RowId, StatementTable, StatusLine},
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

/// The game card-market sells: its three sets of 180 cards.
const SETS: [&str; 3] = ["Ember Reach", "Hollow Crown", "Tidebreak"];
const CARDS: usize = 180 * SETS.len();

/// The singles card-market's store lists (`fixtures/demo.json`), with the set and the lowest
/// Near Mint price there, so the same card costs the same in both apps (a test reads the fixture).
const LISTED: [(&str, &str, u32); 11] = [
    ("Ashen Drake", "Ember Reach", 4250),
    ("Lantern Warden", "Hollow Crown", 348),
    ("Salt Oracle", "Tidebreak", 11800),
    ("Gravel Sprite", "Ember Reach", 37),
    ("Crown of Thorns and Glass", "Hollow Crown", 2780),
    ("Undertow Serpent", "Tidebreak", 210),
    ("Cinder Cartographer", "Ember Reach", 1027),
    ("Moth Court Herald", "Hollow Crown", 50),
    ("Pearl Diver", "Tidebreak", 150),
    ("Kiln Colossus", "Ember Reach", 8600),
    ("Regent of Ash", "Hollow Crown", 6475),
];

/// The rest of the sets' names: a word and a figure, each pair once. No word is a set's, so a
/// filter for a set finds that set's cards.
const PLACES: [&str; 24] = [
    "Ashen", "Lantern", "Salt", "Gravel", "Undertow", "Cinder", "Pearl", "Kiln", "Fallow", "Rime", "Sable", "Iron", "Glass",
    "Thorn", "Silt", "Brine", "Ivory", "Copper", "Lichen", "Storm", "Dusk", "Marsh", "Quarry", "Gilded",
];
const BEINGS: [&str; 24] = [
    "Drake",
    "Warden",
    "Oracle",
    "Sprite",
    "Serpent",
    "Cartographer",
    "Diver",
    "Colossus",
    "Smith",
    "Hound",
    "Witch",
    "Pilgrim",
    "Sentinel",
    "Weaver",
    "Tyrant",
    "Scholar",
    "Courier",
    "Gardener",
    "Hermit",
    "Duelist",
    "Shepherd",
    "Archivist",
    "Lancer",
    "Mystic",
];

/// A copy's condition, and the share of a Near Mint copy's price it sells for.
const CONDITIONS: [(&str, u32); 4] = [("Near Mint", 100), ("Lightly Played", 85), ("Moderately Played", 65), ("Damaged", 40)];

/// Every card in the sets, as `(name, set, Near Mint price in cents)`: card-market's singles first,
/// then the rest, priced by a rarity each card draws once (most are commons).
fn cards() -> Vec<(String, &'static str, u32)> {
    let mut cards: Vec<_> = LISTED.iter().map(|&(name, set, cents)| (name.to_string(), set, cents)).collect();
    // Each place meets each being once, in an order that does not run through one place.
    let pairs = (0..PLACES.len() * BEINGS.len()).map(|k| {
        let (place, round) = (k % PLACES.len(), k / PLACES.len());
        format!("{} {}", PLACES[place], BEINGS[(place + round) % BEINGS.len()])
    });
    for name in pairs {
        if cards.len() == CARDS {
            break;
        }
        if cards.iter().any(|(listed, ..)| *listed == name) {
            continue;
        }
        let k = cards.len() as u32;
        let rarity = k.wrapping_mul(2_654_435_761) % 1000;
        let cents = match rarity {
            0..20 => 2000 + rarity * 7919 % 13000,
            20..100 => 200 + rarity * 7919 % 3800,
            100..300 => 25 + rarity * 104_729 % 275,
            _ => 10 + rarity * 7919 % 40,
        };
        cards.push((name, SETS[cards.len() % SETS.len()], cents));
    }
    cards
}

/// A number that looks random but is the same every run, from `n`.
fn draw(n: usize) -> u32 {
    (n as u32).wrapping_mul(2_654_435_761) >> 16
}

/// How many copies a seller keeps of a card at `cents`: a few of the dear ones, many commons.
/// The most is what a sold-out card is restocked to.
fn stock(cents: u32) -> std::ops::RangeInclusive<u32> {
    match cents {
        2000.. => 0..=3,
        200.. => 1..=8,
        25.. => 4..=20,
        _ => 8..=40,
    }
}

impl Desk {
    /// `rows` items, the same every time: each card in the sets once, most in Near Mint and some
    /// played, then each again in its next condition, at that condition's share of the price.
    /// Past every card in every condition (2160 rows, more than any page shows) the rows start over.
    pub fn new(rows: usize) -> Self {
        let cards = cards();
        let items: Vec<Item> = (0..rows)
            .map(|i| {
                let card = i % cards.len();
                let (name, set, cents) = &cards[card];
                // card-market's singles start in Near Mint, at its price; seven in ten others do.
                let first = if card < LISTED.len() { 0 } else { [0, 0, 0, 0, 0, 0, 0, 1, 2, 3][draw(card) as usize % 10] };
                let (condition, share) = CONDITIONS[(first + i / cards.len()) % CONDITIONS.len()];
                Item {
                    id: format!("i{i:05}"),
                    name: name.clone(),
                    set: (*set).into(),
                    condition,
                    price_cents: (cents * share / 100).max(1),
                    archived: false,
                }
            })
            .collect();
        let levels = Feed::new(items.iter().enumerate().map(|(i, it)| {
            let range = stock(it.price_cents);
            Level { id: it.id.clone(), qty: range.start() + draw(i + 3) % (range.end() - range.start() + 1) }
        }));
        Desk { items: Arc::new(Mutex::new(items)), levels, archived: Arc::default() }
    }

    fn quantities(&self) -> HashMap<String, u32> {
        self.levels.read(|rows| rows.values().map(|l| (l.id.clone(), l.qty)).collect())
    }
}

pub fn app(desk: Desk) -> App<ServerTiming> {
    App::new(url::router(desk)).assets(views::Static).components(views::COMPONENTS).messages::<Msg>().middleware(ServerTiming)
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
    let view = views::desk(&lines, req.t());
    req.page(html! { <views::Layout req={&req} title="Inventory">{view}</views::Layout> })
}

#[derive(Deserialize, Fields)]
pub struct PriceInput {
    pub price: String,
}

/// Cents from a price as the reader's locale writes money ("12.50", "$1,299.50"), above zero.
fn parse_price(t: Msg, text: &str) -> Option<u32> {
    let cents = u32::try_from(t.parse_money(text, USD)?.0).ok()?;
    (cents > 0).then_some(cents)
}

async fn set_price(
    req: Req,
    State(desk): State<Desk>,
    Params(at): Params<url::SetPrice<'_>>,
    Form(input): Form<PriceInput>,
) -> HtmlResult {
    let (f, id) = (PriceInput::FIELDS, at.id);
    let Some(cents) = parse_price(req.t(), &input.price) else {
        let mut errors = Errors::new();
        errors.add(f.price, "Enter a price above zero, like 12.50.");
        let cell = PriceCell { id: &id, cents: None, typed: &input.price, errors: &errors };
        return req.invalid(&errors, cell, || async { req.redirect(&url::Home) }).await;
    };
    {
        let mut items = desk.items.lock().expect("items");
        let item = items.iter_mut().find(|i| i.id == id).ok_or_else(corten::Error::not_found)?;
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
    // The count left is taken under the same locks, so of two archives the one counted later
    // is the one that archived later.
    let (done, left) = {
        let mut archived = desk.archived.lock().expect("archived");
        let mut items = desk.items.lock().expect("items");
        let done = match archived.get(input.submit.as_str()) {
            Some(done) => done.clone(),
            None => {
                let mut done = Vec::new();
                for item in items.iter_mut().filter(|i| input.ids.contains(&i.id) && !i.archived) {
                    item.archived = true;
                    done.push(item.id.clone());
                }
                archived.insert(input.submit.as_str().to_string(), done.clone());
                done
            }
        };
        (done, items.iter().filter(|i| !i.archived).count())
    };
    let t = req.t();
    // A view renders only when it is sent, so the redirect does not format the status.
    let removed = html! {
        for id in &done { {Remove(RowId(id))} }
        <ItemCount t={t} count={left} />
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
    let sets: Vec<(&str, usize, u64)> = sets.iter().map(|(name, (items, cents))| (name.as_str(), *items, *cents)).collect();
    NoStore(req.page(html! {
        <views::Layout req={&req} title={t.statement_title()}>
            <h1>{t.statement_title()}</h1>
            <StatementTable t={t} sets={&sets} />
        </views::Layout>
    }))
}

/// Sells and restocks a few items every 250 ms, so the grid's live levels have something to show.
pub async fn simulate_sales(desk: Desk) {
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    // What each row is restocked to when it sells out: the most a seller keeps at the price it
    // was listed at.
    let restock: Vec<u32> =
        desk.items.lock().expect("items").iter().map(|item| (*stock(item.price_cents).end()).max(1)).collect();
    let count = restock.len().max(1);
    let mut n = 0usize;
    loop {
        tick.tick().await;
        desk.levels.write(|tx| {
            for k in 0..4 {
                let row = (n * 7 + k * 131) % count;
                if let Some(level) = tx.get_mut(&format!("i{row:05}")) {
                    level.qty = if level.qty == 0 { restock[row] } else { level.qty - 1 };
                }
            }
        });
        n = n.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use corten::axum::{
        body::Body,
        http::{Request, StatusCode},
    };
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

    /// The cards card-market lists cost the same here, in the same set, as its cheapest Near Mint
    /// copy; every name is a card's, and each set has its 180.
    #[test]
    fn the_desk_sells_card_markets_cards_at_its_prices() {
        #[derive(serde::Deserialize)]
        struct Listing {
            product_summary: String,
            price_amount: String,
        }
        #[derive(serde::Deserialize)]
        struct Product {
            title: String,
            subtitle: String,
            market_listings: Vec<Listing>,
        }
        let fixture: HashMap<String, Product> =
            serde_json::from_str(include_str!("../../card-market/fixtures/demo.json")).expect("card-market's fixture");
        let mut checked = 0;
        for product in fixture.values().filter(|p| !p.subtitle.ends_with("Sealed")) {
            let title = &product.title;
            let (_, set, cents) =
                LISTED.iter().find(|(name, ..)| name == title).unwrap_or_else(|| panic!("{title} is not on the desk"));
            assert!(product.subtitle.starts_with(set), "{title}: {}", product.subtitle);
            let near_mint = product
                .market_listings
                .iter()
                .filter(|l| l.product_summary == "Near Mint")
                .map(|l| l.price_amount.replace('.', "").parse::<u32>().unwrap())
                .min();
            assert_eq!(near_mint, Some(*cents), "{title}");
            checked += 1;
        }
        assert_eq!(checked, LISTED.len(), "every listed single is in the fixture");

        let cards = cards();
        assert_eq!(cards.len(), CARDS);
        let names: std::collections::HashSet<_> = cards.iter().map(|(name, ..)| name.as_str()).collect();
        assert_eq!(names.len(), CARDS, "no name twice");
        for set in SETS {
            assert_eq!(cards.iter().filter(|(_, s, _)| *s == set).count(), 180, "{set}");
        }
        let desk = Desk::new(CARDS + 1);
        let items = desk.items.lock().unwrap();
        assert_eq!((items[0].name.as_str(), items[0].condition, items[0].price_cents), ("Ashen Drake", "Near Mint", 4250));
        assert_eq!(
            (items[CARDS].name.as_str(), items[CARDS].condition, items[CARDS].price_cents),
            ("Ashen Drake", "Lightly Played", 3612)
        );
        for (item, (name, _, cents)) in items.iter().zip(LISTED) {
            assert_eq!(
                (item.name.as_str(), item.condition, item.price_cents),
                (name, "Near Mint", cents),
                "card-market's singles come first"
            );
        }
        assert!(desk.quantities()["i00000"] > 0, "the first row has a copy to show");
        let played = items[..CARDS].iter().filter(|item| item.condition != "Near Mint").count();
        assert!((100..250).contains(&played), "some cards start played: {played}");
        let qty = desk.quantities();
        let dear: Vec<_> = items.iter().filter(|item| item.price_cents >= 2000).collect();
        assert!(!dear.is_empty() && dear.iter().all(|item| qty[&item.id] <= 3), "a few copies of a dear card");
    }

    #[test]
    fn prices_parse_like_people_type_them() {
        let t = Msg::default();
        assert_eq!(parse_price(t, "12.5"), Some(1250));
        assert_eq!(parse_price(t, "$3"), Some(300));
        assert_eq!(parse_price(t, "$1,299.50"), Some(129950));
        assert_eq!(parse_price(t, "0"), None);
        assert_eq!(parse_price(t, "1.234"), None);
        assert_eq!(parse_price(t, "abc"), None);
        assert_eq!(parse_price(t, "50000000"), None, "over a u32 of cents");
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
            html.contains(&format!(r#"data-on="click:inventory_grid.toggle($e,^select,{sel})""#)),
            "one handler for every row: {html}"
        );
        assert!(html.contains(r#"<tr data-key="i00003" id="row-i00003"><td><input data-ref="select""#), "{html}");
        assert!(html.contains(r#"data-on="connect:inventory_grid.live(@body,"#), "{html}");
        let unsent = r#"{note:@note,one:%[&quot;&quot;,&quot;count&quot;,&quot; price not saved&quot;],other:%[&quot;&quot;,&quot;count&quot;,&quot; prices not saved&quot;]}"#;
        assert!(
            html.contains(&format!("inventory_grid.priceEdited($e,@body,{unsent})")),
            "a cell's typed event reaches the grid, with the catalog's words for the note: {html}"
        );
        assert!(
            html.contains(r#"commandfor="archive-dialog">Archive</button>"#) && html.contains(r#"<dialog id="archive-dialog">"#),
            "the bulk bar's button opens the dialog without a script: {html}"
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
        assert!(
            html.contains(r#"<th scope="row">Hollow Crown</th><td>3</td><td>$"#),
            "the stock value in the locale's money format: {html}"
        );
        assert!(html.contains(r#"<tfoot><tr><th scope="row">Total</th><td>10</td><td>$"#), "a total of every set: {html}");
        let (_, headers, home) = send_full(&desk, Request::get("/").body(Body::empty()).unwrap()).await;
        assert!(home.contains(r#"<p id="item-count""#) && home.contains(">10 items</p>"), "the grid's count: {home}");
        let timing = headers["server-timing"].to_str().unwrap();
        assert!(timing.starts_with("app;dur="), "{timing}");
    }

    #[test]
    fn a_row_at_quantity_zero_is_marked_sold_out() {
        let desk = Desk::new(2);
        let items = desk.items.lock().unwrap().clone();
        let html = |qty| views::desk(&[Line { item: &items[0], qty }], Msg::default()).to_html();
        assert!(html(0).contains("data-sold-out"), "{}", html(0));
        assert!(!html(3).contains("data-sold-out"), "{}", html(3));
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
            let html = views::desk(&lines, Msg::default()).to_html();
            let rounds = 200;
            let start = std::time::Instant::now();
            for _ in 0..rounds {
                std::hint::black_box(views::desk(std::hint::black_box(&lines), Msg::default()).to_html());
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
