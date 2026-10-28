//! Price alerts: a form checked on the server, the alerts a browser keeps with a way to remove
//! one, and a store whose reads catch up with its writes a moment later. A write answers with its
//! receipt, which the address carries (`/alerts?saved=7`); until reads have it, the page says it is
//! saving and reads itself again.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use corten::{i18n::Minor, prelude::*};
use serde::Deserialize;

use crate::{
    account::Viewer,
    app::{Msg, url},
    layout::Layout,
    market::{Market, USD},
    recent::Recent,
    theme::*,
    visitor::Visitor,
};

/// How long a write takes to reach reads. The store stands in for a database whose reads come
/// from a replica a moment behind.
pub(crate) const LAG: Duration = Duration::from_millis(1500);

/// Alerts a browser keeps at most.
const MOST: usize = 20;

/// Browsers whose alerts are kept; past it the one used longest ago is forgotten (`Recent`), as
/// lists are.
const KEPT: usize = 10_000;

/// One alert: a stock and the price that sets it off, in cents.
#[derive(Clone, Debug, PartialEq)]
struct Alert {
    id: u64,
    ticker: &'static str,
    above: u64,
}

/// A change a write makes.
enum Change {
    Add(Alert),
    Remove(u64),
}

/// The alerts, as the handlers share them.
#[derive(Clone)]
pub struct AlertStore {
    book: Arc<Mutex<Book>>,
    lag: Duration,
}

struct Book {
    /// Each browser's alerts, as reads see them.
    alerts: Recent<Vec<Alert>>,
    /// Writes reads do not see yet, oldest first: when each lands, its receipt, whose, and what.
    pending: VecDeque<(Instant, u64, String, Change)>,
    /// The receipt of the last write made, and of the last one reads see.
    made: u64,
    landed: u64,
    /// The last alert's id.
    last: u64,
}

impl Book {
    /// Applies every write due by `now`, in order.
    fn catch_up(&mut self, now: Instant) {
        while self.pending.front().is_some_and(|(due, ..)| *due <= now) {
            let (_, receipt, visitor, change) = self.pending.pop_front().expect("checked above");
            let list = self.alerts.entry(&visitor, Vec::new);
            match change {
                Change::Add(alert) => list.push(alert),
                Change::Remove(id) => list.retain(|alert| alert.id != id),
            }
            self.landed = receipt;
        }
    }

    /// The ids of `visitor`'s alerts once every write made has landed.
    fn will_have(&mut self, visitor: &str) -> Vec<u64> {
        let mut ids: Vec<u64> = self.alerts.get_mut(visitor).map_or_else(Vec::new, |list| list.iter().map(|a| a.id).collect());
        for (_, _, whose, change) in &self.pending {
            if whose == visitor {
                match change {
                    Change::Add(alert) => ids.push(alert.id),
                    Change::Remove(id) => ids.retain(|kept| kept != id),
                }
            }
        }
        ids
    }

    /// Queues `change` to land `lag` after `now`, and returns its receipt.
    fn write(&mut self, now: Instant, lag: Duration, visitor: &str, change: Change) -> u64 {
        self.made += 1;
        self.pending.push_back((now + lag, self.made, visitor.to_string(), change));
        self.made
    }
}

/// What a read sees: the browser's alerts, and what became of the write it waits for.
struct Reading {
    alerts: Vec<Alert>,
    status: Status,
}

/// What became of the write whose receipt the address carries.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Status {
    /// No receipt, or a number this store has not given yet (an edited address).
    None,
    Saving,
    Saved,
}

