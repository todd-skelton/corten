//! A stock watchlist: the stocks you follow with their prices live, the market's news, price
//! alerts and your account. The table says which of Corten's pieces each route uses.
//!
//! | Route | Piece |
//! |---|---|
//! | `GET /` | `req.screen`: HTML for browsers, `{"title","data"}` for `Accept: application/json`; prices and today's change kept current by a script from a feed, in the page's money and percent formats |
//! | `POST /watch`, `/unwatch` | one form's two actions (a row's Remove button joins the form with `form=`), answered with the rows and the form as a patch with JavaScript, the whole screen without, JSON for data clients; the list is kept on the server (`lists.rs`) |
//! | `GET /feed/stocks`, `/feed/stocks/live` | the live feed the watchlist subscribes to |
//! | `GET /live/trades` | fragments streamed to `<corten-live>`: the last trade, no script of its own |
//! | `GET /news` | an area with components of its own (`Area::components`): their stylesheets load with it only; a filter applied on change keeps the reader's place (`data-keep-place`), sectors in the address (`UrlValue`), Load more adds only the new stories (`view::Append`) |
//! | `GET /news/{id}/read` | a redirect to a place on another site (`Elsewhere::at`), which a client navigation follows in one request |
//! | `POST /news/briefing` | a write that sends the browser back to the form, at the end of a long page (`route.at(Id)`) |
//! | `GET /alerts?until=..`, `POST /alerts` | a form checked on the server (`req.invalid`, field errors), then a write answered in place while its read catches up (`ReadAgain`), as a patch read again from its route, or as the page, which asks to be read again however it was reached |
//! | `POST /sign-in`, `/sign-out` | set and clear a cookie; the layout shows the viewer (`Option<Caller<Viewer>>`), and the runtime reloads the document |
//! | `GET /account` | an area whose every route takes `Caller<Viewer>` (`caller = Viewer`): anyone else is sent to the watchlist, where the sign-in button is |
//! | `POST /avatar` | a form with a file (`Upload`): the picture is read as it arrives, then the other fields; a body over its limit is answered with a message in the form (`UploadError::TooLarge`) |
//! | `GET /echo` | a WebSocket (`Upgrade`): the handshake is checked as a write is, the frames are tokio-tungstenite's |
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use corten::{
    ClientZone, LiveResponse, LiveSince,
    axum::http::{StatusCode, request::Parts},
    feed::{FeedResponse, Since},
    prelude::*,
    security::cookie::{self, SetCookie},
};
use serde::Deserialize;

use crate::{
    lists::{Lists, Watching},
    market::{self, Market, Stock, USD},
    news::{self, Readers},
    theme::*,
};

corten::i18n::messages! {
    /// The app's text. One locale here; a real app lists each catalog.
    pub struct Msg { "en" => "locales/en.json" }
}

/// What the handlers share: the market, each browser's watchlist and the briefing's readers. A
/// handler takes the part it needs (`State<Market>`).
#[derive(Clone, FromRef)]
pub struct Shared {
    market: Market,
    lists: Lists,
    readers: Readers,
}

/// The app: its routes, areas and the task that moves the market. Every page brings its
/// document, `Layout` with the page in it. Everything else is Corten's defaults.
pub fn app(market: Market) -> App {
    let trading = market.clone();
    let shared = Shared { market, lists: Lists::default(), readers: Readers::default() };
    App::new(url::router(shared.clone()))
        .components(&[TOKENS, LivePrices])
        .area(|area| news::url::area(area, shared).components(news::COMPONENTS))
        .area(account::area)
        .error_page(error_page, ())
        // Started by `serve` once it listens, stopped at shutdown; a test's `app.router()` starts
        // none, so its prices stay put.
        .task("market", move |signal| market::simulate(trading, signal))
        .client_zone()
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
            <section class={(card(), stack(Space::_3), max_w_prose())}>
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

/// Every URL the app serves and links to, with its handlers, which share the app's state; links
/// built from these cannot drift from the router.
#[corten::routes(state = Shared)]
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

    #[route("/echo", get = echo)]
    pub struct Echo;
}

