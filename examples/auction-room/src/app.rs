//! A live card auction: people sign in and bid on the same lots at once.
//!
//! | Route | What |
//! |---|---|
//! | `GET /` | the lots, with each one's price and time left, kept current while the page is open |
//! | `GET /live` | the lobby's list, streamed to its `<corten-live>` |
//! | `GET /lots/{id}` | a lot: its price, its bids and the bid form; read again every few seconds without JavaScript while it is open |
//! | `GET /lots/{id}/live` | the lot as it stands, streamed |
//! | `GET /lots/{id}/bids` | every bid on the lot, a CSV download |
//! | `GET, POST /sign-in` | a session, kept in the store; at most `SIGN_IN_FAILURES` refused sign-ins a minute per address |
//! | `POST /sign-out` | ends the session |
//! | `POST /me/bids/{lot}` | a bid: written once per submit key, refused below the price, at most `BIDS_PER_MINUTE` a minute per bidder |
//! | `GET /me/notices` | the signed-in bidder's own news (outbid, won), streamed to them alone |
//! | `GET /house`, `POST /house/lots/{id}/end` | the house's desk, where it ends a lot early |
use std::{
    collections::HashMap,
    fmt::Write as _,
    hash::Hash,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use corten::{
    ClientIp, Error, LiveResponse, LiveSince, Media, Signal,
    axum::{
        extract::{FromRequestParts, Request},
        http::{Method, StatusCode, request::Parts},
        response::{IntoResponse, Response},
    },
    i18n::Minor,
    prelude::*,
    security::{
        Nonce,
        cookie::{self, SetCookie},
    },
};
use serde::Deserialize;

use crate::{
    room::{News, Room},
    store::{self, Lot, MAX_CENTS, Placed, Refusal, Standing, Store},
    text::{Msg, USD},
    views::{self, BidForm, Board, LotStanding, NoticeLine, SignInForm, You},
};

/// The auction: its store, and what the pages show live.
#[derive(Clone)]
pub struct Auction {
    pub store: Store,
    pub room: Room,
    /// What signs the house in (`HOUSE_KEY`); without one, no one can.
    house_key: Option<Arc<str>>,
}

/// The demo's lots: card-market's cards, each ending some minutes after the server starts.
/// `(card, set, condition, opening cents, step cents, minutes)`.
const DEMO: [(&str, &str, &str, u64, u64, u64); 6] = [
    ("Cinder Cartographer", "Ember Reach", "Moderately Played", 500, 50, 4),
    ("Crown of Thorns and Glass", "Hollow Crown", "Near Mint", 1500, 100, 7),
    ("Ashen Drake", "Ember Reach", "Near Mint", 2500, 100, 10),
    ("Regent of Ash", "Hollow Crown", "Lightly Played", 4000, 250, 15),
    ("Kiln Colossus", "Ember Reach", "Near Mint", 5000, 250, 20),
    ("Salt Oracle", "Tidebreak", "Near Mint", 8000, 500, 30),
];

/// The demo's lots for round `round`, from `now`: `ashen-drake`, then `ashen-drake-2`.
fn demo_lots(round: usize, now: SystemTime) -> Vec<Lot> {
    DEMO.iter()
        .map(|&(card, set, condition, opening_cents, step_cents, minutes)| {
            let slug = card.to_lowercase().replace(' ', "-");
            Lot {
                id: if round == 0 { slug } else { format!("{slug}-{}", round + 1) },
                card: card.into(),
                set: set.into(),
                condition: condition.into(),
                opening_cents,
                step_cents,
                ends_at: now + Duration::from_secs(minutes * 60),
            }
        })
        .collect()
}

impl Auction {
    /// Opens the store at `url`, with a new round of the demo's lots when none is open, and
    /// loads every lot into the room.
    pub async fn open(url: &str, house_key: Option<String>) -> Result<Self, store::Error> {
        let store = Store::open(url).await?;
        let now = SystemTime::now();
        let mut lots = store.lots().await?;
        if lots.iter().all(|lot| lot.ends_at <= now) {
            store.add_lots(&demo_lots(lots.len() / DEMO.len(), now)).await?;
            lots = store.lots().await?;
        }
        let house_key = house_key.filter(|key| !key.is_empty()).map(Into::into);
        let auction = Auction { store, room: Room::default(), house_key };
        for lot in &lots {
            auction.refresh(&lot.id).await?;
        }
        Ok(auction)
    }

    /// Reads lot `id` from the store into the room, after it changed: what everyone watching
    /// it sees next.
    async fn refresh(&self, id: &str) -> Result<Option<Standing>, store::Error> {
        let standing = self.store.standing(id).await?;
        if let Some(standing) = &standing {
            self.room.update(standing.clone());
        }
        Ok(standing)
    }

    /// What the layout shows of who is signed in.
    pub(crate) fn you(&self, name: &str, house: bool) -> You {
        You { name: name.to_string(), house, notice: self.room.notice(name) }
    }
}

/// The app: the pages, the bidder's area with its limit, the house's area, and the task that
/// closes lots as their time runs out.
pub fn app(auction: Auction) -> App {
    let closing = auction.clone();
    App::new(url::router(auction.clone()))
        .area(|area| door::area(area, auction.clone()).middleware(SignInLimit(Limit::new(SIGN_IN_FAILURES))))
        .area(|area| {
            me::area(area, auction.clone()).middleware(BidLimit { auction: auction.clone(), limit: Limit::new(BIDS_PER_MINUTE) })
        })
        .area(|area| house::area(area, auction.clone()))
        .assets(views::Static)
        .components(views::COMPONENTS)
        .components(corten_ui::COMPONENTS)
        .messages::<Msg>()
        .error_page(views::error_page, auction.clone())
        .task("closing", move |signal| close_lots(closing, signal))
}

/// Anyone may look: the lots, a lot, its bids.
#[corten::routes(state = Auction)]
pub mod url {
    #[route("/", get = home)]
    pub struct Home;

    #[route("/live", get = board_live)]
    pub struct BoardLive;

    #[route("/lots/{id}", get = lot)]
    pub struct Lot<'a> {
        pub id: &'a str,
    }

    #[route("/lots/{id}/live", get = lot_live)]
    pub struct LotLive<'a> {
        pub id: &'a str,
    }

    #[route("/lots/{id}/bids", get = history)]
    pub struct History<'a> {
        pub id: &'a str,
    }

    #[route("/sign-out", post = sign_out)]
    pub struct SignOut;
}

