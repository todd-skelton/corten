//! A watchlist: one screen that uses every Corten piece.
//!
//! | Route | Piece |
//! |---|---|
//! | `GET /` | `req.screen`: HTML for browsers, `{"title","data"}` for `Accept: application/json` |
//! | `POST /watch` | form patch with JavaScript, full page without, JSON for data clients |
//! | `GET /feed/stocks`, `/feed/stocks/live` | the live feed `<live-prices>` subscribes to |
//! | `GET /live/trades` | fragments streamed to `<corten-live>`: the last trade, no script of its own |
//! | `GET /about` | a plain page, reached by client navigation, with a component in an area (`Area::components`): its stylesheet is linked from this page only |
//! | `POST /sign-in` | sets a cookie; the shell shows the viewer, and the runtime reloads the document |
//! | `GET /account` | an area only someone signed in may use (`Area::guard`): anyone else is sent to the watchlist, where the sign-in button is |
use std::{sync::Arc, time::Duration};

use corten::{
    axum::{http::header, response::IntoResponse},
    feed::{Feed, Row, Since},
    prelude::*,
    security::cookie::{self, SetCookie},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

#[derive(Clone)]
pub struct Market {
    pub stocks: Feed<Stock>,
    /// The last trade, as text: what `<corten-live>` shows.
    pub trades: Arc<watch::Sender<String>>,
}

impl Market {
    pub fn new() -> Self {
        let stocks = [("s1", "Acme Corp", 12_999), ("s2", "Globex", 54_000), ("s3", "Initech", 31_050)];
        Market {
            stocks: Feed::new(stocks.map(|(id, name, cents)| Stock::new(id, name, cents))),
            trades: Arc::new(watch::Sender::new("No trades yet.".into())),
        }
    }
}

/// The app: routes, the shell, and who is signed in. Everything else is Corten's defaults.
pub fn app(market: Market) -> App<Viewer> {
    App::new(routes(market))
        .context(|headers| {
            // A real app reads its session here; the example trusts a cookie, which is fine for a
            // demo and nothing else.
            let viewer = cookie::get(headers, "viewer").filter(|n| !n.is_empty()).map(|name| Viewer { name: name.into() });
            async move { viewer }
        })
        .shell(shell)
        .components(&[LivePrices])
        .area(url::About::PATH, Area::new(Router::new().route(url::About::PATH, get(about))).components(&[AboutNote]))
        // Every route under /account, now and later, needs someone signed in.
        .area(
            url::Account::PATH,
            Area::new(Router::new().route(url::Account::PATH, get(account))).guard(|viewer, _| match viewer {
                Some(_) => Access::ALLOW,
                None => Access::sign_in(&url::Home),
            }),
        )
}

/// Every URL the app serves and links to; links built from these cannot drift from the router.
mod url {
    use corten::Route;

    #[derive(Route)]
    #[route("/")]
    pub struct Home;

    #[derive(Route)]
    #[route("/watch")]
    pub struct Watch;

    #[derive(Route)]
    #[route("/about")]
    pub struct About;

    #[derive(Route)]
    #[route("/sign-in")]
    pub struct SignIn;

    #[derive(Route)]
    #[route("/account")]
    pub struct Account;

    #[derive(Route)]
    #[route("/feed/stocks")]
    pub struct Stocks;

    #[derive(Route)]
    #[route("/feed/stocks/live")]
    pub struct StocksLive;

    #[derive(Route)]
    #[route("/live/trades")]
    pub struct Trades;
}

fn routes(market: Market) -> Router {
    Router::new()
        .route(url::Home::PATH, get(home))
        .route(url::Watch::PATH, post(watch))
        .route(url::SignIn::PATH, post(sign_in))
        .route(url::Stocks::PATH, get(|State(m): State<Market>| async move { m.stocks.snapshot() }))
        .route(url::StocksLive::PATH, get(|State(m): State<Market>, since: Since| async move { m.stocks.live(since) }))
        .route(url::Trades::PATH, get(trades))
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
                <link rel="stylesheet" href={doc.asset("style.css")}>
                {doc.head}
            </head>
            <body>
                <header>
                    <nav>
                        <a href={url::Home} data-nav aria-current={doc.current(&url::Home)}>"Watchlist"</a>
                        <a href={url::About} data-nav aria-current={doc.current(&url::About)}>"About"</a>
                        <a href={url::Account} data-nav aria-current={doc.current(&url::Account)}>"Account"</a>
                    </nav>
                    match viewer {
                        Some(viewer) => <span>"Signed in as "{viewer.name}</span>,
                        None => <form method="post" action={url::SignIn} data-patch><button>"Sign in as Ada"</button></form>,
                    }
                </header>
                <main>{doc.main}</main>
            </body>
        </html>
    }
}