/// A WebSocket that answers each text message in capitals. The handshake is Corten's, checked
/// as a write is (another site's page cannot open it); the frames are tokio-tungstenite's.
async fn echo(upgrade: corten::upgrade::Upgrade) -> corten::Result<corten::upgrade::Switch> {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::{
        WebSocketStream,
        tungstenite::{Message, protocol::Role},
    };
    upgrade.websocket(|stream, signal| async move {
        let mut socket = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
        loop {
            let message = tokio::select! {
                message = socket.next() => message,
                // The app is shutting down: say goodbye rather than drop the connection.
                _ = signal.aborted() => {
                    let _ = socket.close(None).await;
                    break;
                }
            };
            let Some(Ok(Message::Text(text))) = message else { break };
            if socket.send(Message::text(text.as_str().to_uppercase())).await.is_err() {
                break;
            }
        }
    })
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
            <body class={(bg_bg(), fg_ink())}>
                <header class={(bg_surface(), border_b_line())}>
                    <div
                        class={(row(Space::_3), justify_between(), flex_wrap(), max_w_page(), mx_auto(), px_5(), py_3())}
                    >
                        <nav class={(row(Space::_1), flex_wrap())}>
                            <span class={(weight_bold(), tracking_tight(), px_2())}>"Watchlist"</span>
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
                <main class={(max_w_page(), mx_auto(), px_5(), py_6())}>{children}</main>
            </body>
        </html>
    }
}

async fn sign_in(req: Req) -> HtmlResult {
    Ok(req.redirect(&url::Home)?.with_cookie(SetCookie::new("viewer", "Ada")))
}

async fn sign_out(req: Req) -> HtmlResult {
    Ok(req.redirect(&url::Home)?.with_cookie(SetCookie::remove("viewer")))
}

/// The last trade as each one happens, for `<corten-live src>`: every item is a fragment the
/// runtime swaps in by `id`, like a form's patch, with the trade count as its cursor. The page
/// says which trade it shows (`since`), and a reconnect which one it last got, so the first
/// fragment is only sent when the browser is behind; otherwise the stream waits for the next.
async fn trades(State(market): State<Market>, since: LiveSince) -> LiveResponse {
    corten::live_watch(market.trades.subscribe(), since, |(n, _)| *n, |(_, text)| LastTrade { text }.to_html())
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
        let stocks = market.stocks.read(|rows| watching.ids.iter().filter_map(|id| rows.get(*id).cloned()).collect());
        let others = Market::listed().filter(|(id, _)| !watching.has(id)).collect();
        let (trades_seen, last_trade) = market.trades.borrow().clone();
        Home { stocks, last_trade, trades_seen, others, message }
    }
}

fn home_view(home: &Home, t: Msg) -> impl Render {
    html! {
        <div class={(stack(Space::_1), mb_5())}>
            <h1>"Watchlist"</h1>
            <p class={muted()}>
                "Prices and today's change arrive as trades happen. Without JavaScript the page still shows them."
            </p>
        </div>
        <div
            class={(display_grid(), columns(Responsive::new(Columns::_1).md(Columns::Main)), gap_5(), items_start())}
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
/// since nothing updates without it. The script counts the updates to the stocks shown, with a
/// message of the page's locale (`t.js()`), and writes each price and change with the page's
/// money and percent formats, so the text a script computes is the server's. Each row's Remove
/// button submits the watch form (`form={WatchFormId}`) to `/unwatch`, so every change to the
/// list is one form's submit, and the runtime keeps only the newest answer. A change replaces
/// the component, whose new rows subscribe again (a snapshot, then the stream); it starts
/// unpaused, and a removed row's button takes focus with it (the message says what changed).
/// The cells print the text the market wrote when the price moved (`Stock::price`,
/// `Stock::change`), in the app's one locale. Styled by `live_prices.css`.
#[component(css, js)]
fn LivePrices<'a>(stocks: &'a [Stock], feed: &'a str, t: Msg) -> impl Patch {
    let (pause, price, change, updates) = refs();
    let (updated, money, percent) = (t.js().prices_updates(), t.js().money(USD), t.js().percent(2));
    let f = UnwatchForm::FIELDS;
    html! {
        <div
            id={PricesId}
            class={stack(Space::_3)}
            on:connect={js::watch(scope, pause, price, change, money, percent, feed, signal, updates, updated)}
        >
            <div class={(row(Space::_3), justify_between())}>
                <h2>"Your stocks"</h2>
                <button ref={pause} type="button" aria-pressed="false" hidden on:click={js::pause(scope, target, price, change)}>
                    "Pause updates"
                </button>
            </div>
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
                            <td ref={change} class={Change} key={stock.id} data-trend={stock.trend()}>{&stock.change}</td>
                            <td>
                                <button
                                    class={Remove}
                                    form={WatchFormId}
                                    formaction={url::Unwatch}
                                    name={f.remove}
                                    value={stock.id}
                                    aria-label={("Remove ", &stock.name)}
                                >
                                    "Remove"
                                </button>
                            </td>
                        </tr>
                    }
                    if stocks.is_empty() {
                        <tr>
                            <td colspan="4" class={Empty}>
                                "You are not watching any stocks. Pick one, and its price shows here."
                            </td>
                        </tr>
                    }
                </tbody>
            </table>
            <p ref={updates} class={muted()}></p>
        </div>
    }
}