impl AlertStore {
    pub fn new(lag: Duration) -> Self {
        let book = Book { alerts: Recent::new(KEPT), pending: VecDeque::new(), made: 0, landed: 0, last: 0 };
        AlertStore { book: Arc::new(Mutex::new(book)), lag }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `visitor`'s alerts as reads see them at `now`, and what became of write `saved`.
    fn read(&self, now: Instant, visitor: Option<&str>, saved: Option<u64>) -> Reading {
        let mut book = self.lock();
        book.catch_up(now);
        let alerts = visitor.and_then(|v| book.alerts.get_mut(v)).cloned().unwrap_or_default();
        let status = match saved {
            Some(receipt) if receipt > 0 && receipt <= book.landed => Status::Saved,
            Some(receipt) if receipt > 0 && receipt <= book.made => Status::Saving,
            _ => Status::None,
        };
        Reading { alerts, status }
    }

    /// Adds an alert for `visitor` and returns the write's receipt, unless it would have more than
    /// `MOST`. The count and the write are made under one lock, so two saves sent at once cannot
    /// both pass it.
    fn add(&self, now: Instant, visitor: &str, ticker: &'static str, above: u64) -> Option<u64> {
        let mut book = self.lock();
        book.catch_up(now);
        if book.will_have(visitor).len() >= MOST {
            return None;
        }
        book.last += 1;
        let alert = Alert { id: book.last, ticker, above };
        Some(book.write(now, self.lag, visitor, Change::Add(alert)))
    }

    /// Removes `visitor`'s alert `id` and returns the write's receipt, if it has that alert and
    /// is not already removing it.
    fn remove(&self, now: Instant, visitor: &str, id: u64) -> Option<u64> {
        let mut book = self.lock();
        book.catch_up(now);
        if !book.will_have(visitor).contains(&id) {
            return None;
        }
        Some(book.write(now, self.lag, visitor, Change::Remove(id)))
    }
}

/// A new price alert, as the form sends it.
#[derive(Deserialize, Fields, Default)]
pub(crate) struct AlertInput {
    id: String,
    above: String,
}

/// The alert to remove, as its button sends it.
#[derive(Deserialize, Fields)]
pub(crate) struct RemoveInput {
    alert: u64,
}

/// The alert form's id: a submit replaces it, with the messages of a refused one.
#[derive(Id)]
struct AlertFormId;

/// The alert form: the stock, the price, and what became of the last write. After a refused
/// submit `input` and `errors` are its, so the form comes back as the user left it; after a
/// save they are empty, as the page shows them.
#[component]
fn AlertForm<'a>(input: &'a AlertInput, errors: &'a Errors, status: Status) -> impl Patch {
    let f = AlertInput::FIELDS;
    let e = errors.within(AlertFormId);
    html! {
        <form id={AlertFormId} method="post" action={url::Alerts { saved: None }} data-patch class={(card(), stack(Space::_3))}>
            <h2>"New alert"</h2>
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
            <AlertStatus status={status} />
        </form>
    }
}

/// The list of alerts: a remove answers the status line, and the list is read again with it.
#[derive(Id)]
struct AlertListId;

/// The alerts a browser keeps, each with its remove button.
#[component]
fn AlertList<'a>(t: Msg, alerts: &'a [Alert]) -> impl Patch {
    let f = RemoveInput::FIELDS;
    html! {
        <section id={AlertListId} class={(card(), stack(Space::_3))}>
            <h2>"Your alerts"</h2>
            if alerts.is_empty() {
                <p class={muted()}>"No alerts yet."</p>
            } else {
                <ul class={stack(Space::_2)}>
                    for alert in alerts {
                        <li class={(row(Space::_3), justify_between())}>
                            <span>{alert.ticker}" above "{t.money(Minor(alert.above), USD)}</span>
                            <form method="post" action={url::RemoveAlert} data-patch>
                                <input type="hidden" name={f.alert} value={alert.id}>
                                <button
                                    class={secondary()}
                                    aria-label={("Remove the ", alert.ticker, " alert above ", t.money(Minor(alert.above), USD))}
                                >
                                    "Remove"
                                </button>
                            </form>
                        </li>
                    }
                </ul>
            }
        </section>
    }
}

/// The alerts page, which reads a write that has not landed yet. Until reads have the address's
/// receipt the page says so and asks to be read again, which the runtime does in place however
/// the page was reached; once the write is in, the page stops asking. The alerts are not sent: a
/// real app tells the user from a task of its own.
pub(crate) async fn page(
    req: Req,
    viewer: Option<Caller<Viewer>>,
    State(store): State<AlertStore>,
    visitor: Visitor,
    Query(at): Query<url::Alerts>,
) -> HtmlResult {
    let read = store.read(Instant::now(), visitor.known(), at.saved);
    let t: Msg = req.t();
    // A read again from a write's patch: the status line and the list, asking again from this
    // route while the write is landing. A view renders only when it is sent, so a page load does
    // not build the read-again.
    let from = url::Alerts { saved: at.saved };
    let (status, alerts) = (read.status, &read.alerts);
    let again = (status == Status::Saving).then(|| ReadAgain { after: Duration::from_secs(1) }.from(&from));
    let parts = html! {
        <AlertStatus status={status} />
        <AlertList t={t} alerts={alerts} />
        {again}
    };
    let page = || async { alerts_page(&req, viewer.as_deref(), &read, &AlertInput::default(), &Errors::new()) };
    req.patch_or(parts, page).await
}