/// Signing in, with the limit on refused sign-ins.
#[corten::routes(area = SignIn, state = Auction)]
pub mod door {
    #[route("/sign-in", get = sign_in_page, post = sign_in)]
    pub struct SignIn;
}

/// A buyer's own: every route here, now and later, takes the buyer (the house is refused), and
/// the bid limit covers them all.
#[corten::routes(area = Me, caller = Buyer, state = Auction)]
pub mod me {
    #[route("/me")]
    pub struct Me;

    #[route("/me/bids/{lot}", post = bid)]
    pub struct Bid<'a> {
        pub lot: &'a str,
    }

    #[route("/me/notices", get = notices)]
    pub struct Notices;
}

/// The house's desk: only the house, signed in with the house key, reaches it.
#[corten::routes(area = House, caller = Auctioneer, state = Auction)]
pub mod house {
    #[route("/house", get = desk)]
    pub struct House;

    #[route("/house/lots/{id}/end", post = end_lot)]
    pub struct End<'a> {
        pub id: &'a str,
    }
}

/// Who is signed in: the session the `session` cookie names, looked up in the store.
#[derive(Clone)]
pub struct Bidder {
    pub name: String,
    /// The house's session: it sees everything but does not bid.
    pub house: bool,
    /// The session's token, which signing out ends.
    session: String,
}

/// The page's key mixes in who it is for, and the key is written into the page: it hashes what
/// the pages show, never the session's token, which stays in its `HttpOnly` cookie.
impl std::hash::Hash for Bidder {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (&self.name, self.house).hash(state);
    }
}

/// The session the cookie names, or none (sign in). A store that cannot answer is
/// `Refused::unavailable`, a 503: no one is sent to sign in again because the store is down.
async fn session(parts: &Parts, auction: &Auction) -> Result<Bidder, Refused> {
    let Some(token) = cookie::get(&parts.headers, "session") else {
        return Err(Refused::sign_in(&door::SignIn));
    };
    match auction.store.session(token).await {
        Ok(Some((name, house))) => Ok(Bidder { name, house, session: token.to_string() }),
        Ok(None) => Err(Refused::sign_in(&door::SignIn)),
        Err(error) => Err(Refused::unavailable(error)),
    }
}

impl Identify<Auction> for Bidder {
    async fn identify(parts: &Parts, auction: &Auction) -> Result<Self, Refused> {
        session(parts, auction).await
    }
}

/// The house, signed in. Someone signed in who is not the house is refused (403), not sent to
/// sign in again.
#[derive(Clone, Hash)]
pub struct Auctioneer {
    pub name: String,
}

impl Identify<Auction> for Auctioneer {
    async fn identify(parts: &Parts, auction: &Auction) -> Result<Self, Refused> {
        match session(parts, auction).await? {
            Bidder { name, house: true, .. } => Ok(Auctioneer { name }),
            Bidder { .. } => Err(Refused::forbidden()),
        }
    }
}

/// Someone signed in who may bid: anyone but the house, which runs the auction (it would be
/// bidding on its own lots). The house is refused (403), as anyone is from the house's desk.
#[derive(Clone, Hash)]
pub struct Buyer {
    pub name: String,
}