/// The price table.
#[derive(Id)]
struct PricesId;

/// The watch form: a watch or a removal replaces it, since its choices change.
#[derive(Id)]
struct WatchFormId;

/// The outcome of the last watch or removal, in the form.
#[derive(Id)]
struct WatchMessageId;

/// Adds a stock to the list: the stocks not watched yet, and what the last change did. The rows'
/// Remove buttons belong to it too.
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
                    <select name={f.id}>for (id, name) in others { <option value={id}>{name}" ("{id}")"</option> }</select>
                </label>
                <button class={w_full()}>"Watch"</button>
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

/// The whole watchlist screen, for a browser or a data client, with the list's cookie for a
/// browser that had none.
fn home_screen(req: &Req, viewer: Option<&Viewer>, watching: &mut Watching, home: &Home) -> HtmlResult {
    let view = home_view(home, req.t());
    let res = req.screen(home, html! { <Layout req={req} title="Watchlist" viewer={viewer} head="">{view}</Layout> })?;
    Ok(watching.with_cookie(res))
}

async fn home(req: Req, viewer: Option<Caller<Viewer>>, State(market): State<Market>, mut watching: Watching) -> HtmlResult {
    let home = Home::new(&market, &watching, String::new());
    home_screen(&req, viewer.as_deref(), &mut watching, &home)
}

/// The stock a watch adds.
#[derive(Deserialize, Fields)]
struct StockForm {
    id: String,
}

/// The stock a row's Remove button takes off. The form's other fields (the stock picked to
/// watch) come along and are not read.
#[derive(Deserialize, Fields)]
struct UnwatchForm {
    remove: String,
}

async fn watch(
    req: Req,
    viewer: Option<Caller<Viewer>>,
    State(market): State<Market>,
    State(lists): State<Lists>,
    mut watching: Watching,
    zone: Option<ClientZone>,
    Form(form): Form<StockForm>,
) -> HtmlResult {
    let mut message = match market.stocks.read(|rows| rows.get(&form.id).map(|s| format!("Watching {} at {}.", s.name, s.price)))
    {
        Some(message) => {
            watching.add(&lists, &form.id);
            message
        }
        None => "That stock is not on the market.".into(),
    };
    // The browser's zone, which the runtime keeps once a page has loaded: no script of the app's.
    if let Some(ClientZone(zone)) = zone {
        message.push_str(&format!(" Alerts follow {zone} time."));
    }
    changed(&req, viewer.as_deref(), &market, &mut watching, message)
}

async fn unwatch(
    req: Req,
    viewer: Option<Caller<Viewer>>,
    State(market): State<Market>,
    State(lists): State<Lists>,
    mut watching: Watching,
    Form(form): Form<UnwatchForm>,
) -> HtmlResult {
    let message = market
        .stocks
        .read(|rows| rows.get(&form.remove).map(|s| format!("Stopped watching {}.", s.name)))
        .unwrap_or_else(|| "That stock is not on the market.".into());
    watching.remove(&lists, &form.remove);
    changed(&req, viewer.as_deref(), &market, &mut watching, message)
}

/// The answer to a change of the list: with JavaScript, the prices and the form as a patch; without
/// it, or to a native screen or an agent, the whole screen.
fn changed(req: &Req, viewer: Option<&Viewer>, market: &Market, watching: &mut Watching, message: String) -> HtmlResult {
    let home = Home::new(market, watching, message);
    if req.mode != Mode::Action {
        return home_screen(req, viewer, watching, &home);
    }
    let res = req.patch(html! {
        <LivePrices stocks={&home.stocks} feed={url::Stocks::PATH} t={req.t()} />
        <WatchForm others={&home.others} message={&home.message} />
    })?;
    Ok(watching.with_cookie(res))
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

/// What the last upload did: empty on the page, then each answer's text, announced.
#[component]
fn AvatarResult(text: &str) -> impl Patch {
    html! { <p id={AvatarResultId} aria-live="polite">{text}</p> }
}

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
        Ok((form, pictures)) => req.patch(AvatarResult { text: &format!("{}: {}", form.name, pictures.join(", ")) }),
        Err(UploadError::TooLarge { .. }) => {
            req.patch_with_status(StatusCode::PAYLOAD_TOO_LARGE, AvatarResult { text: "The pictures are over 1 MB together." })
        }
        Err(error) => Err(error.into()),
    }
}

