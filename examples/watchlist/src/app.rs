//! A stock watchlist: the stocks you follow with their prices live, the market's news, price
//! alerts and your account. Each route shows one of Corten's pieces at work.
//!
//! | Route | Piece |
//! |---|---|
//! | `GET /` | `req.screen`: HTML for browsers, `{"title","data"}` for `Accept: application/json`; prices and today's change kept current by a script from a feed, in the page's money and percent formats |
//! | `POST /watch`, `/unwatch` | form patches with JavaScript (the list and the form), the whole screen without, JSON for data clients; the list is kept in a cookie |
//! | `GET /feed/stocks`, `/feed/stocks/live` | the live feed the watchlist subscribes to |
//! | `GET /live/trades` | fragments streamed to `<corten-live>`: the last trade, no script of its own |
//! | `GET /news` | an area with components of its own (`Area::components`): their stylesheets load with it only; a filter applied on change keeps the reader's place (`data-keep-place`), sectors in the address (`UrlValue`), Load more adds only the new stories (`view::Append`) |
//! | `GET /news/{id}/read` | a redirect to a place on another site (`Elsewhere::at`), which a client navigation follows in one request |
//! | `POST /news/briefing` | a write that sends the browser back to the form, at the end of a long page (`route.at(Id)`) |
//! | `GET /alerts?until=..`, `POST /alerts` | a write answered in place while its read catches up (`ReadAgain`), as a patch read again from its route, or as the page, which asks to be read again however it was reached |
//! | `POST /sign-in`, `/sign-out` | set and clear a cookie; the layout shows the viewer (`Option<Caller<Viewer>>`), and the runtime reloads the document |
//! | `GET /account` | an area whose every route takes `Caller<Viewer>` (`caller = Viewer`): anyone else is sent to the watchlist, where the sign-in button is |
//! | `POST /avatar` | a form with a file (`Upload`): the picture is read as it arrives, then the other fields; a body over its limit is answered with a message in the form (`UploadError::TooLarge`) |
use std::{
    convert::Infallible,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use corten::{
    Fragment, LiveResponse, LiveSince,
    axum::{
        extract::FromRequestParts,
        http::{HeaderValue, StatusCode, header, request::Parts},
    },
    feed::{FeedResponse, Since},
    prelude::*,
    security::cookie::{self, SetCookie},
};
use serde::Deserialize;

use crate::{
    market::{self, Market, Stock, USD},
    news::{self, Readers},
    theme::*,
};

corten::i18n::messages! {
    /// The app's text. One locale here; a real app lists each catalog.
    pub struct Msg { "en" => "locales/en.json" }
}

/// The app: its routes, areas and the task that moves the market. Every page brings its
/// document, `Layout` with the page in it. Everything else is Corten's defaults.
pub fn app(market: Market) -> App {
    let trading = market.clone();
    App::new(url::router(market))
        .components(&[TOKENS, LivePrices])
        .area(|area| news::url::area(area, Readers::default()).components(news::COMPONENTS))
        .area(account::area)
        .error_page(error_page, ())
        // Started by `serve` once it listens, stopped at shutdown; a test's `app.router()` starts
        // none, so its prices stay put.
        .task("market", move |signal| market::simulate(trading, signal))
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
                <p><a href={url::Home}>"Back to your watchlist"</a></p>
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

    #[route("/unwatch", post = unwatch)]
    pub struct Unwatch;

    #[route("/sign-in", post = sign_in)]
    pub struct SignIn;

    #[route("/sign-out", post = sign_out)]
    pub struct SignOut;

    #[route("/avatar", post = avatar)]
    pub struct Avatar;

    #[route("/feed/stocks", get = stocks)]
    pub struct Stocks;

    #[route("/feed/stocks/live", get = stocks_live)]
    pub struct StocksLive;

    #[route("/live/trades", get = trades)]
    pub struct Trades;

    /// The alerts page; `until` is the time a saved alert lands (see `alerts`). The handler
    /// reads it back with this struct.
    #[route("/alerts", get = alerts, post = save_alert)]
    #[derive(serde::Deserialize)]
    pub struct Alerts {
        pub until: Option<u64>,
    }
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

/// The stocks this browser watches, in the order it added them: its `watching` cookie, tickers
/// joined by dots. A first visit watches three. A real app keeps a signed-in user's list in its
/// database; a cookie keeps one for anyone, signed in or not.
pub struct Watching(Vec<String>);

/// What a first visit watches.
const FIRST_WATCHED: [&str; 3] = ["ACME", "GLBX", "INIT"];

impl Watching {
    pub fn has(&self, id: &str) -> bool {
        self.0.iter().any(|watched| watched == id)
    }

    /// The cookie that keeps the list for a year.
    fn cookie(&self) -> HeaderValue {
        SetCookie::new("watching", &self.0.join(".")).max_age(365 * 24 * 60 * 60).header()
    }
}

impl<S: Sync> FromRequestParts<S> for Watching {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Infallible> {
        Ok(Watching(match cookie::get(&parts.headers, "watching") {
            // Only stocks still on the market, each once.
            Some(list) => list.split('.').filter(|id| Market::lists(id)).fold(Vec::new(), |mut ids, id| {
                if !ids.iter().any(|seen| seen == id) {
                    ids.push(id.to_string());
                }
                ids
            }),
            None => FIRST_WATCHED.map(String::from).to_vec(),
        }))
    }
}

/// The document around every page: its title, its own `head` tags, who is looking and the page.
/// Client navigation swaps the title, the tags and the page only; signing in changes the viewer,
/// which makes the runtime load the next page in full.
#[component]
pub fn Layout<'a>(
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
                            <a href={url::Home} data-nav aria-current={req.current(&url::Home)}>"Stocks"</a>
                            <a href={news::url::News::default()} data-nav aria-current={req.current(&news::url::News::default())}>
                                "News"
                            </a>
                            <a
                                href={url::Alerts { until: None }}
                                data-nav
                                aria-current={req.current(&url::Alerts { until: None })}
                            >
                                "Alerts"
                            </a>
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

async fn sign_out(req: Req) -> HtmlResult {
    let mut res = req.redirect(&url::Home)?;
    res.headers_mut().append(header::SET_COOKIE, SetCookie::remove("viewer").header());
    Ok(res)
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
    /// The stocks watched, in the order they were added.
    stocks: Vec<Stock>,
    last_trade: String,
    /// The trade count `last_trade` is at, where the page's live stream starts from.
    #[serde(skip)]
    trades_seen: u64,
    /// The stocks not watched, by name, as `(ticker, name)`: the watch form's choices.
    #[serde(skip)]
    others: Vec<(&'static str, &'static str)>,
    message: String,
}

impl Home {
    fn new(market: &Market, watching: &Watching, message: String) -> Self {
        let stocks = market.stocks.read(|rows| watching.0.iter().filter_map(|id| rows.get(id).cloned()).collect());
        let others = Market::listed().filter(|(id, _)| !watching.has(id)).collect();
        let (trades_seen, last_trade) = market.trades.borrow().clone();
        Home { stocks, last_trade, trades_seen, others, message }
    }
}

fn home_view(home: &Home, t: Msg) -> impl Render {
    html! {
        <div class={(stack(Space::_1), mb(Space::_5))}>
            <h1>"Watchlist"</h1>
            <p class={muted()}>
                "Prices and today's change arrive as trades happen. Without JavaScript the page still shows them."
            </p>
        </div>
        <div
            class={(display(Display::Grid), columns(Responsive::new(Columns::_1).md(Columns::Main)), gap(Space::_5), items(Items::Start))}
        >
            <section class={card()}>
                <LivePrices stocks={&home.stocks} feed={url::Stocks::PATH} t={t} />
            </section>
            <aside class={stack(Space::_5)}>
                <WatchForm others={&home.others} message={&home.message} />
                <section class={(card(), stack(Space::_2))}>
                    <h2>"Market activity"</h2>
                    <corten-live src={url::Trades} since={home.trades_seen}><LastTrade text={&home.last_trade} /></corten-live>
                </section>
            </aside>
        </div>
    }
}

/// The watched stocks with their prices and today's change, rendered here first and then kept
/// current by `live_prices.js` from the feed. The pause button shows only when the script runs,
/// since nothing updates without it. The script counts the updates and shows the count with a
/// message of the page's locale (`t.js()`), and writes each price and change with the page's
/// money and percent formats, so the text a script computes is the server's. The formats are
/// written into the page once, for `watch`, which keeps them for `pause`. A watch or unwatch
/// replaces it, and the new one subscribes again. Styled by `live_prices.css`.
#[component(css, js)]
fn LivePrices<'a>(stocks: &'a [Stock], feed: &'a str, t: Msg) -> impl Patch {
    let (pause, price, change, updates) = refs();
    let (updated, money, percent) = (t.js().prices_updates(), t.js().money(USD), t.js().percent(2));
    let f = StockForm::FIELDS;
    html! {
        <div
            id={PricesId}
            class={stack(Space::_3)}
            on:connect={js::watch(scope, pause, price, change, money, percent, feed, signal, updates, updated)}
        >
            <div class={(row(Space::_3), justify(Justify::Between))}>
                <h2>"Your stocks"</h2>
                <button
                    ref={pause}
                    type="button"
                    aria-pressed="false"
                    hidden
                    on:click={js::pause(scope, target, price, change)}
                >
                    "Pause updates"
                </button>
            </div>
            if stocks.is_empty() {
                <p class={muted()}>"You are not watching any stocks. Pick one, and its price shows here."</p>
            } else {
                <table class={Table}>
                    <thead>
                        <tr>
                            <th>"Stock"</th>
                            <th>"Price"</th>
                            <th>"Today"</th>
                            <th><span class={VisuallyHidden}>"Remove"</span></th>
                        </tr>
                    </thead>
                    <tbody>
                        for stock in stocks {
                            <tr>
                                <td>
                                    <span class={Name}>{stock.name}</span>
                                    <span class={Ticker}>{stock.id}</span>
                                </td>
                                <td ref={price} class={Amount} key={stock.id}>{stock.price}</td>
                                <td ref={change} class={Change} key={stock.id} data-trend={stock.trend()}>
                                    {t.percent(stock.ratio(), 2)}
                                </td>
                                <td>
                                    <form method="post" action={url::Unwatch} data-patch>
                                        <input type="hidden" name={f.id} value={stock.id}>
                                        <button class={Remove} aria-label={("Stop watching ", &stock.name)}>"Remove"</button>
                                    </form>
                                </td>
                            </tr>
                        }
                    </tbody>
                </table>
            }
            <output ref={updates} class={muted()} aria-live="polite"></output>
        </div>
    }
}

/// The watched stocks: a watch or unwatch replaces them.
#[derive(Id)]
struct PricesId;

/// The watch form: a watch or unwatch replaces it, since its choices change.
#[derive(Id)]
struct WatchFormId;

/// The outcome of the last watch or unwatch, in the form.
#[derive(Id)]
struct WatchMessageId;

/// Adds a stock to the list: the stocks not watched yet, and what the last change did.
#[component]
fn WatchForm<'a>(others: &'a [(&'static str, &'static str)], message: &'a str) -> impl Patch {
    let f = StockForm::FIELDS;
    html! {
        <form id={WatchFormId} method="post" action={url::Watch} data-patch class={(card(), stack(Space::_3))}>
            <h2>"Watch a stock"</h2>
            if others.is_empty() {
                <p class={muted()}>"You are watching every stock on the market."</p>
            } else {
                <label class={stack(Space::_1)}>
                    <span class={muted()}>"Stock"</span>
                    <select name={f.id}>
                        for (id, name) in others { <option value={id}>{name}" ("{id}")"</option> }
                    </select>
                </label>
                <button class={w(W::Full)}>"Watch"</button>
            }
            <p id={WatchMessageId} aria-live="polite">{message}</p>
        </form>
    }
}