fn alerts_page(req: &Req, viewer: Option<&Viewer>, read: &Reading, input: &AlertInput, errors: &Errors) -> HtmlResult {
    let again = (read.status == Status::Saving).then_some(ReadAgain { after: Duration::from_secs(1) });
    req.page(html! {
        <Layout req={req} title="Alerts" viewer={viewer} head={again}>
            <div class={(stack(Space::_4), max_w_prose())}>
                <div class={stack(Space::_1)}>
                    <h1>"Price alerts"</h1>
                    <p class={muted()}>"Hear about it when a stock goes above a price."</p>
                </div>
                <AlertForm input={input} errors={errors} status={read.status} />
                <AlertList t={req.t()} alerts={&read.alerts} />
            </div>
        </Layout>
    })
}

/// Saves an alert. The server checks the price and says what is wrong next to the field. With
/// JavaScript the answer is the form, with any field a refused submit marked cleared, read again
/// until the alert is in; without, the alerts page at the write's receipt, which reads itself
/// again.
pub(crate) async fn save(
    req: Req,
    viewer: Option<Caller<Viewer>>,
    State(store): State<AlertStore>,
    mut visitor: Visitor,
    Form(input): Form<AlertInput>,
) -> HtmlResult {
    let f = AlertInput::FIELDS;
    let mut errors = Errors::new();
    let ticker = Market::ticker(&input.id);
    if ticker.is_none() {
        errors.add_form("Pick a stock on the market.");
    }
    // The price as the reader's locale writes money (`t.parse_money`), in cents.
    let t: Msg = req.t();
    let above = match t.parse_money(&input.above, USD).map(|cents| cents.0) {
        None => {
            errors.add(f.above, "Enter a price in dollars and cents, such as 129.99.");
            0
        }
        Some(0) => {
            errors.add(f.above, "Enter a price above zero.");
            0
        }
        Some(cents) => cents,
    };
    let saved = match ticker {
        Some(ticker) if errors.is_empty() => store.add(Instant::now(), visitor.id(), ticker, above),
        _ => None,
    };
    if errors.is_empty() && saved.is_none() {
        errors.add_form(format!("You have {MOST} alerts. Remove one to add another."));
    }
    let Some(receipt) = saved.filter(|_| errors.is_empty()) else {
        let read = store.read(Instant::now(), visitor.known(), None);
        let form = AlertForm { input: &input, errors: &errors, status: Status::None };
        return req.invalid(&errors, form, || async { alerts_page(&req, viewer.as_deref(), &read, &input, &errors) }).await;
    };
    // With JavaScript, the whole form, empty as the page that a save without JavaScript goes to,
    // so a field a refused submit marked is cleared; the reads that follow answer the status line
    // and the list.
    let from = url::Alerts { saved: Some(receipt) };
    let again = ReadAgain { after: Duration::from_secs(1) }.from(&from);
    let (empty, none) = (AlertInput::default(), Errors::new());
    let form = html! {
        <AlertForm input={&empty} errors={&none} status={Status::Saving} />
        {again}
    };
    let res = req.patch_or(form, || async { req.redirect(&from) }).await?;
    Ok(visitor.with_cookie(res))
}

/// Removes an alert: the status line says it is saving, and the list is read again until the
/// write is in. One the browser does not have, or is already removing (a second click while the
/// first lands), writes nothing: the answer is the list as reads see it.
pub(crate) async fn remove(
    req: Req,
    viewer: Option<Caller<Viewer>>,
    State(store): State<AlertStore>,
    visitor: Visitor,
    Form(input): Form<RemoveInput>,
) -> HtmlResult {
    let now = Instant::now();
    let Some(receipt) = visitor.known().and_then(|id| store.remove(now, id, input.alert)) else {
        let read = store.read(now, visitor.known(), None);
        let list = AlertList { t: req.t(), alerts: &read.alerts };
        return req
            .patch_or(list, || async { alerts_page(&req, viewer.as_deref(), &read, &AlertInput::default(), &Errors::new()) })
            .await;
    };
    let from = url::Alerts { saved: Some(receipt) };
    let again = ReadAgain { after: Duration::from_secs(1) }.from(&from);
    let status = html! {
        <AlertStatus status={Status::Saving} />
        {again}
    };
    req.patch_or(status, || async { req.redirect(&from) }).await
}

/// The alerts page's status line.
#[derive(Id)]
struct AlertStatusId;