/// A new price alert, as the form sends it.
#[derive(Deserialize, Fields, Default)]
struct AlertInput {
    id: String,
    above: String,
}

/// The alert form's id: a submit replaces it, with the messages of a refused one.
#[derive(Id)]
struct AlertFormId;

/// The alert form: the stock, the price, and what became of the last alert saved. After a refused
/// submit `input` and `errors` are its, so the form comes back as the user left it; after a
/// save they are empty, as the page shows them.
#[component]
fn AlertForm<'a>(input: &'a AlertInput, errors: &'a Errors, until: Option<u64>) -> impl Patch {
    let f = AlertInput::FIELDS;
    let e = errors.within(AlertFormId);
    html! {
        <form id={AlertFormId} method="post" action={url::Alerts { until: None }} data-patch class={(card(), stack(Space::_3))}>
            {errors.form_message()}
            <label class={stack(Space::_1)}>
                <span class={muted()}>"Stock"</span>
                <select name={f.id}>
                    for (id, name) in Market::listed() { <option value={id} selected={id == input.id}>{name}" ("{id}")"</option> }
                </select>
            </label>
            <label class={stack(Space::_1)}>
                <span class={muted()}>"When the price goes above (USD)"</span>
                <input type="text" inputmode="decimal" name={f.above} value={&input.above} required {..e.aria(f.above)}>
            </label>
            {e.message(f.above)}
            <button id={SaveAlertId}>"Save alert"</button>
            <AlertStatus until={until} />
        </form>
    }
}

/// The alerts page, which reads an alert that has not landed yet. A real app's address carries
/// the write's receipt; here it is the time the write lands, in milliseconds since 1970. Until
/// then the page says so and asks to be read again, which the runtime does in place however the
/// page was reached; once the alert is in, the page stops asking. Alerts are not kept or sent:
/// a real app stores them and tells the user from a task of its own.
async fn alerts(req: Req, viewer: Option<Caller<Viewer>>, Query(at): Query<url::Alerts>) -> HtmlResult {
    // A read again from the form's patch: the status line alone, which asks again from this
    // route while the alert is landing.
    // A view renders only when it is sent, so a page load does not build the read-again.
    let from = url::Alerts { until: at.until };
    let status = html! {
        <AlertStatus until={at.until} />
        {asks_again(at.until).then(|| ReadAgain { after: Duration::from_secs(1) }.from(&from))}
    };
    let page = || async { alerts_page(&req, viewer.as_deref(), &AlertInput::default(), &Errors::new(), at.until) };
    req.patch_or(status, page).await
}

fn alerts_page(req: &Req, viewer: Option<&Viewer>, input: &AlertInput, errors: &Errors, until: Option<u64>) -> HtmlResult {
    let again = asks_again(until).then_some(ReadAgain { after: Duration::from_secs(1) });
    req.page(html! {
        <Layout req={req} title="Alerts" viewer={viewer} head={again}>
            <div class={(stack(Space::_4), max_w_prose())}>
                <div class={stack(Space::_1)}>
                    <h1>"Price alerts"</h1>
                    <p class={muted()}>"Hear about it when a stock goes above a price."</p>
                </div>
                <AlertForm input={input} errors={errors} until={until} />
            </div>
        </Layout>
    })
}