impl Identify<Auction> for Buyer {
    async fn identify(parts: &Parts, auction: &Auction) -> Result<Self, Refused> {
        match session(parts, auction).await? {
            Bidder { house: true, .. } => Err(Refused::forbidden()),
            Bidder { name, .. } => Ok(Buyer { name }),
        }
    }
}

/// How many bids one bidder may send a minute.
pub const BIDS_PER_MINUTE: u32 = 20;

/// Counts what each client does in a minute from its first: `count` says whether `who` may go
/// ahead, or how long until it may. At most `CLIENTS` are kept; past that the minutes that ended
/// are dropped, and while it is still full a new client waits (card-market's write limit does the
/// same).
pub struct Limit<K> {
    limit: u32,
    minutes: Mutex<HashMap<K, (Instant, u32)>>,
}

/// How many clients a limit keeps at once.
const CLIENTS: usize = 4096;

impl<K: Hash + Eq> Limit<K> {
    fn new(limit: u32) -> Self {
        Limit { limit, minutes: Mutex::new(HashMap::new()) }
    }

    /// Counts one for `who` unless it is at the limit: `None` when it may go ahead, else how long
    /// until it may.
    fn count(&self, who: K, now: Instant) -> Option<Duration> {
        let mut minutes = self.minutes.lock().unwrap_or_else(|e| e.into_inner());
        let Some(minute) = Self::minute(&mut minutes, who, now) else {
            return Some(Duration::from_secs(60));
        };
        if minute.1 >= self.limit {
            return Some(Self::left(minute.0, now));
        }
        minute.1 += 1;
        None
    }

    /// How long until `who` may go ahead, when it is at the limit.
    fn wait(&self, who: &K, now: Instant) -> Option<Duration> {
        let minutes = self.minutes.lock().unwrap_or_else(|e| e.into_inner());
        let &(start, count) = minutes.get(who)?;
        (count >= self.limit && now.duration_since(start) < Duration::from_secs(60)).then(|| Self::left(start, now))
    }

    /// Counts one for `who`, whatever the count (a refused attempt that already happened).
    fn add(&self, who: K, now: Instant) {
        let mut minutes = self.minutes.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(minute) = Self::minute(&mut minutes, who, now) {
            minute.1 += 1;
        }
    }

    /// `who`'s minute, started again once a minute has passed; `None` when `CLIENTS` are kept
    /// and none of their minutes has ended.
    fn minute(minutes: &mut HashMap<K, (Instant, u32)>, who: K, now: Instant) -> Option<&mut (Instant, u32)> {
        if minutes.len() >= CLIENTS && !minutes.contains_key(&who) {
            minutes.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(60));
            if minutes.len() >= CLIENTS {
                return None;
            }
        }
        let minute = minutes.entry(who).or_insert((now, 0));
        if now.duration_since(minute.0) >= Duration::from_secs(60) {
            *minute = (now, 0);
        }
        Some(minute)
    }

    /// What is left of a minute that started at `start`, in whole seconds, at least one.
    fn left(start: Instant, now: Instant) -> Duration {
        let left = Duration::from_secs(60).saturating_sub(now.duration_since(start));
        Duration::from_secs(left.as_secs().max(1))
    }
}

/// Counts each buyer's bids in a minute from their first, by who is signed in, not by address
/// (bidders in one office share one). A bid past the limit is refused with `Refused::too_many`:
/// a 429 with `Retry-After`, the error page for a browser. Reads pass straight through. It is
/// the buyer area's, so no other route meets it, and it reads the same `Caller<Buyer>` the
/// handler does: the store is asked once per request. Anyone can sign in as any name here, so
/// anyone can use up another's count (a real app signs people in with a password or a provider).
pub struct BidLimit {
    auction: Auction,
    limit: Limit<String>,
}

impl Middleware for BidLimit {
    async fn call(&self, req: Request, next: impl Next) -> Response {
        if req.method() != Method::POST {
            return next.run(req).await;
        }
        let (mut parts, body) = req.into_parts();
        // Signed out, or the house: the refusal the handler would give, without counting.
        let buyer = match Caller::<Buyer>::from_request_parts(&mut parts, &self.auction).await {
            Ok(Caller(buyer)) => buyer,
            Err(refused) => return refused,
        };
        if let Some(wait) = self.limit.count(buyer.name, Instant::now()) {
            return Refused::too_many(wait).into_response();
        }
        next.run(Request::from_parts(parts, body)).await
    }
}

/// How many refused sign-ins an address may have a minute.
pub const SIGN_IN_FAILURES: u32 = 5;

/// Counts refused sign-ins (a wrong house key, or no name) by address, so the house key cannot be
/// guessed at speed: past `SIGN_IN_FAILURES` in a minute, a sign-in is refused with
/// `Refused::too_many` before the key is compared, so a locked-out client learns nothing about
/// it. A sign-in that succeeds is not counted; a request whose address cannot be known (a test's)
/// is not counted. Behind a proxy, `PROXIES` (`Policy::proxy`) makes the address the client's.
pub struct SignInLimit(Limit<IpAddr>);

