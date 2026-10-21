//! A watchlist: one screen that uses every Corten piece.
//!
//! | Route | Piece |
//! |---|---|
//! | `GET /` | `req.screen`: HTML for browsers, `{"title","data"}` for `Accept: application/json` |
//! | `POST /watch` | form patch with JavaScript, full page without, JSON for data clients |
//! | `GET /feed/cards`, `/feed/cards/live` | the live feed `<live-prices>` subscribes to |
//! | `GET /about` | a plain page, reached by client navigation |
//! | `POST /sign-in` | sets a cookie; the shell shows the viewer, and the runtime reloads the document |
use std::time::Duration;

use corten::{
    axum::{http::header, response::IntoResponse},
    feed::{Feed, Row, Since},
    prelude::*,
    security::cookie::{self, SetCookie},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct Market {
    pub cards: Feed<Card>,
}

impl Market {
    pub fn new() -> Self {
        let cards =
            [("c1", "Charizard ex", 12_999), ("c2", "Umbreon VMAX", 54_000), ("c3", "Pikachu with Grey Felt Hat", 31_050)];
        Market { cards: Feed::new(cards.map(|(id, name, cents)| Card::new(id, name, cents))) }
    }
}

/// The app: routes, the shell, and who is signed in. Everything else is Corten's defaults.
pub fn app(market: Market) -> App {
    App::new(routes(market)).shell(shell).context(|headers| {
        // A real app reads its session here; the example trusts a cookie, which is fine for a
        // demo and nothing else.
        let viewer = cookie::get(headers, "viewer").filter(|n| !n.is_empty()).map(|name| Viewer { name: name.into() });
        async move { viewer }
    })
}

fn routes(market: Market) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/watch", post(watch))
        .route("/about", get(about))
        .route("/sign-in", post(sign_in))
        .route("/feed/cards", get(|State(m): State<Market>| async move { m.cards.snapshot() }))
        .route("/feed/cards/live", get(|State(m): State<Market>, since: Since| async move { m.cards.live(since) }))
        .with_state(market)
}

/// Who is looking: the shell's context.
#[derive(Clone, Hash)]
pub struct Viewer {
    pub name: String,
}

/// The document around every page. Client navigation swaps `<main>` only; signing in changes
/// the viewer, which makes the runtime load the next page in full.
fn shell(doc: Document, viewer: Option<Viewer>) -> impl Render {
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                {doc.head}
                <link rel="stylesheet" href={doc.asset("style.css")}>
            </head>
            <body>
                <header class="site">
                    <nav>
                        <a href="/" data-nav aria-current={doc.current("/")}>"Watchlist"</a>
                        <a href="/about" data-nav aria-current={doc.current("/about")}>"About"</a>
                    </nav>
                    match viewer {
                        Some(viewer) => <span class="viewer">"Signed in as "{viewer.name}</span>,
                        None => <form method="post" action="/sign-in" data-patch><button>"Sign in as Ada"</button></form>,
                    }
                </header>
                <main>{doc.main}</main>
            </body>
        </html>
    }
}

async fn sign_in(req: Req) -> corten::Result {
    let mut res = req.redirect("/")?;
    res.headers_mut().append(header::SET_COOKIE, SetCookie::new("viewer", "Ada").header());
    Ok(res.into_response())
}

/// A feed row. `price` is the display text, kept with the cents so every surface shows the
/// same string and no client re-implements the formatting.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub price_cents: u32,
    pub price: String,
}

impl Card {
    fn new(id: &str, name: &str, cents: u32) -> Self {
        Card { id: id.into(), name: name.into(), price_cents: cents, price: format_price(cents) }
    }

    fn set_price(&mut self, cents: u32) {
        self.price_cents = cents;
        self.price = format_price(cents);
    }
}

impl Row for Card {
    fn id(&self) -> &str {
        &self.id
    }
}

fn format_price(cents: u32) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

/// Moves one price per second, so the live island has something to show.
pub async fn simulate_market(cards: Feed<Card>) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut n = 0u64;
    loop {
        tick.tick().await;
        let id = format!("c{}", n % 3 + 1);
        let delta = [125i64, -50, 200, -150][(n % 4) as usize];
        n = n.wrapping_add(1);
        cards.write(|tx| {
            if let Some(card) = tx.get_mut(&id) {
                card.set_price((card.price_cents as i64 + delta).max(100) as u32);
            }
        });
    }
}

/// The watchlist screen's data: what the view shows, and what a data client gets.
#[derive(Serialize)]
pub struct Home {
    cards: Vec<Card>,
    message: String,
}

impl Home {
    fn new(market: &Market, message: String) -> Self {
        Home { cards: market.cards.read(|rows| rows.values().cloned().collect()), message }
    }
}

