//! A deck builder: one screen where a player builds a deck from their collection, with most of
//! what rich client screens do (selection, drag and drop, keyboard moves, undo, a chart, autosave)
//! done in the browser, and the deck rules on the server alone.
//!
//! | Route | What |
//! |---|---|
//! | `GET /` | the collection and the deck; `?q=` filters and `?sort=` sorts the collection for a browser without JavaScript |
//! | `POST /deck` | the deck as its form holds it, with the version it was built on: saved once per submit key, refused by the rules (422) or when another tab saved first, or the server restarted (409) |
//! | `GET /feed/deck`, `/feed/deck/live` | the saved deck, which the page follows to notice another tab's save |
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use corten::{
    Mode,
    axum::http::StatusCode,
    feed::{Feed, FeedResponse, Row, Since},
    prelude::*,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    cards::{self, Card, Refusal},
    text::Msg,
    views::{self, Answer, SaveNote},
};

/// A card in the saved deck and its copies: the rows of the deck's feed.
#[derive(Clone, Serialize, JsonSchema)]
pub struct Slot {
    pub id: String,
    pub count: u32,
}

impl Row for Slot {
    fn id(&self) -> &str {
        &self.id
    }
}

/// The collection, and the deck saved from it. The deck's version is its feed's generation and
/// sequence number: each save that changes it is one write, so a page that knows both knows the
/// deck. The deck is kept in memory, so a restart starts a new generation at 0: a page left open
/// across it holds a version of another generation, which is stale whatever its number.
#[derive(Clone)]
pub struct Builder {
    cards: Arc<[Card]>,
    index: Arc<HashMap<String, usize>>,
    pub deck: Feed<Slot>,
    /// The saves applied in the last day, so one sent again (its answer lost on the way) answers
    /// as it did. Every save holds this lock, so the version it checks is the one it writes after.
    /// A real store keeps the keys next to the deck, in its transaction.
    saves: Arc<Mutex<Saves>>,
}

/// The saves applied lately by submit key, and their keys oldest first, to forget them by age.
#[derive(Default)]
struct Saves {
    by_key: HashMap<SubmitKey, Applied>,
    order: VecDeque<(Instant, SubmitKey)>,
}

/// A save that was applied: which of the page's sends it was, and the version it made.
struct Applied {
    draft: u64,
    version: u64,
}

/// How long an applied save's key is remembered: a page sends a lost save again within moments,
/// and a key forgotten sooner could not say the save was made. A day is what payment APIs keep
/// an idempotency key for. A page that sends one later finds its version old: the player is
/// offered both decks.
const KEPT_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// What became of a save, with the deck's version after it.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Saved: the version it made, and which of the page's sends under this key it was.
    Saved { version: u64, draft: u64 },
    /// The rules refused it; the deck is as it was.
    Refused(u64, Refusal),
    /// It was built on an older version: another tab saved first, or the server restarted since.
    Stale(u64),
}

/// A deck as `card id -> copies`, no zeros.
pub type Deck = BTreeMap<String, u32>;

impl Builder {
    /// The collection (`DECK_CARDS` cards, 240 unless set) and the starter deck.
    pub fn new() -> Self {
        Self::with(std::env::var("DECK_CARDS").ok().and_then(|n| n.parse().ok()).unwrap_or(240))
    }

    /// `size` cards, and the starter deck at version 0 of a new generation.
    pub fn with(size: usize) -> Self {
        Self::from(size, None)
    }

    /// `size` cards and the starter deck, in the generation named, or a new one.
    fn from(size: usize, generation: Option<&str>) -> Self {
        let cards: Arc<[Card]> = cards::collection(size).into();
        let index = cards.iter().enumerate().map(|(i, c)| (c.id.clone(), i)).collect();
        let starter = cards::STARTER.map(|(id, count)| Slot { id: id.into(), count });
        let deck = match generation {
            Some(generation) => Feed::follow(generation, 0, starter),
            None => Feed::new(starter),
        };
        Builder { cards, index: Arc::new(index), deck, saves: Arc::default() }
    }