/// The alert form's button: a save answers the whole form, and the button keeps focus by its id
/// though a cleared error moves it up the form.
#[derive(Id)]
struct SaveAlertId;

/// What became of the last write: saving, saved, or none.
#[component]
fn AlertStatus(status: Status) -> impl Patch {
    let text = match status {
        Status::None => "",
        Status::Saving => "Saving...",
        Status::Saved => "Saved.",
    };
    html! { <p id={AlertStatusId} role="status">{text}</p> }
}

#[cfg(test)]
mod tests {
    use corten::axum::{
        body::Body,
        http::{Request, StatusCode},
    };

    use super::*;
    use crate::testing::{VISITOR, call, call_on, get_with, post, post_with, router_with_lag, text};

    #[test]
    fn reads_see_a_write_once_it_lands() {
        let store = AlertStore::new(LAG);
        let now = Instant::now();
        let receipt = store.add(now, "a", "ACME", 15000).unwrap();
        let before = store.read(now, Some("a"), Some(receipt));
        assert!(before.alerts.is_empty() && before.status == Status::Saving);
        let after = store.read(now + LAG, Some("a"), Some(receipt));
        assert_eq!(after.alerts, [Alert { id: 1, ticker: "ACME", above: 15000 }]);
        assert_eq!(after.status, Status::Saved);
        assert!(store.read(now + LAG, Some("b"), None).alerts.is_empty(), "another browser's are its own");
        // A receipt the store did not give is no write to wait for.
        assert_eq!(store.read(now + LAG, Some("a"), Some(receipt + 1)).status, Status::None);
        assert_eq!(store.read(now + LAG, Some("a"), Some(0)).status, Status::None);
        let removed = store.remove(now + LAG, "a", 1).unwrap();
        assert_eq!(store.read(now + LAG, Some("a"), Some(removed)).alerts.len(), 1, "still there until the removal lands");
        assert!(store.remove(now + LAG, "a", 1).is_none(), "a second removal writes nothing");
        assert!(store.remove(now + LAG, "b", 1).is_none(), "nor one of another browser's");
        assert!(store.read(now + LAG * 2, Some("a"), Some(removed)).alerts.is_empty());
    }

    #[test]
    fn a_browser_keeps_twenty_alerts_and_memory_stays_bounded() {
        let store = AlertStore::new(Duration::ZERO);
        let now = Instant::now();
        for n in 0..MOST {
            assert!(store.add(now, "a", "ACME", n as u64 + 1).is_some());
        }
        assert!(store.add(now, "a", "ACME", 99).is_none(), "the twenty-first is refused");
        assert!(store.remove(now, "a", 999).is_none(), "removing an alert it does not have makes no room");
        assert!(store.add(now, "a", "ACME", 99).is_none());
        assert!(store.remove(now, "a", 1).is_some());
        assert!(store.add(now, "a", "ACME", 99).is_some(), "a removal makes room");
        for n in 0..KEPT {
            store.add(now, &n.to_string(), "ACME", 1);
        }
        store.read(now, None, None);
        let mut book = store.lock();
        assert_eq!(book.alerts.len(), (KEPT, KEPT));
        assert!(book.alerts.get_mut("a").is_none(), "the browser used longest ago is forgotten");
    }