fn home_view(home: &Home) -> impl Render {
    let f = WatchForm::FIELDS;
    html! {
        <h1>"Watchlist"</h1>
        <live-prices data-src="/feed/cards">
            <table class="cards">
                <thead><tr><th>"Card"</th><th>"Price"</th></tr></thead>
                <tbody>
                    for card in &home.cards {
                        <tr><td>{card.name}</td><td data-ref="price" data-id={card.id}>{card.price}</td></tr>
                    }
                </tbody>
            </table>
        </live-prices>
        <form method="post" action="/watch" data-patch>
            <label>"Card "<select name={f.id}>for card in &home.cards { <option value={card.id}>{card.name}</option> }</select></label>
            <button>"Watch"</button>
        </form>
        <WatchMessage message={&home.message} />
    }
}

/// The watch result: rendered inside the home page, and on its own as a patch.
#[component]
fn WatchMessage(message: &str) -> impl Render {
    html! { <p class="watch-message" id="watch-message" aria-live="polite">{message}</p> }
}

const HOME_TITLE: &str = "Watchlist | Corten";

async fn home(req: Req, State(market): State<Market>) -> corten::Result {
    let home = Home::new(&market, String::new());
    req.screen(HOME_TITLE, &home, home_view(&home))
}

#[derive(Deserialize, Fields)]
struct WatchForm {
    id: String,
}

async fn watch(req: Req, State(market): State<Market>, Form(form): Form<WatchForm>) -> corten::Result {
    let message = market
        .cards
        .read(|rows| rows.get(&form.id).map(|c| format!("Watching {} at {}.", c.name, c.price)))
        .unwrap_or_else(|| "That card is not on the list.".into());
    match req.mode {
        Mode::Action => req.patch(WatchMessage { message: &message }),
        // No JavaScript, or a native screen or agent: the whole screen with the message.
        _ => {
            let home = Home::new(&market, message);
            req.screen(HOME_TITLE, &home, home_view(&home))
        }
    }
}

async fn about(req: Req) -> corten::Result {
    req.page(
        "About | Corten",
        html! { <h1>"About"</h1><p>"Rendered on the server and swapped into the page by the runtime."</p> },
    )
}

#[cfg(test)]
mod tests {
    use corten::axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    async fn send(req: Request<Body>) -> (String, String) {
        let res = app(Market::new()).router().oneshot(req).await.unwrap();
        let kind = res.headers()["content-type"].to_str().unwrap().to_string();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        (kind, String::from_utf8(body.to_vec()).unwrap())
    }

    fn watch_request(mode: Option<&str>, accept: Option<&str>) -> Request<Body> {
        let mut req = Request::post("/watch").header("content-type", "application/x-www-form-urlencoded");
        if let Some(mode) = mode {
            req = req.header("corten", mode);
        }
        if let Some(accept) = accept {
            req = req.header("accept", accept);
        }
        req.body(Body::from("id=c2")).unwrap()
    }

    /// Islands import these declarations, so `npm run check` checks them against the Rust types.
    #[test]
    fn island_types_match_the_server() {
        corten::types::Types::new().add::<Card>().sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
    }

    #[tokio::test]
    async fn home_renders_rows_and_island_for_browsers() {
        let (kind, body) = send(Request::get("/").body(Body::empty()).unwrap()).await;
        assert!(kind.starts_with("text/html"));
        assert!(body.contains("/islands/live-prices.js\">"), "the island module is preloaded: {body}");
        assert!(body.contains(r#"<td data-ref="price" data-id="c1">$129.99</td>"#), "server renders prices first");
    }

    #[tokio::test]
    async fn the_shell_marks_the_current_link_and_shows_the_viewer() {
        let (_, guest) = send(Request::get("/about").body(Body::empty()).unwrap()).await;
        assert!(guest.contains(r#"<a href="/about" data-nav aria-current="page">About</a>"#), "{guest}");
        assert!(guest.contains(r#"<a href="/" data-nav>Watchlist</a>"#));
        assert!(guest.contains("Sign in as Ada"));
        let signed_in = Request::get("/").header("cookie", "viewer=Ada").body(Body::empty()).unwrap();
        let (_, ada) = send(signed_in).await;
        assert!(ada.contains("Signed in as Ada") && !ada.contains("Sign in as Ada"));
    }

    #[tokio::test]
    async fn home_is_json_for_data_clients() {
        let (kind, body) = send(Request::get("/").header("accept", "application/json").body(Body::empty()).unwrap()).await;
        assert_eq!(kind, "application/json");
        assert!(body.starts_with(r#"{"title":"Watchlist | Corten","data":{"cards":[{"id":"c1","name":"Charizard ex","price_cents":12999,"price":"$129.99"}"#), "{body}");
    }

    #[tokio::test]
    async fn one_watch_handler_serves_every_surface() {
        let (_, patch) = send(watch_request(Some("action"), None)).await;
        assert_eq!(
            patch,
            r#"<p class="watch-message" id="watch-message" aria-live="polite">Watching Umbreon VMAX at $540.00.</p>"#
        );
        let (kind, page) = send(watch_request(None, None)).await;
        assert!(kind.starts_with("text/html") && page.contains("<html") && page.contains("Watching Umbreon VMAX"));
        let (kind, data) = send(watch_request(None, Some("application/json"))).await;
        assert_eq!(kind, "application/json");
        assert!(data.ends_with(r#""message":"Watching Umbreon VMAX at $540.00."}}"#), "{data}");
    }
}