/// The last trade: rendered inside the home page, then replaced by each fragment `/live/trades`
/// streams.
#[derive(Id)]
struct LastTradeId;

#[component]
fn LastTrade(text: &str) -> impl Render {
    html! { <p id={LastTradeId} aria-live="polite">{text}</p> }
}

/// The whole watchlist screen, for a browser or a data client.
fn home_screen(req: &Req, viewer: Option<&Viewer>, home: &Home) -> HtmlResult {
    let view = home_view(home, req.t());
    req.screen(home, html! { <Layout req={req} title="Watchlist" viewer={viewer} head="">{view}</Layout> })
}

async fn home(req: Req, viewer: Option<Caller<Viewer>>, State(market): State<Market>, watching: Watching) -> HtmlResult {
    home_screen(&req, viewer.as_deref(), &Home::new(&market, &watching, String::new()))
}

/// The stock a watch or unwatch form names.
#[derive(Deserialize, Fields)]
struct StockForm {
    id: String,
}

async fn watch(
    req: Req,
    viewer: Option<Caller<Viewer>>,
    State(market): State<Market>,
    mut watching: Watching,
    Form(form): Form<StockForm>,
) -> HtmlResult {
    let message = match market.stocks.read(|rows| rows.get(&form.id).map(|s| format!("Watching {} at {}.", s.name, s.price))) {
        Some(message) => {
            if !watching.has(&form.id) {
                watching.0.push(form.id);
            }
            message
        }
        None => "That stock is not on the market.".into(),
    };
    changed(&req, viewer.as_deref(), &market, &watching, message)
}