    #[tokio::test]
    async fn an_alert_is_read_again_until_it_lands() {
        // Writes that take an hour to land, so every read here comes before.
        let app = router_with_lag(Duration::from_secs(3600));
        for (n, price) in ["150", "150.5", ".5", "$1,299.99"].into_iter().enumerate() {
            let body: &'static str = Box::leak(format!("id=ACME&above={price}").into_boxed_str());
            let patch = text(call_on(&app, post_with("/alerts", Some("action"), None, Some(VISITOR), body)).await).await;
            // The whole form, unmarked, as the user left it.
            assert!(patch.starts_with(r#"<form id="alert-form""#) && !patch.contains("aria-invalid"), "{price}: {patch}");
            let waits = format!(
                r#"<p id="alert-status" role="status">Saving...</p></form><template data-corten-read-again="1" data-from="/alerts?saved={}">"#,
                n + 1
            );
            assert!(patch.contains(&waits), "{price}: {patch}");
        }
        // Read again: the status line and the list, still waiting.
        let req =
            Request::get("/alerts?saved=4").header("corten", "action").header("cookie", VISITOR).body(Body::empty()).unwrap();
        let again = text(call_on(&app, req).await).await;
        assert!(
            again.starts_with(r#"<p id="alert-status" role="status">Saving...</p><section id="alert-list""#)
                && again.contains("No alerts yet.")
                && again.contains(r#"data-from="/alerts?saved=4""#),
            "{again}"
        );
        // Without JavaScript, the page at the receipt.
        let res = call_on(&app, post_with("/alerts", None, None, Some(VISITOR), "id=ACME&above=150.5")).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        assert_eq!(res.headers()["location"], "/alerts?saved=5");
    }

    #[tokio::test]
    async fn a_landed_alert_is_listed_and_can_be_removed() {
        let app = router_with_lag(Duration::ZERO);
        call_on(&app, post_with("/alerts", Some("action"), None, Some(VISITOR), "id=ACME&above=150")).await;
        let req =
            Request::get("/alerts?saved=1").header("corten", "action").header("cookie", VISITOR).body(Body::empty()).unwrap();
        let landed = text(call_on(&app, req).await).await;
        assert!(landed.starts_with(r#"<p id="alert-status" role="status">Saved.</p>"#), "{landed}");
        assert!(landed.contains("<span>ACME above $150.00</span>") && !landed.contains("data-corten-read-again"), "{landed}");
        assert!(landed.contains(r#"<input type="hidden" name="alert" value="1">"#), "{landed}");
        let page = text(call_on(&app, get_with("/alerts", VISITOR)).await).await;
        assert!(page.contains("ACME above $150.00"), "the page lists it too");
        let other = text(call_on(&app, get_with("/alerts", "visitor=fedcba9876543210fedcba9876543210")).await).await;
        assert!(other.contains("No alerts yet."), "another browser's list is its own");
        let removed =
            text(call_on(&app, post_with("/alerts/remove", Some("action"), None, Some(VISITOR), "alert=1")).await).await;
        assert!(removed.starts_with(r#"<p id="alert-status" role="status">Saving...</p><template data-corten-read-again="1" data-from="/alerts?saved=2">"#), "{removed}");
        let page = text(call_on(&app, get_with("/alerts?saved=2", VISITOR)).await).await;
        assert!(page.contains("No alerts yet.") && page.contains(r#"role="status">Saved.</p>"#), "{page}");
    }

    #[tokio::test]
    async fn a_removal_of_an_alert_the_browser_does_not_have_answers_the_list() {
        let app = router_with_lag(Duration::ZERO);
        call_on(&app, post_with("/alerts", Some("action"), None, Some(VISITOR), "id=ACME&above=150")).await;
        let other = "visitor=fedcba9876543210fedcba9876543210";
        let list = text(call_on(&app, post_with("/alerts/remove", Some("action"), None, Some(other), "alert=1")).await).await;
        assert!(list.starts_with(r#"<section id="alert-list""#) && list.contains("No alerts yet."), "{list}");
        assert!(!list.contains("data-corten-read-again"), "nothing to wait for: {list}");
        let page = text(call_on(&app, get_with("/alerts", VISITOR)).await).await;
        assert!(page.contains("ACME above $150.00"), "the owner's alert stays");
    }

    #[tokio::test]
    async fn a_browser_with_twenty_alerts_is_asked_to_remove_one() {
        let app = router_with_lag(Duration::ZERO);
        for _ in 0..MOST {
            call_on(&app, post_with("/alerts", Some("action"), None, Some(VISITOR), "id=ACME&above=150")).await;
        }
        let res = call_on(&app, post_with("/alerts", Some("action"), None, Some(VISITOR), "id=ACME&above=150")).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let form = text(res).await;
        assert!(form.contains("You have 20 alerts. Remove one to add another."), "{form}");
    }

    #[tokio::test]
    async fn a_first_save_names_the_browser() {
        let res = call(post("/alerts", Some("action"), None, "id=ACME&above=150")).await;
        let cookie = res.headers()["set-cookie"].to_str().unwrap();
        assert!(cookie.starts_with("visitor="), "{cookie}");
        let res = call(post_with("/alerts", Some("action"), None, Some(VISITOR), "id=ACME&above=150")).await;
        assert!(res.headers().get("set-cookie").is_none(), "a browser with an id keeps it");
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
    async fn a_receipt_the_store_did_not_give_is_not_waited_for() {
        let page = text(call(get_with("/alerts?saved=999", VISITOR)).await).await;
        assert!(
            !page.contains("data-corten-read-again") && page.contains(r#"<p id="alert-status" role="status"></p>"#),
            "{page}"
        );
    }
}