impl Middleware for SignInLimit {
    async fn call(&self, mut req: Request, next: impl Next) -> Response {
        let ip = (req.method() == Method::POST).then(|| ClientIp::of(&mut req)).flatten().map(|ClientIp(ip)| ip);
        let Some(ip) = ip else {
            return next.run(req).await;
        };
        if let Some(wait) = self.0.wait(&ip, Instant::now()) {
            return Refused::too_many(wait).into_response();
        }
        let res = next.run(req).await;
        if res.status() == StatusCode::UNPROCESSABLE_ENTITY {
            self.0.add(ip, Instant::now());
        }
        res
    }
}

/// How often a page with an open lot is read again by a browser without JavaScript: often enough
/// to follow the bidding, seldom enough that a bid being typed is rarely lost to a reload.
const READ_AGAIN: Duration = Duration::from_secs(15);

async fn home(req: Req, State(auction): State<Auction>, viewer: Option<Caller<Bidder>>) -> HtmlResult {
    let you = viewer.map(|Caller(b)| auction.you(&b.name, b.house));
    let (t, now, version) = (req.t(), SystemTime::now(), *auction.room.board().borrow());
    let lots = auction.room.all();
    // Without JavaScript the list is read again while any lot is open.
    let again = lots.iter().any(|s| !s.ended(now)).then_some(ReadAgain { after: READ_AGAIN }.without_javascript());
    req.page(html! {
        <views::Layout req={&req} title="Lots" you={you.as_ref()} head={again}>
            <h1>"Lots"</h1>
            <corten-live src={url::BoardLive} since={version}><Board t={t} lots={&lots} now={now} /></corten-live>
        </views::Layout>
    })
}

/// The lobby's list each time any lot changes.
async fn board_live(req: Req, State(auction): State<Auction>, since: LiveSince) -> LiveResponse {
    let (t, room) = (req.t(), auction.room.clone());
    corten::live_watch(
        auction.room.board(),
        since,
        |version| *version,
        move |_| Board { t, lots: &room.all(), now: SystemTime::now() }.to_html(),
    )
}

async fn lot(
    req: Req,
    State(auction): State<Auction>,
    Params(at): Params<url::Lot<'_>>,
    viewer: Option<Caller<Bidder>>,
) -> HtmlResult {
    let you = viewer.map(|Caller(b)| auction.you(&b.name, b.house));
    lot_page(&req, &auction, &at.id, you.as_ref(), "", &Errors::new(), true)
}

/// A lot's page, with the bid form showing `typed` and `errors` (a refused bid without
/// JavaScript). `again`: while the lot is open, a browser without JavaScript reads it again;
/// never on a refused bid's page, which is the answer to a POST (reading it again would be a GET
/// of the bid's URL).
fn lot_page(req: &Req, auction: &Auction, id: &str, you: Option<&You>, typed: &str, errors: &Errors, again: bool) -> HtmlResult {
    let standing = &auction.room.lot(id).ok_or_else(Error::not_found)?;
    let (t, now) = (req.t(), SystemTime::now());
    let again = (again && !standing.ended(now)).then_some(ReadAgain { after: READ_AGAIN }.without_javascript());
    req.page(html! {
        <views::Layout req={req} title={&standing.lot.card} you={you} head={again}>
            <views::LotPage t={t} standing={standing} now={now} you={you} typed={typed} errors={errors} />
        </views::Layout>
    })
}

/// The lot as it stands, each time it changes, for who is looking (it says when they lead).
async fn lot_live(
    req: Req,
    State(auction): State<Auction>,
    Params(at): Params<url::LotLive<'_>>,
    viewer: Option<Caller<Bidder>>,
    since: LiveSince,
) -> corten::Result<LiveResponse> {
    let lot = auction.room.follow(&at.id).ok_or_else(Error::not_found)?;
    let (t, name) = (req.t(), viewer.map(|Caller(b)| b.name));
    Ok(corten::live_watch(
        lot,
        since,
        |standing| standing.version,
        move |standing| LotStanding { t, standing, now: SystemTime::now(), you: name.as_deref() }.to_html(),
    ))
}

/// Every bid on a lot, oldest first, as a CSV file to download.
async fn history(State(auction): State<Auction>, Params(at): Params<url::History<'_>>) -> corten::Result<Media> {
    auction.room.lot(&at.id).ok_or_else(Error::not_found)?;
    let mut csv = String::from("bidder,amount_usd,unix_ms\n");
    for bid in auction.store.history(&at.id).await? {
        let ms = bid.at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let _ = writeln!(csv, "{},{}.{:02},{ms}", csv_field(&bid.bidder), bid.cents / 100, bid.cents % 100);
    }
    Ok(Media::new("text/csv; charset=utf-8", csv).attachment(&format!("{}-bids.csv", at.id)))
}