async fn unwatch(
    req: Req,
    viewer: Option<Caller<Viewer>>,
    State(market): State<Market>,
    mut watching: Watching,
    Form(form): Form<StockForm>,
) -> HtmlResult {
    let message = market
        .stocks
        .read(|rows| rows.get(&form.id).map(|s| format!("Stopped watching {}.", s.name)))
        .unwrap_or_else(|| "That stock is not on the market.".into());
    watching.0.retain(|id| *id != form.id);
    changed(&req, viewer.as_deref(), &market, &watching, message)
}

/// The answer to a change of the list: with JavaScript, the list and the form as a patch; without
/// it, or to a native screen or an agent, the whole screen. Either way the browser keeps the list.
fn changed(req: &Req, viewer: Option<&Viewer>, market: &Market, watching: &Watching, message: String) -> HtmlResult {
    let home = Home::new(market, watching, message);
    let mut res = match req.mode {
        Mode::Action => req.patch(html! {
            <LivePrices stocks={&home.stocks} feed={url::Stocks::PATH} t={req.t()} />
            <WatchForm others={&home.others} message={&home.message} />
        }),
        _ => home_screen(req, viewer, &home),
    }?;
    res.headers_mut().append(header::SET_COOKIE, watching.cookie());
    Ok(res)
}