/// An alert that lands in a second and a half. With JavaScript the form's answer is the form,
/// with any field a refused submit marked cleared, and its status line is then read again
/// until the alert is in; without, the alerts page, which reads itself again.
/// The server checks the price and says what is wrong next to the field.
async fn save_alert(req: Req, viewer: Option<Caller<Viewer>>, Form(input): Form<AlertInput>) -> HtmlResult {
    let f = AlertInput::FIELDS;
    let mut errors = Errors::new();
    if Market::ticker(&input.id).is_none() {
        errors.add_form("Pick a stock on the market.");
    }
    match cents(&input.above) {
        None => errors.add(f.above, "Enter a price in dollars and cents, such as 129.99."),
        Some(0) => errors.add(f.above, "Enter a price above zero."),
        Some(_) => {}
    }
    if !errors.is_empty() {
        let form = AlertForm { input: &input, errors: &errors, until: None };
        return req.invalid(&errors, form, || async { alerts_page(&req, viewer.as_deref(), &input, &errors, None) }).await;
    }
    // With JavaScript, the whole form, empty as the page that a save without JavaScript goes to,
    // so a field a refused submit marked is cleared; the reads that follow answer the status line
    // alone.
    let from = url::Alerts { until: Some(now_millis() + 1500) };
    let again = ReadAgain { after: Duration::from_secs(1) }.from(&from);
    let (empty, none) = (AlertInput::default(), Errors::new());
    let form = html! {
        <AlertForm input={&empty} errors={&none} until={from.until} />
        {again}
    };
    req.patch_or(form, || async { req.redirect(&from) }).await
}

/// A price as a person types it (`130`, `129.9`, `129.99`, `.5`, `$1,299.00`), in cents: digits
/// with commas between them, at most two after the point.
fn cents(price: &str) -> Option<u64> {
    let price = price.trim();
    let price = price.strip_prefix('$').unwrap_or(price);
    let (whole, fraction) = price.split_once('.').unwrap_or((price, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    let grouped = whole.is_empty() || whole.split(',').all(|group| !group.is_empty() && digits(group));
    let whole: String = whole.chars().filter(|c| *c != ',').collect();
    if !grouped || (whole.is_empty() && fraction.is_empty()) || whole.len() > 12 || fraction.len() > 2 || !digits(fraction) {
        return None;
    }
    let whole = if whole.is_empty() { 0 } else { whole.parse::<u64>().ok()? };
    Some(whole * 100 + format!("{fraction:0<2}").parse::<u64>().ok()?)
}

/// The alerts page's status line.
#[derive(Id)]
struct AlertStatusId;

/// The alert form's button: a save answers the whole form, and the button keeps focus by its id
/// though a cleared error moves it up the form.
#[derive(Id)]
struct SaveAlertId;

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

/// True while the page should read itself again: the write has not landed, and is due within a
/// minute. A time further out is not one this app wrote (an edited address), and reading it again
/// every second until then would only load the server; the page says it is saving and stops.
fn asks_again(until: Option<u64>) -> bool {
    landing(until) && until.is_some_and(|until| until.saturating_sub(now_millis()) <= 60_000)
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
            <div class={(stack(Space::_4), max_w_prose())}>
                <h1>"Account"</h1>
                <section class={(card(), row(Space::_3), justify_between(), flex_wrap())}>
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
                    <AvatarResult text="" />
                </form>
            </div>
        </Layout>
    })
}

#[cfg(test)]
mod tests {
    use corten::axum::{Router, body::Body, http::Request, response::Response};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::news::{NewsQuery, Sector};

    /// A list id, as the app writes one.
    const LIST: &str = "list=0123456789abcdef0123456789abcdef";

    fn router() -> Router {
        app(Market::new()).router()
    }

    async fn call_on(router: &Router, req: Request<Body>) -> Response {
        router.clone().oneshot(req).await.unwrap()
    }

    async fn call(req: Request<Body>) -> Response {
        call_on(&router(), req).await
    }

