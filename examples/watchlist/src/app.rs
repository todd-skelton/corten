//! A watchlist: one screen that uses every Corten piece.
//!
//! | Route | Piece |
//! |---|---|
//! | `GET /` | `req.screen`: HTML for browsers, `{"title","data"}` for `Accept: application/json` |
//! | `POST /watch` | form patch with JavaScript, full page without, JSON for data clients |
//! | `GET /feed/stocks`, `/feed/stocks/live` | the live feed `<live-prices>` subscribes to |
//! | `GET /live/trades` | fragments streamed to `<corten-live>`: the last trade, no script of its own |
//! | `GET /about?history=on` | a plain page, reached by client navigation, with a component in an area (`Area::components`): its stylesheet is linked from this page only; a filter applied on change keeps the user's place (`data-keep-place`), and Load more adds only the new rows (`view::Append`) |
//! | `POST /sign-in` | sets a cookie; the layout shows the viewer (`Option<Caller<Viewer>>`), and the runtime reloads the document |
//! | `GET /account` | an area whose every route takes `Caller<Viewer>` (`caller = Viewer`): anyone else is sent to the watchlist, where the sign-in button is |
//! | `GET /saved?until=..` | a read catching up with a write: until then the page asks to be read again (`ReadAgain` in its head tags), however it was reached |
//! | `POST /saved` | a write answered in place while its read catches up: the status line, as a patch that is read again from `/saved?until=..` (`ReadAgain::from`) until the write is in |
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use corten::{
    Fragment, LiveResponse, LiveSince,
    axum::http::{header, request::Parts},
    feed::{Feed, FeedResponse, Row, Since},
    i18n::{Currency, Minor},
    prelude::*,
    security::cookie::{self, SetCookie},
    view::Append,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::theme::*;

corten::i18n::messages! {
    /// The app's text. One locale here; a real app lists each catalog.
    pub struct Msg { "en" => "locales/en.json" }
}

#[derive(Clone)]
pub struct Market {
    pub stocks: Feed<Stock>,
    /// The last trade, as text: what `<corten-live>` shows, and how many trades came before it
    /// (the stream's cursor).
    pub trades: Arc<watch::Sender<(u64, String)>>,
}

impl Market {
    pub fn new() -> Self {
        let stocks = [("s1", "Acme Corp", 12_999), ("s2", "Globex", 54_000), ("s3", "Initech", 31_050)];
        Market {
            stocks: Feed::new(stocks.map(|(id, name, cents)| Stock::new(id, name, cents))),
            trades: Arc::new(watch::Sender::new((0, "No trades yet.".into()))),
        }
    }
}

/// The app: its routes and areas. Every page brings its document, `Layout` with the page in it.
/// Everything else is Corten's defaults.
pub fn app(market: Market) -> App {
    App::new(url::router(market))
        .components(&[TOKENS, LivePrices])
        .area(|area| about::area(area).components(&[AboutNote, AboutFilter]))
        .area(account::area)
        .error_page(error_page, ())
}

/// The page for a failed request, in the layout, for who is asking: a signed-in user who mistypes
/// a URL still sees themselves signed in. The viewer is looked up only for a failure.
async fn error_page(failure: Failure, req: Req, viewer: Option<Caller<Viewer>>) -> HtmlResult {
    let (title, text) = match failure {
        Failure::NotFound => ("Not found", "There is nothing at this address."),
        Failure::Refused { .. } | Failure::CrossSite => ("Not allowed", "You do not have access to this page."),
        Failure::Unidentified { .. } | Failure::TimedOut | Failure::Offline => ("Not available", "Try again in a moment."),
        Failure::WrongMethod | Failure::Rejected { .. } | Failure::Failed { .. } | Failure::Panicked { .. } => {
            ("Something went wrong", "Please try again.")
        }
    };
    req.page(html! {
        <Layout req={&req} title={title} viewer={viewer.as_deref()} head="">
            <section class={(card(), stack(Space::_3), max_w(MaxW::Prose))}>
                <h1>{title}</h1>
                <p>{text}</p>
                if let Some(detail) = failure.detail() {
                    <pre>{detail}</pre>
                }
                <p><a href={url::Home}>"Back to the watchlist"</a></p>
            </section>
        </Layout>
    })
}

/// Every URL the app serves and links to, with its handlers, which share the market; links built
/// from these cannot drift from the router.
#[corten::routes(state = Market)]
mod url {
    #[route("/", get = home)]
    pub struct Home;

    #[route("/watch", post = watch)]
    pub struct Watch;

    #[route("/sign-in", post = sign_in)]
    pub struct SignIn;

    #[route("/feed/stocks", get = stocks)]
    pub struct Stocks;

    #[route("/feed/stocks/live", get = stocks_live)]
    pub struct StocksLive;

    #[route("/live/trades", get = trades)]
    pub struct Trades;

    #[route("/saved", get = saved, post = save)]
    pub struct Saved {
        pub until: Option<u64>,
    }
}

/// A page with a stylesheet of its own: an area, so other pages do not download it.
#[corten::routes(area = About)]
mod about {
    #[route("/about", get = about)]
    pub struct About;

    #[route("/about/thanks", post = thank)]
    pub struct Thank;
}

/// Someone's own pages: every route here, now and later, takes the viewer, so no one signed out
/// reaches one (they are sent to the watchlist, where the sign-in button is).
#[corten::routes(area = Account, caller = Viewer)]
mod account {
    #[route("/account", get = account)]
    pub struct Account;
}

/// The stocks feed: a snapshot, then live changes from a cursor.
async fn stocks(State(market): State<Market>) -> FeedResponse {
    market.stocks.snapshot()
}

async fn stocks_live(State(market): State<Market>, since: Since) -> FeedResponse {
    market.stocks.live(since)
}

/// Who is looking, which the layout shows.
#[derive(Clone, Hash)]
pub struct Viewer {
    pub name: String,
}

/// A real app reads its session here; the example trusts a cookie, which is fine for a demo and
/// nothing else.
impl<S: Sync> Identify<S> for Viewer {
    async fn identify(parts: &Parts, _: &S) -> Result<Self, Refused> {
        match cookie::get(&parts.headers, "viewer").filter(|name| !name.is_empty()) {
            Some(name) => Ok(Viewer { name: name.into() }),
            None => Err(Refused::sign_in(&url::Home)),
        }
    }
}

/// The document around every page: its title, its own `head` tags, who is looking and the page.
/// Client navigation swaps the title, the tags and the page only; signing in changes the viewer,
/// which makes the runtime load the next page in full.
#[component]
fn Layout<'a>(
    req: &'a Req,
    title: &'a str,
    viewer: Option<&'a Viewer>,
    head: impl Render,
    children: impl Render,
) -> impl Document {
    html! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <link rel="stylesheet" href={req.asset("style.css")}>
                <link rel="icon" type="image/svg+xml" href={req.asset("favicon.svg")}>
                <title>{title}" | Corten"</title>
                {head}
                <Corten />
            </head>
            <body class={(bg(Color::Bg), fg(Color::Ink))}>
                <header class={(bg(Color::Surface), border_b(BorderB::Line))}>
                    <div
                        class={(row(Space::_3), justify(Justify::Between), wrap(Wrap::Wrap), max_w(MaxW::Page), mx(Mx::Auto), px(Space::_5), py(Space::_3))}
                    >
                        <nav class={(row(Space::_1), wrap(Wrap::Wrap))}>
                            <span class={(weight(Weight::Bold), tracking(Tracking::Tight), px(Space::_2))}>"Watchlist"</span>
                            <a href={url::Home} data-nav aria-current={req.current(&url::Home)}>"Prices"</a>
                            <a href={about::About} data-nav aria-current={req.current(&about::About)}>"About"</a>
                            <a href={account::Account} data-nav aria-current={req.current(&account::Account)}>"Account"</a>
                        </nav>
                        match viewer {
                            Some(viewer) => <span class={muted()}>"Signed in as "{&viewer.name}</span>,
                            None => <form method="post" action={url::SignIn} data-patch><button>"Sign in as Ada"</button></form>,
                        }
                    </div>
                </header>
                <main class={(max_w(MaxW::Page), mx(Mx::Auto), px(Space::_5), py(Space::_6))}>{children}</main>
            </body>
        </html>
    }
}