#[derive(Deserialize, Fields)]
struct AvatarForm {
    name: String,
    picture: Files,
}

#[derive(Id)]
struct AvatarFormId;

#[derive(Id)]
struct AvatarResultId;

/// A form with a file: what came, read as it arrives (a picture over 64 KB is refused, the form
/// still read). A body over 1 MiB is answered with the form's message, not the error page.
async fn avatar(req: Req, mut upload: Upload<AvatarForm>) -> HtmlResult {
    let f = AvatarForm::FIELDS;
    upload.limit(1 << 20);
    // Every read can find the body over its limit, so they are one block and its outcome is
    // matched once.
    let read = async {
        let mut pictures = Vec::new();
        while let Some(mut file) = upload.next_file().await? {
            if file.is(f.picture) {
                let size =
                    file.bytes_up_to(64 << 10).await?.map_or_else(|| "too large".to_string(), |b| format!("{} bytes", b.len()));
                pictures.push(format!("{} ({}, {size})", file.filename(), file.content_type()));
            }
        }
        Ok::<_, UploadError>((upload.fields().await?, pictures))
    };
    match read.await {
        Ok((form, pictures)) => {
            req.patch(html! { <p id={AvatarResultId}>{format!("{}: {}", form.name, pictures.join(", "))}</p> })
        }
        Err(UploadError::TooLarge { .. }) => {
            let message = html! { <p id={AvatarResultId}>"The pictures are over 1 MB together."</p> };
            req.patch_with_status(StatusCode::PAYLOAD_TOO_LARGE, message)
        }
        Err(error) => Err(error.into()),
    }
}

/// A new price alert.
#[derive(Deserialize, Fields)]
struct AlertForm {
    id: String,
    above: String,
}

/// The alerts page, which reads an alert that has not landed yet. A real app's address carries
/// the write's receipt; here it is the time the write lands, in milliseconds since 1970. Until
/// then the page says so and asks to be read again, which the runtime does in place however the
/// page was reached; once the alert is in, the page stops asking.
async fn alerts(req: Req, viewer: Option<Caller<Viewer>>, Query(at): Query<url::Alerts>) -> HtmlResult {
    if req.mode == Mode::Action {
        return alert_status(&req, at.until);
    }
    let f = AlertForm::FIELDS;
    let again = landing(at.until).then_some(ReadAgain { after: Duration::from_secs(1) });
    req.page(html! {
        <Layout req={&req} title="Alerts" viewer={viewer.as_deref()} head={again}>
            <div class={(stack(Space::_4), max_w(MaxW::Prose))}>
                <div class={stack(Space::_1)}>
                    <h1>"Price alerts"</h1>
                    <p class={muted()}>"Hear about it when a stock goes above a price."</p>
                </div>
                <form method="post" action={url::Alerts { until: None }} data-patch class={(card(), stack(Space::_3))}>
                    <label class={stack(Space::_1)}>
                        <span class={muted()}>"Stock"</span>
                        <select name={f.id}>
                            for (id, name) in Market::listed() { <option value={id}>{name}" ("{id}")"</option> }
                        </select>
                    </label>
                    <label class={stack(Space::_1)}>
                        <span class={muted()}>"When the price goes above (USD)"</span>
                        <input type="number" name={f.above} min="0.01" step="0.01" inputmode="decimal" required>
                    </label>
                    <button>"Save alert"</button>
                    <AlertStatus until={at.until} />
                </form>
            </div>
        </Layout>
    })
}

