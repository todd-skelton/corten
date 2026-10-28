//! A stock watchlist: the stocks you follow with their prices live, the market's news, price
//! alerts and your account. The table says which of Corten's pieces each route uses, and the file
//! it is in. This file holds the routes, the app and its error page; `layout.rs` the document
//! around every page.
//!
//! | Route | File | Piece |
//! |---|---|---|
//! | `GET /` | `home.rs` | `req.screen`: HTML for browsers, `{"title","data"}` for `Accept: application/json`; prices and today's change kept current by a script from a feed, in the page's money and percent formats |
//! | `POST /watch`, `/unwatch` | `home.rs` | one form's two actions (a row's Remove button joins the form with `form=`), answered with the rows and the form as a patch with JavaScript, the whole screen without, JSON for data clients; the list is kept on the server (`lists.rs`) under the browser's id (`visitor.rs`) |
//! | `GET /feed/stocks`, `/feed/stocks/live` | `home.rs` | the live feed the watchlist subscribes to |
//! | `GET /live/trades` | `home.rs` | fragments streamed to `<corten-live>`: the last trade, no script of its own |
//! | `GET /news` | `news.rs` | an area with components of its own (`Area::components`): their stylesheets load with it only; a filter applied on change keeps the reader's place (`data-keep-place`), sectors in the address (`UrlValue`), Load more adds only the new stories (`view::Append`) |
//! | `GET /news/{id}/read` | `news.rs` | a redirect to a place on another site (`Elsewhere::at`), which a client navigation follows in one request |
//! | `POST /news/briefing` | `news.rs` | a write that sends the browser back to the form, at the end of a long page (`route.at(Id)`) |
//! | `GET /alerts?saved=..`, `POST /alerts`, `POST /alerts/remove` | `alerts.rs` | a form checked on the server (`req.invalid`, field errors) and a list of what the browser keeps; a write answers with its receipt, and while reads catch up (`ReadAgain`) the status and list are read again from their route as a patch, or the page asks to be read again however it was reached |
//! | `POST /sign-in`, `/sign-out` | `account.rs` | set and clear a cookie; the layout shows the viewer (`Option<Caller<Viewer>>`), and the runtime reloads the document |
//! | `GET /account` | `account.rs` | an area whose every route takes `Caller<Viewer>` (`caller = Viewer`): anyone else is sent to the watchlist, where the sign-in button is |
//! | `POST /avatar` | `account.rs` | a form with a file (`Upload`): the picture is read as it arrives, its type from its bytes, and kept; the header and the account page show it; a body over its limit is answered with a message in the form (`UploadError::TooLarge`) |
//! | `GET /pictures/{id}` | `account.rs` | an uploaded picture as its type (`Media`), at a URL that names its bytes, so every cache keeps it for good (`Public::immutable`) |
//! | `GET /quotes` | `socket.rs` | a WebSocket (`Upgrade`) for a ticker outside the browser: it names stocks and is sent each one's price as it trades; the handshake is checked as a write is, the frames are tokio-tungstenite's, and shutdown closes it |
use corten::prelude::*;

use crate::{
    account::{self, Pictures, Viewer},
    alerts::{self, AlertStore},
    home,
    layout::Layout,
    lists::Lists,
    market::{self, Market},
    news::{self, Readers},
    socket,
    theme::*,
};

corten::i18n::messages! {
    /// The app's text. One locale here; a real app lists each catalog.
    pub struct Msg { "en" => "locales/en.json" }
}

/// What the handlers share: the market, each browser's watchlist and alerts, the pictures people
/// uploaded and the briefing's readers. A handler takes the part it needs (`State<Market>`).
#[derive(Clone, FromRef)]
pub struct Shared {
    market: Market,
    lists: Lists,
    alerts: AlertStore,
    pictures: Pictures,
    readers: Readers,
}

/// The app: its routes, areas and the task that moves the market. Every page brings its
/// document, `Layout` with the page in it. Everything else is Corten's defaults.
pub fn app(market: Market) -> App {
    app_with(market, AlertStore::new(alerts::LAG))
}

/// The app with an alert store of the caller's: a test's lands its writes at once.
pub(crate) fn app_with(market: Market, alerts: AlertStore) -> App {
    let trading = market.clone();
    let shared = Shared { market, lists: Lists::default(), alerts, pictures: Pictures::default(), readers: Readers::default() };
    App::new(url::router(shared.clone()))
        .components(&[TOKENS, home::LivePrices])
        .area(|area| news::url::area(area, shared.clone()).components(news::COMPONENTS))
        .area(|area| account::url::area(area, shared.clone()))
        // The error page shows who is asking, with their picture, so it has the app's state too.
        .error_page(error_page, shared)
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
pub mod url {
    #[route("/", get = home::page)]
    pub struct Home;

    #[route("/watch", post = home::watch)]
    pub struct Watch;

    #[route("/unwatch", post = home::unwatch)]
    pub struct Unwatch;

    #[route("/sign-in", post = account::sign_in)]
    pub struct SignIn;

    #[route("/sign-out", post = account::sign_out)]
    pub struct SignOut;

    #[route("/avatar", post = account::avatar)]
    pub struct Avatar;

    /// An uploaded picture; `id` names its bytes (`account::Pictures`).
    #[route("/pictures/{id}", get = account::picture)]
    pub struct Picture<'a> {
        pub id: &'a str,
    }

    #[route("/feed/stocks", get = home::stocks)]
    pub struct Stocks;

    #[route("/feed/stocks/live", get = home::stocks_live)]
    pub struct StocksLive;

    #[route("/live/trades", get = home::trades)]
    pub struct Trades;

    /// The alerts page; `saved` is the receipt of a write the page waits for (see
    /// `alerts::page`). The handler reads it back with this struct.
    #[route("/alerts", get = alerts::page, post = alerts::save)]
    #[derive(serde::Deserialize)]
    pub struct Alerts {
        pub saved: Option<u64>,
    }

    #[route("/alerts/remove", post = alerts::remove)]
    pub struct RemoveAlert;

    /// Prices as they trade, for a client outside the browser (`socket.rs`).
    #[route("/quotes", get = socket::quotes)]
    pub struct Quotes;
}