async fn sign_in(req: Req) -> HtmlResult {
    let mut res = req.redirect(&url::Home)?;
    res.headers_mut().append(header::SET_COOKIE, SetCookie::new("viewer", "Ada").header());
    Ok(res)
}

/// A feed row. `price` is the display text, for clients with no formatting of their own (a
/// native screen, an agent); the page's script writes `price_cents` with the page's format
/// (`t.js().money(USD)`), which writes what the server does.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToJson)]
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

/// The currency of every price.
const USD: Currency = Currency::new("USD");

/// The feed's one text for every viewer, in the app's default locale (`$1,234.50`).
fn format_price(cents: u32) -> String {
    Msg::default().money(Minor(cents), USD).to_string()
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
            market.trades.send_modify(|(n, text)| {
                *n += 1;
                *text = trade;
            });
        }
    }
}

/// The last trade as each one happens, for `<corten-live src>`: every item is a fragment the
/// runtime swaps in by `id`, like a form's patch, with the trade count as its cursor. The page
/// says which trade it shows (`since`), and a reconnect which one it last got, so the first
/// fragment is only sent when the browser is behind; otherwise the stream waits for the next.
async fn trades(State(market): State<Market>, LiveSince(since): LiveSince) -> LiveResponse {
    let mut next = market.trades.subscribe();
    let current = next.borrow().0.to_string();
    if since.as_deref() != Some(current.as_str()) {
        next.mark_changed();
    }
    corten::live(futures_util::stream::unfold(next, |mut next| async move {
        next.changed().await.ok()?;
        let (n, text) = next.borrow_and_update().clone();
        Some((Fragment::new(LastTrade { text: &text }.to_html()).cursor(n.to_string()), next))
    }))
}