async fn sign_in(req: Req) -> corten::Result {
    let mut res = req.redirect(&url::Home)?;
    res.headers_mut().append(header::SET_COOKIE, SetCookie::new("viewer", "Ada").header());
    Ok(res.into_response())
}

/// A feed row. `price` is the display text, kept with the cents so every surface shows the
/// same string and no client re-implements the formatting.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Stock {
    pub id: String,
    pub name: String,
    pub price_cents: u32,
    pub price: String,
}

impl Stock {
    fn new(id: &str, name: &str, cents: u32) -> Self {
        Stock { id: id.into(), name: name.into(), price_cents: cents, price: format_price(cents) }
    }

    fn set_price(&mut self, cents: u32) {
        self.price_cents = cents;
        self.price = format_price(cents);
    }
}

impl Row for Stock {
    fn id(&self) -> &str {
        &self.id
    }
}

fn format_price(cents: u32) -> String {
    format!("${}.{:02}", cents / 100, cents % 100)
}

/// Moves one price per second, so the live prices and the last trade have something to show.
pub async fn simulate_market(market: Market) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    let mut n = 0u64;
    loop {
        tick.tick().await;
        let id = format!("s{}", n % 3 + 1);
        let delta = [125i64, -50, 200, -150][(n % 4) as usize];
        n = n.wrapping_add(1);
        let trade = market.stocks.write(|tx| {
            let stock = tx.get_mut(&id)?;
            stock.set_price((stock.price_cents as i64 + delta).max(100) as u32);
            Some(format!("Last trade: {} at {}.", stock.name, stock.price))
        });
        if let Some(trade) = trade {
            market.trades.send_replace(trade);
        }
    }
}

/// The last trade as each one happens, for `<corten-live src>`: every item is a fragment the
/// runtime swaps in by `id`, like a form's patch.
async fn trades(State(market): State<Market>) -> impl IntoResponse {
    let next = market.trades.subscribe();
    corten::live(futures_util::stream::unfold(next, |mut next| async move {
        next.changed().await.ok()?;
        let html = LastTrade { text: &next.borrow_and_update() }.to_html();
        Some((html, next))
    }))
}

/// The watchlist screen's data: what the view shows, and what a data client gets.
#[derive(Serialize)]
pub struct Home {
    stocks: Vec<Stock>,
    last_trade: String,
    message: String,
}

impl Home {
    fn new(market: &Market, message: String) -> Self {
        let stocks = market.stocks.read(|rows| rows.values().cloned().collect());
        Home { stocks, last_trade: market.trades.borrow().clone(), message }
    }
}

fn home_view(home: &Home) -> impl Render {
    let f = WatchForm::FIELDS;
    html! {
        <h1>"Watchlist"</h1>
        <LivePrices stocks={&home.stocks} feed={url::Stocks::PATH} />
        <corten-live src={url::Trades}><LastTrade text={&home.last_trade} /></corten-live>
        <form method="post" action={url::Watch} data-patch>
            <label>
                "Stock "<select name={f.id}>for stock in &home.stocks { <option value={stock.id}>{stock.name}</option> }</select>
            </label>
            <button>"Watch"</button>
        </form>
        <WatchMessage message={&home.message} />
    }
}

/// The prices, rendered here first and then kept current by `live_prices.js` from the feed. The
/// pause button shows only when the script runs, since nothing updates without it. Styled by
/// `live_prices.css`.
#[component(css, js)]
fn LivePrices<'a>(stocks: &'a [Stock], feed: &'a str) -> impl Render {
    let (pause, price) = refs();
    html! {
        <div on:connect={js::watch(scope, pause, price, feed, signal)}>
            <button ref={pause} type="button" aria-pressed="false" hidden on:click={js::pause(scope, target, price)}>
                "Pause updates"
            </button>
            <table class={Table}>
                <thead>
                    <tr>
                        <th>"Stock"</th>
                        <th>"Price"</th>
                    </tr>
                </thead>
                <tbody>
                    for stock in stocks {
                        <tr>
                            <td>{stock.name}</td>
                            <td ref={price} class={Amount} key={stock.id}>{stock.price}</td>
                        </tr>
                    }
                </tbody>
            </table>
        </div>
    }
}

/// The watch result's id: the patch that answers the form replaces it.
#[derive(Id)]
struct WatchMessageId;

/// The watch result: rendered inside the home page, and on its own as a patch.
#[component]
fn WatchMessage(message: &str) -> impl Render {
    html! { <p id={WatchMessageId} aria-live="polite">{message}</p> }
}