/// A name as one CSV field: with a `'` in front when a spreadsheet would read it as a formula
/// (`=`, `+`, `-`, `@`), and quoted when it holds a comma, a quote or a line break.
fn csv_field(text: &str) -> String {
    let text = if text.starts_with(['=', '+', '-', '@']) { format!("'{text}") } else { text.to_string() };
    if text.contains([',', '"', '\n', '\r']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text }
}

/// A bid as the form posts it.
#[derive(Deserialize, Fields)]
pub struct BidInput {
    /// As the reader's locale writes money: "12.50", "$1,250".
    pub amount: String,
    /// This submit, so one sent twice (a double click, a retry) is one bid.
    pub submit: SubmitKey,
}

/// Places a bid. In one transaction, the store refuses it below the price and the step, after
/// the lot closed, or when this submit was already written; then everyone watching sees the
/// new price, and the bidder who led is told alone.
async fn bid(
    req: Req,
    State(auction): State<Auction>,
    Caller(buyer): Caller<Buyer>,
    Params(at): Params<me::Bid<'_>>,
    Form(input): Form<BidInput>,
) -> HtmlResult {
    let (t, f, id): (Msg, _, _) = (req.t(), BidInput::FIELDS, at.lot.as_str());
    let mut errors = Errors::new();
    match t.parse_money(&input.amount, USD).filter(|cents| cents.0 > 0) {
        None => errors.add(f.amount, "Enter an amount, like 12.50."),
        Some(Minor(cents)) => match auction.store.bid(id, &buyer.name, cents, input.submit.as_str()).await? {
            Ok(placed) => {
                let standing = auction.refresh(id).await?.ok_or_else(Error::not_found)?;
                if let Placed::New { outbid: Some(outbid) } = placed {
                    auction.room.tell(&outbid, &standing.lot, News::Outbid { cents });
                }
                let none = Errors::new();
                let answer = html! {
                    <BidForm lot={id} typed="" errors={&none} />
                    <LotStanding t={t} standing={&standing} now={SystemTime::now()} you={Some(buyer.name.as_str())} />
                };
                return req.patch_or(answer, || async { req.redirect(&url::Lot { id }) }).await;
            }
            Err(Refusal::NoLot) => return Err(Error::not_found()),
            Err(Refusal::Closed) => errors.add(f.amount, "Bidding on this lot has closed."),
            Err(Refusal::TooHigh) => errors.add(f.amount, format!("Bid at most {}.", t.money(Minor(MAX_CENTS), USD))),
            Err(Refusal::TooLow { minimum }) => errors.add(f.amount, format!("Bid {} or more.", t.money(Minor(minimum), USD))),
        },
    }
    let form = BidForm { lot: id, typed: &input.amount, errors: &errors };
    let you = auction.you(&buyer.name, false);
    req.invalid(&errors, form, || async { lot_page(&req, &auction, id, Some(&you), &input.amount, &errors, false) }).await
}

/// The signed-in bidder's news (outbid, won), on every page they have open.
async fn notices(req: Req, State(auction): State<Auction>, Caller(buyer): Caller<Buyer>, since: LiveSince) -> LiveResponse {
    let t = req.t();
    corten::live_watch(
        auction.room.notices(&buyer.name),
        since,
        |notice| notice.seq,
        move |notice| NoticeLine { t, notice }.to_html(),
    )
}

/// The sign-in form as posted.
#[derive(Deserialize, Fields)]
pub struct SignInInput {
    pub name: String,
    /// Empty for a bidder.
    #[serde(default)]
    pub house_key: String,
}

async fn sign_in_page(req: Req, State(auction): State<Auction>, viewer: Option<Caller<Bidder>>) -> HtmlResult {
    let you = viewer.map(|Caller(b)| auction.you(&b.name, b.house));
    let none = Errors::new();
    req.page(html! {
        <views::Layout req={&req} title="Sign in" you={you.as_ref()} head="">
            <SignInForm typed="" errors={&none} />
        </views::Layout>
    })
}

/// Signs in under a name, in a new session kept in the store; one this browser had ends. With
/// the house key it is the house's session. There is no password: anyone may bid as any name,
/// which a real app would not allow.
async fn sign_in(
    req: Req,
    State(auction): State<Auction>,
    before: Option<Caller<Bidder>>,
    Form(input): Form<SignInInput>,
) -> HtmlResult {
    let f = SignInInput::FIELDS;
    let name = input.name.trim();
    let house = !input.house_key.is_empty();
    let mut errors = Errors::new();
    if name.is_empty() || name.chars().count() > 40 {
        errors.add(f.name, "Enter a name of up to 40 characters.");
    }
    if house && !auction.house_key.as_deref().is_some_and(|key| same(key, &input.house_key)) {
        errors.add(f.house_key, "That is not the house key.");
    }
    if !errors.is_empty() {
        let (typed, errors) = (input.name.as_str(), &errors);
        let form = SignInForm { typed, errors };
        let page = || async {
            req.page(html! {
                <views::Layout req={&req} title="Sign in" you={None} head="">
                    <SignInForm typed={typed} errors={errors} />
                </views::Layout>
            })
        };
        return req.invalid(errors, form, page).await;
    }
    if let Some(Caller(before)) = before {
        auction.store.end_session(&before.session).await?;
    }
    let token = Nonce::generate().0.to_string();
    auction.store.start_session(&token, name, house).await?;
    let res = if house { req.redirect(&house::House)? } else { req.redirect(&url::Home)? };
    Ok(res.with_cookie(SetCookie::new("session", &token)))
}