/// The watchlist screen's data: what the view shows, and what a data client gets.
#[derive(ToJson)]
pub struct Home {
    stocks: Vec<Stock>,
    last_trade: String,
    /// The trade count `last_trade` is at, where the page's live stream starts from.
    #[serde(skip)]
    trades_seen: u64,
    message: String,
}

impl Home {
    fn new(market: &Market, message: String) -> Self {
        let stocks = market.stocks.read(|rows| rows.values().cloned().collect());
        let (trades_seen, last_trade) = market.trades.borrow().clone();
        Home { stocks, last_trade, trades_seen, message }
    }
}

fn home_view(home: &Home, t: Msg) -> impl Render {
    let f = WatchForm::FIELDS;
    html! {
        <div class={(stack(Space::_1), mb(Space::_5))}>
            <h1>"Watchlist"</h1>
            <p class={muted()}>"Prices stream from the server as they change. Without JavaScript the page still shows them."</p>
        </div>
        <div
            class={(display(Display::Grid), columns(Responsive::new(Columns::_1).md(Columns::Main)), gap(Space::_5), items(Items::Start))}
        >
            <section class={card()}>
                <LivePrices stocks={&home.stocks} feed={url::Stocks::PATH} t={t} />
            </section>
            <aside class={stack(Space::_5)}>
                <section class={(card(), stack(Space::_2))}>
                    <h2>"Market activity"</h2>
                    <corten-live src={url::Trades} since={home.trades_seen}><LastTrade text={&home.last_trade} /></corten-live>
                </section>
                <form method="post" action={url::Watch} data-patch class={(card(), stack(Space::_3))}>
                    <h2>"Watch a stock"</h2>
                    <label class={stack(Space::_1)}>
                        <span class={muted()}>"Stock"</span>
                        <select name={f.id}>for stock in &home.stocks { <option value={stock.id}>{stock.name}</option> }</select>
                    </label>
                    <button class={w(W::Full)}>"Watch"</button>
                    <WatchMessage message={&home.message} />
                </form>
            </aside>
        </div>
    }
}