    async fn text(res: Response) -> String {
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    async fn send(req: Request<Body>) -> (String, String) {
        let res = call(req).await;
        let kind = res.headers()["content-type"].to_str().unwrap().to_string();
        (kind, text(res).await)
    }

    fn get(uri: &str) -> Request<Body> {
        Request::get(uri).body(Body::empty()).unwrap()
    }

    fn get_with(uri: &str, cookie: &str) -> Request<Body> {
        Request::get(uri).header("cookie", cookie).body(Body::empty()).unwrap()
    }

    fn post(uri: &str, mode: Option<&str>, accept: Option<&str>, body: &'static str) -> Request<Body> {
        post_with(uri, mode, accept, None, body)
    }

    fn post_with(uri: &str, mode: Option<&str>, accept: Option<&str>, cookie: Option<&str>, body: &'static str) -> Request<Body> {
        let mut req = Request::post(uri).header("content-type", "application/x-www-form-urlencoded");
        for (name, value) in [("corten", mode), ("accept", accept), ("cookie", cookie)] {
            if let Some(value) = value {
                req = req.header(name, value);
            }
        }
        req.body(Body::from(body)).unwrap()
    }

    /// The tickers of the rows a page or patch shows, in order.
    fn rows(body: &str) -> Vec<&str> {
        body.split(r#"data-ref="price" class="live-prices__amount" data-key=""#).skip(1).map(|s| &s[..4]).collect()
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
            "the script gets the money and percent formats and the message it fills in, once: {body}"
        );
        assert!(
            body.contains(r#"<td data-ref="price" class="live-prices__amount" data-key="ACME">$129.99</td>"#),
            "server renders prices first: {body}"
        );
        assert!(
            body.contains(r#"<td data-ref="change" class="live-prices__change" data-key="ACME" data-trend="flat">0%</td>"#),
            "and today's change, in the page's percent format: {body}"
        );
        assert_eq!(rows(&body), ["ACME", "GLBX", "INIT"], "a first visit watches three");
        assert!(
            body.contains(r#"<button class="live-prices__remove" form="watch-form" formaction="/unwatch" name="remove" value="ACME" aria-label="Remove Acme Corp">Remove</button>"#),
            "a row's Remove button submits the watch form: {body}"
        );
        assert!(body.contains(r#"<option value="HLTH">Halcyon Health (HLTH)</option>"#), "the others are the form's choices");
        assert!(!body.contains(r#"<option value="ACME">"#), "a watched stock is not offered again");
        assert!(body.contains(
            r#"<corten-live src="/live/trades" since="0"><p id="last-trade" aria-live="polite">No trades yet.</p></corten-live>"#
        ));
    }

    #[tokio::test]
    async fn a_browser_gets_a_list_id_once_and_keeps_its_list_on_the_server() {
        let first = call(get("/")).await;
        let cookie = first.headers()["set-cookie"].to_str().unwrap();
        assert!(cookie.starts_with("list=") && cookie[5..].find(';') == Some(32), "{cookie}");
        assert!(cookie.ends_with("; Path=/; SameSite=Lax; Max-Age=31536000; HttpOnly; Secure"), "{cookie}");
        let app = router();
        let res = call_on(&app, post_with("/watch", Some("action"), None, Some(LIST), "id=SOLR")).await;
        assert!(res.headers().get("set-cookie").is_none(), "a browser with a list keeps its id");
        let page = text(call_on(&app, get_with("/", LIST)).await).await;
        assert_eq!(rows(&page), ["ACME", "GLBX", "INIT", "SOLR"]);
        // Another browser, and a cookie the app did not write, see a list of their own.
        let other = text(call_on(&app, get_with("/", "list=nope")).await).await;
        assert_eq!(rows(&other), ["ACME", "GLBX", "INIT"]);
    }

    #[tokio::test]
    async fn a_page_that_only_reads_the_list_makes_no_id() {
        let res = call(get("/news?mine=on")).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert!(res.headers().get("set-cookie").is_none(), "News reads the list and sets nothing");
        // A first change without a cookie names its new list once, in the answer.
        let app = router();
        let res = call_on(&app, post("/watch", Some("action"), None, "id=SOLR")).await;
        let cookies: Vec<_> = res.headers().get_all("set-cookie").iter().map(|c| c.to_str().unwrap().to_string()).collect();
        assert!(cookies.len() == 1 && cookies[0].starts_with("list="), "{cookies:?}");
        let list = cookies[0].split(';').next().unwrap().to_string();
        assert_eq!(rows(&text(call_on(&app, get_with("/", &list)).await).await), ["ACME", "GLBX", "INIT", "SOLR"]);
    }

    #[tokio::test]
    async fn two_removals_sent_at_once_both_land() {
        let app = router();
        let (a, b) = tokio::join!(
            call_on(&app, post_with("/unwatch", Some("action"), None, Some(LIST), "id=HLTH&remove=ACME")),
            call_on(&app, post_with("/unwatch", Some("action"), None, Some(LIST), "id=HLTH&remove=GLBX")),
        );
        assert_eq!((a.status(), b.status()), (StatusCode::OK, StatusCode::OK));
        let page = text(call_on(&app, get_with("/", LIST)).await).await;
        assert_eq!(rows(&page), ["INIT"]);
    }

    #[tokio::test]
    async fn an_empty_list_keeps_its_table_for_the_next_watch() {
        let app = router();
        for body in ["remove=ACME", "remove=GLBX", "remove=INIT"] {
            call_on(&app, post_with("/unwatch", Some("action"), None, Some(LIST), body)).await;
        }
        let page = text(call_on(&app, get_with("/", LIST)).await).await;
        assert!(page.contains("<tbody><tr><td colspan=\"4\"") && page.contains("You are not watching any stocks."), "{page}");
    }

    #[tokio::test]
    async fn the_shell_marks_the_current_link_and_shows_the_viewer() {
        let (_, guest) = send(get("/news")).await;
        assert!(guest.contains(r#"<a href="/news" data-nav aria-current="page">News</a>"#), "{guest}");
        assert!(guest.contains(r#"<a href="/" data-nav>Stocks</a>"#));
        assert!(guest.contains("Sign in as Ada"));
        let (_, ada) = send(get_with("/", "viewer=Ada")).await;
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

    /// The headlines of the stories a page or patch shows, in order.
    fn headlines(body: &str) -> Vec<&str> {
        body.split(r#"/read">"#).skip(1).map(|s| &s[..s.find('<').unwrap()]).collect()
    }

    #[tokio::test]
    async fn load_more_sends_only_the_next_stories() {
        let more =
            Request::get("/news?sector=energy&sector=technology&shown=3").header("corten", "action").body(Body::empty()).unwrap();
        let (_, patch) = send(more).await;
        assert!(patch.starts_with(r#"<template id="stories" data-append><li class="#), "{patch}");
        // Five stories are about energy or technology: the page had three, these are the last two.
        assert_eq!(
            headlines(&patch),
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
        assert_eq!(headlines(&page).len(), 6, "{page}");
    }

    #[tokio::test]
    async fn a_sector_link_writes_the_parameters_the_page_reads() {
        let link = news::url::News { filter: NewsQuery { sector: vec![Sector::Energy, Sector::Health], ..NewsQuery::default() } };
        assert_eq!(link.href(), "/news?sector=energy&sector=health");
        let (_, page) = send(get(&link.href())).await;
        assert_eq!(headlines(&page).len(), 3, "the first page of the four energy and health stories: {page}");
        assert!(page.contains(r#"<input type="checkbox" name="sector" value="energy" checked>"#), "{page}");
        assert!(page.contains(r#"<details open><summary>Sectors</summary>"#), "a group with a box checked is drawn open: {page}");
        assert!(page.contains(r#"<a href="/news?sector=energy">Energy</a>"#), "a story's sector links to its stories: {page}");
        let mine = news::url::News { filter: NewsQuery { mine: Some(true), shown: Some(3), ..NewsQuery::default() } };
        assert_eq!(mine.href(), "/news?mine=true&shown=3");
        assert_eq!(news::url::News::default().href(), "/news");
    }

    #[tokio::test]
    async fn the_watchlist_filter_shows_stories_about_the_stocks_watched() {
        let app = router();
        call_on(&app, post_with("/unwatch", Some("action"), None, Some(LIST), "remove=ACME")).await;
        // The checkbox sends `on`; a link writes `true`.
        for uri in ["/news?mine=on", "/news?mine=true"] {
            let page = text(call_on(&app, get_with(uri, LIST)).await).await;
            assert!(page.contains(r#"<h2 id="stories-title">Stories about your stocks</h2>"#), "{page}");
            assert_eq!(
                headlines(&page),
                [
                    "Globex wins approval for its northern pipeline",
                    "Initech moves its reporting software to subscriptions",
                    "Globex holds its dividend despite lower gas prices"
                ],
                "{uri}"
            );
        }
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
        assert!(page.contains("You&#39;re on the list for the morning briefing."), "{page}");
    }

    #[tokio::test]
    async fn an_address_the_server_refuses_is_answered_next_to_the_field() {
        let res = call(post("/news/briefing", Some("action"), None, "email=nobody")).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let patch = text(res).await;
        assert!(patch.starts_with(r#"<form id="briefing""#), "{patch}");
        assert!(
            patch.contains(
                r#"value="nobody" required autocomplete="email" aria-invalid="true" aria-describedby="briefing-email-error">"#
            ),
            "{patch}"
        );
        assert!(patch.contains("Enter an email address, such as ada@example.com."), "{patch}");
        // Without JavaScript, the page with the form as the reader left it.
        let res = call(post("/news/briefing", None, None, "email=a%40b")).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let page = text(res).await;
        assert!(page.contains("<h1>Market news</h1>") && page.contains(r#"value="a@b""#), "{page}");
    }

    #[tokio::test]
    async fn the_account_page_is_for_someone_signed_in() {
        let res = call(get("/account")).await;
        assert_eq!((res.status().as_u16(), &res.headers()["location"]), (303, &"/".parse().unwrap()));
        let (_, ada) = send(get_with("/account", "viewer=Ada")).await;
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
    async fn one_watch_handler_serves_every_surface() {
        let res = call(post_with("/watch", Some("action"), None, Some(LIST), "id=HLTH")).await;
        let patch = text(res).await;
        assert!(patch.starts_with(r#"<div data-scope="live-prices" id="prices" "#), "the prices first: {patch}");
        assert_eq!(rows(&patch), ["ACME", "GLBX", "INIT", "HLTH"]);
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
    async fn removing_takes_the_stock_off_the_list_and_offers_it_again() {
        let patch = text(call(post_with("/unwatch", Some("action"), None, Some(LIST), "id=HLTH&remove=GLBX")).await).await;
        assert_eq!(rows(&patch), ["ACME", "INIT"]);
        assert!(patch.contains(r#"<option value="GLBX">Globex (GLBX)</option>"#), "{patch}");
        assert!(patch.contains("Stopped watching Globex."), "{patch}");
    }

    #[tokio::test]
    async fn an_alert_is_read_again_until_it_lands() {
        for price in ["150", "150.5", ".5", "$1,299.99"] {
            let body: &'static str = Box::leak(format!("id=ACME&above={price}").into_boxed_str());
            let (_, patch) = send(post("/alerts", Some("action"), None, body)).await;
            // The whole form, unmarked, as the user left it.
            assert!(patch.starts_with(r#"<form id="alert-form""#) && !patch.contains("aria-invalid"), "{price}: {patch}");
            assert!(
                patch.contains(r#"<p id="alert-status" role="status">Saving your alert...</p></form><template data-corten-read-again="1" data-from="/alerts?until="#),
                "{price}: {patch}"
            );
        }
        let (_, landed) = send(Request::get("/alerts?until=1").header("corten", "action").body(Body::empty()).unwrap()).await;
        assert_eq!(landed, r#"<p id="alert-status" role="status">Your alert is saved.</p>"#);
        let res = call(post("/alerts", None, None, "id=ACME&above=150.5")).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert!(res.headers()["location"].to_str().unwrap().starts_with("/alerts?until="));
    }

    #[tokio::test]
    async fn an_alert_the_server_refuses_says_why_next_to_the_field() {
        let res = call(post("/alerts", Some("action"), None, "id=GLBX&above=1.234")).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let form = text(res).await;
        assert!(form.starts_with(r#"<form id="alert-form""#), "the form, as a patch: {form}");
        assert!(form.contains(r#"<option value="GLBX" selected>"#), "as the user left it: {form}");
        assert!(
            form.contains(
                r#"name="above" value="1.234" required aria-invalid="true" aria-describedby="alert-form-above-error">"#
            ),
            "{form}"
        );
        assert!(form.contains("Enter a price in dollars and cents, such as 129.99."), "{form}");
        let zero = text(call(post("/alerts", Some("action"), None, "id=ACME&above=0.00")).await).await;
        assert!(zero.contains("Enter a price above zero."), "{zero}");
        let unknown = text(call(post("/alerts", Some("action"), None, "id=NOPE&above=5")).await).await;
        assert!(unknown.contains("Pick a stock on the market."), "{unknown}");
        // Without JavaScript, the page with the form.
        let res = call(post("/alerts", None, None, "id=ACME&above=-3")).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(text(res).await.contains("<h1>Price alerts</h1>"));
    }

    #[tokio::test]
    async fn a_page_waits_for_a_write_a_minute_out_at_most() {
        let soon = now_millis() + 30_000;
        let page = text(call(get(&format!("/alerts?until={soon}"))).await).await;
        assert!(page.contains("data-corten-read-again") && page.contains("Saving your alert..."), "{page}");
        let edited = now_millis() + 3_600_000;
        let page = text(call(get(&format!("/alerts?until={edited}"))).await).await;
        assert!(!page.contains("data-corten-read-again") && page.contains("Saving your alert..."), "{page}");
    }

    #[test]
    fn a_price_is_read_in_cents() {
        let read = ["130", "129.9", "129.99", "0.5", ".5", " $1,299.00 ", "1,000,000"].map(cents);
        assert_eq!(read, [Some(13_000), Some(12_990), Some(12_999), Some(50), Some(50), Some(129_900), Some(100_000_000)]);
        assert_eq!(["", ".", "1.234", "1e3", "-1", "1,,0", ",100", "12a"].map(cents), [None; 8]);
    }
}