/// The last trade: rendered inside the home page, then replaced by each fragment `/live/trades`
/// streams.
#[derive(Id)]
struct LastTradeId;

#[component]
fn LastTrade(text: &str) -> impl Render {
    html! { <p id={LastTradeId} aria-live="polite">{text}</p> }
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
        .stocks
        .read(|rows| rows.get(&form.id).map(|c| format!("Watching {} at {}.", c.name, c.price)))
        .unwrap_or_else(|| "That stock is not on the list.".into());
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
        html! {
            <h1>"About"</h1>
            <p>"Rendered on the server and swapped into the page by the runtime."</p>
            <AboutNote />
        },
    )
}

/// The guard on `/account` has let only a signed-in viewer through.
async fn account(req: Req) -> corten::Result {
    let name = req.context::<Viewer>().map_or("", |viewer| &viewer.name);
    req.page(
        "Account | Corten",
        html! {
            <h1>"Account"</h1>
            <p>"Signed in as "{name}"."</p>
        },
    )
}

/// A note only the about page shows. Its stylesheet, `about_note.css`, is in the `/about`
/// section, so the watchlist never downloads it; a client navigation here loads it before the
/// page is swapped in.
#[component(css)]
fn AboutNote() -> impl Render {
    html! { <p class={Note}>"This note's styles load with the about page only."</p> }
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
        req.body(Body::from("id=s2")).unwrap()
    }

    /// The script's rows are checked against these declarations by `npm run check`.
    #[test]
    fn script_types_match_the_server() {
        corten::types::Types::new().add::<Stock>().sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
    }

    #[tokio::test]
    async fn home_renders_rows_and_script_for_browsers() {
        let (kind, body) = send(Request::get("/").body(Body::empty()).unwrap()).await;
        assert!(kind.starts_with("text/html"));
        assert!(body.contains("/scripts/live_prices.js\">"), "the script module is preloaded: {body}");
        assert!(body.contains(r#"data-on="connect:live_prices.watch($c,@pause,#price,&quot;/feed/stocks&quot;,$s)""#), "{body}");
        assert!(
            body.contains(r#"<td data-ref="price" class="live-prices__amount" data-key="s1">$129.99</td>"#),
            "server renders prices first: {body}"
        );
        assert!(body.contains(
            r#"<corten-live src="/live/trades"><p id="last-trade" aria-live="polite">No trades yet.</p></corten-live>"#
        ));
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
    async fn only_the_about_page_links_its_section_stylesheet() {
        let links = |body: &str| body.matches("data-corten-style").count();
        let (_, home) = send(Request::get("/").body(Body::empty()).unwrap()).await;
        assert_eq!(links(&home), 0, "{home}");
        let (_, about) = send(Request::get("/about").body(Body::empty()).unwrap()).await;
        assert!(about.contains(r#"/sections/about.css" data-corten-style>"#), "{about}");
        assert!(about.contains(r#"<p class="about-note__note">"#), "{about}");
    }

    #[tokio::test]
    async fn the_account_page_is_for_someone_signed_in() {
        let res = app(Market::new()).router().oneshot(Request::get("/account").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!((res.status().as_u16(), &res.headers()["location"]), (303, &"/".parse().unwrap()));
        let signed_in = Request::get("/account").header("cookie", "viewer=Ada").body(Body::empty()).unwrap();
        let (_, ada) = send(signed_in).await;
        assert!(ada.contains("<h1>Account</h1><p>Signed in as Ada.</p>"), "{ada}");
    }

    #[tokio::test]
    async fn home_is_json_for_data_clients() {
        let (kind, body) = send(Request::get("/").header("accept", "application/json").body(Body::empty()).unwrap()).await;
        assert_eq!(kind, "application/json");
        assert!(body.starts_with(r#"{"title":"Watchlist | Corten","data":{"stocks":[{"id":"s1","name":"Acme Corp","price_cents":12999,"price":"$129.99"}"#), "{body}");
    }

    #[tokio::test]
    async fn one_watch_handler_serves_every_surface() {
        let (_, patch) = send(watch_request(Some("action"), None)).await;
        assert_eq!(patch, r#"<p id="watch-message" aria-live="polite">Watching Globex at $540.00.</p>"#);
        let (kind, page) = send(watch_request(None, None)).await;
        assert!(kind.starts_with("text/html") && page.contains("<html") && page.contains("Watching Globex"));
        let (kind, data) = send(watch_request(None, Some("application/json"))).await;
        assert_eq!(kind, "application/json");
        assert!(data.ends_with(r#""message":"Watching Globex at $540.00."}}"#), "{data}");
    }
}