/// An alert that lands in a second and a half. With JavaScript the form's answer is the status
/// line, read again until the alert is in; without, the alerts page, which reads itself again.
/// The form's `min` and `step` stop a person before a price that is not one; a client that skips
/// them is refused.
async fn save_alert(req: Req, Form(form): Form<AlertForm>) -> HtmlResult {
    if !Market::lists(&form.id) || cents(&form.above).is_none_or(|cents| cents == 0) {
        return Err(corten::Error::status(StatusCode::UNPROCESSABLE_ENTITY));
    }
    let until = Some(now_millis() + 1500);
    if req.mode == Mode::Action {
        return alert_status(&req, until);
    }
    req.redirect(&url::Alerts { until })
}

/// A price as the form sends it (`130`, `129.9`, `129.99`), in cents.
fn cents(price: &str) -> Option<u64> {
    let (whole, fraction) = price.split_once('.').unwrap_or((price, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || whole.len() > 12 || fraction.len() > 2 || !digits(whole) || !digits(fraction) {
        return None;
    }
    let fraction = format!("{fraction:0<2}");
    Some(whole.parse::<u64>().ok()? * 100 + fraction.parse::<u64>().ok()?)
}

/// The status line alone, as a patch: while the alert is landing it asks to be read again from
/// the alerts route with the write's time in it, which answers this again.
fn alert_status(req: &Req, until: Option<u64>) -> HtmlResult {
    let from = url::Alerts { until };
    let again = landing(until).then(|| ReadAgain { after: Duration::from_secs(1) }.from(&from));
    req.patch(html! {
        <AlertStatus until={until} />
        {again}
    })
}

/// The alerts page's status line.
#[derive(Id)]
struct AlertStatusId;

/// What became of the last alert saved: landing, saved, or none yet.
#[component]
fn AlertStatus(until: Option<u64>) -> impl Patch {
    let text = match until {
        None => "",
        Some(_) if landing(until) => "Saving your alert...",
        Some(_) => "Your alert is saved.",
    };
    html! { <p id={AlertStatusId} role="status">{text}</p> }
}

/// True while a write that lands at `until` (milliseconds since 1970) has not.
fn landing(until: Option<u64>) -> bool {
    until.is_some_and(|until| now_millis() < until)
}

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Only someone signed in gets here: the handler takes the viewer.
async fn account(req: Req, Caller(viewer): Caller<Viewer>) -> HtmlResult {
    let f = AvatarForm::FIELDS;
    let viewer = &viewer;
    req.page(html! {
        <Layout req={&req} title="Account" viewer={Some(viewer)} head="">
            <div class={(stack(Space::_4), max_w(MaxW::Prose))}>
                <h1>"Account"</h1>
                <section class={(card(), row(Space::_3), justify(Justify::Between), wrap(Wrap::Wrap))}>
                    <p>"Signed in as "{&viewer.name}"."</p>
                    <form method="post" action={url::SignOut} data-patch><button class={secondary()}>"Sign out"</button></form>
                </section>
                <form
                    id={AvatarFormId}
                    method="post"
                    action={url::Avatar}
                    enctype="multipart/form-data"
                    data-patch
                    class={(card(), stack(Space::_3))}
                >
                    <h2>"Profile picture"</h2>
                    <label class={stack(Space::_1)}>
                        <span class={muted()}>"Display name"</span>
                        <input name={f.name} value={&viewer.name} required autocomplete="nickname">
                    </label>
                    <label class={stack(Space::_1)}>
                        <span class={muted()}>"Picture (up to 64 KB)"</span>
                        <input type="file" name={f.picture} accept="image/*" required>
                    </label>
                    <button>"Upload"</button>
                    <p id={AvatarResultId} aria-live="polite"></p>
                </form>
            </div>
        </Layout>
    })
}

#[cfg(test)]
mod tests {
    use corten::axum::{body::Body, http::Request, response::Response};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::news::Sector;

    async fn call(req: Request<Body>) -> Response {
        app(Market::new()).router().oneshot(req).await.unwrap()
    }

    async fn send(req: Request<Body>) -> (String, String) {
        let res = call(req).await;
        let kind = res.headers()["content-type"].to_str().unwrap().to_string();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        (kind, String::from_utf8(body.to_vec()).unwrap())
    }

    fn get(uri: &str) -> Request<Body> {
        Request::get(uri).body(Body::empty()).unwrap()
    }

    fn post(uri: &str, mode: Option<&str>, accept: Option<&str>, body: &'static str) -> Request<Body> {
        let mut req = Request::post(uri).header("content-type", "application/x-www-form-urlencoded");
        if let Some(mode) = mode {
            req = req.header("corten", mode);
        }
        if let Some(accept) = accept {
            req = req.header("accept", accept);
        }
        req.body(Body::from(body)).unwrap()
    }

    /// The script's rows are checked against these declarations by `npm run check`.
    #[test]
    fn script_types_match_the_server() {
        corten::types::Types::new().add::<Stock>().sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
    }

    #[tokio::test]
    async fn home_renders_the_watched_stocks_and_script_for_browsers() {
        let (kind, body) = send(get("/")).await;
        assert!(kind.starts_with("text/html"));
        assert!(body.contains("/scripts/live_prices.js\">"), "the script module is preloaded: {body}");
        assert!(
            body.contains(r#"data-on="connect:live_prices.watch($c,@pause,#price,#change,%{&quot;z&quot;:&quot;0&quot;,&quot;s&quot;:[&quot;.&quot;,&quot;,&quot;,3,3,1],&quot;t&quot;:[&quot;$\u0001&quot;,&quot;-$\u0001&quot;],&quot;d&quot;:2},%{&quot;z&quot;:&quot;0&quot;,&quot;s&quot;:[&quot;.&quot;,&quot;,&quot;,3,3,1],&quot;t&quot;:[&quot;\u0001%&quot;,&quot;-\u0001%&quot;],&quot;p&quot;:2},&quot;/feed/stocks&quot;,$s,@updates,%[&quot;Price updates: &quot;,&quot;count&quot;,&quot;&quot;])""#),
            "the script gets the money and percent formats and the message it fills in: {body}"
        );
        assert!(
            body.contains(r#"<td data-ref="price" class="live-prices__amount" data-key="ACME">$129.99</td>"#),
            "server renders prices first: {body}"
        );
        assert!(
            body.contains(r#"<td data-ref="change" class="live-prices__change" data-key="ACME" data-trend="flat">0%</td>"#),
            "and today's change, in the page's percent format: {body}"
        );
        assert_eq!(body.matches(r#"data-ref="price""#).count(), 3, "a first visit watches three: {body}");
        assert!(body.contains(r#"<option value="HLTH">Halcyon Health (HLTH)</option>"#), "the others are the form's choices");
        assert!(!body.contains(r#"<option value="ACME">"#), "a watched stock is not offered again");
        assert!(body.contains(
            r#"<corten-live src="/live/trades" since="0"><p id="last-trade" aria-live="polite">No trades yet.</p></corten-live>"#
        ));
    }

    #[tokio::test]
    async fn the_watch_list_is_the_cookie_s_stocks_on_the_market_each_once() {
        let req = Request::get("/").header("cookie", "watching=SOLR.NOPE.SOLR.GLBX").body(Body::empty()).unwrap();
        let (_, body) = send(req).await;
        let keys: Vec<&str> =
            body.split(r#"data-ref="price" class="live-prices__amount" data-key=""#).skip(1).map(|s| &s[..4]).collect();
        assert_eq!(keys, ["SOLR", "GLBX"], "{body}");
        let req = Request::get("/").header("cookie", "watching=").body(Body::empty()).unwrap();
        let (_, empty) = send(req).await;
        assert!(empty.contains("You are not watching any stocks.") && !empty.contains("<table"), "{empty}");
    }

    #[tokio::test]
    async fn the_shell_marks_the_current_link_and_shows_the_viewer() {
        let (_, guest) = send(get("/news")).await;
        assert!(guest.contains(r#"<a href="/news" data-nav aria-current="page">News</a>"#), "{guest}");
        assert!(guest.contains(r#"<a href="/" data-nav>Stocks</a>"#));
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
    async fn only_the_news_pages_link_their_area_stylesheet() {
        let links = |body: &str| body.matches("data-corten-style").count();
        let (_, home) = send(get("/")).await;
        assert_eq!(links(&home), 0, "{home}");
        let (_, news) = send(get("/news")).await;
        assert!(news.contains(r#"/sections/news.css" data-corten-style>"#), "{news}");
        assert!(news.contains(r#"<p class="disclaimer__note">"#), "{news}");
    }

    #[tokio::test]
    async fn load_more_sends_only_the_next_stories() {
        let more =
            Request::get("/news?sector=energy&sector=technology&shown=3").header("corten", "action").body(Body::empty()).unwrap();
        let (_, patch) = send(more).await;
        assert!(patch.starts_with(r#"<template id="stories" data-append><li class="#), "{patch}");
        let headlines: Vec<&str> = patch.split(r#"/read">"#).skip(1).map(|s| &s[..s.find('<').unwrap()]).collect();
        // Five stories are about energy or technology: the page had three, these are the last two.
        assert_eq!(
            headlines,
            [
                "Sunreach Solar cuts panel prices for the second time this year",
                "Globex holds its dividend despite lower gas prices"
            ]
        );
        assert!(
            patch.ends_with(concat!(
                r#"<form id="load-more" method="get" action="/news" data-patch><input type="hidden" name="sector" value="energy">"#,
                r#"<input type="hidden" name="sector" value="technology"><input type="hidden" name="shown" value="5">"#,
                r#"<button class="bg-surface fg-accent w-full" hidden>Load more stories</button></form>"#
            )),
            "the button for what is left, with the filter: {patch}"
        );
        // Without JavaScript the same form loads the page with every story so far.
        let (_, page) = send(get("/news?shown=3")).await;
        assert_eq!(page.matches("/read\">").count(), 6, "{page}");
    }

    #[tokio::test]
    async fn a_sector_link_writes_the_word_the_filter_reads() {
        let link = news::url::News { sector: Some(Sector::Energy), subscribed: None };
        assert_eq!(link.href(), "/news?sector=energy");
        let (_, page) = send(get(&link.href())).await;
        assert_eq!(page.matches("/read\">").count(), 3, "the three energy stories: {page}");
        assert!(page.contains(r#"<input type="checkbox" name="sector" value="energy" checked>"#), "{page}");
        assert!(page.contains(r#"<details open><summary>Sectors</summary>"#), "a group with a box checked is drawn open: {page}");
    }

    #[tokio::test]
    async fn the_watchlist_filter_shows_stories_about_the_stocks_watched() {
        let req = Request::get("/news?mine=on").header("cookie", "watching=HLTH").body(Body::empty()).unwrap();
        let (_, page) = send(req).await;
        assert!(page.contains(r#"<h2 id="stories-title">Stories about your stocks</h2>"#), "{page}");
        assert_eq!(page.matches("/read\">").count(), 1, "{page}");
        assert!(page.contains(r#"<a href="/news/halcyon-trial/read">"#), "{page}");
    }

    #[tokio::test]
    async fn a_story_link_goes_to_its_place_on_the_publishers_page() {
        let res = call(get("/news/acme-outlook/read")).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers()["location"], "https://news.example.com/markets/acme-outlook#guidance");
        assert_eq!(call(get("/news/nope/read")).await.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn subscribing_goes_back_to_the_form_which_says_so() {
        let res = call(post("/news/briefing", None, None, "email=ada%40example.com")).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers()["location"], "/news?subscribed=true#briefing");
        let (_, page) = send(get("/news?subscribed=true")).await;
        assert!(page.contains("You&#39;re subscribed."), "{page}");
        assert_eq!(call(post("/news/briefing", None, None, "email=nobody")).await.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn the_account_page_is_for_someone_signed_in() {
        let res = call(get("/account")).await;
        assert_eq!((res.status().as_u16(), &res.headers()["location"]), (303, &"/".parse().unwrap()));
        let signed_in = Request::get("/account").header("cookie", "viewer=Ada").body(Body::empty()).unwrap();
        let (_, ada) = send(signed_in).await;
        assert!(ada.contains("<p>Signed in as Ada.</p>"), "{ada}");
        assert!(
            ada.contains(r#"enctype="multipart/form-data""#) && ada.contains(r#"<p id="avatar-result" aria-live="polite"></p>"#),
            "{ada}"
        );
        let res = call(post("/sign-out", None, None, "")).await;
        assert!(res.headers()["set-cookie"].to_str().unwrap().starts_with("viewer=; Path=/; SameSite=Lax; Max-Age=0"));
    }

    #[tokio::test]
    async fn home_is_json_for_data_clients() {
        let (kind, body) = send(Request::get("/").header("accept", "application/json").body(Body::empty()).unwrap()).await;
        assert_eq!(kind, "application/json");
        assert!(
            body.starts_with(concat!(
                r#"{"title":"Watchlist | Corten","data":{"stocks":[{"id":"ACME","name":"Acme Corp","price_cents":12999,"#,
                r#""open_cents":12999,"change_bp":0,"price":"$129.99","change":"0%"}"#
            )),
            "{body}"
        );
        assert!(!body.contains("HLTH") && !body.contains("trades_seen"), "only the watched stocks: {body}");
    }

    #[tokio::test]
    async fn one_watch_handler_serves_every_surface_and_keeps_the_list() {
        let res = call(post("/watch", Some("action"), None, "id=HLTH")).await;
        assert_eq!(
            res.headers()["set-cookie"].to_str().unwrap(),
            "watching=ACME.GLBX.INIT.HLTH; Path=/; SameSite=Lax; Max-Age=31536000; HttpOnly; Secure"
        );
        let patch = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        assert!(patch.starts_with(r#"<div data-scope="live-prices" id="prices" "#), "the list first: {patch}");
        assert!(patch.contains(r#"data-key="HLTH">$88.40</td>"#), "{patch}");
        assert!(patch.contains(r#"<form id="watch-form" method="post" action="/watch" data-patch"#), "{patch}");
        assert!(!patch.contains(r#"<option value="HLTH">"#), "{patch}");
        assert!(
            patch.ends_with(r#"<p id="watch-message" aria-live="polite">Watching Halcyon Health at $88.40.</p></form>"#),
            "{patch}"
        );
        let (kind, page) = send(post("/watch", None, None, "id=HLTH")).await;
        assert!(kind.starts_with("text/html") && page.contains("<html") && page.contains("Watching Halcyon Health"));
        let (kind, data) = send(post("/watch", None, Some("application/json"), "id=HLTH")).await;
        assert_eq!(kind, "application/json");
        assert!(data.ends_with(r#""message":"Watching Halcyon Health at $88.40."}}"#), "{data}");
    }

    #[tokio::test]
    async fn unwatching_takes_the_stock_off_the_list() {
        let res = call(post("/unwatch", Some("action"), None, "id=GLBX")).await;
        assert!(res.headers()["set-cookie"].to_str().unwrap().starts_with("watching=ACME.INIT;"));
        let patch = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        assert!(
            !patch.contains(r#"data-key="GLBX""#) && patch.contains(r#"<option value="GLBX">Globex (GLBX)</option>"#),
            "{patch}"
        );
        assert!(patch.contains("Stopped watching Globex."), "{patch}");
    }

    #[tokio::test]
    async fn an_alert_is_read_again_until_it_lands() {
        let (_, patch) = send(post("/alerts", Some("action"), None, "id=ACME&above=150")).await;
        assert!(patch.starts_with(r#"<p id="alert-status" role="status">Saving your alert...</p><template data-corten-read-again="1" data-from="/alerts?until="#), "{patch}");
        let (_, landed) = send(Request::get("/alerts?until=1").header("corten", "action").body(Body::empty()).unwrap()).await;
        assert_eq!(landed, r#"<p id="alert-status" role="status">Your alert is saved.</p>"#);
        let res = call(post("/alerts", None, None, "id=ACME&above=150.5")).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(res.headers()["location"].to_str().unwrap().starts_with("/alerts?until="));
        for refused in ["id=ACME&above=0", "id=ACME&above=1.234", "id=ACME&above=-3", "id=NOPE&above=5"] {
            assert_eq!(
                call(post("/alerts", Some("action"), None, refused)).await.status(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{refused}"
            );
        }
    }

    #[test]
    fn a_price_is_read_in_cents() {
        assert_eq!(
            [cents("130"), cents("129.9"), cents("129.99"), cents("0.5")],
            [Some(13_000), Some(12_990), Some(12_999), Some(50)]
        );
        assert_eq!([cents(""), cents(".5"), cents("1.234"), cents("1e3"), cents("-1")], [None; 5]);
    }
}


