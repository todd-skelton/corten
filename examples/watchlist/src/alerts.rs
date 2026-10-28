//! Price alerts: a form checked on the server, then a write answered in place while its read
//! catches up.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use corten::prelude::*;
use serde::Deserialize;

use crate::{account::Viewer, app::url, layout::Layout, market::Market, theme::*};

/// A new price alert, as the form sends it.
#[derive(Deserialize, Fields, Default)]
pub(crate) struct AlertInput {
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
pub(crate) async fn page(req: Req, viewer: Option<Caller<Viewer>>, Query(at): Query<url::Alerts>) -> HtmlResult {
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
pub(crate) async fn save(req: Req, viewer: Option<Caller<Viewer>>, Form(input): Form<AlertInput>) -> HtmlResult {
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

#[cfg(test)]
mod tests {
    use corten::axum::{
        body::Body,
        http::{Request, StatusCode},
    };

    use super::*;
    use crate::testing::{call, get, post, send, text};

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