/// The prices, rendered here first and then kept current by `live_prices.js` from the feed. The
/// pause button shows only when the script runs, since nothing updates without it. The script
/// counts the updates and shows the count with a message of the page's locale (`t.js()`), and
/// writes each price with the page's money format, so the text a script computes is the
/// server's. Styled by `live_prices.css`.
#[component(css, js)]
fn LivePrices<'a>(stocks: &'a [Stock], feed: &'a str, t: Msg) -> impl Render {
    let (pause, price, updates) = refs();
    let (updated, money) = (t.js().prices_updates(), t.js().money(USD));
    html! {
        <div class={stack(Space::_3)} on:connect={js::watch(scope, pause, price, money, feed, signal, updates, updated)}>
            <div class={(row(Space::_3), justify(Justify::Between))}>
                <h2>"Prices"</h2>
                <button ref={pause} type="button" aria-pressed="false" hidden on:click={js::pause(scope, target, price, money)}>
                    "Pause updates"
                </button>
            </div>
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
            <output ref={updates} class={muted()} aria-live="polite"></output>
        </div>
    }
}

/// The watch result's id: the patch that answers the form replaces it.
#[derive(Id)]
struct WatchMessageId;

/// The watch result: rendered inside the home page, and on its own as a patch.
#[component]
fn WatchMessage(message: &str) -> impl Patch {
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

async fn home(req: Req, viewer: Option<Caller<Viewer>>, State(market): State<Market>) -> HtmlResult {
    let home = Home::new(&market, String::new());
    let view = home_view(&home, req.t());
    req.screen(&home, html! { <Layout req={&req} title="Watchlist" viewer={viewer.as_deref()} head="">{view}</Layout> })
}

#[derive(Deserialize, Fields)]
struct WatchForm {
    id: String,
}

async fn watch(
    req: Req,
    viewer: Option<Caller<Viewer>>,
    State(market): State<Market>,
    Form(form): Form<WatchForm>,
) -> HtmlResult {
    let message = market
        .stocks
        .read(|rows| rows.get(&form.id).map(|c| format!("Watching {} at {}.", c.name, c.price)))
        .unwrap_or_else(|| "That stock is not on the list.".into());
    match req.mode {
        Mode::Action => req.patch(WatchMessage { message: &message }),
        // No JavaScript, or a native screen or agent: the whole screen with the message.
        _ => {
            let home = Home::new(&market, message);
            let view = home_view(&home, req.t());
            req.screen(&home, html! { <Layout req={&req} title="Watchlist" viewer={viewer.as_deref()} head="">{view}</Layout> })
        }
    }
}

/// The about page's filter and how much of the history it shows: a GET form, so both are in
/// the address.
#[derive(Deserialize, Fields)]
struct AboutQuery {
    #[serde(default)]
    history: bool,
    shown: Option<usize>,
}

/// The project's history, one page of it at a time.
const HISTORY: [&str; 6] =
    ["Corten began as a watchlist.", "Then came forms.", "Then live prices.", "Then areas.", "Then handlers.", "Then this list."];
const HISTORY_PAGE: usize = 2;

async fn about(req: Req, viewer: Option<Caller<Viewer>>, Query(q): Query<AboutQuery>) -> HtmlResult {
    let shown = q.shown.unwrap_or(0).min(HISTORY.len());
    let next = (shown + HISTORY_PAGE).min(HISTORY.len());
    // Load more with JavaScript: only the next page of the history, after the rows the page has,
    // and the button for the page after it.
    if req.mode == Mode::Action {
        return req.patch(html! {
            {Append(HistoryId, html! { for event in &HISTORY[shown..next] { <li>{event}</li> } })}
            <LoadMore shown={next} />
        });
    }
    // The whole page, with every row shown so far: a first load, a reload, no JavaScript.
    let shown = if q.history { next } else { 0 };
    req.page(html! {
        <Layout req={&req} title="About" viewer={viewer.as_deref()} head="">
            <div class={(stack(Space::_4), max_w(MaxW::Prose))}>
                <h1>"About"</h1>
                <p>"Rendered on the server and swapped into the page by the runtime."</p>
                <AboutNote />
                <section class={(card(), stack(Space::_3))}>
                    <AboutFilter history={q.history} />
                    if q.history {
                        <ol id={HistoryId}>for event in &HISTORY[..shown] { <li>{event}</li> }</ol>
                        <LoadMore shown={shown} />
                    }
                </section>
                <form id={ThanksId} method="post" action={about::Thank} data-patch>
                    <button>"Thank the authors"</button>
                </form>
            </div>
        </Layout>
    })
}

/// The form at the end of the about page.
#[derive(Id)]
struct ThanksId;

/// A write that sends the browser back to the form it came from, at the end of a long page:
/// `About.at(ThanksId)` is `/about#thanks`, the id the form's markup prints.
async fn thank(req: Req) -> HtmlResult {
    req.redirect(&about::About.at(ThanksId))
}

#[derive(Id)]
struct HistoryId;

#[derive(Id)]
struct LoadMoreId;

/// Loads the next page of the history: a GET form, so without JavaScript it is the page with one
/// page more, and with it (`data-patch`) the answer adds only the new rows (`view::Append`).
#[component]
fn LoadMore(shown: usize) -> impl Patch {
    let f = AboutQuery::FIELDS;
    html! {
        <form id={LoadMoreId} method="get" action={about::About} data-patch>
            <input type="hidden" name={f.history} value="on">
            <input type="hidden" name={f.shown} value={shown}>
            <button class={secondary()} hidden={shown >= HISTORY.len()}>"Load more"</button>
        </form>
    }
}

/// A filter applied as soon as it changes: the form is submitted on change by its handler, and
/// data-keep-place keeps the user where they were, on the box they checked, while the page
/// changes under them: the rail keeps its scroll and the groups they opened stay open, though the
/// server draws a group open only when its box is checked.
#[component(css, js)]
fn AboutFilter(history: bool) -> impl Render {
    let f = AboutQuery::FIELDS;
    html! {
        <form method="get" action={about::About} data-keep-place on:change={js::apply(target)}>
            <div class={Rail}>
                <details>
                    <summary>"About these filters"</summary>
                    <p>"Each one applies as soon as it changes."</p>
                </details>
                <details open={history}>
                    <summary>"History"</summary>
                    <label><input type="checkbox" name={f.history} checked={history}>" Show the history"</label>
                </details>
            </div>
            <button class={secondary()}>"Apply"</button>
        </form>
    }
}

#[derive(Deserialize)]
struct SavedQuery {
    until: Option<u64>,
}

/// A page that reads a write which has not landed yet. A real app's address carries the write's
/// receipt; here it is the time the write lands, in milliseconds since 1970. Until then the page
/// says so and asks to be read again, which the runtime does in place however the page was
/// reached; once the write is in, the page stops asking.
async fn saved(req: Req, viewer: Option<Caller<Viewer>>, Query(query): Query<SavedQuery>) -> HtmlResult {
    if req.mode == Mode::Action {
        return saved_status(&req, query.until);
    }
    let pending = landing(query.until);
    let again = pending.then_some(ReadAgain { after: Duration::from_secs(1) });
    let text = if pending { "Saving your change..." } else { "Your change is saved." };
    req.page(html! {
        <Layout req={&req} title="Saved" viewer={viewer.as_deref()} head={again}>
            <section class={(card(), stack(Space::_3), max_w(MaxW::Prose))}>
                <h1>"Saved"</h1>
                <p id={SavedId} role="status">{text}</p>
                <form method="post" action={url::Saved { until: None }} data-patch>
                    <button>"Save again"</button>
                </form>
            </section>
        </Layout>
    })
}

/// A write that lands in a second and a half. With JavaScript the form's answer is the status
/// line, read again until the write is in; without, the saved page, which reads itself again.
async fn save(req: Req) -> HtmlResult {
    let until = Some(now_millis() + 1500);
    if req.mode == Mode::Action {
        return saved_status(&req, until);
    }
    req.redirect(&url::Saved { until })
}

/// The status line alone, as a patch: while the write is pending it asks to be read again from
/// the saved route with the write's time in it, which answers this again.
fn saved_status(req: &Req, until: Option<u64>) -> HtmlResult {
    let pending = landing(until);
    let text = if pending { "Saving your change..." } else { "Your change is saved." };
    let from = url::Saved { until };
    let again = pending.then(|| ReadAgain { after: Duration::from_secs(1) }.from(&from));
    req.patch(html! {
        <p id={SavedId} role="status">{text}</p>
        {again}
    })
}

/// True while a write that lands at `until` (milliseconds since 1970) has not.
fn landing(until: Option<u64>) -> bool {
    until.is_some_and(|until| now_millis() < until)
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The saved page's status line.
#[derive(Id)]
struct SavedId;

/// Only someone signed in gets here: the handler takes the viewer.
async fn account(req: Req, Caller(viewer): Caller<Viewer>) -> HtmlResult {
    let viewer = &viewer;
    req.page(html! {
        <Layout req={&req} title="Account" viewer={Some(viewer)} head="">
            <section class={(card(), stack(Space::_2), max_w(MaxW::Prose))}>
                <h1>"Account"</h1>
                <p>"Signed in as "{&viewer.name}"."</p>
            </section>
        </Layout>
    })
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
        assert!(
            body.contains(r#"data-on="connect:live_prices.watch($c,@pause,#price,%{&quot;z&quot;:&quot;0&quot;,&quot;s&quot;:[&quot;.&quot;,&quot;,&quot;,3,3,1],&quot;t&quot;:[&quot;$\u0001&quot;,&quot;-$\u0001&quot;],&quot;d&quot;:2},&quot;/feed/stocks&quot;,$s,@updates,%[&quot;Price updates: &quot;,&quot;count&quot;,&quot;&quot;])""#),
            "the script gets the money format and the message it fills in: {body}"
        );
        assert!(
            body.contains(r#"<td data-ref="price" class="live-prices__amount" data-key="s1">$129.99</td>"#),
            "server renders prices first: {body}"
        );
        assert!(body.contains(
            r#"<corten-live src="/live/trades" since="0"><p id="last-trade" aria-live="polite">No trades yet.</p></corten-live>"#
        ));
    }

    #[tokio::test]
    async fn the_shell_marks_the_current_link_and_shows_the_viewer() {
        let (_, guest) = send(Request::get("/about").body(Body::empty()).unwrap()).await;
        assert!(guest.contains(r#"<a href="/about" data-nav aria-current="page">About</a>"#), "{guest}");
        assert!(guest.contains(r#"<a href="/" data-nav>Prices</a>"#));
        assert!(guest.contains("Sign in as Ada"));
        let signed_in = Request::get("/").header("cookie", "viewer=Ada").body(Body::empty()).unwrap();
        let (_, ada) = send(signed_in).await;
        assert!(ada.contains("Signed in as Ada") && !ada.contains("Sign in as Ada"));
        let lost =
            Request::get("/nope").header("cookie", "viewer=Ada").header("accept", "text/html").body(Body::empty()).unwrap();
        let (_, lost) = send(lost).await;
        assert!(lost.contains("<h1>Not found</h1>") && lost.contains("Signed in as Ada"), "the error page too: {lost}");
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
    async fn load_more_sends_only_the_new_rows() {
        let more = Request::get("/about?history=on&shown=2").header("corten", "action").body(Body::empty()).unwrap();
        let (_, patch) = send(more).await;
        assert_eq!(
            patch,
            concat!(
                r#"<template id="history" data-append><li>Then live prices.</li><li>Then areas.</li></template>"#,
                r#"<form id="load-more" method="get" action="/about" data-patch><input type="hidden" name="history" value="on">"#,
                r#"<input type="hidden" name="shown" value="4"><button class="bg-surface fg-accent">Load more</button></form>"#
            )
        );
        // Without JavaScript the same form loads the page with every row so far.
        let (_, page) = send(Request::get("/about?history=on&shown=2").body(Body::empty()).unwrap()).await;
        assert_eq!(page.matches("<li>").count(), 4, "{page}");
        assert!(page.contains(r#"<input type="checkbox" name="history" checked>"#), "{page}");
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