/// Whether two keys are the same, in a time that does not say how much of them matched.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// Ends the session in the store and takes the cookie away.
async fn sign_out(req: Req, State(auction): State<Auction>, viewer: Option<Caller<Bidder>>) -> HtmlResult {
    if let Some(Caller(viewer)) = viewer {
        auction.store.end_session(&viewer.session).await?;
    }
    Ok(req.redirect(&url::Home)?.with_cookie(SetCookie::remove("session")))
}

/// The house's desk: every lot, with a button that ends an open one now.
async fn desk(req: Req, State(auction): State<Auction>, Caller(house): Caller<Auctioneer>) -> HtmlResult {
    let you = auction.you(&house.name, true);
    let (t, now, lots) = (req.t(), SystemTime::now(), auction.room.all());
    req.page(html! {
        <views::Layout req={&req} title="House" you={Some(&you)} head="">
            <views::HouseDesk t={t} lots={&lots} now={now} />
        </views::Layout>
    })
}

/// Ends a lot now. The closing task tells its winner.
async fn end_lot(
    req: Req,
    State(auction): State<Auction>,
    _: Caller<Auctioneer>,
    Params(at): Params<house::End<'_>>,
) -> HtmlResult {
    auction.store.end_lot(&at.id).await?;
    auction.refresh(&at.id).await?.ok_or_else(Error::not_found)?;
    auction.room.ends_moved.notify_one();
    req.redirect(&house::House)
}

/// Closes each lot as its time runs out: its pages draw it closed, and its winner is told. Sleeps
/// until the next lot ends, or until the house moves an end, and stops when the app shuts down.
async fn close_lots(auction: Auction, signal: Signal) {
    loop {
        // A store that cannot answer now is asked again shortly.
        let next = close_due(&auction).await.unwrap_or_else(|_| Some(SystemTime::now() + Duration::from_secs(5)));
        let wait = next.and_then(|at| at.duration_since(SystemTime::now()).ok()).unwrap_or(Duration::from_secs(3600));
        tokio::select! {
            biased;
            _ = signal.aborted() => return,
            _ = auction.room.ends_moved.notified() => {}
            _ = tokio::time::sleep(wait) => {}
        }
    }
}

/// Closes the lots whose time ran out, in the store, and tells each winner. The store closes a lot
/// once, in the lock bids are written in, so the winner is the last bid it took, and a restart
/// does not tell anyone twice. Returns when the next lot ends.
async fn close_due(auction: &Auction) -> Result<Option<SystemTime>, store::Error> {
    let now = SystemTime::now();
    for standing in auction.room.all().iter().filter(|s| !s.closed && s.ended(now)) {
        if let Some(closed) = auction.store.close_lot(&standing.lot.id).await? {
            auction.refresh(&standing.lot.id).await?;
            if let Some(top) = closed.top {
                auction.room.tell(&top.bidder, &standing.lot, News::Won { cents: top.cents });
            }
        }
    }
    Ok(auction.room.all().iter().filter(|s| !s.ended(now)).map(|s| s.lot.ends_at).min())
}

