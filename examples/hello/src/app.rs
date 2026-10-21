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

use axum::{
    extract::{Request, State as With},
    http::header,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Form, Router,
};
use corten::{
    feed::{Feed, Row, Since},
    security::cookie::{self, SetCookie},
    Head, Mode, Page, Template,
};
use serde::{Deserialize, Serialize};

pub type Req = corten::Req<Shell>;

#[derive(Clone)]
pub struct State {
    pub cards: Feed<Card>,
}

impl State {
    pub fn new() -> Self {
        let cards =
            [("c1", "Charizard ex", 12_999), ("c2", "Umbreon VMAX", 54_000), ("c3", "Pikachu with Grey Felt Hat", 31_050)];
        State { cards: Feed::new(cards.map(|(id, name, cents)| Card::new(id, name, cents))) }
    }
}

pub fn router(state: State) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/watch", post(watch))
        .route("/about", get(about))
        .route("/sign-in", post(sign_in))
        .route("/feed/cards", get(|With(s): With<State>| async move { s.cards.snapshot() }))
        .route("/feed/cards/live", get(|With(s): With<State>, since: Since| async move { s.cards.live(since) }))
        .merge(corten::routes())
        .with_state(state)
        .layer(middleware::from_fn(viewer))
}

/// Who is looking: the shell's context. A real app reads its session here; the example trusts a
/// cookie, which is fine for a demo and nothing else.
#[derive(Clone, Hash)]
pub struct Viewer {
    pub name: String,
}

async fn viewer(mut req: Request, next: Next) -> Response {
    if let Some(name) = cookie::get(req.headers(), "viewer").filter(|n| !n.is_empty()) {
        let name = name.to_string();
        req.extensions_mut().insert(Viewer { name });
    }
    next.run(req).await
}

async fn sign_in(req: Req) -> corten::Result {
    let mut res = req.redirect("/")?;
    res.headers_mut().append(header::SET_COOKIE, SetCookie::new("viewer", "Ada").header());
    Ok(res.into_response())
}

/// A feed row. `price` is the display text, kept with the cents so every surface shows the
/// same string and no client re-implements the formatting.
#[derive(Clone, Debug, Serialize, Deserialize)]
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

pub struct Shell;

#[derive(Template)]
#[template(path = "shell.html")]
struct ShellTemplate<'a> {
    head: &'a Head<'a>,
    main: &'a str,
    viewer: Option<&'a Viewer>,
}

impl corten::Shell for Shell {
    type Context = Viewer;

    fn render(head: &Head<'_>, main: &str, viewer: Option<&Viewer>) -> askama::Result<String> {
        ShellTemplate { head, main, viewer }.render()
    }
}

/// The watchlist screen. Its fields are the view model every surface reads.
#[derive(Template, Serialize)]
#[template(path = "home.html")]
pub struct Home {
    cards: Vec<Card>,
    message: String,
}

impl Home {
    fn new(state: &State, message: String) -> Self {
        Home { cards: state.cards.read(|rows| rows.values().cloned().collect()), message }
    }
}

impl Page for Home {
    fn title(&self) -> String {
        "Watchlist | Corten".into()
    }
}

/// The watch result: rendered inside the home page, and on its own as a patch.
#[derive(Template)]
#[template(path = "watch_message.html")]
struct WatchMessage<'a> {
    message: &'a str,
}

#[derive(Template)]
#[template(path = "about.html")]
struct About;

impl Page for About {
    fn title(&self) -> String {
        "About | Corten".into()
    }
}

async fn home(req: Req, With(state): With<State>) -> corten::Result {
    req.screen(&Home::new(&state, String::new()))
}

#[derive(Deserialize)]
struct WatchForm {
    id: String,
}

async fn watch(req: Req, With(state): With<State>, Form(form): Form<WatchForm>) -> corten::Result {
    let message = state
        .cards
        .read(|rows| rows.get(&form.id).map(|c| format!("Watching {} at {}.", c.name, c.price)))
        .unwrap_or_else(|| "That card is not on the list.".into());
    match req.mode {
        Mode::Action => req.patch(&WatchMessage { message: &message }),
        // No JavaScript, or a native screen or agent: the whole screen with the message.
        _ => req.screen(&Home::new(&state, message)),
    }
}

async fn about(req: Req) -> corten::Result {
    req.page(&About)
}

#[cfg(test)]
mod tests {
    use axum::{body::Body, http::Request};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    async fn send(req: Request<Body>) -> (String, String) {
        let res = router(State::new()).oneshot(req).await.unwrap();
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

    #[tokio::test]
    async fn home_renders_rows_and_island_for_browsers() {
        let (kind, body) = send(Request::get("/").body(Body::empty()).unwrap()).await;
        assert!(kind.starts_with("text/html"));
        assert!(body.contains(r#"<link rel="modulepreload" href="/static/islands/live-prices.js">"#));
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