    pub fn card(&self, id: &str) -> Option<&Card> {
        self.index.get(id).map(|&i| &self.cards[i])
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Saves> {
        self.saves.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The deck's generation: a version is only comparable with one of the same generation.
    pub fn generation(&self) -> &str {
        self.deck.generation()
    }

    /// The saved deck and its version, read together.
    pub fn saved(&self) -> (u64, Deck) {
        let _saves = self.lock();
        (self.deck.seq(), self.deck.read(|rows| rows.values().map(|s| (s.id.clone(), s.count)).collect()))
    }

    /// Saves `deck` (the page's send `draft`) if the saved deck is still at `version` of
    /// `generation` and the rules allow it. A submit key already applied answers as it did then,
    /// whatever the deck: the page sent it again because the answer was lost, and the draft says
    /// which deck was saved.
    pub fn save(&self, generation: &str, version: u64, deck: Deck, key: &SubmitKey, draft: u64) -> Outcome {
        let mut saves = self.lock();
        let at = Instant::now();
        while saves.order.front().is_some_and(|(applied, _)| at.duration_since(*applied) > KEPT_FOR) {
            if let Some((_, old)) = saves.order.pop_front() {
                saves.by_key.remove(&old);
            }
        }
        if let Some(applied) = saves.by_key.get(key) {
            return Outcome::Saved { version: applied.version, draft: applied.draft };
        }
        let now = self.deck.seq();
        if generation != self.generation() || version != now {
            return Outcome::Stale(now);
        }
        if let Err(refusal) = cards::check(&deck, |id| self.card(id)) {
            return Outcome::Refused(now, refusal);
        }
        // Read before the write: every save holds the lock, so nothing writes in between.
        let gone: Vec<String> = self.deck.read(|rows| rows.keys().filter(|id| !deck.contains_key(*id)).cloned().collect());
        self.deck.write(|tx| {
            for id in gone {
                tx.remove(&id);
            }
            for (id, &count) in &deck {
                if tx.get(id).map(|s| s.count) != Some(count) {
                    tx.insert(Slot { id: id.clone(), count });
                }
            }
        });
        let version = self.deck.seq();
        saves.by_key.insert(key.clone(), Applied { draft, version });
        saves.order.push_back((at, key.clone()));
        Outcome::Saved { version, draft }
    }

    /// The deck a form posted: each card with its count, then the card an Add button names, or
    /// the picked cards for Add selected (a browser without JavaScript). A cleared count is none.
    fn posted(&self, input: &DeckInput) -> Result<Deck, Refusal> {
        if input.card.len() != input.count.len() {
            return Err(Refusal::NotANumber);
        }
        let mut deck = Deck::new();
        for (id, count) in input.card.iter().zip(&input.count) {
            let count = count.trim();
            let copies: u32 = if count.is_empty() { 0 } else { count.parse().map_err(|_| Refusal::NotANumber)? };
            let n = deck.entry(id.clone()).or_default();
            *n = n.saturating_add(copies);
        }
        let added = input.add.iter().chain(input.add_picked.as_ref().map_or(&[][..], |_| &input.pick[..]));
        for id in added {
            let n = deck.entry(id.clone()).or_default();
            *n = n.saturating_add(1);
        }
        deck.retain(|_, n| *n > 0);
        Ok(deck)
    }
}

/// The app: one screen, the save and the deck's feed.
pub fn app(builder: Builder) -> App {
    App::new(url::router(builder))
        .assets(views::Static)
        .components(views::COMPONENTS)
        .components(&[corten_ui::CopyButton])
        .messages::<Msg>()
}

#[corten::routes(state = Builder)]
pub mod url {
    /// The builder. The query filters and sorts the collection for a browser without JavaScript.
    #[route("/", get = home)]
    #[derive(Default)]
    pub struct Home {
        #[route(query)]
        pub view: super::DeckQuery,
    }

    #[route("/deck", post = save)]
    pub struct Save;

    #[route("/feed/deck", get = deck_feed, rows = Slot)]
    pub struct DeckFeed;

    #[route("/feed/deck/live", get = deck_live, rows = Slot)]
    pub struct DeckLive;
}

/// How the collection is shown without JavaScript: a filter and an order. With it, the page
/// filters and sorts as the player types and clicks, with no request.
#[derive(Clone, Default, Deserialize, Route, Fields)]
#[route(query)]
pub struct DeckQuery {
    pub q: Option<String>,
    pub sort: Option<Sort>,
}

/// The collection's order.
#[derive(Clone, Copy, PartialEq, Debug, UrlValue, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Sort {
    Name,
    Cost,
    Owned,
}

/// The deck as its form posts it: each line's card and count (`card[i]` has `count[i]`), the
/// version it was built on, and, without JavaScript, the card an Add button names or the picked
/// cards with Add selected.
#[derive(Deserialize, Fields)]
pub struct DeckInput {
    /// The generation of the version the deck was built on.
    pub generation: String,
    pub version: u64,
    #[serde(default)]
    pub card: Vec<String>,
    /// As typed: a cleared count is no copies.
    #[serde(default)]
    pub count: Vec<String>,
    /// A collection row's Add button.
    pub add: Option<String>,
    /// The collection's checked rows, added by Add selected.
    #[serde(default)]
    pub pick: Vec<String>,
    /// The Add selected button.
    pub add_picked: Option<String>,
    /// This submit, so one sent again is saved once.
    pub submit: SubmitKey,
    /// Which of the page's sends this is: the script counts them, so an answer to one sent again
    /// says which deck was saved. 0 without JavaScript.
    #[serde(default)]
    pub draft: u64,
    /// The page's filter and order, to come back to after a save without JavaScript.
    pub q: Option<String>,
    pub sort: Option<Sort>,
}

async fn deck_feed(State(builder): State<Builder>) -> FeedResponse<Slot> {
    builder.deck.snapshot()
}

async fn deck_live(State(builder): State<Builder>, since: Since) -> FeedResponse<Slot> {
    builder.deck.live(since)
}

async fn home(req: Req, State(builder): State<Builder>, Params(at): Params<url::Home>) -> HtmlResult {
    deck_page(&req, &builder, &at.view, &Answer::None, StatusCode::OK)
}

/// The builder's page, with what the last save said (a refused one without JavaScript).
fn deck_page(req: &Req, builder: &Builder, view: &DeckQuery, answer: &Answer, status: StatusCode) -> HtmlResult {
    let (version, saved) = builder.saved();
    let screen = views::screen(req.t(), builder, &saved, version, view, answer);
    req.page_with_status(status, html! { <views::Layout req={req} title="Deck builder">{screen}</views::Layout> })
}

/// Saves the deck. With JavaScript the answer is the save note, which tells the page's script what
/// became of the save; without, a saved deck goes back to the page and a refused one is the page
/// with the reason.
async fn save(req: Req, State(builder): State<Builder>, Form(input): Form<DeckInput>) -> HtmlResult {
    let outcome = match builder.posted(&input) {
        Ok(deck) => builder.save(&input.generation, input.version, deck, &input.submit, input.draft),
        Err(refusal) => Outcome::Refused(builder.saved().0, refusal),
    };
    let (answer, status) = match &outcome {
        Outcome::Saved { version, draft } => (Answer::Saved { version: *version, draft: *draft }, StatusCode::OK),
        Outcome::Refused(version, refusal) => {
            (Answer::Refused { version: *version, message: refusal.to_string() }, StatusCode::UNPROCESSABLE_ENTITY)
        }
        Outcome::Stale(version) => (Answer::Stale { version: *version }, StatusCode::CONFLICT),
    };
    if req.mode == Mode::Action {
        return req.patch_with_status(status, SaveNote { answer: &answer });
    }
    let view = DeckQuery { q: input.q, sort: input.sort };
    match outcome {
        Outcome::Saved { .. } => req.redirect(&url::Home { view }),
        _ => deck_page(&req, &builder, &view, &answer, status),
    }
}

#[cfg(test)]
mod tests {
    use corten::axum::{
        Router,
        body::Body,
        http::{Request, header},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;

    /// The tests' deck generation.
    const GEN: &str = "g";

    const KEYS: [&str; 5] = [
        "3f2504e0-4f89-41d3-9a0c-0305e82c3301",
        "9b2c7f0e-1d2a-4c3b-8e4f-5a6b7c8d9e0f",
        "5d1c2b3a-4e5f-4a6b-9c7d-8e9f0a1b2c3d",
        "7a6b5c4d-3e2f-4a1b-8c9d-0e1f2a3b4c5d",
        "1b2c3d4e-5f60-4718-9a2b-3c4d5e6f7a8b",
    ];

    async fn send(router: &Router, req: Request<Body>) -> (StatusCode, Option<String>, String) {
        let res = router.clone().oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        let location = parts.headers.get(header::LOCATION).map(|l| l.to_str().unwrap().to_string());
        (parts.status, location, String::from_utf8(body.collect().await.unwrap().to_bytes().to_vec()).unwrap())
    }

    /// A save as the page's script sends it (`action`) or a browser without JavaScript does.
    fn post(body: &str, action: bool) -> Request<Body> {
        let mut req = Request::post("/deck").header("content-type", "application/x-www-form-urlencoded");
        if action {
            req = req.header("corten", "action");
        }
        req.body(Body::from(body.to_string())).unwrap()
    }

    /// The form's body for `deck` at `version`.
    fn body(generation: &str, version: u64, deck: &[(&str, u32)], key: &str) -> String {
        let mut body = format!("generation={generation}&version={version}&submit={key}");
        for (id, n) in deck {
            body.push_str(&format!("&card={id}&count={n}"));
        }
        body
    }

    fn starter() -> Vec<(&'static str, u32)> {
        cards::STARTER.to_vec()
    }

    #[tokio::test]
    async fn a_save_writes_the_deck_once_per_key_and_answers_with_its_version() {
        let builder = Builder::from(60, Some(GEN));
        let router = app(builder.clone()).router();
        let mut deck = starter();
        deck.push(("salt-oracle", 1));
        let (status, _, note) = send(&router, post(&format!("{}&draft=1", body(GEN, 0, &deck, KEYS[0])), true)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(note.contains(r#"<span id="save-note""#) && note.contains("Saved."), "{note}");
        assert!(note.contains(r#"save_note.answered($t,&quot;saved&quot;,1,1)"#), "{note}");
        assert_eq!(builder.saved().1.get("salt-oracle"), Some(&1));
        // Sent again (its answer lost): the same answer, no second write.
        let (status, _, note) = send(&router, post(&format!("{}&draft=1", body(GEN, 0, &deck, KEYS[0])), true)).await;
        assert!(status == StatusCode::OK && note.contains("&quot;saved&quot;,1,1)"), "{note}");
        assert_eq!(builder.deck.seq(), 1);
        // The same key with a changed deck (made while the answer was lost): nothing is written,
        // and the answer names the send that was saved, so the page sends the change next.
        deck.pop();
        let (status, _, note) = send(&router, post(&format!("{}&draft=2", body(GEN, 0, &deck, KEYS[0])), true)).await;
        assert!(status == StatusCode::OK && note.contains("&quot;saved&quot;,1,1)"), "{note}");
        assert_eq!((builder.deck.seq(), builder.saved().1.get("salt-oracle")), (1, Some(&1)));
        // A new key, built on the old version, is stale.
        let (status, _, note) = send(&router, post(&body(GEN, 0, &deck, KEYS[1]), true)).await;
        assert!(status == StatusCode::CONFLICT && note.contains("&quot;stale&quot;,1,0)"), "{note}");
    }

    #[test]
    fn a_version_from_before_a_restart_is_stale_after_it() {
        let key = |i: usize| {
            use serde::de::{IntoDeserializer, value::Error};
            SubmitKey::deserialize(IntoDeserializer::<Error>::into_deserializer(KEYS[i])).unwrap()
        };
        let deck = |extra: &str| {
            let mut deck: Deck = cards::STARTER.iter().map(|&(id, n)| (id.to_string(), n)).collect();
            deck.insert(extra.into(), 1);
            deck
        };
        // A page saved twice before the restart: version 2.
        let before = Builder::from(60, Some("a"));
        before.save("a", 0, deck("salt-oracle"), &key(0), 1);
        assert_eq!(before.save("a", 1, deck("pearl-diver"), &key(1), 2), Outcome::Saved { version: 2, draft: 2 });
        // After it the deck starts again in another generation, and writes as far as version 2:
        // the same number is not the same deck.
        let after = Builder::from(60, Some("b"));
        after.save("b", 0, deck("lantern-warden"), &key(2), 1);
        after.save("b", 1, deck("undertow-serpent"), &key(3), 2);
        assert_eq!(after.save("a", 2, deck("kiln-colossus"), &key(4), 3), Outcome::Stale(2));
        assert_eq!(after.save("b", 2, deck("kiln-colossus"), &key(4), 3), Outcome::Saved { version: 3, draft: 3 });
    }

    #[tokio::test]
    async fn a_deck_the_rules_refuse_is_not_saved_and_says_why() {
        let builder = Builder::from(60, Some(GEN));
        let router = app(builder.clone()).router();
        let mut deck = starter();
        deck[4] = ("ashen-drake", 3);
        let (status, _, note) = send(&router, post(&body(GEN, 0, &deck, KEYS[0]), true)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(note.contains("Not saved: You own 2 copies of Ashen Drake."), "{note}");
        assert!(note.contains("&quot;refused&quot;,0,0)"), "{note}");
        assert_eq!(builder.deck.seq(), 0);
        let (status, _, note) =
            send(&router, post("generation=g&version=0&submit=3f2504e0-4f89-41d3-9a0c-0305e82c3301&card=gravel-sprite&count=two", true)).await;
        assert!(status == StatusCode::UNPROCESSABLE_ENTITY && note.contains("Enter a number of copies"), "{note}");
    }

    #[tokio::test]
    async fn without_javascript_add_and_save_are_forms_and_a_refusal_is_the_page() {
        let builder = Builder::from(60, Some(GEN));
        let router = app(builder.clone()).router();
        // An Add button: the deck as it is, and one more card.
        let add = format!("{}&add=pearl-diver&q=pearl&sort=cost", body(GEN, 0, &starter(), KEYS[0]));
        let (status, location, _) = send(&router, post(&add, false)).await;
        assert_eq!((status, location.as_deref()), (StatusCode::SEE_OTHER, Some("/?q=pearl&sort=cost")));
        assert_eq!(builder.saved().1.get("pearl-diver"), Some(&1));
        // Add selected: every picked card.
        let mut deck = starter();
        deck.push(("pearl-diver", 1));
        let picked = format!("{}&pick=undertow-serpent&pick=moth-court-herald&add_picked=1", body(GEN, 1, &deck, KEYS[1]));
        assert_eq!(send(&router, post(&picked, false)).await.0, StatusCode::SEE_OTHER);
        let saved = builder.saved().1;
        assert_eq!((saved.get("undertow-serpent"), saved.get("moth-court-herald")), (Some(&1), Some(&3)));
        // A count cleared is none: the card leaves the deck.
        let mut deck: Vec<_> = saved.iter().map(|(id, n)| (id.as_str(), *n)).collect();
        deck.retain(|(id, _)| *id != "pearl-diver");
        let cleared = format!("{}&card=pearl-diver&count=", body(GEN, 2, &deck, KEYS[2]));
        assert_eq!(send(&router, post(&cleared, false)).await.0, StatusCode::SEE_OTHER);
        assert_eq!(builder.saved().1.get("pearl-diver"), None);
        // Refused: the page, with the reason and the deck as saved.
        let refused = format!("{}&card=kiln-colossus&count=1&add=kiln-colossus", body(GEN, 3, &deck, KEYS[3]));
        let (status, _, page) = send(&router, post(&refused, false)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(page.contains("<!DOCTYPE html>") && page.contains("You own 1 copy of Kiln Colossus."), "{page}");
    }

    #[tokio::test]
    async fn a_save_built_on_an_old_version_is_refused_as_stale_and_the_page_shows_the_deck_now() {
        let builder = Builder::from(60, Some(GEN));
        let router = app(builder.clone()).router();
        let mut deck = starter();
        deck.push(("pearl-diver", 2));
        assert_eq!(send(&router, post(&body(GEN, 0, &deck, KEYS[0]), true)).await.0, StatusCode::OK);
        // Another tab, still at version 0.
        let (status, _, page) = send(&router, post(&body(GEN, 0, &starter(), KEYS[1]), false)).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(page.contains("changed in another tab") && page.contains(r#"name="version" value="1""#), "{page}");
        assert!(page.contains("Pearl Diver"), "the deck as it is now: {page}");
    }

    #[tokio::test]
    async fn the_page_lists_the_collection_and_the_deck_and_filters_without_javascript() {
        let builder = Builder::from(60, Some(GEN));
        let router = app(builder).router();
        let get = |uri: &str| Request::get(uri).header("accept", "text/html").body(Body::empty()).unwrap();
        let (status, _, page) = send(&router, get("/")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(page.matches(r#"<tr data-key=""#).count(), 60, "every card is a CollectionRow");
        assert_eq!(
            page.matches(r#"<li data-key=""#).count(),
            cards::STARTER.len() + 1,
            "every deck line is a DeckLine, and the template's"
        );
        assert!(page.contains("/scripts/deck_builder.js\">") && page.contains("/scripts/save_note.js\">"), "{page}");
        let (_, _, page) = send(&router, get("/?q=drake&sort=cost")).await;
        assert_eq!(page.matches(r#"<tr data-key=""#).count(), 60, "the filter hides rows; the script can show them again");
        let shown: Vec<&str> =
            page.split(r#"<tr data-key=""#).skip(1).filter(|row| !row[..row.find('>').unwrap()].contains(" hidden")).collect();
        assert_eq!(shown.len(), 1, "one card matches: {shown:?}");
        assert!(shown[0].starts_with("ashen-drake"), "{}", shown[0]);
        // Sorted by cost, cheapest first.
        let first = page.split(r#"<tr data-key=""#).nth(1).unwrap();
        assert!(first.contains("<td><span>0</span></td>") || first.contains("<td><span>1</span></td>"), "{first}");
    }

    /// The scripts' data is checked against these declarations by `npm run check`.
    #[test]
    fn script_types_match_the_server() {
        corten::types::Types::new()
            .add::<Slot>()
            .components(views::COMPONENTS)
            .sync(concat!(env!("CARGO_MANIFEST_DIR"), "/types/app.d.ts"));
    }
}