#[cfg(test)]
mod tests {
    use corten::axum::{
        Router,
        body::Body,
        http::{HeaderMap, Request, header},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::room::Notice;

    /// Submit keys, as the bid form draws them.
    const KEYS: [&str; 3] =
        ["3f2504e0-4f89-41d3-9a0c-0305e82c3301", "9b2c7f0e-1d2a-4c3b-8e4f-5a6b7c8d9e0f", "5d1c2b3a-4e5f-4a6b-9c7d-8e9f0a1b2c3d"];

    /// The auction, and its app's router, built once as `serve` builds it: the bid limit counts
    /// across requests.
    async fn auction() -> (Auction, Router) {
        let auction = Auction::open("sqlite::memory:", Some("hammer".into())).await.unwrap();
        let router = app(auction.clone()).router();
        (auction, router)
    }

    async fn send(router: &Router, req: Request<Body>) -> (StatusCode, HeaderMap, String) {
        let res = router.clone().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        (parts.status, parts.headers, String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap())
    }

    /// A form post, as the runtime sends it (`action`) or as a browser without JavaScript does.
    fn post(uri: &str, body: &str, cookie: Option<&str>, action: bool) -> Request<Body> {
        let mut req = Request::post(uri).header("content-type", "application/x-www-form-urlencoded");
        if let Some(cookie) = cookie {
            req = req.header("cookie", cookie);
        }
        if action {
            req = req.header("corten", "action");
        }
        req.body(Body::from(body.to_string())).unwrap()
    }

    fn get(uri: &str, cookie: Option<&str>) -> Request<Body> {
        let mut req = Request::get(uri).header("accept", "text/html");
        if let Some(cookie) = cookie {
            req = req.header("cookie", cookie);
        }
        req.body(Body::empty()).unwrap()
    }

    /// Signs in and returns the session's cookie, `session=...`.
    async fn sign_in(router: &Router, name: &str, key: &str) -> String {
        let (status, headers, _) = send(router, post("/sign-in", &format!("name={name}&house_key={key}"), None, false)).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        headers[header::SET_COOKIE].to_str().unwrap().split(';').next().unwrap().to_string()
    }

    async fn bid(router: &Router, cookie: &str, amount: &str, key: &str) -> (StatusCode, String) {
        let body = format!("amount={amount}&submit={key}");
        let (status, _, body) = send(router, post("/me/bids/ashen-drake", &body, Some(cookie), true)).await;
        (status, body)
    }

    #[tokio::test]
    async fn a_bid_counts_once_per_submit_is_refused_below_the_price_and_tells_who_was_outbid() {
        let (auction, router) = auction().await;
        let (ada, grace) = (sign_in(&router, "Ada", "").await, sign_in(&router, "Grace", "").await);
        let (status, body) = bid(&router, &ada, "25.00", KEYS[0]).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.contains("You lead."), "{body}");
        // The same submit again (a double click, a retry) is no second bid.
        assert_eq!(bid(&router, &ada, "25.00", KEYS[0]).await.0, StatusCode::OK);
        assert_eq!(auction.room.lot("ashen-drake").unwrap().count, 1);
        let (status, body) = bid(&router, &grace, "25.50", KEYS[1]).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("Bid $26.00 or more."), "{body}");
        assert_eq!(auction.room.notice("Ada"), Notice::default());
        assert_eq!(bid(&router, &grace, "$26", KEYS[2]).await.0, StatusCode::OK);
        assert_eq!(auction.room.notice("Ada").news, News::Outbid { cents: 2600 });
        assert_eq!(auction.room.notice("Grace").news, News::None);
    }

    #[tokio::test]
    async fn without_javascript_a_bid_is_a_form_and_a_refused_one_is_the_page_with_its_error() {
        let (_, router) = auction().await;
        let ada = sign_in(&router, "Ada", "").await;
        let body = format!("amount=25&submit={}", KEYS[0]);
        let (status, headers, _) = send(&router, post("/me/bids/ashen-drake", &body, Some(&ada), false)).await;
        assert_eq!((status, headers[header::LOCATION].to_str().unwrap()), (StatusCode::SEE_OTHER, "/lots/ashen-drake"));
        let body = format!("amount=1&submit={}", KEYS[1]);
        let (status, _, page) = send(&router, post("/me/bids/ashen-drake", &body, Some(&ada), false)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(page.contains("<!DOCTYPE html>") && page.contains("Bid $26.00 or more."), "{page}");
        // The answer to a POST: reading it again would GET the bid's URL.
        assert!(!page.contains(r#"http-equiv="refresh""#), "{page}");
        let body = format!("amount=99999999999999&submit={}", KEYS[2]);
        let (status, _, page) = send(&router, post("/me/bids/ashen-drake", &body, Some(&ada), false)).await;
        assert!(status == StatusCode::UNPROCESSABLE_ENTITY && page.contains("Bid at most $1,000,000.00."), "{page}");
        // An open lot's page reads itself again without JavaScript.
        let (_, _, page) = send(&router, get("/lots/ashen-drake", Some(&ada))).await;
        assert!(page.contains(r#"http-equiv="refresh""#), "{page}");
    }

    #[tokio::test]
    async fn bidding_signed_out_goes_to_sign_in() {
        let (_, router) = auction().await;
        let body = format!("amount=25&submit={}", KEYS[0]);
        let (status, headers, _) = send(&router, post("/me/bids/ashen-drake", &body, None, false)).await;
        assert_eq!((status, headers[header::LOCATION].to_str().unwrap()), (StatusCode::SEE_OTHER, "/sign-in"));
    }

    #[tokio::test]
    async fn only_the_house_reaches_its_desk_and_the_house_does_not_bid() {
        let (_, router) = auction().await;
        let ada = sign_in(&router, "Ada", "").await;
        assert_eq!(send(&router, get("/house", Some(&ada))).await.0, StatusCode::FORBIDDEN);
        assert_eq!(send(&router, get("/house", None)).await.0, StatusCode::SEE_OTHER);
        let (status, _, body) = send(&router, post("/sign-in", "name=Mallory&house_key=guess", None, true)).await;
        assert!(status == StatusCode::UNPROCESSABLE_ENTITY && body.contains("That is not the house key."), "{body}");
        let house = sign_in(&router, "House", "hammer").await;
        assert_eq!(send(&router, get("/house", Some(&house))).await.0, StatusCode::OK);
        assert_eq!(bid(&router, &house, "25", KEYS[0]).await.0, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn a_bidder_past_the_limit_is_refused_until_the_minute_turns() {
        let (_, router) = auction().await;
        let (ada, grace) = (sign_in(&router, "Ada", "").await, sign_in(&router, "Grace", "").await);
        for _ in 0..BIDS_PER_MINUTE {
            assert_eq!(bid(&router, &ada, "1", KEYS[0]).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        }
        let (status, headers, _) = send(&router, post("/me/bids/ashen-drake", "amount=1", Some(&ada), true)).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert!(headers.contains_key(header::RETRY_AFTER));
        // Counted by bidder: another is not held back.
        assert_eq!(bid(&router, &grace, "1", KEYS[1]).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        // Reads are not counted.
        assert_eq!(send(&router, get("/lots/ashen-drake", Some(&ada))).await.0, StatusCode::OK);
    }

    #[tokio::test]
    async fn an_address_with_many_refused_sign_ins_is_refused_before_the_key_is_compared() {
        use corten::axum::extract::connect_info::MockConnectInfo;
        let (_, router) = auction().await;
        let router = router.layer(MockConnectInfo(std::net::SocketAddr::from(([203, 0, 113, 7], 4000))));
        for _ in 0..SIGN_IN_FAILURES {
            let (status, ..) = send(&router, post("/sign-in", "name=Mallory&house_key=guess", None, false)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        }
        let (status, headers, _) = send(&router, post("/sign-in", "name=Mallory&house_key=hammer", None, false)).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert!(headers.contains_key(header::RETRY_AFTER));
    }

    #[tokio::test]
    async fn a_store_that_cannot_answer_is_a_503_not_a_sign_in() {
        let (auction, router) = auction().await;
        let ada = sign_in(&router, "Ada", "").await;
        auction.store.close().await;
        assert_eq!(send(&router, get("/lots/ashen-drake", Some(&ada))).await.0, StatusCode::SERVICE_UNAVAILABLE);
        let body = format!("amount=25&submit={}", KEYS[0]);
        let (status, headers, _) = send(&router, post("/me/bids/ashen-drake", &body, Some(&ada), false)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(!headers.contains_key(header::LOCATION));
    }

    #[tokio::test]
    async fn the_house_ends_a_lot_and_its_top_bidder_is_told() {
        let (auction, router) = auction().await;
        let (ada, house) = (sign_in(&router, "Ada", "").await, sign_in(&router, "House", "hammer").await);
        assert_eq!(bid(&router, &ada, "25", KEYS[0]).await.0, StatusCode::OK);
        let (status, ..) = send(&router, post("/house/lots/ashen-drake/end", "", Some(&house), false)).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        close_due(&auction).await.unwrap();
        assert_eq!(auction.room.notice("Ada").news, News::Won { cents: 2500 });
        // Closed once: closing again (a restart's task) tells no one twice.
        auction.room.tell("Ada", &auction.room.lot("ashen-drake").unwrap().lot, News::None);
        close_due(&auction).await.unwrap();
        assert_eq!(auction.room.notice("Ada").news, News::None);
        let (_, _, page) = send(&router, get("/lots/ashen-drake", Some(&ada))).await;
        assert!(page.contains("You won this lot.") && page.contains("Bidding on this lot has closed."), "{page}");
        assert!(!page.contains(r#"http-equiv="refresh""#), "a closed lot does not read itself again");
        let (status, body) = bid(&router, &ada, "30", KEYS[1]).await;
        assert!(status == StatusCode::UNPROCESSABLE_ENTITY && body.contains("Bidding on this lot has closed."), "{body}");
    }

    #[tokio::test]
    async fn a_lots_bids_download_as_csv_a_spreadsheet_cannot_run() {
        let (_, router) = auction().await;
        let ada = sign_in(&router, "%3DHYPERLINK(1)", "").await;
        assert_eq!(bid(&router, &ada, "25", KEYS[0]).await.0, StatusCode::OK);
        let (status, headers, body) = send(&router, get("/lots/ashen-drake/bids", None)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_DISPOSITION], r#"attachment; filename="ashen-drake-bids.csv""#);
        assert!(body.starts_with("bidder,amount_usd,unix_ms\n'=HYPERLINK(1),25.00,"), "{body}");
        assert_eq!(csv_field(r#"Ada "the" Bidder, Jr"#), r#""Ada ""the"" Bidder, Jr""#);
    }
}
